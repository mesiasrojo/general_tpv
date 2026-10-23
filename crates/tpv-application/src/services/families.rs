use std::sync::Arc;

use tpv_domain::catalog::Family;
use tpv_domain::ids::StoreId;
use tpv_domain::master_data::MasterDataKind;

use crate::dto::{SyncContext, UpsertFamilyRequest};
use crate::error::{ApplicationError, Result};
use crate::ports::CatalogRepository;
use crate::services::master_data::MasterDataService;

/// Manages editable product families and publishes their master-data versions.
#[derive(Clone)]
pub struct FamilyService {
    catalog: Arc<dyn CatalogRepository>,
    master_data: MasterDataService,
}

impl FamilyService {
    pub fn new(catalog: Arc<dyn CatalogRepository>, master_data: MasterDataService) -> Self {
        Self {
            catalog,
            master_data,
        }
    }

    pub async fn list(&self) -> Result<Vec<Family>> {
        self.catalog.list_families().await
    }

    pub async fn upsert(
        &self,
        context: &SyncContext,
        store_id: StoreId,
        request: UpsertFamilyRequest,
    ) -> Result<Family> {
        let family_id = request.family_id.unwrap_or_default();
        let mut family = Family::new(family_id, request.code, request.name)?;
        family.active = request.active;

        if let Some(existing) = self.catalog.find_family_by_code(&family.code).await? {
            if existing.id != family.id {
                return Err(ApplicationError::Conflict(format!(
                    "family code {} is already in use",
                    family.code
                )));
            }
        }

        self.catalog.save_family(&family).await?;
        let payload = serde_json::to_value(&family)
            .map_err(|error| ApplicationError::Serialization(error.to_string()))?;
        self.master_data
            .publish_change(
                context,
                store_id,
                MasterDataKind::Family,
                family.id.as_uuid(),
                payload,
                false,
            )
            .await?;
        Ok(family)
    }
}
