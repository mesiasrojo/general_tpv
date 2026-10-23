use thiserror::Error;
use tpv_domain::DomainError;

pub type Result<T> = std::result::Result<T, ApplicationError>;

#[derive(Debug, Error)]
pub enum ApplicationError {
    #[error(transparent)]
    Domain(#[from] DomainError),

    #[error("not found: {0}")]
    NotFound(String),

    #[error("conflict: {0}")]
    Conflict(String),

    #[error("validation error: {0}")]
    Validation(String),

    #[error("unauthorized: {0}")]
    Unauthorized(String),

    #[error("internal error: {0}")]
    Internal(String),

    #[error("repository error: {0}")]
    Repository(String),

    #[error("serialization error: {0}")]
    Serialization(String),
}

impl ApplicationError {
    pub fn repository(error: impl std::fmt::Display) -> Self {
        Self::Repository(error.to_string())
    }
}
