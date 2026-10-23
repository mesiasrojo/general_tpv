use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use time::OffsetDateTime;
use uuid::Uuid;

use crate::error::{DomainError, Result};
use crate::ids::{DeviceId, StoreId, TenantId};

/// Transport envelope for replication. The hash is only a technical integrity
/// check for the envelope; it is not a fiscal hash chain.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SyncEvent {
    pub event_id: Uuid,
    pub tenant_id: TenantId,
    pub store_id: StoreId,
    pub device_id: DeviceId,
    pub device_seq: u64,
    pub aggregate_type: String,
    pub aggregate_id: Uuid,
    pub event_type: String,
    pub schema_version: u32,
    pub occurred_at: OffsetDateTime,
    pub payload: Value,
    pub hash: String,
}

#[derive(Serialize)]
struct HashInput<'a> {
    event_id: Uuid,
    tenant_id: TenantId,
    store_id: StoreId,
    device_id: DeviceId,
    device_seq: u64,
    aggregate_type: &'a str,
    aggregate_id: Uuid,
    event_type: &'a str,
    schema_version: u32,
    occurred_at: OffsetDateTime,
    payload: &'a Value,
}

impl SyncEvent {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        tenant_id: TenantId,
        store_id: StoreId,
        device_id: DeviceId,
        device_seq: u64,
        aggregate_type: impl Into<String>,
        aggregate_id: Uuid,
        event_type: impl Into<String>,
        schema_version: u32,
        occurred_at: OffsetDateTime,
        payload: Value,
    ) -> Result<Self> {
        let aggregate_type = aggregate_type.into().trim().to_string();
        let event_type = event_type.into().trim().to_string();
        if aggregate_type.is_empty() || event_type.is_empty() {
            return Err(DomainError::Validation(
                "sync aggregate type and event type are required".into(),
            ));
        }
        if schema_version == 0 {
            return Err(DomainError::Validation(
                "sync schema version must be positive".into(),
            ));
        }
        let mut event = Self {
            event_id: Uuid::now_v7(),
            tenant_id,
            store_id,
            device_id,
            device_seq,
            aggregate_type,
            aggregate_id,
            event_type,
            schema_version,
            occurred_at,
            payload,
            hash: String::new(),
        };
        event.hash = event.compute_hash()?;
        Ok(event)
    }

    pub fn verify_hash(&self) -> bool {
        self.compute_hash().is_ok_and(|hash| hash == self.hash)
    }

    fn compute_hash(&self) -> Result<String> {
        let input = HashInput {
            event_id: self.event_id,
            tenant_id: self.tenant_id,
            store_id: self.store_id,
            device_id: self.device_id,
            device_seq: self.device_seq,
            aggregate_type: &self.aggregate_type,
            aggregate_id: self.aggregate_id,
            event_type: &self.event_type,
            schema_version: self.schema_version,
            occurred_at: self.occurred_at,
            payload: &self.payload,
        };
        let bytes =
            serde_json::to_vec(&input).map_err(|error| DomainError::Internal(error.to_string()))?;
        let digest = Sha256::digest(bytes);
        Ok(hex::encode(digest))
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SyncBatch {
    pub device_id: DeviceId,
    pub events: Vec<SyncEvent>,
}

impl SyncBatch {
    pub fn new(device_id: DeviceId, events: Vec<SyncEvent>) -> Result<Self> {
        if events.len() > 100 {
            return Err(DomainError::Validation(
                "sync batches are limited to 100 events".into(),
            ));
        }
        if events.iter().any(|event| event.device_id != device_id) {
            return Err(DomainError::Validation(
                "sync batch contains an event from another device".into(),
            ));
        }
        Ok(Self { device_id, events })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn creates_and_verifies_event_hash() {
        let event = SyncEvent::new(
            TenantId::new(),
            StoreId::new(),
            DeviceId::new(),
            1,
            "sale",
            Uuid::now_v7(),
            "sale.confirmed",
            1,
            time::macros::datetime!(2026-10-22 19:15:46 UTC),
            json!({"total": 1210}),
        )
        .unwrap();
        assert!(event.verify_hash());
    }

    #[test]
    fn detects_tampering() {
        let mut event = SyncEvent::new(
            TenantId::new(),
            StoreId::new(),
            DeviceId::new(),
            1,
            "sale",
            Uuid::now_v7(),
            "sale.confirmed",
            1,
            time::macros::datetime!(2026-10-22 19:15:46 UTC),
            json!({"total": 1210}),
        )
        .unwrap();
        event.payload = json!({"total": 1});
        assert!(!event.verify_hash());
    }

    #[test]
    fn limits_batch_size() {
        let device_id = DeviceId::new();
        let events = (0..101)
            .map(|seq| {
                SyncEvent::new(
                    TenantId::new(),
                    StoreId::new(),
                    device_id,
                    seq,
                    "sale",
                    Uuid::now_v7(),
                    "sale.confirmed",
                    1,
                    time::macros::datetime!(2026-10-22 19:15:46 UTC),
                    json!({}),
                )
                .unwrap()
            })
            .collect();
        assert!(SyncBatch::new(device_id, events).is_err());
    }
}
