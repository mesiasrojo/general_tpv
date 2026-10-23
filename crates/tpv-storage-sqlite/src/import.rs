use std::collections::HashMap;

use async_trait::async_trait;
use sqlx::{Row, Sqlite, Transaction};
use tpv_application::{
    ApplicationError, ProductImportPreview, ProductImportRepository, ProductImportRow, Result,
    SyncContext,
};
use tpv_domain::catalog::Product;
use tpv_domain::ids::{ProductId, StoreId, TenantId};
use tpv_domain::master_data::{MasterDataChange, MasterDataKind};
use tpv_domain::sync::SyncEvent;
use uuid::Uuid;

use crate::error::{StorageError, StorageResult};
use crate::helpers::{format_time, parse_uuid};
use crate::master_data::upsert_master_change_tx;
use crate::persistence::enqueue_event_tx;
use crate::store::{SqliteStore, row_to_product};

/// SQLite has a limited number of bind parameters per statement. Batches are
/// chunked so imports can contain thousands of rows without hitting the limit.
const QUERY_CHUNK: usize = 400;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Outcome {
    Inserted,
    Updated,
    Unchanged,
}

struct ResolvedRow {
    source_id: String,
    product: Product,
    outcome: Outcome,
}

#[async_trait]
impl ProductImportRepository for SqliteStore {
    async fn preview_import(
        &self,
        source_system: &str,
        batch_id: &str,
        rows: &[ProductImportRow],
    ) -> Result<ProductImportPreview> {
        let mut tx = self.pool().begin().await.map_err(StorageError::from)?;
        if let Some(preview) = read_batch_tx(&mut tx, source_system, batch_id).await? {
            tx.commit().await.map_err(StorageError::from)?;
            return Ok(preview);
        }
        let resolved = resolve_rows_tx(&mut tx, source_system, rows).await?;
        tx.commit().await.map_err(StorageError::from)?;
        Ok(count_outcomes(&resolved))
    }

    async fn commit_import(
        &self,
        context: &SyncContext,
        store_id: StoreId,
        source_system: &str,
        batch_id: &str,
        rows: &[ProductImportRow],
    ) -> Result<ProductImportPreview> {
        let mut tx = self.pool().begin().await.map_err(StorageError::from)?;
        if let Some(preview) = read_batch_tx(&mut tx, source_system, batch_id).await? {
            tx.commit().await.map_err(StorageError::from)?;
            return Ok(preview);
        }

        let resolved = resolve_rows_tx(&mut tx, source_system, rows).await?;
        let preview = count_outcomes(&resolved);
        let entity_ids = resolved
            .iter()
            .filter(|row| row.outcome != Outcome::Unchanged)
            .map(|row| row.product.id.as_uuid())
            .collect::<Vec<_>>();
        let mut versions =
            load_versions_tx(&mut tx, context.tenant_id, store_id, &entity_ids).await?;
        let now = time::OffsetDateTime::now_utc();

        for (index, row) in resolved.iter().enumerate() {
            upsert_product_tx(&mut tx, &row.product).await?;
            upsert_source_tx(&mut tx, source_system, &row.source_id, row.product.id, now).await?;
            if row.outcome == Outcome::Unchanged {
                continue;
            }
            let entity_id = row.product.id.as_uuid();
            let version = versions
                .get(&entity_id)
                .copied()
                .unwrap_or(0)
                .saturating_add(1);
            versions.insert(entity_id, version);
            let occurred_at = now
                .checked_add(time::Duration::nanoseconds(
                    i64::try_from(index).unwrap_or_default(),
                ))
                .unwrap_or(now);
            let change = MasterDataChange::new(
                MasterDataKind::Product,
                entity_id,
                version,
                context.device_id,
                occurred_at,
                serde_json::to_value(&row.product).map_err(StorageError::from)?,
                false,
            )
            .map_err(StorageError::from)?;
            upsert_master_change_tx(&mut tx, context.tenant_id, store_id, &change, occurred_at)
                .await?;
            let sequence = u64::try_from(occurred_at.unix_timestamp_nanos()).unwrap_or_default();
            let event = SyncEvent::new(
                context.tenant_id,
                store_id,
                context.device_id,
                sequence,
                "master_data",
                entity_id,
                "master_data.upsert",
                1,
                occurred_at,
                serde_json::to_value(&change).map_err(StorageError::from)?,
            )
            .map_err(StorageError::from)?;
            enqueue_event_tx(&mut tx, &event).await?;
        }

        insert_batch_tx(&mut tx, source_system, batch_id, rows.len(), preview, now).await?;
        tx.commit().await.map_err(StorageError::from)?;
        Ok(preview)
    }
}

