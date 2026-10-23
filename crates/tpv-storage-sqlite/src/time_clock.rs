use async_trait::async_trait;
use sqlx::Row;
use sqlx::sqlite::SqliteRow;
use tpv_application::{Result as AppResult, TimeClockRepository};
use tpv_domain::audit::AuditEvent;
use tpv_domain::ids::{StoreId, TimeEntryId, UserId};
use tpv_domain::timeclock::TimeEntry;

use crate::error::StorageResult;
use crate::helpers::{format_time, parse_time, parse_uuid};
use crate::persistence::insert_audit_tx;
use crate::store::SqliteStore;

fn row_to_time_entry(row: &SqliteRow) -> StorageResult<TimeEntry> {
    Ok(TimeEntry {
        id: TimeEntryId::from_uuid(parse_uuid(&row.try_get::<String, _>("id")?)?),
        user_id: UserId::from_uuid(parse_uuid(&row.try_get::<String, _>("user_id")?)?),
        store_id: StoreId::from_uuid(parse_uuid(&row.try_get::<String, _>("store_id")?)?),
        clock_in: parse_time(&row.try_get::<String, _>("clock_in")?)?,
        clock_out: row
            .try_get::<Option<String>, _>("clock_out")?
            .map(|value| parse_time(&value))
            .transpose()?,
        correction_reason: row.try_get("correction_reason")?,
    })
}

impl SqliteStore {
    async fn save_time_entry_inner(
        &self,
        entry: &TimeEntry,
        audit: &AuditEvent,
    ) -> StorageResult<()> {
        let mut tx = self.pool().begin().await?;
        sqlx::query(
            "INSERT INTO time_entries
                (id, user_id, store_id, clock_in, clock_out, correction_reason)
             VALUES (?, ?, ?, ?, ?, ?)
             ON CONFLICT(id) DO UPDATE SET
                clock_in = excluded.clock_in,
                clock_out = excluded.clock_out,
                correction_reason = excluded.correction_reason",
        )
        .bind(entry.id.to_string())
        .bind(entry.user_id.to_string())
        .bind(entry.store_id.to_string())
        .bind(format_time(entry.clock_in)?)
        .bind(entry.clock_out.map(format_time).transpose()?)
        .bind(entry.correction_reason.as_deref())
        .execute(&mut *tx)
        .await?;
        insert_audit_tx(&mut tx, audit).await?;
        tx.commit().await?;
        Ok(())
    }

    async fn find_open_time_entry(
        &self,
        user_id: UserId,
        store_id: StoreId,
    ) -> StorageResult<Option<TimeEntry>> {
        let row = sqlx::query(
            "SELECT * FROM time_entries
             WHERE user_id = ? AND store_id = ? AND clock_out IS NULL
             ORDER BY clock_in DESC LIMIT 1",
        )
        .bind(user_id.to_string())
        .bind(store_id.to_string())
        .fetch_optional(self.pool())
        .await?;
        row.as_ref().map(row_to_time_entry).transpose()
    }

    async fn list_time_entries_inner(
        &self,
        store_id: StoreId,
        user_id: Option<UserId>,
        limit: u32,
    ) -> StorageResult<Vec<TimeEntry>> {
        let rows = if let Some(user_id) = user_id {
            sqlx::query(
                "SELECT * FROM time_entries
                 WHERE store_id = ? AND user_id = ?
                 ORDER BY clock_in DESC LIMIT ?",
            )
            .bind(store_id.to_string())
            .bind(user_id.to_string())
            .bind(i64::from(limit))
            .fetch_all(self.pool())
            .await?
        } else {
            sqlx::query(
                "SELECT * FROM time_entries
                 WHERE store_id = ?
                 ORDER BY clock_in DESC LIMIT ?",
            )
            .bind(store_id.to_string())
            .bind(i64::from(limit))
            .fetch_all(self.pool())
            .await?
        };
        rows.iter().map(row_to_time_entry).collect()
    }
}

#[async_trait]
impl TimeClockRepository for SqliteStore {
    async fn save(&self, entry: &TimeEntry, audit: &AuditEvent) -> AppResult<()> {
        Ok(self.save_time_entry_inner(entry, audit).await?)
    }

    async fn find_open(&self, user_id: UserId, store_id: StoreId) -> AppResult<Option<TimeEntry>> {
        Ok(self.find_open_time_entry(user_id, store_id).await?)
    }

    async fn list_recent(
        &self,
        store_id: StoreId,
        user_id: Option<UserId>,
        limit: u32,
    ) -> AppResult<Vec<TimeEntry>> {
        Ok(self
            .list_time_entries_inner(store_id, user_id, limit)
            .await?)
    }
}
