use async_trait::async_trait;
use rust_decimal::Decimal;
use sqlx::sqlite::SqliteRow;
use sqlx::{QueryBuilder, Row, Sqlite, Transaction};
use tpv_application::{Result as AppResult, StockCountCommand, StockCountResult, StockRepository};
use tpv_domain::Sku;
use tpv_domain::audit::AuditEvent;
use tpv_domain::ids::{ProductId, SaleId, SaleLineId, StockMovementId, StoreId};
use tpv_domain::invoice::{Invoice, InvoiceKind};
use tpv_domain::money::Quantity;
use tpv_domain::sale::{LineKind, Sale};
use tpv_domain::stock::{StockLevel, StockMovement, StockMovementKind};

use crate::error::{StorageError, StorageResult};
use crate::helpers::{
    enum_from_text, enum_to_text, format_time, parse_decimal, parse_optional_time, parse_time,
    parse_uuid,
};
use crate::persistence::insert_audit_tx;
use crate::store::SqliteStore;

fn row_to_level(row: &SqliteRow) -> StorageResult<StockLevel> {
    Ok(StockLevel {
        store_id: StoreId::from_uuid(parse_uuid(&row.try_get::<String, _>("store_id")?)?),
        product_id: ProductId::from_uuid(parse_uuid(&row.try_get::<String, _>("product_id")?)?),
        sku: Sku::new(row.try_get::<String, _>("sku")?)?,
        name: row.try_get("name")?,
        quantity: parse_decimal(&row.try_get::<String, _>("quantity")?)?,
        min_quantity: parse_decimal(&row.try_get::<String, _>("min_quantity")?)?,
        updated_at: parse_optional_time(row.try_get("updated_at")?)?,
    })
}

fn row_to_movement(row: &SqliteRow) -> StorageResult<StockMovement> {
    let sale_id = row
        .try_get::<Option<String>, _>("sale_id")?
        .map(|value| parse_uuid(&value).map(SaleId::from_uuid))
        .transpose()?;
    let sale_line_id = row
        .try_get::<Option<String>, _>("sale_line_id")?
        .map(|value| parse_uuid(&value).map(SaleLineId::from_uuid))
        .transpose()?;
    Ok(StockMovement {
        id: StockMovementId::from_uuid(parse_uuid(&row.try_get::<String, _>("id")?)?),
        store_id: StoreId::from_uuid(parse_uuid(&row.try_get::<String, _>("store_id")?)?),
        product_id: ProductId::from_uuid(parse_uuid(&row.try_get::<String, _>("product_id")?)?),
        kind: enum_from_text(&row.try_get::<String, _>("kind")?)?,
        quantity: Quantity::new(parse_decimal(&row.try_get::<String, _>("quantity")?)?)?,
        reason: row.try_get("reason")?,
        sale_id,
        sale_line_id,
        occurred_at: parse_time(&row.try_get::<String, _>("occurred_at")?)?,
    })
}

impl SqliteStore {
    async fn load_stock_level(
        &self,
        store_id: StoreId,
        product_id: ProductId,
    ) -> StorageResult<Option<StockLevel>> {
        let row = sqlx::query(
            "SELECT ? AS store_id, p.id AS product_id, p.sku, p.name,
                    COALESCE(sl.quantity, '0') AS quantity,
                    COALESCE(sl.min_quantity, '0') AS min_quantity,
                    sl.updated_at
             FROM products p
             LEFT JOIN stock_levels sl
                ON sl.product_id = p.id AND sl.store_id = ?
             WHERE p.id = ?",
        )
        .bind(store_id.to_string())
        .bind(store_id.to_string())
        .bind(product_id.to_string())
        .fetch_optional(self.pool())
        .await?;
        row.as_ref().map(row_to_level).transpose()
    }
}

