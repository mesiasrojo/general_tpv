use async_trait::async_trait;
use sqlx::Row;
use tpv_application::{QuarantinedEvent, Result as AppResult, SyncOutboxRepository, SyncStatus};
use tpv_domain::sync::SyncEvent;

use crate::error::{StorageError, StorageResult};
use crate::helpers::{format_time, parse_optional_time};
use crate::store::SqliteStore;

impl SqliteStore {
    async fn pending_events_inner(&self, limit: u32) -> StorageResult<Vec<SyncEvent>> {
        let rows = sqlx::query(
            "SELECT envelope_json FROM sync_outbox
             WHERE sent_at IS NULL AND failed_at IS NULL
             ORDER BY id LIMIT ?",
        )
        .bind(i64::from(limit))
        .fetch_all(self.pool())
        .await?;
        rows.iter()
            .map(|row| {
                let envelope = row.try_get::<String, _>("envelope_json")?;
                Ok(serde_json::from_str(&envelope)?)
            })
            .collect()
    }

    async fn mark_events_sent_inner(&self, event_ids: &[uuid::Uuid]) -> StorageResult<()> {
        let mut tx = self.pool().begin().await?;
        let sent_at = format_time(time::OffsetDateTime::now_utc())?;
        for event_id in event_ids {
            sqlx::query(
                "UPDATE sync_outbox
                 SET sent_at = ?, attempts = attempts + 1, last_error = NULL
                 WHERE event_id = ? AND sent_at IS NULL",
            )
            .bind(&sent_at)
            .bind(event_id.to_string())
            .execute(&mut *tx)
            .await?;
        }
        tx.commit().await?;
        Ok(())
    }

    async fn mark_events_failed_inner(
        &self,
        event_ids: &[uuid::Uuid],
        error: &str,
    ) -> StorageResult<()> {
        let mut tx = self.pool().begin().await?;
        let failed_at = format_time(time::OffsetDateTime::now_utc())?;
        for event_id in event_ids {
            sqlx::query(
                "UPDATE sync_outbox
                 SET failed_at = ?, last_error = ?, attempts = attempts + 1
                 WHERE event_id = ? AND sent_at IS NULL AND failed_at IS NULL",
            )
            .bind(&failed_at)
            .bind(error)
            .bind(event_id.to_string())
            .execute(&mut *tx)
            .await?;
        }
        tx.commit().await?;
        Ok(())
    }

    async fn sync_status_inner(&self) -> StorageResult<SyncStatus> {
        let row = sqlx::query(
            "SELECT
                SUM(CASE WHEN sent_at IS NULL AND failed_at IS NULL THEN 1 ELSE 0 END) AS pending,
                SUM(CASE WHEN sent_at IS NOT NULL THEN 1 ELSE 0 END) AS sent,
                SUM(CASE WHEN failed_at IS NOT NULL THEN 1 ELSE 0 END) AS failed,
                MIN(CASE WHEN sent_at IS NULL AND failed_at IS NULL THEN created_at END) AS oldest_pending_at,
                MAX(sent_at) AS last_sent_at,
                MAX(failed_at) AS last_failed_at
             FROM sync_outbox",
        )
        .fetch_one(self.pool())
        .await?;
        let last_error: Option<String> = sqlx::query_scalar(
            "SELECT last_error FROM sync_outbox
             WHERE failed_at IS NOT NULL AND last_error IS NOT NULL
             ORDER BY failed_at DESC LIMIT 1",
        )
        .fetch_optional(self.pool())
        .await?;
        let pending = row.try_get::<Option<i64>, _>("pending")?.unwrap_or(0);
        let sent = row.try_get::<Option<i64>, _>("sent")?.unwrap_or(0);
        let failed = row.try_get::<Option<i64>, _>("failed")?.unwrap_or(0);
        Ok(SyncStatus {
            pending: u32::try_from(pending)
                .map_err(|_| StorageError::InvalidData("invalid pending count".into()))?,
            sent: u32::try_from(sent)
                .map_err(|_| StorageError::InvalidData("invalid sent count".into()))?,
            failed: u32::try_from(failed)
                .map_err(|_| StorageError::InvalidData("invalid failed count".into()))?,
            oldest_pending_at: parse_optional_time(row.try_get("oldest_pending_at")?)?,
            last_sent_at: parse_optional_time(row.try_get("last_sent_at")?)?,
            last_failed_at: parse_optional_time(row.try_get("last_failed_at")?)?,
            last_error,
        })
    }

    async fn list_failed_inner(&self, limit: u32) -> StorageResult<Vec<QuarantinedEvent>> {
        let rows = sqlx::query(
            "SELECT envelope_json, attempts, failed_at, last_error FROM sync_outbox
             WHERE failed_at IS NOT NULL
             ORDER BY failed_at DESC LIMIT ?",
        )
        .bind(i64::from(limit))
        .fetch_all(self.pool())
        .await?;
        rows.iter()
            .map(|row| {
                let envelope = row.try_get::<String, _>("envelope_json")?;
                let attempts = row.try_get::<i64, _>("attempts")?;
                let failed_at = row.try_get::<String, _>("failed_at")?;
                Ok(QuarantinedEvent {
                    event: serde_json::from_str(&envelope)?,
                    attempts: u32::try_from(attempts).map_err(|_| {
                        StorageError::InvalidData("invalid sync attempts count".into())
                    })?,
                    failed_at: crate::helpers::parse_time(&failed_at)?,
                    last_error: row.try_get("last_error")?,
                })
            })
            .collect()
    }

    async fn requeue_failed_inner(&self, event_ids: &[uuid::Uuid]) -> StorageResult<u64> {
        let mut tx = self.pool().begin().await?;
        let mut requeued = 0_u64;
        for event_id in event_ids {
            let result = sqlx::query(
                "UPDATE sync_outbox
                 SET failed_at = NULL, last_error = NULL
                 WHERE event_id = ? AND sent_at IS NULL AND failed_at IS NOT NULL",
            )
            .bind(event_id.to_string())
            .execute(&mut *tx)
            .await?;
            requeued = requeued.saturating_add(result.rows_affected());
        }
        tx.commit().await?;
        Ok(requeued)
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

    async fn mark_events_failed(&self, event_ids: &[uuid::Uuid], error: &str) -> AppResult<()> {
        Ok(self.mark_events_failed_inner(event_ids, error).await?)
    }

    async fn sync_status(&self) -> AppResult<SyncStatus> {
        Ok(self.sync_status_inner().await?)
    }

    async fn list_failed(&self, limit: u32) -> AppResult<Vec<QuarantinedEvent>> {
        Ok(self.list_failed_inner(limit).await?)
    }

    async fn requeue_failed(&self, event_ids: &[uuid::Uuid]) -> AppResult<u64> {
        Ok(self.requeue_failed_inner(event_ids).await?)
    }
}
