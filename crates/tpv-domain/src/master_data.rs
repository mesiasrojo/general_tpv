use serde::{Deserialize, Serialize};
use serde_json::Value;
use time::OffsetDateTime;
use uuid::Uuid;

use crate::error::{DomainError, Result};
use crate::ids::{DeviceId, StoreId, TenantId};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MasterDataKind {
    Product,
    Customer,
    Seller,
    Store,
}

impl MasterDataKind {
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Product => "product",
            Self::Customer => "customer",
            Self::Seller => "seller",
            Self::Store => "store",
        }
    }

    pub fn parse(value: &str) -> Result<Self> {
        match value {
            "product" => Ok(Self::Product),
            "customer" => Ok(Self::Customer),
            "seller" => Ok(Self::Seller),
            "store" => Ok(Self::Store),
            other => Err(DomainError::Validation(format!(
                "unknown master data kind: {other}"
            ))),
        }
    }
}

/// A versioned change to a master-data entity.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MasterDataChange {
    pub kind: MasterDataKind,
    pub entity_id: Uuid,
    pub version: u64,
    pub device_id: DeviceId,
    pub occurred_at: OffsetDateTime,
    pub payload: Value,
    pub deleted: bool,
}

impl MasterDataChange {
    pub fn new(
        kind: MasterDataKind,
        entity_id: Uuid,
        version: u64,
        device_id: DeviceId,
        occurred_at: OffsetDateTime,
        payload: Value,
        deleted: bool,
    ) -> Result<Self> {
        if entity_id.is_nil() {
            return Err(DomainError::Validation(
                "master data entity id is required".into(),
            ));
        }
        if version == 0 {
            return Err(DomainError::Validation(
                "master data version must be positive".into(),
            ));
        }
        if !payload.is_object() && !payload.is_null() {
            return Err(DomainError::Validation(
                "master data payload must be a JSON object".into(),
            ));
        }
        Ok(Self {
            kind,
            entity_id,
            version,
            device_id,
            occurred_at,
            payload,
            deleted,
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConflictReason {
    SameVersionDiverged,
}

impl ConflictReason {
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::SameVersionDiverged => "same_version_diverged",
        }
    }

    pub fn parse(value: &str) -> Result<Self> {
        match value {
            "same_version_diverged" => Ok(Self::SameVersionDiverged),
            other => Err(DomainError::Validation(format!(
                "unknown master data conflict reason: {other}"
            ))),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConflictOutcome {
    AcceptIncoming,
    KeepLocal,
}

impl ConflictOutcome {
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::AcceptIncoming => "accept_incoming",
            Self::KeepLocal => "keep_local",
        }
    }

    pub fn parse(value: &str) -> Result<Self> {
        match value {
            "accept_incoming" => Ok(Self::AcceptIncoming),
            "keep_local" => Ok(Self::KeepLocal),
            other => Err(DomainError::Validation(format!(
                "unknown master data conflict outcome: {other}"
            ))),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ChangeResolution {
    pub outcome: ConflictOutcome,
    pub conflict: Option<ConflictReason>,
}

impl ChangeResolution {
    #[must_use]
    pub fn applied(self) -> bool {
        self.outcome == ConflictOutcome::AcceptIncoming
    }
}

/// Deterministic conflict policy: higher version wins and stale versions are
/// ignored; for the same version, later `occurred_at` wins and device id breaks
/// exact ties. Divergent same-version changes are recorded as conflicts for
/// auditing.
#[must_use]
pub fn resolve_change(
    local: Option<&MasterDataChange>,
    incoming: &MasterDataChange,
) -> ChangeResolution {
    let Some(local) = local else {
        return ChangeResolution {
            outcome: ConflictOutcome::AcceptIncoming,
            conflict: None,
        };
    };
    if local.version < incoming.version {
        return ChangeResolution {
            outcome: ConflictOutcome::AcceptIncoming,
            conflict: None,
        };
    }
    if local.version > incoming.version {
        // Stale replays are common when a client re-pulls history; they are
        // ignored without generating a conflict record.
        return ChangeResolution {
            outcome: ConflictOutcome::KeepLocal,
            conflict: None,
        };
    }
    if local.payload == incoming.payload && local.deleted == incoming.deleted {
        return ChangeResolution {
            outcome: ConflictOutcome::KeepLocal,
            conflict: None,
        };
    }
    let incoming_wins = (incoming.occurred_at, incoming.device_id.as_uuid())
        > (local.occurred_at, local.device_id.as_uuid());
    ChangeResolution {
        outcome: if incoming_wins {
            ConflictOutcome::AcceptIncoming
        } else {
            ConflictOutcome::KeepLocal
        },
        conflict: Some(ConflictReason::SameVersionDiverged),
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MasterDataConflict {
    pub id: Uuid,
    pub tenant_id: TenantId,
    pub store_id: StoreId,
    pub kind: MasterDataKind,
    pub entity_id: Uuid,
    pub local_version: u64,
    pub incoming: MasterDataChange,
    pub reason: ConflictReason,
    pub outcome: ConflictOutcome,
    pub detected_at: OffsetDateTime,
}

#[derive(Debug, Clone)]
pub struct MasterDataApplyResult {
    pub applied: bool,
    pub conflict: Option<MasterDataConflict>,
}

impl MasterDataApplyResult {
    #[must_use]
    pub fn applied() -> Self {
        Self {
            applied: true,
            conflict: None,
        }
    }

    #[must_use]
    pub fn ignored() -> Self {
        Self {
            applied: false,
            conflict: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use time::macros::datetime;

    fn change(
        version: u64,
        device_id: DeviceId,
        at: OffsetDateTime,
        payload: Value,
    ) -> MasterDataChange {
        MasterDataChange::new(
            MasterDataKind::Product,
            Uuid::from_u128(1),
            version,
            device_id,
            at,
            payload,
            false,
        )
        .unwrap()
    }

    #[test]
    fn higher_version_wins() {
        let device = DeviceId::new();
        let now = datetime!(2026-10-23 12:00:00 UTC);
        let local = change(2, device, now, json!({"name": "A"}));
        let incoming = change(3, device, now, json!({"name": "B"}));
        let resolution = resolve_change(Some(&local), &incoming);
        assert!(resolution.applied());
        assert!(resolution.conflict.is_none());
    }

    #[test]
    fn older_version_is_ignored_as_stale() {
        let device = DeviceId::new();
        let now = datetime!(2026-10-23 12:00:00 UTC);
        let local = change(3, device, now, json!({"name": "A"}));
        let incoming = change(2, device, now, json!({"name": "B"}));
        let resolution = resolve_change(Some(&local), &incoming);
        assert!(!resolution.applied());
        assert!(resolution.conflict.is_none());
    }

    #[test]
    fn same_version_uses_timestamp_and_device_tiebreak() {
        let device_a = DeviceId::from_uuid(Uuid::from_u128(10));
        let device_b = DeviceId::from_uuid(Uuid::from_u128(20));
        let now = datetime!(2026-10-23 12:00:00 UTC);
        let local = change(1, device_a, now, json!({"name": "A"}));
        let later = change(
            1,
            device_b,
            now + time::Duration::seconds(1),
            json!({"name": "B"}),
        );
        let resolution = resolve_change(Some(&local), &later);
        assert!(resolution.applied());
        assert_eq!(
            resolution.conflict,
            Some(ConflictReason::SameVersionDiverged)
        );

        let same_time = change(1, device_b, now, json!({"name": "C"}));
        let resolution = resolve_change(Some(&local), &same_time);
        assert!(resolution.applied());
        assert_eq!(
            resolution.conflict,
            Some(ConflictReason::SameVersionDiverged)
        );
    }

    #[test]
    fn identical_change_is_idempotent() {
        let device = DeviceId::new();
        let now = datetime!(2026-10-23 12:00:00 UTC);
        let local = change(1, device, now, json!({"name": "A"}));
        let incoming = local.clone();
        let resolution = resolve_change(Some(&local), &incoming);
        assert!(!resolution.applied());
        assert!(resolution.conflict.is_none());
    }
}
