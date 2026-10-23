use std::str::FromStr;

use rust_decimal::Decimal;
use serde::Serialize;
use serde::de::DeserializeOwned;
use time::OffsetDateTime;
use time::format_description::well_known::Rfc3339;
use uuid::Uuid;

use crate::error::{StorageError, StorageResult};

pub(crate) fn format_time(value: OffsetDateTime) -> StorageResult<String> {
    value
        .format(&Rfc3339)
        .map_err(|error| StorageError::InvalidData(error.to_string()))
}

pub(crate) fn parse_time(value: &str) -> StorageResult<OffsetDateTime> {
    OffsetDateTime::parse(value, &Rfc3339)
        .map_err(|error| StorageError::InvalidData(format!("timestamp {value}: {error}")))
}

pub(crate) fn parse_optional_time(value: Option<String>) -> StorageResult<Option<OffsetDateTime>> {
    value.as_deref().map(parse_time).transpose()
}

pub(crate) fn parse_decimal(value: &str) -> StorageResult<Decimal> {
    Decimal::from_str(value)
        .map_err(|error| StorageError::InvalidData(format!("decimal {value}: {error}")))
}

pub(crate) fn parse_uuid(value: &str) -> StorageResult<Uuid> {
    Uuid::parse_str(value)
        .map_err(|error| StorageError::InvalidData(format!("uuid {value}: {error}")))
}

pub(crate) fn enum_to_text<T: Serialize>(value: &T) -> StorageResult<String> {
    let encoded = serde_json::to_string(value)?;
    Ok(encoded.trim_matches('"').to_string())
}

pub(crate) fn enum_from_text<T: DeserializeOwned>(value: &str) -> StorageResult<T> {
    let encoded = format!("\"{value}\"");
    Ok(serde_json::from_str(&encoded)?)
}

pub(crate) fn i64_to_u64(value: i64, field: &str) -> StorageResult<u64> {
    u64::try_from(value)
        .map_err(|_| StorageError::InvalidData(format!("{field} cannot be negative")))
}
