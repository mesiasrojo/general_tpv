use async_trait::async_trait;
use sqlx::Row;
use sqlx::sqlite::SqliteRow;
use tpv_application::{CashRepository, Result as AppResult};
use tpv_domain::audit::AuditEvent;
use tpv_domain::cash::{
    CashCloseRecord, CashMovement, CashReopenRecord, CashSession, CashSessionState,
};
use tpv_domain::ids::{CashSessionId, RegisterId, StoreId, UserId};
use tpv_domain::money::Money;
use tpv_domain::sync::SyncEvent;

use crate::error::{StorageError, StorageResult};
use crate::helpers::{enum_from_text, enum_to_text, format_time, parse_time, parse_uuid};
use crate::persistence::{enqueue_event_tx, insert_audit_tx};
use crate::store::SqliteStore;

fn row_to_cash_session(row: &SqliteRow) -> StorageResult<CashSession> {
    let movements: Vec<CashMovement> =
        serde_json::from_str(&row.try_get::<String, _>("movements_json")?)?;
    let close_history: Vec<CashCloseRecord> =
        serde_json::from_str(&row.try_get::<String, _>("close_history_json")?)?;
    let reopen_history: Vec<CashReopenRecord> =
        serde_json::from_str(&row.try_get::<String, _>("reopen_history_json")?)?;
    Ok(CashSession {
        id: CashSessionId::from_uuid(parse_uuid(&row.try_get::<String, _>("id")?)?),
        store_id: StoreId::from_uuid(parse_uuid(&row.try_get::<String, _>("store_id")?)?),
        register_id: RegisterId::from_uuid(parse_uuid(&row.try_get::<String, _>("register_id")?)?),
        opened_by: UserId::from_uuid(parse_uuid(&row.try_get::<String, _>("opened_by")?)?),
        opened_at: parse_time(&row.try_get::<String, _>("opened_at")?)?,
        opening_float: Money::from_cents(row.try_get("opening_float_cents")?),
        movements,
        close_history,
        reopen_history,
        state: enum_from_text::<CashSessionState>(&row.try_get::<String, _>("state")?)?,
        version: u32::try_from(row.try_get::<i64, _>("version")?)
            .map_err(|_| StorageError::InvalidData("invalid cash session version".into()))?,
    })
}

impl SqliteStore {
    pub(crate) async fn load_cash_session(
        &self,
        id: CashSessionId,
    ) -> StorageResult<Option<CashSession>> {
        let row = sqlx::query("SELECT * FROM cash_sessions WHERE id = ?")
            .bind(id.to_string())
            .fetch_optional(self.pool())
            .await?;
        row.as_ref().map(row_to_cash_session).transpose()
    }
}

impl SqliteStore {
    async fn save_cash_session_inner(
        &self,
        session: &CashSession,
        event: Option<&SyncEvent>,
        audit: &AuditEvent,
    ) -> StorageResult<()> {
        let mut tx = self.pool().begin().await?;
        let movements = serde_json::to_string(&session.movements)?;
        let close_history = serde_json::to_string(&session.close_history)?;
        let reopen_history = serde_json::to_string(&session.reopen_history)?;
        sqlx::query(
            "INSERT INTO cash_sessions
                (id, store_id, register_id, opened_by, opened_at, opening_float_cents,
                 state, version, movements_json, close_history_json, reopen_history_json)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
             ON CONFLICT(id) DO UPDATE SET
                state = excluded.state,
                version = excluded.version,
                movements_json = excluded.movements_json,
                close_history_json = excluded.close_history_json,
                reopen_history_json = excluded.reopen_history_json",
        )
        .bind(session.id.to_string())
        .bind(session.store_id.to_string())
        .bind(session.register_id.to_string())
        .bind(session.opened_by.to_string())
        .bind(format_time(session.opened_at)?)
        .bind(session.opening_float.cents())
        .bind(enum_to_text(&session.state)?)
        .bind(i64::from(session.version))
        .bind(movements)
        .bind(close_history)
        .bind(reopen_history)
        .execute(&mut *tx)
        .await?;
        if let Some(event) = event {
            enqueue_event_tx(&mut tx, event).await?;
        }
        insert_audit_tx(&mut tx, audit).await?;
        tx.commit().await?;
        Ok(())
    }

    async fn find_open_cash_inner(
        &self,
        register_id: RegisterId,
    ) -> StorageResult<Option<CashSession>> {
        let row = sqlx::query(
            "SELECT id FROM cash_sessions
             WHERE register_id = ? AND state = 'open'
             ORDER BY opened_at DESC LIMIT 1",
        )
        .bind(register_id.to_string())
        .fetch_optional(self.pool())
        .await?;
        let Some(row) = row else {
            return Ok(None);
        };
        let id = CashSessionId::from_uuid(parse_uuid(&row.try_get::<String, _>("id")?)?);
        self.load_cash_session(id).await
    }

    async fn list_cash_session_ids(
        &self,
        store_id: StoreId,
        limit: u32,
    ) -> StorageResult<Vec<CashSessionId>> {
        let rows = sqlx::query(
            "SELECT id FROM cash_sessions WHERE store_id = ? ORDER BY opened_at DESC LIMIT ?",
        )
        .bind(store_id.to_string())
        .bind(i64::from(limit))
        .fetch_all(self.pool())
        .await?;
        rows.iter()
            .map(|row| {
                Ok(CashSessionId::from_uuid(parse_uuid(
                    &row.try_get::<String, _>("id")?,
                )?))
            })
            .collect()
    }
}

#[async_trait]
impl CashRepository for SqliteStore {
    async fn save(
        &self,
        session: &CashSession,
        event: Option<&SyncEvent>,
        audit: &AuditEvent,
    ) -> AppResult<()> {
        Ok(self.save_cash_session_inner(session, event, audit).await?)
    }

    async fn find(&self, id: CashSessionId) -> AppResult<Option<CashSession>> {
        Ok(self.load_cash_session(id).await?)
    }

    async fn find_open(&self, register_id: RegisterId) -> AppResult<Option<CashSession>> {
        Ok(self.find_open_cash_inner(register_id).await?)
    }

    async fn list_recent(&self, store_id: StoreId, limit: u32) -> AppResult<Vec<CashSession>> {
        let ids = self.list_cash_session_ids(store_id, limit).await?;
        let mut sessions = Vec::with_capacity(ids.len());
        for id in ids {
            if let Some(session) = self.load_cash_session(id).await? {
                sessions.push(session);
            }
        }
        Ok(sessions)
    }
}