async fn resolve_rows_tx(
    tx: &mut Transaction<'_, Sqlite>,
    source_system: &str,
    rows: &[ProductImportRow],
) -> Result<Vec<ResolvedRow>> {
    let mut skus = rows
        .iter()
        .map(|row| row.product.sku.as_str().to_string())
        .collect::<Vec<_>>();
    skus.sort();
    skus.dedup();
    let mut source_ids = rows
        .iter()
        .map(|row| row.source_id.clone())
        .collect::<Vec<_>>();
    source_ids.sort();
    source_ids.dedup();

    let by_sku = load_products_by_sku_tx(tx, &skus).await?;
    let mappings = load_source_mappings_tx(tx, source_system, &source_ids).await?;
    let mut mapped_ids = mappings.values().copied().collect::<Vec<_>>();
    mapped_ids.sort_unstable();
    mapped_ids.dedup();
    let by_id = load_products_by_id_tx(tx, &mapped_ids).await?;

    let mut resolved = Vec::with_capacity(rows.len());
    for row in rows {
        let mapped_id = mappings.get(&row.source_id).copied();
        let existing = mapped_id
            .and_then(|id| by_id.get(&id))
            .or_else(|| by_sku.get(row.product.sku.as_str()));
        let (product, outcome) = match existing {
            None => (row.product.clone(), Outcome::Inserted),
            Some(existing) => {
                if let Some(mapped_id) = mapped_id {
                    if let Some(other) = by_sku.get(row.product.sku.as_str()) {
                        if other.id.as_uuid() != mapped_id {
                            return Err(ApplicationError::Conflict(format!(
                                "el SKU {} ya pertenece a otro producto y el origen {} apunta a otro",
                                row.product.sku, row.source_id
                            )));
                        }
                    }
                }
                let mut candidate = row.product.clone();
                candidate.id = existing.id;
                let outcome = if candidate == *existing {
                    Outcome::Unchanged
                } else {
                    Outcome::Updated
                };
                (candidate, outcome)
            }
        };
        resolved.push(ResolvedRow {
            source_id: row.source_id.clone(),
            product,
            outcome,
        });
    }
    Ok(resolved)
}

fn count_outcomes(rows: &[ResolvedRow]) -> ProductImportPreview {
    let mut preview = ProductImportPreview::default();
    for row in rows {
        match row.outcome {
            Outcome::Inserted => preview.inserted += 1,
            Outcome::Updated => preview.updated += 1,
            Outcome::Unchanged => preview.unchanged += 1,
        }
    }
    preview
}

async fn read_batch_tx(
    tx: &mut Transaction<'_, Sqlite>,
    source_system: &str,
    batch_id: &str,
) -> StorageResult<Option<ProductImportPreview>> {
    let row = sqlx::query(
        "SELECT inserted, updated, unchanged
         FROM product_import_batches
         WHERE source_system = ? AND batch_id = ?",
    )
    .bind(source_system)
    .bind(batch_id)
    .fetch_optional(&mut **tx)
    .await?;
    row.map(|row| {
        let inserted = row.try_get::<i64, _>("inserted")?;
        let updated = row.try_get::<i64, _>("updated")?;
        let unchanged = row.try_get::<i64, _>("unchanged")?;
        Ok(ProductImportPreview {
            already_imported: true,
            inserted: usize::try_from(inserted)
                .map_err(|_| StorageError::InvalidData("invalid inserted count".into()))?,
            updated: usize::try_from(updated)
                .map_err(|_| StorageError::InvalidData("invalid updated count".into()))?,
            unchanged: usize::try_from(unchanged)
                .map_err(|_| StorageError::InvalidData("invalid unchanged count".into()))?,
        })
    })
    .transpose()
}

async fn insert_batch_tx(
    tx: &mut Transaction<'_, Sqlite>,
    source_system: &str,
    batch_id: &str,
    total_rows: usize,
    preview: ProductImportPreview,
    committed_at: time::OffsetDateTime,
) -> StorageResult<()> {
    sqlx::query(
        "INSERT INTO product_import_batches
            (id, source_system, batch_id, total_rows, inserted, updated, unchanged, committed_at)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(Uuid::now_v7().to_string())
    .bind(source_system)
    .bind(batch_id)
    .bind(i64::try_from(total_rows).unwrap_or(i64::MAX))
    .bind(i64::try_from(preview.inserted).unwrap_or(i64::MAX))
    .bind(i64::try_from(preview.updated).unwrap_or(i64::MAX))
    .bind(i64::try_from(preview.unchanged).unwrap_or(i64::MAX))
    .bind(format_time(committed_at)?)
    .execute(&mut **tx)
    .await?;
    Ok(())
}

async fn upsert_product_tx(
    tx: &mut Transaction<'_, Sqlite>,
    product: &Product,
) -> StorageResult<()> {
    sqlx::query(
        "INSERT INTO products
            (id, sku, barcode, name, family_code, unit_price, tax_code, tax_percent_bp, active)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)
         ON CONFLICT(id) DO UPDATE SET
            sku = excluded.sku,
            barcode = excluded.barcode,
            name = excluded.name,
            family_code = excluded.family_code,
            unit_price = excluded.unit_price,
            tax_code = excluded.tax_code,
            tax_percent_bp = excluded.tax_percent_bp,
            active = excluded.active",
    )
    .bind(product.id.to_string())
    .bind(product.sku.as_str())
    .bind(product.barcode.as_deref())
    .bind(&product.name)
    .bind(product.family_code.as_deref())
    .bind(product.unit_price.to_string())
    .bind(&product.tax_rate.code)
    .bind(i64::from(product.tax_rate.percent_bp))
    .bind(i64::from(product.active))
    .execute(&mut **tx)
    .await?;
    Ok(())
}

