use async_trait::async_trait;
use sqlx::Row;
use tpv_application::{AuditRepository, Result as AppResult};
use tpv_domain::audit::{AuditAction, AuditEvent};
use tpv_domain::ids::{AuditEventId, StoreId, UserId};

use crate::error::StorageResult;
use crate::helpers::{enum_from_text, parse_time, parse_uuid};
use crate::store::SqliteStore;

impl SqliteStore {
    async fn list_audit_inner(
        &self,
        store_id: Option<StoreId>,
        limit: u32,
    ) -> StorageResult<Vec<AuditEvent>> {
        let rows = if let Some(store_id) = store_id {
            sqlx::query(
                "SELECT * FROM audit_log WHERE store_id = ? ORDER BY occurred_at DESC LIMIT ?",
            )
            .bind(store_id.to_string())
            .bind(i64::from(limit))
            .fetch_all(self.pool())
            .await?
        } else {
            sqlx::query("SELECT * FROM audit_log ORDER BY occurred_at DESC LIMIT ?")
                .bind(i64::from(limit))
                .fetch_all(self.pool())
                .await?
        };
        let mut events = Vec::with_capacity(rows.len());
        for row in rows {
            let actor_id = row
                .try_get::<Option<String>, _>("actor_id")?
                .map(|value| parse_uuid(&value).map(UserId::from_uuid))
                .transpose()?;
            let row_store_id = row
                .try_get::<Option<String>, _>("store_id")?
                .map(|value| parse_uuid(&value).map(StoreId::from_uuid))
                .transpose()?;
            events.push(AuditEvent {
                id: AuditEventId::from_uuid(parse_uuid(&row.try_get::<String, _>("id")?)?),
                occurred_at: parse_time(&row.try_get::<String, _>("occurred_at")?)?,
                actor_id,
                store_id: row_store_id,
                action: enum_from_text::<AuditAction>(&row.try_get::<String, _>("action")?)?,
                entity_type: row.try_get("entity_type")?,
                entity_id: row.try_get("entity_id")?,
                details: serde_json::from_str(&row.try_get::<String, _>("details_json")?)?,
            });
        }
        Ok(events)
    }
}

#[async_trait]
impl AuditRepository for SqliteStore {
    async fn list_recent(
        &self,
        store_id: Option<StoreId>,
        limit: u32,
    ) -> AppResult<Vec<AuditEvent>> {
        Ok(self.list_audit_inner(store_id, limit).await?)
    }
}