/// Applies one movement and updates its level inside the caller's transaction.
/// Returns `false` when the movement was already recorded (idempotent replay).
pub(crate) async fn insert_stock_movement_tx(
    tx: &mut Transaction<'_, Sqlite>,
    movement: &StockMovement,
) -> StorageResult<bool> {
    let inserted = sqlx::query(
        "INSERT OR IGNORE INTO stock_movements
            (id, store_id, product_id, kind, quantity, reason, sale_id, sale_line_id, occurred_at)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(movement.id.to_string())
    .bind(movement.store_id.to_string())
    .bind(movement.product_id.to_string())
    .bind(enum_to_text(&movement.kind)?)
    .bind(movement.quantity.as_decimal().to_string())
    .bind(movement.reason.as_deref())
    .bind(movement.sale_id.map(|id| id.to_string()))
    .bind(movement.sale_line_id.map(|id| id.to_string()))
    .bind(format_time(movement.occurred_at)?)
    .execute(&mut **tx)
    .await?
    .rows_affected()
        > 0;
    if !inserted {
        return Ok(false);
    }

    let current = sqlx::query_scalar::<_, String>(
        "SELECT quantity FROM stock_levels WHERE store_id = ? AND product_id = ?",
    )
    .bind(movement.store_id.to_string())
    .bind(movement.product_id.to_string())
    .fetch_optional(&mut **tx)
    .await?
    .map(|value| parse_decimal(&value))
    .transpose()?
    .unwrap_or(Decimal::ZERO);
    let next = current + movement.signed_quantity();
    sqlx::query(
        "INSERT INTO stock_levels (store_id, product_id, quantity, min_quantity, updated_at)
         VALUES (?, ?, ?, '0', ?)
         ON CONFLICT(store_id, product_id) DO UPDATE SET
            quantity = excluded.quantity,
            updated_at = excluded.updated_at",
    )
    .bind(movement.store_id.to_string())
    .bind(movement.product_id.to_string())
    .bind(next.normalize().to_string())
    .bind(format_time(movement.occurred_at)?)
    .execute(&mut **tx)
    .await?;
    Ok(true)
}

/// Replenishes stock for the lines of a rectificative invoice. The movement is
/// written in the invoice transaction so the document and the ledger cannot
/// diverge. Multiple partial returns of the same sale line are valid, therefore
/// the movement references the original sale but not the sale line.
pub(crate) async fn record_invoice_return_stock_tx(
    tx: &mut Transaction<'_, Sqlite>,
    invoice: &Invoice,
) -> StorageResult<()> {
    if !matches!(invoice.kind, InvoiceKind::Rectificative { .. }) {
        return Ok(());
    }
    for line in &invoice.lines {
        if line.kind != LineKind::Sale {
            continue;
        }
        let product_id = sqlx::query_scalar::<_, Option<String>>(
            "SELECT product_id FROM sale_lines WHERE id = ?",
        )
        .bind(line.source_line_id.to_string())
        .fetch_optional(&mut **tx)
        .await?
        .flatten();
        let Some(product_id) = product_id else {
            continue;
        };
        let product_id = ProductId::from_uuid(parse_uuid(&product_id)?);
        let movement = StockMovement::new(
            StockMovementId::new(),
            invoice_store_id(tx, invoice).await?,
            product_id,
            StockMovementKind::Return,
            line.quantity,
            Some(format!("Devolución factura {}", invoice.full_number())),
            Some(invoice.sale_id),
            None,
            invoice.issued_at,
        )?;
        insert_stock_movement_tx(tx, &movement).await?;
    }
    Ok(())
}

async fn invoice_store_id(
    tx: &mut Transaction<'_, Sqlite>,
    invoice: &Invoice,
) -> StorageResult<StoreId> {
    let store_id = sqlx::query_scalar::<_, String>("SELECT store_id FROM invoices WHERE id = ?")
        .bind(invoice.id.to_string())
        .fetch_optional(&mut **tx)
        .await?
        .ok_or_else(|| StorageError::NotFound(format!("invoice {}", invoice.id)))?;
    Ok(StoreId::from_uuid(parse_uuid(&store_id)?))
}

/// Discounts stock for a freshly confirmed sale. Draft saves never call this.
pub(crate) async fn record_sale_stock_tx(
    tx: &mut Transaction<'_, Sqlite>,
    sale: &Sale,
) -> StorageResult<()> {
    if !sale.is_confirmed() {
        return Ok(());
    }
    let occurred_at = sale.closed_at.unwrap_or(sale.created_at);
    for line in &sale.lines {
        let Some(product_id) = line.product_id else {
            continue;
        };
        let kind = match line.kind {
            LineKind::Sale => StockMovementKind::Sale,
            LineKind::Return => StockMovementKind::Return,
        };
        let movement = StockMovement::new(
            StockMovementId::new(),
            sale.store_id,
            product_id,
            kind,
            line.quantity,
            None,
            Some(sale.id),
            Some(line.id),
            occurred_at,
        )?;
        insert_stock_movement_tx(tx, &movement).await?;
    }
    Ok(())
}

impl SqliteStore {
    async fn list_stock_levels_inner(&self, store_id: StoreId) -> StorageResult<Vec<StockLevel>> {
        let rows = sqlx::query(
            "SELECT ? AS store_id, p.id AS product_id, p.sku, p.name,
                    COALESCE(sl.quantity, '0') AS quantity,
                    COALESCE(sl.min_quantity, '0') AS min_quantity,
                    sl.updated_at
             FROM products p
             LEFT JOIN stock_levels sl
                ON sl.product_id = p.id AND sl.store_id = ?
             ORDER BY lower(p.name), p.sku",
        )
        .bind(store_id.to_string())
        .bind(store_id.to_string())
        .fetch_all(self.pool())
        .await?;
        rows.iter().map(row_to_level).collect()
    }

    async fn list_stock_levels_for_products_inner(
        &self,
        store_id: StoreId,
        product_ids: &[ProductId],
    ) -> StorageResult<Vec<StockLevel>> {
        if product_ids.is_empty() {
            return Ok(Vec::new());
        }
        let mut query = QueryBuilder::new("SELECT ");
        query.push_bind(store_id.to_string());
        query.push(
            " AS store_id, p.id AS product_id, p.sku, p.name,
                    COALESCE(sl.quantity, '0') AS quantity,
                    COALESCE(sl.min_quantity, '0') AS min_quantity,
                    sl.updated_at
             FROM products p
             LEFT JOIN stock_levels sl
                ON sl.product_id = p.id AND sl.store_id = ",
        );
        query.push_bind(store_id.to_string());
        query.push(" WHERE p.id IN (");
        {
            let mut separated = query.separated(", ");
            for product_id in product_ids {
                separated.push_bind(product_id.to_string());
            }
        }
        query.push(") ORDER BY lower(p.name), p.sku");
        let rows = query.build().fetch_all(self.pool()).await?;
        rows.iter().map(row_to_level).collect()
    }

    async fn count_low_stock_inner(&self, store_id: StoreId) -> StorageResult<u64> {
        let count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*)
             FROM products p
             JOIN stock_levels sl
               ON sl.product_id = p.id AND sl.store_id = ?
             WHERE CAST(sl.min_quantity AS REAL) > 0
               AND CAST(sl.quantity AS REAL) <= CAST(sl.min_quantity AS REAL)",
        )
        .bind(store_id.to_string())
        .fetch_one(self.pool())
        .await?;
        Ok(u64::try_from(count).unwrap_or_default())
    }

    async fn list_stock_movements_inner(
        &self,
        store_id: StoreId,
        product_id: ProductId,
        limit: u32,
    ) -> StorageResult<Vec<StockMovement>> {
        let rows = sqlx::query(
            "SELECT * FROM stock_movements
             WHERE store_id = ? AND product_id = ?
             ORDER BY occurred_at DESC, rowid DESC
             LIMIT ?",
        )
        .bind(store_id.to_string())
        .bind(product_id.to_string())
        .bind(i64::from(limit))
        .fetch_all(self.pool())
        .await?;
        rows.iter().map(row_to_movement).collect()
    }

    async fn record_stock_movement_inner(
        &self,
        movement: &StockMovement,
        audit: &AuditEvent,
    ) -> StorageResult<()> {
        let mut tx = self.pool().begin().await?;
        insert_stock_movement_tx(&mut tx, movement).await?;
        insert_audit_tx(&mut tx, audit).await?;
        tx.commit().await?;
        Ok(())
    }

    async fn set_stock_count_inner(
        &self,
        command: &StockCountCommand,
        audit: &AuditEvent,
    ) -> StorageResult<StockCountResult> {
        let mut tx = self.pool().begin().await?;
        let previous = sqlx::query_scalar::<_, String>(
            "SELECT quantity FROM stock_levels WHERE store_id = ? AND product_id = ?",
        )
        .bind(command.store_id.to_string())
        .bind(command.product_id.to_string())
        .fetch_optional(&mut *tx)
        .await?
        .map(|value| parse_decimal(&value))
        .transpose()?
        .unwrap_or(Decimal::ZERO);
        let difference = command.counted_quantity - previous;
        let movement = if difference.is_zero() {
            // Even a count without differences leaves the level timestamp and
            // the audit trail, but no artificial zero-quantity movement.
            sqlx::query(
                "INSERT INTO stock_levels (store_id, product_id, quantity, min_quantity, updated_at)
                 VALUES (?, ?, ?, '0', ?)
                 ON CONFLICT(store_id, product_id) DO UPDATE SET
                    quantity = excluded.quantity,
                    updated_at = excluded.updated_at",
            )
            .bind(command.store_id.to_string())
            .bind(command.product_id.to_string())
            .bind(command.counted_quantity.normalize().to_string())
            .bind(format_time(command.occurred_at)?)
            .execute(&mut *tx)
            .await?;
            None
        } else {
            let kind = if difference.is_sign_negative() {
                StockMovementKind::AdjustmentOut
            } else {
                StockMovementKind::AdjustmentIn
            };
            let quantity = if difference.is_sign_negative() {
                -difference
            } else {
                difference
            };
            let movement = StockMovement::new(
                command.movement_id,
                command.store_id,
                command.product_id,
                kind,
                Quantity::new(quantity)?,
                Some(command.reason.clone()),
                None,
                None,
                command.occurred_at,
            )?;
            insert_stock_movement_tx(&mut tx, &movement).await?;
            Some(movement)
        };
        insert_audit_tx(&mut tx, audit).await?;
        tx.commit().await?;
        let level = self
            .load_stock_level(command.store_id, command.product_id)
            .await?
            .ok_or_else(|| StorageError::NotFound(format!("stock level {}", command.product_id)))?;
        Ok(StockCountResult {
            level,
            previous_quantity: previous,
            counted_quantity: command.counted_quantity,
            difference,
            movement,
        })
    }

    async fn set_stock_minimum_inner(
        &self,
        store_id: StoreId,
        product_id: ProductId,
        min_quantity: Decimal,
        audit: &AuditEvent,
    ) -> StorageResult<StockLevel> {
        let exists = sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM products WHERE id = ?")
            .bind(product_id.to_string())
            .fetch_one(self.pool())
            .await?
            > 0;
        if !exists {
            return Err(StorageError::NotFound(format!("product {product_id}")));
        }
        let mut tx = self.pool().begin().await?;
        sqlx::query(
            "INSERT INTO stock_levels (store_id, product_id, quantity, min_quantity, updated_at)
             VALUES (?, ?, '0', ?, ?)
             ON CONFLICT(store_id, product_id) DO UPDATE SET
                min_quantity = excluded.min_quantity,
                updated_at = excluded.updated_at",
        )
        .bind(store_id.to_string())
        .bind(product_id.to_string())
        .bind(min_quantity.normalize().to_string())
        .bind(format_time(audit.occurred_at)?)
        .execute(&mut *tx)
        .await?;
        insert_audit_tx(&mut tx, audit).await?;
        tx.commit().await?;
        self.load_stock_level(store_id, product_id)
            .await?
            .ok_or_else(|| StorageError::NotFound(format!("stock level {product_id}")))
    }
}

