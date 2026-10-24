use std::sync::Arc;

use tpv_domain::ids::StoreId;
use tpv_domain::master_data::MasterDataKind;
use tpv_domain::people::Seller;

use crate::dto::{SyncContext, UpsertSellerRequest};
use crate::error::{ApplicationError, Result};
use crate::ports::CatalogRepository;
use crate::services::master_data::MasterDataService;

/// Manages sellers and publishes their master-data versions.
#[derive(Clone)]
pub struct SellerService {
    catalog: Arc<dyn CatalogRepository>,
    master_data: MasterDataService,
}

impl SellerService {
    pub fn new(catalog: Arc<dyn CatalogRepository>, master_data: MasterDataService) -> Self {
        Self {
            catalog,
            master_data,
        }
    }

    pub async fn list(&self) -> Result<Vec<Seller>> {
        self.catalog.list_sellers().await
    }

    pub async fn upsert(
        &self,
        context: &SyncContext,
        store_id: StoreId,
        request: UpsertSellerRequest,
    ) -> Result<Seller> {
        let seller_id = request.seller_id.unwrap_or_default();
        let mut seller = Seller::new(seller_id, request.code, request.name)?;
        seller.phone1 = clean_optional(request.phone1);
        seller.phone2 = clean_optional(request.phone2);
        seller.active = request.active;

        if let Some(existing) = self.catalog.find_seller_by_code(&seller.code).await? {
            if existing.id != seller.id {
                return Err(ApplicationError::Conflict(format!(
                    "seller code {} is already in use",
                    seller.code
                )));
            }
        }

        self.catalog.save_seller(&seller).await?;
        let payload = serde_json::to_value(&seller)
            .map_err(|error| ApplicationError::Serialization(error.to_string()))?;
        self.master_data
            .publish_change(
                context,
                store_id,
                MasterDataKind::Seller,
                seller.id.as_uuid(),
                payload,
                false,
            )
            .await?;
        Ok(seller)
    }
}

fn clean_optional(value: Option<String>) -> Option<String> {
    value
        .map(|text| text.trim().to_string())
        .filter(|text| !text.is_empty())
}
