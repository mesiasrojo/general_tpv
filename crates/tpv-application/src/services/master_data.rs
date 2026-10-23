use std::sync::Arc;

use serde_json::Value;
use tpv_domain::ids::{StoreId, TenantId};
use tpv_domain::master_data::{
    MasterDataApplyResult, MasterDataChange, MasterDataConflict, MasterDataKind,
};
use tpv_domain::sync::SyncEvent;
use uuid::Uuid;

use crate::dto::SyncContext;
use crate::error::Result;
use crate::ports::{Clock, MasterDataRepository};

#[derive(Clone)]
pub struct MasterDataService {
    repository: Arc<dyn MasterDataRepository>,
    clock: Arc<dyn Clock>,
}

impl MasterDataService {
    pub fn new(repository: Arc<dyn MasterDataRepository>, clock: Arc<dyn Clock>) -> Self {
        Self { repository, clock }
    }

    /// Creates a new local version, persists it and appends the corresponding
    /// event to the transactional outbox.
    pub async fn publish_change(
        &self,
        context: &SyncContext,
        store_id: StoreId,
        kind: MasterDataKind,
        entity_id: Uuid,
        payload: Value,
        deleted: bool,
    ) -> Result<SyncEvent> {
        self.repository
            .publish_change(crate::dto::PublishMasterDataRequest {
                context: context.clone(),
                store_id,
                kind,
                entity_id,
                payload,
                deleted,
                occurred_at: self.clock.now(),
            })
            .await
    }

    pub async fn apply_change(
        &self,
        tenant_id: TenantId,
        store_id: StoreId,
        change: &MasterDataChange,
    ) -> Result<MasterDataApplyResult> {
        self.repository
            .apply_change(tenant_id, store_id, change)
            .await
    }

    pub async fn list_conflicts(&self, limit: u32) -> Result<Vec<MasterDataConflict>> {
        self.repository.list_conflicts(limit).await
    }
}
