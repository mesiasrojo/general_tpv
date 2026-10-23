use thiserror::Error;

pub type Result<T> = std::result::Result<T, DomainError>;

#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum DomainError {
    #[error("validation error: {0}")]
    Validation(String),

    #[error("invalid state: {0}")]
    InvalidState(String),

    #[error("money overflow")]
    MoneyOverflow,

    #[error("empty sale")]
    EmptySale,

    #[error("payment exceeds the amount due")]
    PaymentExceedsTotal,

    #[error("payments do not match the sale total")]
    PaymentMismatch,

    #[error("invalid tax rate: {0} basis points")]
    InvalidTaxRate(u32),

    #[error("invalid discount: {0} basis points")]
    InvalidDiscount(u32),

    #[error("invalid quantity")]
    InvalidQuantity,

    #[error("invalid Spanish tax identifier")]
    InvalidTaxId,

    #[error("document series exhausted")]
    SeriesExhausted,

    #[error("not found: {0}")]
    NotFound(String),

    #[error("conflict: {0}")]
    Conflict(String),

    #[error("internal error: {0}")]
    Internal(String),
}