async fn upsert_source_tx(
    tx: &mut Transaction<'_, Sqlite>,
    source_system: &str,
    source_id: &str,
    product_id: ProductId,
    updated_at: time::OffsetDateTime,
) -> StorageResult<()> {
    sqlx::query(
        "INSERT INTO product_import_sources
            (source_system, source_id, product_id, updated_at)
         VALUES (?, ?, ?, ?)
         ON CONFLICT(source_system, source_id) DO UPDATE SET
            product_id = excluded.product_id,
            updated_at = excluded.updated_at",
    )
    .bind(source_system)
    .bind(source_id)
    .bind(product_id.to_string())
    .bind(format_time(updated_at)?)
    .execute(&mut **tx)
    .await?;
    Ok(())
}

async fn load_products_by_sku_tx(
    tx: &mut Transaction<'_, Sqlite>,
    skus: &[String],
) -> StorageResult<HashMap<String, Product>> {
    let mut products = HashMap::new();
    for chunk in skus.chunks(QUERY_CHUNK) {
        let sql = format!(
            "SELECT * FROM products WHERE sku IN ({})",
            placeholders(chunk.len())
        );
        let mut query = sqlx::query(&sql);
        for sku in chunk {
            query = query.bind(sku);
        }
        for row in query.fetch_all(&mut **tx).await? {
            let product = row_to_product(&row)?;
            products.insert(product.sku.as_str().to_string(), product);
        }
    }
    Ok(products)
}

async fn load_products_by_id_tx(
    tx: &mut Transaction<'_, Sqlite>,
    ids: &[Uuid],
) -> StorageResult<HashMap<Uuid, Product>> {
    let mut products = HashMap::new();
    for chunk in ids.chunks(QUERY_CHUNK) {
        let sql = format!(
            "SELECT * FROM products WHERE id IN ({})",
            placeholders(chunk.len())
        );
        let mut query = sqlx::query(&sql);
        for id in chunk {
            query = query.bind(id.to_string());
        }
        for row in query.fetch_all(&mut **tx).await? {
            let product = row_to_product(&row)?;
            products.insert(product.id.as_uuid(), product);
        }
    }
    Ok(products)
}

async fn load_source_mappings_tx(
    tx: &mut Transaction<'_, Sqlite>,
    source_system: &str,
    source_ids: &[String],
) -> StorageResult<HashMap<String, Uuid>> {
    let mut mappings = HashMap::new();
    for chunk in source_ids.chunks(QUERY_CHUNK) {
        let sql = format!(
            "SELECT source_id, product_id
             FROM product_import_sources
             WHERE source_system = ? AND source_id IN ({})",
            placeholders(chunk.len())
        );
        let mut query = sqlx::query(&sql).bind(source_system);
        for source_id in chunk {
            query = query.bind(source_id);
        }
        for row in query.fetch_all(&mut **tx).await? {
            let source_id: String = row.try_get("source_id")?;
            let product_id: String = row.try_get("product_id")?;
            mappings.insert(source_id, parse_uuid(&product_id)?);
        }
    }
    Ok(mappings)
}

async fn load_versions_tx(
    tx: &mut Transaction<'_, Sqlite>,
    tenant_id: TenantId,
    store_id: StoreId,
    entity_ids: &[Uuid],
) -> StorageResult<HashMap<Uuid, u64>> {
    let mut versions = HashMap::new();
    for chunk in entity_ids.chunks(QUERY_CHUNK) {
        let sql = format!(
            "SELECT entity_id, version
             FROM master_data_state
             WHERE tenant_id = ? AND store_id = ? AND kind = ?
               AND entity_id IN ({})",
            placeholders(chunk.len())
        );
        let mut query = sqlx::query(&sql)
            .bind(tenant_id.to_string())
            .bind(store_id.to_string())
            .bind(MasterDataKind::Product.as_str());
        for entity_id in chunk {
            query = query.bind(entity_id.to_string());
        }
        for row in query.fetch_all(&mut **tx).await? {
            let entity_id: String = row.try_get("entity_id")?;
            let version: i64 = row.try_get("version")?;
            versions.insert(
                parse_uuid(&entity_id)?,
                u64::try_from(version).unwrap_or(u64::MAX),
            );
        }
    }
    Ok(versions)
}

fn placeholders(count: usize) -> String {
    vec!["?"; count].join(",")
}
