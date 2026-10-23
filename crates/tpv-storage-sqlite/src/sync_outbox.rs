use async_trait::async_trait;
use sqlx::Row;
use tpv_application::{Result as AppResult, SyncOutboxRepository};
use tpv_domain::sync::SyncEvent;

use crate::error::StorageResult;
use crate::helpers::format_time;
use crate::store::SqliteStore;

impl SqliteStore {
    async fn pending_events_inner(&self, limit: u32) -> StorageResult<Vec<SyncEvent>> {
        let rows = sqlx::query(
            "SELECT envelope_json FROM sync_outbox
             WHERE sent_at IS NULL ORDER BY id LIMIT ?",
        )
        .bind(i64::from(limit))
        .fetch_all(self.pool())
        .await?;
        let mut events = Vec::with_capacity(rows.len());
        for row in rows {
            let envelope = row.try_get::<String, _>("envelope_json")?;
            events.push(serde_json::from_str(&envelope)?);
        }
        Ok(events)
    }

    async fn mark_events_sent_inner(&self, event_ids: &[uuid::Uuid]) -> StorageResult<()> {
        let mut tx = self.pool().begin().await?;
        let sent_at = format_time(time::OffsetDateTime::now_utc())?;
        for event_id in event_ids {
            sqlx::query("UPDATE sync_outbox SET sent_at = ? WHERE event_id = ?")
                .bind(&sent_at)
                .bind(event_id.to_string())
                .execute(&mut *tx)
                .await?;
        }
        tx.commit().await?;
        Ok(())
    }
}

#[async_trait]
impl SyncOutboxRepository for SqliteStore {
    async fn pending_events(&self, limit: u32) -> AppResult<Vec<SyncEvent>> {
        Ok(self.pending_events_inner(limit).await?)
    }

    async fn mark_events_sent(&self, event_ids: &[uuid::Uuid]) -> AppResult<()> {
        Ok(self.mark_events_sent_inner(event_ids).await?)
    }
}
