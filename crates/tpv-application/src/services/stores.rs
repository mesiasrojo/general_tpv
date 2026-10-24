use std::sync::Arc;

use tpv_domain::ids::StoreId;
use tpv_domain::master_data::MasterDataKind;
use tpv_domain::people::{Address, Store};

use crate::dto::{SyncContext, UpsertStoreRequest};
use crate::error::{ApplicationError, Result};
use crate::ports::CatalogRepository;
use crate::services::master_data::MasterDataService;

/// Manages stores and publishes their master-data versions.
#[derive(Clone)]
pub struct StoreService {
    catalog: Arc<dyn CatalogRepository>,
    master_data: MasterDataService,
}

impl StoreService {
    pub fn new(catalog: Arc<dyn CatalogRepository>, master_data: MasterDataService) -> Self {
        Self {
            catalog,
            master_data,
        }
    }

    pub async fn list(&self) -> Result<Vec<Store>> {
        self.catalog.list_stores().await
    }

    pub async fn upsert(
        &self,
        context: &SyncContext,
        store_id: StoreId,
        request: UpsertStoreRequest,
    ) -> Result<Store> {
        let target_id = request.store_id.unwrap_or_default();
        let mut store = Store::new(target_id, request.code, request.name, request.tax_id)?;
        store.phone = clean_optional(request.phone);
        store.address = Address {
            street: request.street.unwrap_or_default().trim().to_string(),
            number: request.number.unwrap_or_default().trim().to_string(),
            locality: request.locality.unwrap_or_default().trim().to_string(),
            postal_code: request.postal_code.unwrap_or_default().trim().to_string(),
            province: request.province.unwrap_or_default().trim().to_string(),
        };
        store.active = request.active;

        if let Some(existing) = self.catalog.find_store_by_code(&store.code).await? {
            if existing.id != store.id {
                return Err(ApplicationError::Conflict(format!(
                    "store code {} is already in use",
                    store.code
                )));
            }
        }

        self.catalog.save_store(&store).await?;
        let payload = serde_json::to_value(&store)
            .map_err(|error| ApplicationError::Serialization(error.to_string()))?;
        self.master_data
            .publish_change(
                context,
                store_id,
                MasterDataKind::Store,
                store.id.as_uuid(),
                payload,
                false,
            )
            .await?;
        Ok(store)
    }
}

fn clean_optional(value: Option<String>) -> Option<String> {
    value
        .map(|text| text.trim().to_string())
        .filter(|text| !text.is_empty())
}
