use std::sync::Arc;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use sqlx::postgres::PgPoolOptions;
use sqlx::{PgPool, Row};
use thiserror::Error;
use tokio::sync::RwLock;
use tpv_domain::sync::SyncEvent;
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StoredEvent {
    pub seq: u64,
    pub event: SyncEvent,
    pub received_at: time::OffsetDateTime,
}

#[derive(Debug, Clone, Default)]
pub struct PushStoreResult {
    pub accepted: Vec<Uuid>,
    pub duplicates: Vec<Uuid>,
    pub cursor: u64,
}

#[derive(Debug, Clone, Copy, Serialize)]
pub struct StoreHealth {
    pub events: i64,
    pub cursor: i64,
}

#[derive(Debug, Error)]
pub enum StoreError {
    #[error("central database error: {0}")]
    Database(String),
}

impl From<sqlx::Error> for StoreError {
    fn from(value: sqlx::Error) -> Self {
        Self::Database(value.to_string())
    }
}

#[async_trait]
pub trait CentralStore: Send + Sync {
    async fn push(&self, events: Vec<SyncEvent>) -> Result<PushStoreResult, StoreError>;
    async fn pull(
        &self,
        cursor: u64,
        store_id: Option<Uuid>,
        limit: u32,
    ) -> Result<Vec<StoredEvent>, StoreError>;
    async fn list_events(&self, limit: u32) -> Result<Vec<StoredEvent>, StoreError>;
    async fn list_sales(&self, limit: u32) -> Result<Vec<StoredEvent>, StoreError>;
    async fn health(&self) -> Result<StoreHealth, StoreError>;
}

#[derive(Default)]
struct MemoryLedger {
    events: Vec<StoredEvent>,
    next_seq: u64,
}

#[derive(Default)]
pub struct MemoryStore {
    inner: Arc<RwLock<MemoryLedger>>,
}

impl MemoryStore {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }
}

#[async_trait]
impl CentralStore for MemoryStore {
    async fn push(&self, events: Vec<SyncEvent>) -> Result<PushStoreResult, StoreError> {
        let mut ledger = self.inner.write().await;
        let mut accepted = Vec::new();
        let mut duplicates = Vec::new();
        for event in events {
            if ledger
                .events
                .iter()
                .any(|stored| stored.event.event_id == event.event_id)
            {
                duplicates.push(event.event_id);
                continue;
            }
            ledger.next_seq += 1;
            let seq = ledger.next_seq;
            accepted.push(event.event_id);
            ledger.events.push(StoredEvent {
                seq,
                event,
                received_at: time::OffsetDateTime::now_utc(),
            });
        }
        Ok(PushStoreResult {
            accepted,
            duplicates,
            cursor: ledger.next_seq,
        })
    }

    async fn pull(
        &self,
        cursor: u64,
        store_id: Option<Uuid>,
        limit: u32,
    ) -> Result<Vec<StoredEvent>, StoreError> {
        let ledger = self.inner.read().await;
        Ok(ledger
            .events
            .iter()
            .filter(|stored| stored.seq > cursor)
            .filter(|stored| {
                store_id.is_none_or(|store_id| stored.event.store_id.as_uuid() == store_id)
            })
            .take(limit as usize)
            .cloned()
            .collect())
    }

    async fn list_events(&self, limit: u32) -> Result<Vec<StoredEvent>, StoreError> {
        let ledger = self.inner.read().await;
        Ok(ledger.events.iter().take(limit as usize).cloned().collect())
    }

    async fn list_sales(&self, limit: u32) -> Result<Vec<StoredEvent>, StoreError> {
        let ledger = self.inner.read().await;
        Ok(ledger
            .events
            .iter()
            .filter(|stored| stored.event.aggregate_type == "sale")
            .take(limit as usize)
            .cloned()
            .collect())
    }

    async fn health(&self) -> Result<StoreHealth, StoreError> {
        let ledger = self.inner.read().await;
        Ok(StoreHealth {
            events: ledger.events.len() as i64,
            cursor: ledger.next_seq as i64,
        })
    }
}

#[derive(Clone)]
pub struct PostgresStore {
    pool: PgPool,
}

impl PostgresStore {
    pub async fn connect(database_url: &str) -> Result<Self, StoreError> {
        let pool = PgPoolOptions::new()
            .max_connections(5)
            .connect(database_url)
            .await?;
        sqlx::migrate!("./migrations")
            .run(&pool)
            .await
            .map_err(|error| StoreError::Database(error.to_string()))?;
        Ok(Self { pool })
    }

    async fn max_cursor(&self) -> Result<u64, StoreError> {
        let cursor: i64 = sqlx::query_scalar("SELECT COALESCE(MAX(seq), 0) FROM central_events")
            .fetch_one(&self.pool)
            .await?;
        Ok(cursor as u64)
    }

    async fn query_events(
        &self,
        store_id: Option<Uuid>,
        limit: u32,
        only_sales: bool,
    ) -> Result<Vec<StoredEvent>, StoreError> {
        let rows = sqlx::query(
            "SELECT seq, envelope, received_at
             FROM central_events
             WHERE ($1::uuid IS NULL OR store_id = $1)
               AND (NOT $2 OR aggregate_type = 'sale')
             ORDER BY seq ASC
             LIMIT $3",
        )
        .bind(store_id)
        .bind(only_sales)
        .bind(i64::from(limit))
        .fetch_all(&self.pool)
        .await?;
        rows.iter().map(row_to_event).collect()
    }
}

