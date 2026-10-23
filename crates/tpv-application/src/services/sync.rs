use std::sync::Arc;

use uuid::Uuid;

use crate::dto::{QuarantinedEvent, SyncStatus};
use crate::error::Result;
use crate::ports::SyncOutboxRepository;

/// Local view and operator actions for the outbox queue.
///
/// The transport worker lives in `tpv-sync`; this service only exposes the
/// durable local state so the CLI, desktop console and UI can report it and
/// retry quarantined events without knowing about SQLite.
#[derive(Clone)]
pub struct SyncService {
    outbox: Arc<dyn SyncOutboxRepository>,
}

impl SyncService {
    pub fn new(outbox: Arc<dyn SyncOutboxRepository>) -> Self {
        Self { outbox }
    }

    pub async fn status(&self) -> Result<SyncStatus> {
        self.outbox.sync_status().await
    }

    pub async fn quarantine(&self, limit: u32) -> Result<Vec<QuarantinedEvent>> {
        self.outbox.list_failed(limit).await
    }

    /// Requeues the given quarantined events. Unknown, already sent or still
    /// pending ids are ignored.
    pub async fn retry_failed(&self, event_ids: &[Uuid]) -> Result<u64> {
        if event_ids.is_empty() {
            return Ok(0);
        }
        self.outbox.requeue_failed(event_ids).await
    }

    /// Requeues up to `limit` quarantined events, newest failures first.
    pub async fn retry_all_failed(&self, limit: u32) -> Result<u64> {
        let failed = self.outbox.list_failed(limit).await?;
        let ids = failed
            .iter()
            .map(|entry| entry.event.event_id)
            .collect::<Vec<_>>();
        self.retry_failed(&ids).await
    }
}