#[async_trait]
impl StockRepository for SqliteStore {
    async fn list_levels(&self, store_id: StoreId) -> AppResult<Vec<StockLevel>> {
        Ok(self.list_stock_levels_inner(store_id).await?)
    }

    async fn count_low_stock(&self, store_id: StoreId) -> AppResult<u64> {
        Ok(self.count_low_stock_inner(store_id).await?)
    }

    async fn list_levels_for_products(
        &self,
        store_id: StoreId,
        product_ids: &[ProductId],
    ) -> AppResult<Vec<StockLevel>> {
        Ok(self
            .list_stock_levels_for_products_inner(store_id, product_ids)
            .await?)
    }

    async fn get_level(
        &self,
        store_id: StoreId,
        product_id: ProductId,
    ) -> AppResult<Option<StockLevel>> {
        Ok(self.load_stock_level(store_id, product_id).await?)
    }

    async fn list_movements(
        &self,
        store_id: StoreId,
        product_id: ProductId,
        limit: u32,
    ) -> AppResult<Vec<StockMovement>> {
        Ok(self
            .list_stock_movements_inner(store_id, product_id, limit)
            .await?)
    }

    async fn record_movement(&self, movement: &StockMovement, audit: &AuditEvent) -> AppResult<()> {
        Ok(self.record_stock_movement_inner(movement, audit).await?)
    }

    async fn set_minimum(
        &self,
        store_id: StoreId,
        product_id: ProductId,
        min_quantity: Decimal,
        audit: &AuditEvent,
    ) -> AppResult<StockLevel> {
        Ok(self
            .set_stock_minimum_inner(store_id, product_id, min_quantity, audit)
            .await?)
    }

    async fn set_count(
        &self,
        command: &StockCountCommand,
        audit: &AuditEvent,
    ) -> AppResult<StockCountResult> {
        Ok(self.set_stock_count_inner(command, audit).await?)
    }
}