fn row_to_event(row: &sqlx::postgres::PgRow) -> Result<StoredEvent, StoreError> {
    let envelope: serde_json::Value = row.try_get("envelope")?;
    let event: SyncEvent = serde_json::from_value(envelope)
        .map_err(|error| StoreError::Database(error.to_string()))?;
    Ok(StoredEvent {
        seq: u64::try_from(row.try_get::<i64, _>("seq")?)
            .map_err(|_| StoreError::Database("negative event sequence".into()))?,
        event,
        received_at: row.try_get("received_at")?,
    })
}

#[async_trait]
impl CentralStore for PostgresStore {
    async fn push(&self, events: Vec<SyncEvent>) -> Result<PushStoreResult, StoreError> {
        let mut tx = self.pool.begin().await?;
        let mut accepted = Vec::new();
        let mut duplicates = Vec::new();
        for event in events {
            let envelope = serde_json::to_value(&event)
                .map_err(|error| StoreError::Database(error.to_string()))?;
            let seq: Option<i64> = sqlx::query_scalar(
                "INSERT INTO central_events
                    (event_id, tenant_id, store_id, device_id, device_seq,
                     aggregate_type, aggregate_id, event_type, schema_version,
                     occurred_at, payload, envelope, hash)
                 VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13)
                 ON CONFLICT (event_id) DO NOTHING
                 RETURNING seq",
            )
            .bind(event.event_id)
            .bind(event.tenant_id.as_uuid())
            .bind(event.store_id.as_uuid())
            .bind(event.device_id.as_uuid())
            .bind(event.device_seq as i64)
            .bind(&event.aggregate_type)
            .bind(event.aggregate_id)
            .bind(&event.event_type)
            .bind(event.schema_version as i32)
            .bind(event.occurred_at)
            .bind(&event.payload)
            .bind(envelope)
            .bind(&event.hash)
            .fetch_optional(&mut *tx)
            .await?;
            match seq {
                Some(_) => accepted.push(event.event_id),
                None => duplicates.push(event.event_id),
            }
        }
        tx.commit().await?;
        Ok(PushStoreResult {
            accepted,
            duplicates,
            cursor: self.max_cursor().await?,
        })
    }

    async fn pull(
        &self,
        cursor: u64,
        store_id: Option<Uuid>,
        limit: u32,
    ) -> Result<Vec<StoredEvent>, StoreError> {
        let rows = sqlx::query(
            "SELECT seq, envelope, received_at
             FROM central_events
             WHERE seq > $1
               AND ($2::uuid IS NULL OR store_id = $2)
             ORDER BY seq ASC
             LIMIT $3",
        )
        .bind(cursor as i64)
        .bind(store_id)
        .bind(i64::from(limit))
        .fetch_all(&self.pool)
        .await?;
        rows.iter().map(row_to_event).collect()
    }

    async fn list_events(&self, limit: u32) -> Result<Vec<StoredEvent>, StoreError> {
        self.query_events(None, limit, false).await
    }

    async fn list_sales(&self, limit: u32) -> Result<Vec<StoredEvent>, StoreError> {
        self.query_events(None, limit, true).await
    }

    async fn health(&self) -> Result<StoreHealth, StoreError> {
        let row = sqlx::query(
            "SELECT COUNT(*) AS events, COALESCE(MAX(seq), 0) AS cursor FROM central_events",
        )
        .fetch_one(&self.pool)
        .await?;
        Ok(StoreHealth {
            events: row.try_get("events")?,
            cursor: row.try_get("cursor")?,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn event() -> SyncEvent {
        SyncEvent::new(
            tpv_domain::ids::TenantId::new(),
            tpv_domain::ids::StoreId::new(),
            tpv_domain::ids::DeviceId::new(),
            1,
            "sale",
            Uuid::now_v7(),
            "sale.confirmed",
            1,
            time::OffsetDateTime::now_utc(),
            json!({"total_cents": 140}),
        )
        .unwrap()
    }

    #[tokio::test]
    #[ignore = "requires DATABASE_URL pointing to a PostgreSQL database"]
    async fn postgres_store_roundtrip() {
        let url = std::env::var("DATABASE_URL").expect("DATABASE_URL must be set");
        let store = PostgresStore::connect(&url).await.unwrap();
        let event = event();
        let event_id = event.event_id;
        let result = store.push(vec![event.clone()]).await.unwrap();
        assert!(result.accepted.contains(&event_id));
        let duplicate = store.push(vec![event.clone()]).await.unwrap();
        assert!(duplicate.duplicates.contains(&event_id));
        let pulled = store
            .pull(0, Some(event.store_id.as_uuid()), 100)
            .await
            .unwrap();
        assert!(
            pulled
                .iter()
                .any(|stored| stored.event.event_id == event_id)
        );
        assert!(store.health().await.unwrap().events >= 1);
    }
}
