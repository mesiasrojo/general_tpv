use std::sync::Arc;

use tpv_domain::ids::StoreId;
use tpv_domain::master_data::MasterDataKind;
use tpv_domain::tax::Tax;

use crate::dto::{SyncContext, UpsertTaxRequest};
use crate::error::{ApplicationError, Result};
use crate::ports::CatalogRepository;
use crate::services::master_data::MasterDataService;

/// Manages VAT master records and publishes their master-data versions.
#[derive(Clone)]
pub struct TaxService {
    catalog: Arc<dyn CatalogRepository>,
    master_data: MasterDataService,
}

impl TaxService {
    pub fn new(catalog: Arc<dyn CatalogRepository>, master_data: MasterDataService) -> Self {
        Self {
            catalog,
            master_data,
        }
    }

    pub async fn list(&self) -> Result<Vec<Tax>> {
        self.catalog.list_taxes().await
    }

    pub async fn upsert(
        &self,
        context: &SyncContext,
        store_id: StoreId,
        request: UpsertTaxRequest,
    ) -> Result<Tax> {
        let tax_id = request.tax_id.unwrap_or_default();
        let mut tax = Tax::new(tax_id, request.code, request.name, request.percent_bp)?;
        tax.active = request.active;

        if let Some(existing) = self.catalog.find_tax_by_code(&tax.code).await? {
            if existing.id != tax.id {
                return Err(ApplicationError::Conflict(format!(
                    "tax code {} is already in use",
                    tax.code
                )));
            }
        }

        self.catalog.save_tax(&tax).await?;
        let payload = serde_json::to_value(&tax)
            .map_err(|error| ApplicationError::Serialization(error.to_string()))?;
        self.master_data
            .publish_change(
                context,
                store_id,
                MasterDataKind::Tax,
                tax.id.as_uuid(),
                payload,
                false,
            )
            .await?;
        Ok(tax)
    }
}
