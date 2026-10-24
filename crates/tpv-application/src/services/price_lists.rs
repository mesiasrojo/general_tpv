use std::collections::{HashMap, HashSet};
use std::str::FromStr;
use std::sync::Arc;

use rust_decimal::Decimal;
use tpv_domain::ids::StoreId;
use tpv_domain::master_data::MasterDataKind;
use tpv_domain::pricing::{PriceList, PriceListItem, PriceListSnapshot};

use crate::dto::{SyncContext, UpsertPriceListRequest};
use crate::error::{ApplicationError, Result};
use crate::ports::CatalogRepository;
use crate::services::master_data::MasterDataService;

/// Manages price lists and publishes their complete snapshot as master data.
#[derive(Clone)]
pub struct PriceListService {
    catalog: Arc<dyn CatalogRepository>,
    master_data: MasterDataService,
}

impl PriceListService {
    pub fn new(catalog: Arc<dyn CatalogRepository>, master_data: MasterDataService) -> Self {
        Self {
            catalog,
            master_data,
        }
    }

    pub async fn list(&self) -> Result<Vec<PriceListSnapshot>> {
        self.catalog.list_price_lists().await
    }

    pub async fn upsert(
        &self,
        context: &SyncContext,
        store_id: StoreId,
        request: UpsertPriceListRequest,
    ) -> Result<PriceListSnapshot> {
        let price_list_id = request.price_list_id.unwrap_or_default();
        let mut price_list = PriceList::new(price_list_id, request.code, request.name)?;
        price_list.active = request.active;

        if let Some(existing) = self
            .catalog
            .find_price_list_by_code(&price_list.code)
            .await?
        {
            if existing.id != price_list.id {
                return Err(ApplicationError::Conflict(format!(
                    "price list code {} is already in use",
                    price_list.code
                )));
            }
        }

        let products = self.catalog.list_products().await?;
        let known: HashMap<_, _> = products
            .iter()
            .map(|product| (product.id, product.sku.to_string()))
            .collect();
        let mut seen = HashSet::new();
        let mut items = Vec::with_capacity(request.items.len());
        for item in request.items {
            let Some(sku) = known.get(&item.product_id) else {
                return Err(ApplicationError::Validation(format!(
                    "product {} does not exist",
                    item.product_id
                )));
            };
            if !seen.insert(item.product_id) {
                return Err(ApplicationError::Validation(format!(
                    "product {sku} is repeated in the price list"
                )));
            }
            let price =
                Decimal::from_str(&item.unit_price.trim().replace(',', ".")).map_err(|_| {
                    ApplicationError::Validation(format!("invalid price for product {sku}"))
                })?;
            items.push(PriceListItem::new(item.product_id, price)?);
        }
        let snapshot = PriceListSnapshot::new(price_list, items)?;

        self.catalog.save_price_list(&snapshot).await?;
        let payload = serde_json::to_value(&snapshot)
            .map_err(|error| ApplicationError::Serialization(error.to_string()))?;
        self.master_data
            .publish_change(
                context,
                store_id,
                MasterDataKind::PriceList,
                snapshot.price_list.id.as_uuid(),
                payload,
                false,
            )
            .await?;
        Ok(snapshot)
    }
}
