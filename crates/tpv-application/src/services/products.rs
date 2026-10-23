use std::str::FromStr;
use std::sync::Arc;

use rust_decimal::Decimal;
use tpv_domain::catalog::Product;
use tpv_domain::ids::{Sku, StoreId};
use tpv_domain::master_data::MasterDataKind;
use tpv_domain::tax::TaxRate;

use crate::dto::{SyncContext, UpsertProductRequest};
use crate::error::{ApplicationError, Result};
use crate::ports::CatalogRepository;
use crate::services::master_data::MasterDataService;

/// Coordinates catalog persistence with the versioned master-data outbox.
#[derive(Clone)]
pub struct ProductService {
    catalog: Arc<dyn CatalogRepository>,
    master_data: MasterDataService,
}

impl ProductService {
    pub fn new(catalog: Arc<dyn CatalogRepository>, master_data: MasterDataService) -> Self {
        Self {
            catalog,
            master_data,
        }
    }

    pub async fn list(&self) -> Result<Vec<Product>> {
        self.catalog.list_products().await
    }

    pub async fn upsert(
        &self,
        context: &SyncContext,
        store_id: StoreId,
        request: UpsertProductRequest,
    ) -> Result<Product> {
        let product_id = request.product_id.unwrap_or_default();
        let sku = Sku::new(request.sku)?;
        let tax_rate = TaxRate::new(request.tax_code, request.tax_percent_bp)?;
        let unit_price = Decimal::from_str(request.unit_price.trim())
            .map_err(|_| ApplicationError::Validation("invalid unit price".into()))?;
        let mut product = Product::new(product_id, sku, request.name, unit_price, tax_rate)?;
        if let Some(barcode) = request
            .barcode
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty())
        {
            product.set_barcode(barcode)?;
        }
        product.family_code = request
            .family_code
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty());
        product.active = request.active;

        self.catalog.save_product(&product).await?;
        let payload = serde_json::to_value(&product)
            .map_err(|error| ApplicationError::Serialization(error.to_string()))?;
        self.master_data
            .publish_change(
                context,
                store_id,
                MasterDataKind::Product,
                product.id.as_uuid(),
                payload,
                false,
            )
            .await?;
        Ok(product)
    }

    pub async fn set_active(
        &self,
        context: &SyncContext,
        store_id: StoreId,
        product: &Product,
        active: bool,
    ) -> Result<Product> {
        self.upsert(
            context,
            store_id,
            UpsertProductRequest {
                product_id: Some(product.id),
                sku: product.sku.as_str().to_string(),
                barcode: product.barcode.clone(),
                name: product.name.clone(),
                family_code: product.family_code.clone(),
                unit_price: product.unit_price.to_string(),
                tax_code: product.tax_rate.code.clone(),
                tax_percent_bp: product.tax_rate.percent_bp,
                active,
            },
        )
        .await
    }
}
