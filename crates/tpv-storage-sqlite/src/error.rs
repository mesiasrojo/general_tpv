use thiserror::Error;
use tpv_application::ApplicationError;
use tpv_domain::DomainError;

pub type StorageResult<T> = std::result::Result<T, StorageError>;

#[derive(Debug, Error)]
pub enum StorageError {
    #[error(transparent)]
    Sqlx(#[from] sqlx::Error),

    #[error(transparent)]
    Migration(#[from] sqlx::migrate::MigrateError),

    #[error(transparent)]
    Json(#[from] serde_json::Error),

    #[error(transparent)]
    Domain(#[from] DomainError),

    #[error("invalid stored data: {0}")]
    InvalidData(String),

    #[error("not found: {0}")]
    NotFound(String),
}

impl From<StorageError> for ApplicationError {
    fn from(value: StorageError) -> Self {
        Self::Repository(value.to_string())
    }
}
