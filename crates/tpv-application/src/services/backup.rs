use std::path::Path;
use std::sync::Arc;

use crate::error::Result;
use crate::ports::BackupRepository;

/// Creates consistent encrypted snapshots of the local database.
#[derive(Clone)]
pub struct BackupService {
    repository: Arc<dyn BackupRepository>,
}

impl BackupService {
    pub fn new(repository: Arc<dyn BackupRepository>) -> Self {
        Self { repository }
    }

    pub async fn backup_to(&self, destination: &Path) -> Result<u64> {
        self.repository.backup_to(destination).await
    }
}
