use serde::{Deserialize, Serialize};
use serde_json::Value;
use time::OffsetDateTime;

use crate::error::{DomainError, Result};
use crate::ids::{AuditEventId, StoreId, UserId};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AuditAction {
    Login,
    Logout,
    SaleConfirmed,
    SaleCancelled,
    InvoiceIssued,
    InvoiceRectified,
    CashOpened,
    CashClosed,
    CashReopened,
    CashMovement,
    Reprint,
    MasterDataChanged,
    SyncFailed,
    SettingsChanged,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AuditEvent {
    pub id: AuditEventId,
    pub occurred_at: OffsetDateTime,
    pub actor_id: Option<UserId>,
    pub store_id: Option<StoreId>,
    pub action: AuditAction,
    pub entity_type: String,
    pub entity_id: String,
    pub details: Value,
}

impl AuditEvent {
    pub fn new(
        occurred_at: OffsetDateTime,
        actor_id: Option<UserId>,
        store_id: Option<StoreId>,
        action: AuditAction,
        entity_type: impl Into<String>,
        entity_id: impl Into<String>,
        details: Value,
    ) -> Result<Self> {
        let entity_type = entity_type.into().trim().to_string();
        let entity_id = entity_id.into().trim().to_string();
        if entity_type.is_empty() || entity_id.is_empty() {
            return Err(DomainError::Validation(
                "audit entity type and identifier are required".into(),
            ));
        }
        Ok(Self {
            id: AuditEventId::new(),
            occurred_at,
            actor_id,
            store_id,
            action,
            entity_type,
            entity_id,
            details,
        })
    }
}
