use std::path::Path;

use async_trait::async_trait;
use tpv_application::{BackupRepository, Result as AppResult};

use crate::error::{StorageError, StorageResult};
use crate::store::SqliteStore;

impl SqliteStore {
    /// Uses SQLCipher's `VACUUM INTO` so the snapshot is transactionally
    /// consistent and inherits the encryption key of the live connection.
    pub async fn backup_to(&self, destination: &Path) -> StorageResult<u64> {
        if destination.exists() {
            return Err(StorageError::InvalidData(format!(
                "backup destination already exists: {}",
                destination.display()
            )));
        }
        if let Some(parent) = destination.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let destination = destination.to_str().ok_or_else(|| {
            StorageError::InvalidData("backup destination is not valid UTF-8".into())
        })?;
        sqlx::query("VACUUM INTO ?")
            .bind(destination)
            .execute(self.pool())
            .await?;
        Ok(std::fs::metadata(destination)?.len())
    }
}

#[async_trait]
impl BackupRepository for SqliteStore {
    async fn backup_to(&self, destination: &Path) -> AppResult<u64> {
        Ok(SqliteStore::backup_to(self, destination).await?)
    }
}
