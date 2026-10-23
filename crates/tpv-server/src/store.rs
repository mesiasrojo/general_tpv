use std::sync::Arc;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use sqlx::postgres::PgPoolOptions;
use sqlx::{PgPool, Row};
use thiserror::Error;
use tokio::sync::RwLock;
use tpv_domain::ids::{StoreId, TenantId};
use tpv_domain::master_data::{
    ConflictOutcome, ConflictReason, MasterDataApplyResult, MasterDataChange, MasterDataConflict,
    MasterDataKind, resolve_change,
};
use tpv_domain::people::Role;
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DeviceStatus {
    Active,
    Revoked,
}

impl DeviceStatus {
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Active => "active",
            Self::Revoked => "revoked",
        }
    }

    fn parse(value: &str) -> Result<Self, StoreError> {
        match value {
            "active" => Ok(Self::Active),
            "revoked" => Ok(Self::Revoked),
            other => Err(StoreError::Database(format!(
                "invalid central device status: {other}"
            ))),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeviceRecord {
    pub device_id: Uuid,
    pub tenant_id: Uuid,
    pub store_id: Uuid,
    pub name: String,
    pub status: DeviceStatus,
    pub enrolled_at: time::OffsetDateTime,
    pub last_seen_at: Option<time::OffsetDateTime>,
    pub revoked_at: Option<time::OffsetDateTime>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CentralUserRecord {
    pub id: Uuid,
    pub tenant_id: Uuid,
    pub username: String,
    pub display_name: String,
    pub role: Role,
    pub store_ids: Vec<Uuid>,
    pub active: bool,
    pub version: u64,
    pub created_at: time::OffsetDateTime,
    pub updated_at: time::OffsetDateTime,
    #[serde(default, skip_serializing)]
    pub password_hash: String,
}

#[derive(Debug, Clone)]
pub struct NewCentralUser {
    pub tenant_id: Uuid,
    pub username: String,
    pub display_name: String,
    pub role: Role,
    pub store_ids: Vec<Uuid>,
    pub password_hash: String,
}

#[derive(Debug, Clone)]
pub struct UpdateCentralUser {
    pub display_name: String,
    pub role: Role,
    pub store_ids: Vec<Uuid>,
    pub active: bool,
    pub password_hash: Option<String>,
}

#[derive(Debug, Clone)]
pub struct EventQuery {
    pub limit: u32,
    pub offset: u32,
    pub store_id: Option<Uuid>,
    pub device_id: Option<Uuid>,
    pub event_type: Option<String>,
    pub from: Option<time::OffsetDateTime>,
    pub to: Option<time::OffsetDateTime>,
    pub sales_only: bool,
}

impl Default for EventQuery {
    fn default() -> Self {
        Self {
            limit: 50,
            offset: 0,
            store_id: None,
            device_id: None,
            event_type: None,
            from: None,
            to: None,
            sales_only: false,
        }
    }
}

#[derive(Debug, Clone)]
pub struct EventPage {
    pub items: Vec<StoredEvent>,
    pub total: u64,
}

#[derive(Debug, Clone)]
pub struct NewCentralSession {
    pub token_hash: String,
    pub user_id: Uuid,
    pub tenant_id: Uuid,
    pub username: String,
    pub display_name: String,
    pub role: Role,
    pub expires_at: time::OffsetDateTime,
}

#[derive(Debug, Clone)]
pub struct CentralSessionRecord {
    pub token_hash: String,
    pub user_id: Uuid,
    pub tenant_id: Uuid,
    pub username: String,
    pub display_name: String,
    pub role: Role,
    pub expires_at: time::OffsetDateTime,
}

#[derive(Debug, Clone)]
pub struct EnrollDevice {
    pub device_id: Uuid,
    pub tenant_id: Uuid,
    pub store_id: Uuid,
    pub name: String,
    pub token_hash: String,
}

#[derive(Debug, Error)]
pub enum StoreError {
    #[error("central database error: {0}")]
    Database(String),

    #[error("conflict: {0}")]
    Conflict(String),

    #[error(transparent)]
    Domain(#[from] tpv_domain::DomainError),
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
    async fn list_events(&self, query: &EventQuery) -> Result<EventPage, StoreError>;
    async fn list_sales(&self, query: &EventQuery) -> Result<EventPage, StoreError>;
    async fn health(&self) -> Result<StoreHealth, StoreError>;
    async fn enroll_device(&self, device: EnrollDevice) -> Result<DeviceRecord, StoreError>;
    async fn list_devices(&self) -> Result<Vec<DeviceRecord>, StoreError>;
    async fn revoke_device(&self, device_id: Uuid) -> Result<Option<DeviceRecord>, StoreError>;
    async fn device_by_token_hash(
        &self,
        token_hash: &str,
    ) -> Result<Option<DeviceRecord>, StoreError>;
    async fn mark_device_seen(&self, device_id: Uuid) -> Result<(), StoreError>;
    async fn apply_master_data_change(
        &self,
        tenant_id: Uuid,
        store_id: Uuid,
        change: &MasterDataChange,
    ) -> Result<MasterDataApplyResult, StoreError>;
    async fn list_master_data_conflicts(
        &self,
        limit: u32,
    ) -> Result<Vec<MasterDataConflict>, StoreError>;
    async fn list_central_users(&self) -> Result<Vec<CentralUserRecord>, StoreError>;
    async fn create_central_user(
        &self,
        user: NewCentralUser,
    ) -> Result<CentralUserRecord, StoreError>;
    async fn update_central_user(
        &self,
        id: Uuid,
        update: UpdateCentralUser,
    ) -> Result<Option<CentralUserRecord>, StoreError>;
    async fn set_central_user_password(
        &self,
        id: Uuid,
        password_hash: &str,
    ) -> Result<Option<CentralUserRecord>, StoreError>;
    async fn find_central_users_by_username(
        &self,
        username: &str,
    ) -> Result<Vec<CentralUserRecord>, StoreError>;
    async fn create_central_session(&self, session: NewCentralSession) -> Result<(), StoreError>;
    async fn central_session_by_token_hash(
        &self,
        token_hash: &str,
    ) -> Result<Option<CentralSessionRecord>, StoreError>;
    async fn delete_central_session(&self, token_hash: &str) -> Result<(), StoreError>;
    async fn delete_central_user_sessions(&self, user_id: Uuid) -> Result<(), StoreError>;
}

#[derive(Default)]
struct MemoryLedger {
    events: Vec<StoredEvent>,
    devices: Vec<MemoryDevice>,
    master_data: Vec<MemoryMasterData>,
    master_conflicts: Vec<MasterDataConflict>,
    users: Vec<CentralUserRecord>,
    sessions: Vec<CentralSessionRecord>,
    next_seq: u64,
}

#[derive(Clone)]
struct MemoryDevice {
    record: DeviceRecord,
    token_hash: String,
}

#[derive(Clone)]
struct MemoryMasterData {
    tenant_id: Uuid,
    store_id: Uuid,
    change: MasterDataChange,
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

    async fn list_events(&self, query: &EventQuery) -> Result<EventPage, StoreError> {
        let ledger = self.inner.read().await;
        memory_event_page(&ledger.events, query)
    }

    async fn list_sales(&self, query: &EventQuery) -> Result<EventPage, StoreError> {
        let ledger = self.inner.read().await;
        let mut query = query.clone();
        query.sales_only = true;
        memory_event_page(&ledger.events, &query)
    }

    async fn health(&self) -> Result<StoreHealth, StoreError> {
        let ledger = self.inner.read().await;
        Ok(StoreHealth {
            events: ledger.events.len() as i64,
            cursor: ledger.next_seq as i64,
        })
    }

    async fn enroll_device(&self, device: EnrollDevice) -> Result<DeviceRecord, StoreError> {
        let mut ledger = self.inner.write().await;
        let now = time::OffsetDateTime::now_utc();
        let existing = ledger
            .devices
            .iter()
            .find(|entry| entry.record.device_id == device.device_id)
            .cloned();
        let record = DeviceRecord {
            device_id: device.device_id,
            tenant_id: device.tenant_id,
            store_id: device.store_id,
            name: device.name,
            status: DeviceStatus::Active,
            enrolled_at: now,
            last_seen_at: existing.and_then(|entry| entry.record.last_seen_at),
            revoked_at: None,
        };
        let entry = MemoryDevice {
            record: record.clone(),
            token_hash: device.token_hash,
        };
        if let Some(index) = ledger
            .devices
            .iter()
            .position(|existing| existing.record.device_id == record.device_id)
        {
            ledger.devices[index] = entry;
        } else {
            ledger.devices.push(entry);
        }
        Ok(record)
    }

    async fn list_devices(&self) -> Result<Vec<DeviceRecord>, StoreError> {
        let ledger = self.inner.read().await;
        let mut devices = ledger
            .devices
            .iter()
            .map(|entry| entry.record.clone())
            .collect::<Vec<_>>();
        devices.sort_by_key(|device| std::cmp::Reverse(device.enrolled_at));
        Ok(devices)
    }

    async fn revoke_device(&self, device_id: Uuid) -> Result<Option<DeviceRecord>, StoreError> {
        let mut ledger = self.inner.write().await;
        let now = time::OffsetDateTime::now_utc();
        let Some(entry) = ledger
            .devices
            .iter_mut()
            .find(|entry| entry.record.device_id == device_id)
        else {
            return Ok(None);
        };
        entry.record.status = DeviceStatus::Revoked;
        entry.record.revoked_at = Some(now);
        Ok(Some(entry.record.clone()))
    }

    async fn device_by_token_hash(
        &self,
        token_hash: &str,
    ) -> Result<Option<DeviceRecord>, StoreError> {
        let ledger = self.inner.read().await;
        Ok(ledger
            .devices
            .iter()
            .find(|entry| entry.token_hash == token_hash)
            .map(|entry| entry.record.clone()))
    }

    async fn mark_device_seen(&self, device_id: Uuid) -> Result<(), StoreError> {
        let mut ledger = self.inner.write().await;
        if let Some(entry) = ledger
            .devices
            .iter_mut()
            .find(|entry| entry.record.device_id == device_id)
        {
            entry.record.last_seen_at = Some(time::OffsetDateTime::now_utc());
        }
        Ok(())
    }

    async fn apply_master_data_change(
        &self,
        tenant_id: Uuid,
        store_id: Uuid,
        change: &MasterDataChange,
    ) -> Result<MasterDataApplyResult, StoreError> {
        let mut ledger = self.inner.write().await;
        let local = ledger
            .master_data
            .iter()
            .find(|entry| {
                entry.tenant_id == tenant_id
                    && entry.store_id == store_id
                    && entry.change.kind == change.kind
                    && entry.change.entity_id == change.entity_id
            })
            .cloned();
        let resolution = resolve_change(local.as_ref().map(|entry| &entry.change), change);
        let now = time::OffsetDateTime::now_utc();
        let conflict = if let Some(reason) = resolution.conflict {
            let conflict = MasterDataConflict {
                id: Uuid::now_v7(),
                tenant_id: TenantId::from_uuid(tenant_id),
                store_id: StoreId::from_uuid(store_id),
                kind: change.kind,
                entity_id: change.entity_id,
                local_version: local.as_ref().map_or(0, |entry| entry.change.version),
                incoming: change.clone(),
                reason,
                outcome: resolution.outcome,
                detected_at: now,
            };
            ledger.master_conflicts.push(conflict.clone());
            Some(conflict)
        } else {
            None
        };
        if resolution.applied() {
            if let Some(entry) = ledger.master_data.iter_mut().find(|entry| {
                entry.tenant_id == tenant_id
                    && entry.store_id == store_id
                    && entry.change.kind == change.kind
                    && entry.change.entity_id == change.entity_id
            }) {
                entry.change = change.clone();
            } else {
                ledger.master_data.push(MemoryMasterData {
                    tenant_id,
                    store_id,
                    change: change.clone(),
                });
            }
        }
        Ok(MasterDataApplyResult {
            applied: resolution.applied(),
            conflict,
        })
    }

    async fn list_master_data_conflicts(
        &self,
        limit: u32,
    ) -> Result<Vec<MasterDataConflict>, StoreError> {
        let ledger = self.inner.read().await;
        let mut conflicts = ledger.master_conflicts.clone();
        conflicts.sort_by_key(|left| std::cmp::Reverse(left.detected_at));
        conflicts.truncate(limit as usize);
        Ok(conflicts)
    }

    async fn list_central_users(&self) -> Result<Vec<CentralUserRecord>, StoreError> {
        let ledger = self.inner.read().await;
        let mut users = ledger.users.clone();
        users.sort_by(|left, right| left.username.cmp(&right.username));
        Ok(users)
    }

    async fn create_central_user(
        &self,
        user: NewCentralUser,
    ) -> Result<CentralUserRecord, StoreError> {
        let mut ledger = self.inner.write().await;
        if ledger.users.iter().any(|existing| {
            existing.tenant_id == user.tenant_id
                && existing.username.eq_ignore_ascii_case(&user.username)
        }) {
            return Err(StoreError::Conflict(format!(
                "username {} already exists",
                user.username
            )));
        }
        let now = time::OffsetDateTime::now_utc();
        let record = CentralUserRecord {
            id: Uuid::now_v7(),
            tenant_id: user.tenant_id,
            username: user.username,
            display_name: user.display_name,
            role: user.role,
            store_ids: user.store_ids,
            active: true,
            version: 1,
            created_at: now,
            updated_at: now,
            password_hash: user.password_hash,
        };
        ledger.users.push(record.clone());
        Ok(record)
    }

    async fn update_central_user(
        &self,
        id: Uuid,
        update: UpdateCentralUser,
    ) -> Result<Option<CentralUserRecord>, StoreError> {
        let mut ledger = self.inner.write().await;
        let Some(user) = ledger.users.iter_mut().find(|user| user.id == id) else {
            return Ok(None);
        };
        user.display_name = update.display_name;
        user.role = update.role;
        user.store_ids = update.store_ids;
        user.active = update.active;
        if let Some(password_hash) = update.password_hash {
            user.password_hash = password_hash;
        }
        user.version = user.version.saturating_add(1);
        user.updated_at = time::OffsetDateTime::now_utc();
        Ok(Some(user.clone()))
    }

    async fn set_central_user_password(
        &self,
        id: Uuid,
        password_hash: &str,
    ) -> Result<Option<CentralUserRecord>, StoreError> {
        let mut ledger = self.inner.write().await;
        let Some(user) = ledger.users.iter_mut().find(|user| user.id == id) else {
            return Ok(None);
        };
        user.password_hash = password_hash.to_string();
        user.version = user.version.saturating_add(1);
        user.updated_at = time::OffsetDateTime::now_utc();
        Ok(Some(user.clone()))
    }

    async fn find_central_users_by_username(
        &self,
        username: &str,
    ) -> Result<Vec<CentralUserRecord>, StoreError> {
        let ledger = self.inner.read().await;
        Ok(ledger
            .users
            .iter()
            .filter(|user| user.username == username)
            .cloned()
            .collect())
    }

    async fn create_central_session(&self, session: NewCentralSession) -> Result<(), StoreError> {
        let mut ledger = self.inner.write().await;
        ledger.sessions.retain(|existing| {
            existing.token_hash != session.token_hash
                && existing.expires_at > time::OffsetDateTime::now_utc()
        });
        ledger.sessions.push(CentralSessionRecord {
            token_hash: session.token_hash,
            user_id: session.user_id,
            tenant_id: session.tenant_id,
            username: session.username,
            display_name: session.display_name,
            role: session.role,
            expires_at: session.expires_at,
        });
        Ok(())
    }

    async fn central_session_by_token_hash(
        &self,
        token_hash: &str,
    ) -> Result<Option<CentralSessionRecord>, StoreError> {
        let ledger = self.inner.read().await;
        Ok(ledger
            .sessions
            .iter()
            .find(|session| {
                session.token_hash == token_hash
                    && session.expires_at > time::OffsetDateTime::now_utc()
            })
            .cloned())
    }

    async fn delete_central_session(&self, token_hash: &str) -> Result<(), StoreError> {
        let mut ledger = self.inner.write().await;
        ledger
            .sessions
            .retain(|session| session.token_hash != token_hash);
        Ok(())
    }

    async fn delete_central_user_sessions(&self, user_id: Uuid) -> Result<(), StoreError> {
        let mut ledger = self.inner.write().await;
        ledger.sessions.retain(|session| session.user_id != user_id);
        Ok(())
    }
}

#[derive(Clone)]
pub struct PostgresStore {
    pool: PgPool,
}

/// Filtros compartidos por el conteo y la página de eventos. Los parámetros
/// `$1..$6` son store, device, tipo, desde, hasta y solo-ventas.
const EVENT_FILTER_SQL: &str = "($1::uuid IS NULL OR store_id = $1)
   AND ($2::uuid IS NULL OR device_id = $2)
   AND ($3::text IS NULL OR event_type = $3)
   AND ($4::timestamptz IS NULL OR occurred_at >= $4)
   AND ($5::timestamptz IS NULL OR occurred_at <= $5)
   AND (NOT $6 OR aggregate_type = 'sale')";

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

    async fn query_events(&self, query: &EventQuery) -> Result<EventPage, StoreError> {
        let total = sqlx::query_scalar::<_, i64>(&format!(
            "SELECT COUNT(*) FROM central_events WHERE {EVENT_FILTER_SQL}"
        ))
        .bind(query.store_id)
        .bind(query.device_id)
        .bind(&query.event_type)
        .bind(query.from)
        .bind(query.to)
        .bind(query.sales_only)
        .fetch_one(&self.pool)
        .await?;
        let rows = sqlx::query(&format!(
            "SELECT seq, envelope, received_at
             FROM central_events
             WHERE {EVENT_FILTER_SQL}
             ORDER BY seq DESC
             LIMIT $7 OFFSET $8"
        ))
        .bind(query.store_id)
        .bind(query.device_id)
        .bind(&query.event_type)
        .bind(query.from)
        .bind(query.to)
        .bind(query.sales_only)
        .bind(i64::from(query.limit))
        .bind(i64::from(query.offset))
        .fetch_all(&self.pool)
        .await?;
        let items = rows
            .iter()
            .map(row_to_event)
            .collect::<Result<Vec<_>, _>>()?;
        Ok(EventPage {
            items,
            total: u64::try_from(total).unwrap_or_default(),
        })
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

fn row_to_device(row: &sqlx::postgres::PgRow) -> Result<DeviceRecord, StoreError> {
    Ok(DeviceRecord {
        device_id: row.try_get("device_id")?,
        tenant_id: row.try_get("tenant_id")?,
        store_id: row.try_get("store_id")?,
        name: row.try_get("name")?,
        status: DeviceStatus::parse(&row.try_get::<String, _>("status")?)?,
        enrolled_at: row.try_get("enrolled_at")?,
        last_seen_at: row.try_get("last_seen_at")?,
        revoked_at: row.try_get("revoked_at")?,
    })
}

fn row_to_master_conflict(row: &sqlx::postgres::PgRow) -> Result<MasterDataConflict, StoreError> {
    let kind = MasterDataKind::parse(&row.try_get::<String, _>("kind")?)?;
    let incoming_value: serde_json::Value = row.try_get("incoming")?;
    let incoming = serde_json::from_value(incoming_value)
        .map_err(|error| StoreError::Database(error.to_string()))?;
    Ok(MasterDataConflict {
        id: row.try_get("id")?,
        tenant_id: TenantId::from_uuid(row.try_get("tenant_id")?),
        store_id: StoreId::from_uuid(row.try_get("store_id")?),
        kind,
        entity_id: row.try_get("entity_id")?,
        local_version: u64::try_from(row.try_get::<i64, _>("local_version")?)
            .map_err(|_| StoreError::Database("negative master data version".into()))?,
        incoming,
        reason: ConflictReason::parse(&row.try_get::<String, _>("reason")?)?,
        outcome: ConflictOutcome::parse(&row.try_get::<String, _>("outcome")?)?,
        detected_at: row.try_get("detected_at")?,
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

    async fn list_events(&self, query: &EventQuery) -> Result<EventPage, StoreError> {
        self.query_events(query).await
    }

    async fn list_sales(&self, query: &EventQuery) -> Result<EventPage, StoreError> {
        let mut query = query.clone();
        query.sales_only = true;
        self.query_events(&query).await
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

    async fn enroll_device(&self, device: EnrollDevice) -> Result<DeviceRecord, StoreError> {
        let row = sqlx::query(
            "INSERT INTO central_devices
                (device_id, tenant_id, store_id, name, token_hash, status, enrolled_at)
             VALUES ($1, $2, $3, $4, $5, 'active', NOW())
             ON CONFLICT (device_id) DO UPDATE SET
                tenant_id = EXCLUDED.tenant_id,
                store_id = EXCLUDED.store_id,
                name = EXCLUDED.name,
                token_hash = EXCLUDED.token_hash,
                status = 'active',
                enrolled_at = NOW(),
                revoked_at = NULL
             RETURNING device_id, tenant_id, store_id, name, status,
                       enrolled_at, last_seen_at, revoked_at",
        )
        .bind(device.device_id)
        .bind(device.tenant_id)
        .bind(device.store_id)
        .bind(&device.name)
        .bind(&device.token_hash)
        .fetch_one(&self.pool)
        .await?;
        row_to_device(&row)
    }

    async fn list_devices(&self) -> Result<Vec<DeviceRecord>, StoreError> {
        let rows = sqlx::query(
            "SELECT device_id, tenant_id, store_id, name, status,
                    enrolled_at, last_seen_at, revoked_at
             FROM central_devices
             ORDER BY enrolled_at DESC",
        )
        .fetch_all(&self.pool)
        .await?;
        rows.iter().map(row_to_device).collect()
    }

    async fn revoke_device(&self, device_id: Uuid) -> Result<Option<DeviceRecord>, StoreError> {
        let row = sqlx::query(
            "UPDATE central_devices
             SET status = 'revoked', revoked_at = NOW()
             WHERE device_id = $1
             RETURNING device_id, tenant_id, store_id, name, status,
                       enrolled_at, last_seen_at, revoked_at",
        )
        .bind(device_id)
        .fetch_optional(&self.pool)
        .await?;
        row.as_ref().map(row_to_device).transpose()
    }

    async fn device_by_token_hash(
        &self,
        token_hash: &str,
    ) -> Result<Option<DeviceRecord>, StoreError> {
        let row = sqlx::query(
            "SELECT device_id, tenant_id, store_id, name, status,
                    enrolled_at, last_seen_at, revoked_at
             FROM central_devices WHERE token_hash = $1",
        )
        .bind(token_hash)
        .fetch_optional(&self.pool)
        .await?;
        row.as_ref().map(row_to_device).transpose()
    }

    async fn mark_device_seen(&self, device_id: Uuid) -> Result<(), StoreError> {
        sqlx::query("UPDATE central_devices SET last_seen_at = NOW() WHERE device_id = $1")
            .bind(device_id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    async fn apply_master_data_change(
        &self,
        tenant_id: Uuid,
        store_id: Uuid,
        change: &MasterDataChange,
    ) -> Result<MasterDataApplyResult, StoreError> {
        let mut tx = self.pool.begin().await?;
        let lock_key = format!(
            "{tenant_id}:{store_id}:{}:{}",
            change.kind.as_str(),
            change.entity_id
        );
        sqlx::query("SELECT pg_advisory_xact_lock(hashtext($1)::bigint)")
            .bind(&lock_key)
            .execute(&mut *tx)
            .await?;
        let row = sqlx::query(
            "SELECT change FROM central_master_data
             WHERE tenant_id = $1 AND store_id = $2 AND kind = $3 AND entity_id = $4",
        )
        .bind(tenant_id)
        .bind(store_id)
        .bind(change.kind.as_str())
        .bind(change.entity_id)
        .fetch_optional(&mut *tx)
        .await?;
        let local = row
            .map(|row| {
                let value: serde_json::Value = row.try_get("change")?;
                serde_json::from_value(value)
                    .map_err(|error| StoreError::Database(error.to_string()))
            })
            .transpose()?;
        let resolution = resolve_change(local.as_ref(), change);
        let now = time::OffsetDateTime::now_utc();
        let conflict = if let Some(reason) = resolution.conflict {
            let conflict = MasterDataConflict {
                id: Uuid::now_v7(),
                tenant_id: TenantId::from_uuid(tenant_id),
                store_id: StoreId::from_uuid(store_id),
                kind: change.kind,
                entity_id: change.entity_id,
                local_version: local.as_ref().map_or(0, |local| local.version),
                incoming: change.clone(),
                reason,
                outcome: resolution.outcome,
                detected_at: now,
            };
            let incoming = serde_json::to_value(change)
                .map_err(|error| StoreError::Database(error.to_string()))?;
            sqlx::query(
                "INSERT INTO central_master_conflicts
                    (id, tenant_id, store_id, kind, entity_id, local_version,
                     incoming, reason, outcome, detected_at)
                 VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)",
            )
            .bind(conflict.id)
            .bind(tenant_id)
            .bind(store_id)
            .bind(change.kind.as_str())
            .bind(change.entity_id)
            .bind(i64::try_from(conflict.local_version).unwrap_or(i64::MAX))
            .bind(incoming)
            .bind(conflict.reason.as_str())
            .bind(conflict.outcome.as_str())
            .bind(conflict.detected_at)
            .execute(&mut *tx)
            .await?;
            Some(conflict)
        } else {
            None
        };
        if resolution.applied() {
            let value = serde_json::to_value(change)
                .map_err(|error| StoreError::Database(error.to_string()))?;
            sqlx::query(
                "INSERT INTO central_master_data
                    (tenant_id, store_id, kind, entity_id, version, change, updated_at)
                 VALUES ($1, $2, $3, $4, $5, $6, NOW())
                 ON CONFLICT (tenant_id, store_id, kind, entity_id) DO UPDATE SET
                    version = EXCLUDED.version,
                    change = EXCLUDED.change,
                    updated_at = NOW()",
            )
            .bind(tenant_id)
            .bind(store_id)
            .bind(change.kind.as_str())
            .bind(change.entity_id)
            .bind(i64::try_from(change.version).unwrap_or(i64::MAX))
            .bind(value)
            .execute(&mut *tx)
            .await?;
        }
        tx.commit().await?;
        Ok(MasterDataApplyResult {
            applied: resolution.applied(),
            conflict,
        })
    }

    async fn list_master_data_conflicts(
        &self,
        limit: u32,
    ) -> Result<Vec<MasterDataConflict>, StoreError> {
        let rows = sqlx::query(
            "SELECT id, tenant_id, store_id, kind, entity_id, local_version,
                    incoming, reason, outcome, detected_at
             FROM central_master_conflicts
             ORDER BY detected_at DESC
             LIMIT $1",
        )
        .bind(i64::from(limit))
        .fetch_all(&self.pool)
        .await?;
        rows.iter().map(row_to_master_conflict).collect()
    }

    async fn list_central_users(&self) -> Result<Vec<CentralUserRecord>, StoreError> {
        let rows = sqlx::query(
            "SELECT id, tenant_id, username, display_name, role, store_ids,
                    active, version, created_at, updated_at, password_hash
             FROM central_users
             ORDER BY username",
        )
        .fetch_all(&self.pool)
        .await?;
        rows.iter().map(row_to_central_user).collect()
    }

    async fn create_central_user(
        &self,
        user: NewCentralUser,
    ) -> Result<CentralUserRecord, StoreError> {
        let store_ids = serde_json::to_value(&user.store_ids)
            .map_err(|error| StoreError::Database(error.to_string()))?;
        let username = user.username.clone();
        let row = sqlx::query(
            "INSERT INTO central_users
                (id, tenant_id, username, display_name, role, store_ids,
                 password_hash, active, version, created_at, updated_at)
             VALUES ($1, $2, $3, $4, $5, $6, $7, TRUE, 1, NOW(), NOW())
             RETURNING id, tenant_id, username, display_name, role, store_ids,
                       active, version, created_at, updated_at, password_hash",
        )
        .bind(Uuid::now_v7())
        .bind(user.tenant_id)
        .bind(&user.username)
        .bind(&user.display_name)
        .bind(role_to_str(user.role))
        .bind(store_ids)
        .bind(&user.password_hash)
        .fetch_one(&self.pool)
        .await
        .map_err(|error| {
            if let sqlx::Error::Database(database_error) = &error {
                if database_error.is_unique_violation() {
                    return StoreError::Conflict(format!("username {username} already exists"));
                }
            }
            StoreError::from(error)
        })?;
        row_to_central_user(&row)
    }

    async fn update_central_user(
        &self,
        id: Uuid,
        update: UpdateCentralUser,
    ) -> Result<Option<CentralUserRecord>, StoreError> {
        let store_ids = serde_json::to_value(&update.store_ids)
            .map_err(|error| StoreError::Database(error.to_string()))?;
        let row = if let Some(password_hash) = update.password_hash {
            sqlx::query(
                "UPDATE central_users
                 SET display_name = $2, role = $3, store_ids = $4, active = $5,
                     password_hash = $6, version = version + 1, updated_at = NOW()
                 WHERE id = $1
                 RETURNING id, tenant_id, username, display_name, role, store_ids,
                           active, version, created_at, updated_at, password_hash",
            )
            .bind(id)
            .bind(&update.display_name)
            .bind(role_to_str(update.role))
            .bind(store_ids)
            .bind(update.active)
            .bind(password_hash)
            .fetch_optional(&self.pool)
            .await?
        } else {
            sqlx::query(
                "UPDATE central_users
                 SET display_name = $2, role = $3, store_ids = $4, active = $5,
                     version = version + 1, updated_at = NOW()
                 WHERE id = $1
                 RETURNING id, tenant_id, username, display_name, role, store_ids,
                           active, version, created_at, updated_at, password_hash",
            )
            .bind(id)
            .bind(&update.display_name)
            .bind(role_to_str(update.role))
            .bind(store_ids)
            .bind(update.active)
            .fetch_optional(&self.pool)
            .await?
        };
        row.as_ref().map(row_to_central_user).transpose()
    }

    async fn set_central_user_password(
        &self,
        id: Uuid,
        password_hash: &str,
    ) -> Result<Option<CentralUserRecord>, StoreError> {
        let row = sqlx::query(
            "UPDATE central_users
             SET password_hash = $2, version = version + 1, updated_at = NOW()
             WHERE id = $1
             RETURNING id, tenant_id, username, display_name, role, store_ids,
                       active, version, created_at, updated_at, password_hash",
        )
        .bind(id)
        .bind(password_hash)
        .fetch_optional(&self.pool)
        .await?;
        row.as_ref().map(row_to_central_user).transpose()
    }

    async fn find_central_users_by_username(
        &self,
        username: &str,
    ) -> Result<Vec<CentralUserRecord>, StoreError> {
        let rows = sqlx::query(
            "SELECT id, tenant_id, username, display_name, role, store_ids,
                    active, version, created_at, updated_at, password_hash
             FROM central_users
             WHERE username = $1
             ORDER BY tenant_id",
        )
        .bind(username)
        .fetch_all(&self.pool)
        .await?;
        rows.iter().map(row_to_central_user).collect()
    }

    async fn create_central_session(&self, session: NewCentralSession) -> Result<(), StoreError> {
        sqlx::query("DELETE FROM central_sessions WHERE expires_at <= NOW()")
            .execute(&self.pool)
            .await?;
        sqlx::query(
            "INSERT INTO central_sessions
                (token_hash, user_id, tenant_id, username, display_name, role, expires_at)
             VALUES ($1, $2, $3, $4, $5, $6, $7)
             ON CONFLICT (token_hash) DO UPDATE
             SET user_id = EXCLUDED.user_id,
                 tenant_id = EXCLUDED.tenant_id,
                 username = EXCLUDED.username,
                 display_name = EXCLUDED.display_name,
                 role = EXCLUDED.role,
                 expires_at = EXCLUDED.expires_at",
        )
        .bind(&session.token_hash)
        .bind(session.user_id)
        .bind(session.tenant_id)
        .bind(&session.username)
        .bind(&session.display_name)
        .bind(role_to_str(session.role))
        .bind(session.expires_at)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    async fn central_session_by_token_hash(
        &self,
        token_hash: &str,
    ) -> Result<Option<CentralSessionRecord>, StoreError> {
        let row = sqlx::query(
            "SELECT token_hash, user_id, tenant_id, username, display_name, role, expires_at
             FROM central_sessions
             WHERE token_hash = $1 AND expires_at > NOW()",
        )
        .bind(token_hash)
        .fetch_optional(&self.pool)
        .await?;
        row.as_ref().map(row_to_central_session).transpose()
    }

    async fn delete_central_session(&self, token_hash: &str) -> Result<(), StoreError> {
        sqlx::query("DELETE FROM central_sessions WHERE token_hash = $1")
            .bind(token_hash)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    async fn delete_central_user_sessions(&self, user_id: Uuid) -> Result<(), StoreError> {
        sqlx::query("DELETE FROM central_sessions WHERE user_id = $1")
            .bind(user_id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }
}

fn memory_event_page(events: &[StoredEvent], query: &EventQuery) -> Result<EventPage, StoreError> {
    let filtered = events
        .iter()
        .rev()
        .filter(|stored| {
            if query.sales_only && stored.event.aggregate_type != "sale" {
                return false;
            }
            if let Some(store_id) = query.store_id {
                if stored.event.store_id.as_uuid() != store_id {
                    return false;
                }
            }
            if let Some(device_id) = query.device_id {
                if stored.event.device_id.as_uuid() != device_id {
                    return false;
                }
            }
            if let Some(event_type) = &query.event_type {
                if &stored.event.event_type != event_type {
                    return false;
                }
            }
            if let Some(from) = query.from {
                if stored.event.occurred_at < from {
                    return false;
                }
            }
            if let Some(to) = query.to {
                if stored.event.occurred_at > to {
                    return false;
                }
            }
            true
        })
        .collect::<Vec<_>>();
    let total = u64::try_from(filtered.len()).unwrap_or_default();
    let items = filtered
        .into_iter()
        .skip(query.offset as usize)
        .take(query.limit as usize)
        .cloned()
        .collect();
    Ok(EventPage { items, total })
}

fn role_to_str(role: Role) -> &'static str {
    match role {
        Role::Admin => "admin",
        Role::Manager => "manager",
        Role::Cashier => "cashier",
        Role::Auditor => "auditor",
    }
}

fn role_from_str(value: &str) -> Result<Role, StoreError> {
    match value {
        "admin" => Ok(Role::Admin),
        "manager" => Ok(Role::Manager),
        "cashier" => Ok(Role::Cashier),
        "auditor" => Ok(Role::Auditor),
        _ => Err(StoreError::Database(format!("invalid user role: {value}"))),
    }
}

fn row_to_central_user(row: &sqlx::postgres::PgRow) -> Result<CentralUserRecord, StoreError> {
    let store_ids: serde_json::Value = row.try_get("store_ids")?;
    let store_ids = serde_json::from_value(store_ids)
        .map_err(|error| StoreError::Database(error.to_string()))?;
    Ok(CentralUserRecord {
        id: row.try_get("id")?,
        tenant_id: row.try_get("tenant_id")?,
        username: row.try_get("username")?,
        display_name: row.try_get("display_name")?,
        role: role_from_str(&row.try_get::<String, _>("role")?)?,
        store_ids,
        active: row.try_get("active")?,
        version: u64::try_from(row.try_get::<i64, _>("version")?)
            .map_err(|_| StoreError::Database("negative user version".into()))?,
        created_at: row.try_get("created_at")?,
        updated_at: row.try_get("updated_at")?,
        password_hash: row.try_get("password_hash")?,
    })
}

fn row_to_central_session(row: &sqlx::postgres::PgRow) -> Result<CentralSessionRecord, StoreError> {
    Ok(CentralSessionRecord {
        token_hash: row.try_get("token_hash")?,
        user_id: row.try_get("user_id")?,
        tenant_id: row.try_get("tenant_id")?,
        username: row.try_get("username")?,
        display_name: row.try_get("display_name")?,
        role: role_from_str(&row.try_get::<String, _>("role")?)?,
        expires_at: row.try_get("expires_at")?,
    })
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

    #[tokio::test]
    #[ignore = "requires DATABASE_URL pointing to a PostgreSQL database"]
    async fn postgres_central_user_roundtrip() {
        let url = std::env::var("DATABASE_URL").expect("DATABASE_URL must be set");
        let store = PostgresStore::connect(&url).await.unwrap();
        let tenant_id = Uuid::now_v7();
        let store_id = Uuid::now_v7();
        let username = format!("user-{}", &Uuid::now_v7().simple().to_string()[..12]);
        let created = store
            .create_central_user(NewCentralUser {
                tenant_id,
                username: username.clone(),
                display_name: "Usuario PostgreSQL".into(),
                role: Role::Cashier,
                store_ids: vec![store_id],
                password_hash: "hash-v1".into(),
            })
            .await
            .unwrap();
        assert_eq!(created.version, 1);
        assert_eq!(created.password_hash, "hash-v1");

        let listed = store.list_central_users().await.unwrap();
        assert!(listed.iter().any(|user| user.id == created.id));

        let updated = store
            .update_central_user(
                created.id,
                UpdateCentralUser {
                    display_name: "Usuario Actualizado".into(),
                    role: Role::Manager,
                    store_ids: vec![store_id],
                    active: false,
                    password_hash: Some("hash-v2".into()),
                },
            )
            .await
            .unwrap()
            .unwrap();
        assert_eq!(updated.version, 2);
        assert_eq!(updated.password_hash, "hash-v2");
        assert!(!updated.active);

        let reset = store
            .set_central_user_password(created.id, "hash-v3")
            .await
            .unwrap()
            .unwrap();
        assert_eq!(reset.version, 3);
        assert_eq!(reset.password_hash, "hash-v3");
    }

    #[tokio::test]
    #[ignore = "requires DATABASE_URL pointing to a PostgreSQL database"]
    async fn postgres_device_registry_roundtrip() {
        let url = std::env::var("DATABASE_URL").expect("DATABASE_URL must be set");
        let store = PostgresStore::connect(&url).await.unwrap();
        let device_id = Uuid::now_v7();
        let tenant_id = Uuid::now_v7();
        let store_id = Uuid::now_v7();
        let first_hash = format!("hash-{}", Uuid::now_v7());
        let device = store
            .enroll_device(EnrollDevice {
                device_id,
                tenant_id,
                store_id,
                name: "Caja Postgres".into(),
                token_hash: first_hash.clone(),
            })
            .await
            .unwrap();
        assert_eq!(device.status, DeviceStatus::Active);

        let found = store
            .device_by_token_hash(&first_hash)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(found.device_id, device_id);
        store.mark_device_seen(device_id).await.unwrap();
        let seen = store
            .device_by_token_hash(&first_hash)
            .await
            .unwrap()
            .unwrap();
        assert!(seen.last_seen_at.is_some());

        let revoked = store.revoke_device(device_id).await.unwrap().unwrap();
        assert_eq!(revoked.status, DeviceStatus::Revoked);
        assert!(revoked.revoked_at.is_some());

        let reactivated = store
            .enroll_device(EnrollDevice {
                device_id,
                tenant_id,
                store_id,
                name: "Caja Postgres 2".into(),
                token_hash: format!("hash-{}", Uuid::now_v7()),
            })
            .await
            .unwrap();
        assert_eq!(reactivated.status, DeviceStatus::Active);
        assert!(reactivated.revoked_at.is_none());
        assert_eq!(reactivated.name, "Caja Postgres 2");
    }

    #[tokio::test]
    #[ignore = "requires DATABASE_URL pointing to a PostgreSQL database"]
    async fn postgres_master_data_roundtrip() {
        let url = std::env::var("DATABASE_URL").expect("DATABASE_URL must be set");
        let store = PostgresStore::connect(&url).await.unwrap();
        let tenant_id = Uuid::now_v7();
        let store_id = Uuid::now_v7();
        let entity_id = Uuid::now_v7();
        let device = tpv_domain::ids::DeviceId::new();
        let base = time::OffsetDateTime::now_utc();
        let first = MasterDataChange::new(
            MasterDataKind::Product,
            entity_id,
            1,
            device,
            base,
            serde_json::json!({"name": "Producto A"}),
            false,
        )
        .unwrap();
        let result = store
            .apply_master_data_change(tenant_id, store_id, &first)
            .await
            .unwrap();
        assert!(result.applied);
        assert!(result.conflict.is_none());

        let divergent = MasterDataChange::new(
            MasterDataKind::Product,
            entity_id,
            1,
            device,
            base + time::Duration::seconds(1),
            serde_json::json!({"name": "Producto B"}),
            false,
        )
        .unwrap();
        let result = store
            .apply_master_data_change(tenant_id, store_id, &divergent)
            .await
            .unwrap();
        assert!(result.applied);
        assert!(result.conflict.is_some());

        let conflicts = store.list_master_data_conflicts(10).await.unwrap();
        assert!(
            conflicts
                .iter()
                .any(|conflict| conflict.entity_id == entity_id)
        );
    }
}
