use serde::{Deserialize, Serialize};
use std::fmt;
use uuid::Uuid;

use crate::error::{DomainError, Result};

macro_rules! uuid_id {
    ($name:ident) => {
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
        #[serde(transparent)]
        pub struct $name(Uuid);

        impl $name {
            #[must_use]
            pub fn new() -> Self {
                Self(Uuid::now_v7())
            }

            #[must_use]
            pub fn from_uuid(value: Uuid) -> Self {
                Self(value)
            }

            #[must_use]
            pub fn as_uuid(&self) -> Uuid {
                self.0
            }
        }

        impl Default for $name {
            fn default() -> Self {
                Self::new()
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                self.0.fmt(f)
            }
        }

        impl From<Uuid> for $name {
            fn from(value: Uuid) -> Self {
                Self(value)
            }
        }
    };
}

uuid_id!(TenantId);
uuid_id!(StoreId);
uuid_id!(RegisterId);
uuid_id!(DeviceId);
uuid_id!(UserId);
uuid_id!(SellerId);
uuid_id!(CustomerId);
uuid_id!(ProductId);
uuid_id!(FamilyId);
uuid_id!(TaxId);
uuid_id!(PriceListId);
uuid_id!(SaleId);
uuid_id!(SaleLineId);
uuid_id!(PaymentId);
uuid_id!(InvoiceId);
uuid_id!(CashSessionId);
uuid_id!(CashMovementId);
uuid_id!(SeriesId);
uuid_id!(AuditEventId);
uuid_id!(ReservationId);
uuid_id!(NoteId);
uuid_id!(TimeEntryId);
uuid_id!(PrintJobId);

/// Stock keeping unit. This is the code a cashier types or scans.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Sku(String);

impl Sku {
    pub fn new(value: impl Into<String>) -> Result<Self> {
        let value = value.into().trim().to_string();
        if value.is_empty() {
            return Err(DomainError::Validation("sku must not be empty".into()));
        }
        if value.len() > 32 {
            return Err(DomainError::Validation("sku is too long".into()));
        }
        if value.chars().any(char::is_control) {
            return Err(DomainError::Validation(
                "sku contains control characters".into(),
            ));
        }
        Ok(Self(value))
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for Sku {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

impl TryFrom<&str> for Sku {
    type Error = DomainError;

    fn try_from(value: &str) -> Result<Self> {
        Self::new(value)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sku_rejects_empty_and_control_characters() {
        assert!(Sku::new("").is_err());
        assert!(Sku::new("  ").is_err());
        assert!(Sku::new("A\nB").is_err());
        assert_eq!(Sku::new(" 123 ").unwrap().as_str(), "123");
    }

    #[test]
    fn ids_are_unique_and_ordered_by_time() {
        let first = SaleId::new();
        let second = SaleId::new();
        assert_ne!(first, second);
        assert!(first.as_uuid() < second.as_uuid());
    }
}
