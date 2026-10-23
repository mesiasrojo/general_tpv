use sqlx::{Row, Sqlite, Transaction};
use tpv_application::{
    ApplicationError, MasterDataRepository, PublishMasterDataRequest, Result as AppResult,
};
use tpv_domain::ids::{StoreId, TenantId};
use tpv_domain::master_data::{
    MasterDataApplyResult, MasterDataChange, MasterDataConflict, MasterDataKind, resolve_change,
};
use tpv_domain::sync::SyncEvent;
use uuid::Uuid;

use crate::error::StorageResult;
use crate::helpers::{
    enum_from_text, enum_to_text, format_time, i64_to_u64, parse_time, parse_uuid,
};
use crate::persistence::enqueue_event_tx;
use crate::store::SqliteStore;

async fn read_master_change_tx(
    tx: &mut Transaction<'_, Sqlite>,
    tenant_id: TenantId,
    store_id: StoreId,
    kind: MasterDataKind,
    entity_id: Uuid,
) -> StorageResult<Option<MasterDataChange>> {
    let row = sqlx::query(
        "SELECT change_json FROM master_data_state
         WHERE tenant_id = ? AND store_id = ? AND kind = ? AND entity_id = ?",
    )
    .bind(tenant_id.to_string())
    .bind(store_id.to_string())
    .bind(kind.as_str())
    .bind(entity_id.to_string())
    .fetch_optional(&mut **tx)
    .await?;
    row.map(|row| {
        let encoded: String = row.try_get("change_json")?;
        Ok(serde_json::from_str(&encoded)?)
    })
    .transpose()
}

async fn upsert_master_change_tx(
    tx: &mut Transaction<'_, Sqlite>,
    tenant_id: TenantId,
    store_id: StoreId,
    change: &MasterDataChange,
    updated_at: time::OffsetDateTime,
) -> StorageResult<()> {
    let encoded = serde_json::to_string(change)?;
    sqlx::query(
        "INSERT INTO master_data_state
            (tenant_id, store_id, kind, entity_id, version, change_json, updated_at)
         VALUES (?, ?, ?, ?, ?, ?, ?)
         ON CONFLICT(tenant_id, store_id, kind, entity_id) DO UPDATE SET
            version = excluded.version,
            change_json = excluded.change_json,
            updated_at = excluded.updated_at",
    )
    .bind(tenant_id.to_string())
    .bind(store_id.to_string())
    .bind(change.kind.as_str())
    .bind(change.entity_id.to_string())
    .bind(i64::try_from(change.version).unwrap_or(i64::MAX))
    .bind(encoded)
    .bind(format_time(updated_at)?)
    .execute(&mut **tx)
    .await?;
    Ok(())
}

async fn insert_master_conflict_tx(
    tx: &mut Transaction<'_, Sqlite>,
    conflict: &MasterDataConflict,
) -> StorageResult<()> {
    let incoming = serde_json::to_string(&conflict.incoming)?;
    sqlx::query(
        "INSERT INTO master_data_conflicts
            (id, tenant_id, store_id, kind, entity_id, local_version,
             incoming_json, reason, outcome, detected_at)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(conflict.id.to_string())
    .bind(conflict.tenant_id.to_string())
    .bind(conflict.store_id.to_string())
    .bind(conflict.kind.as_str())
    .bind(conflict.entity_id.to_string())
    .bind(i64::try_from(conflict.local_version).unwrap_or(i64::MAX))
    .bind(incoming)
    .bind(enum_to_text(&conflict.reason)?)
    .bind(enum_to_text(&conflict.outcome)?)
    .bind(format_time(conflict.detected_at)?)
    .execute(&mut **tx)
    .await?;
    Ok(())
}

#[async_trait::async_trait]
impl MasterDataRepository for SqliteStore {
    async fn apply_change(
        &self,
        tenant_id: TenantId,
        store_id: StoreId,
        change: &MasterDataChange,
    ) -> AppResult<MasterDataApplyResult> {
        let result: StorageResult<MasterDataApplyResult> = async {
            let mut tx = self.pool().begin().await?;
            let local =
                read_master_change_tx(&mut tx, tenant_id, store_id, change.kind, change.entity_id)
                    .await?;
            let resolution = resolve_change(local.as_ref(), change);
            let now = time::OffsetDateTime::now_utc();
            let conflict = if let Some(reason) = resolution.conflict {
                let conflict = MasterDataConflict {
                    id: Uuid::now_v7(),
                    tenant_id,
                    store_id,
                    kind: change.kind,
                    entity_id: change.entity_id,
                    local_version: local.as_ref().map_or(0, |local| local.version),
                    incoming: change.clone(),
                    reason,
                    outcome: resolution.outcome,
                    detected_at: now,
                };
                insert_master_conflict_tx(&mut tx, &conflict).await?;
                Some(conflict)
            } else {
                None
            };
            if resolution.applied() {
                upsert_master_change_tx(&mut tx, tenant_id, store_id, change, now).await?;
            }
            tx.commit().await?;
            Ok(MasterDataApplyResult {
                applied: resolution.applied(),
                conflict,
            })
        }
        .await;
        result.map_err(ApplicationError::from)
    }

    async fn publish_change(&self, request: PublishMasterDataRequest) -> AppResult<SyncEvent> {
        let PublishMasterDataRequest {
            context,
            store_id,
            kind,
            entity_id,
            payload,
            deleted,
            occurred_at,
        } = request;
        let result: StorageResult<SyncEvent> = async {
            let mut tx = self.pool().begin().await?;
            let local =
                read_master_change_tx(&mut tx, context.tenant_id, store_id, kind, entity_id)
                    .await?;
            let version = local
                .as_ref()
                .map_or(1, |local| local.version.saturating_add(1));
            let change = MasterDataChange::new(
                kind,
                entity_id,
                version,
                context.device_id,
                occurred_at,
                payload,
                deleted,
            )?;
            upsert_master_change_tx(&mut tx, context.tenant_id, store_id, &change, occurred_at)
                .await?;
            let sequence = u64::try_from(occurred_at.unix_timestamp_nanos()).unwrap_or_default();
            let event_type = if deleted {
                "master_data.deleted"
            } else {
                "master_data.upsert"
            };
            let event = SyncEvent::new(
                context.tenant_id,
                store_id,
                context.device_id,
                sequence,
                "master_data",
                entity_id,
                event_type,
                1,
                occurred_at,
                serde_json::to_value(&change)?,
            )?;
            enqueue_event_tx(&mut tx, &event).await?;
            tx.commit().await?;
            Ok(event)
        }
        .await;
        result.map_err(ApplicationError::from)
    }

    async fn list_conflicts(&self, limit: u32) -> AppResult<Vec<MasterDataConflict>> {
        let result: StorageResult<Vec<MasterDataConflict>> = async {
            let rows = sqlx::query(
                "SELECT id, tenant_id, store_id, kind, entity_id, local_version,
                        incoming_json, reason, outcome, detected_at
                 FROM master_data_conflicts
                 ORDER BY detected_at DESC
                 LIMIT ?",
            )
            .bind(i64::from(limit))
            .fetch_all(self.pool())
            .await?;
            rows.iter()
                .map(|row| {
                    let kind = MasterDataKind::parse(&row.try_get::<String, _>("kind")?)?;
                    let incoming: MasterDataChange =
                        serde_json::from_str(&row.try_get::<String, _>("incoming_json")?)?;
                    Ok(MasterDataConflict {
                        id: parse_uuid(&row.try_get::<String, _>("id")?)?,
                        tenant_id: TenantId::from_uuid(parse_uuid(
                            &row.try_get::<String, _>("tenant_id")?,
                        )?),
                        store_id: StoreId::from_uuid(parse_uuid(
                            &row.try_get::<String, _>("store_id")?,
                        )?),
                        kind,
                        entity_id: parse_uuid(&row.try_get::<String, _>("entity_id")?)?,
                        local_version: i64_to_u64(row.try_get("local_version")?, "local_version")?,
                        incoming,
                        reason: enum_from_text(&row.try_get::<String, _>("reason")?)?,
                        outcome: enum_from_text(&row.try_get::<String, _>("outcome")?)?,
                        detected_at: parse_time(&row.try_get::<String, _>("detected_at")?)?,
                    })
                })
                .collect()
        }
        .await;
        result.map_err(ApplicationError::from)
    }
}
