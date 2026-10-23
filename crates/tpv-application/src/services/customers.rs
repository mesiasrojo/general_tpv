use std::sync::Arc;

use tpv_domain::ids::{CustomerId, StoreId};
use tpv_domain::master_data::MasterDataKind;
use tpv_domain::people::{Address, Customer, validate_spanish_tax_id};

use crate::dto::{SyncContext, UpsertCustomerRequest};
use crate::error::{ApplicationError, Result};
use crate::ports::CatalogRepository;
use crate::services::master_data::MasterDataService;

/// Coordinates customer persistence with the versioned master-data outbox.
#[derive(Clone)]
pub struct CustomerService {
    catalog: Arc<dyn CatalogRepository>,
    master_data: MasterDataService,
}

impl CustomerService {
    pub fn new(catalog: Arc<dyn CatalogRepository>, master_data: MasterDataService) -> Self {
        Self {
            catalog,
            master_data,
        }
    }

    pub async fn list(&self) -> Result<Vec<Customer>> {
        self.catalog.search_customers("", 200).await
    }

    pub async fn get(&self, id: CustomerId) -> Result<Customer> {
        self.catalog
            .find_customer(id)
            .await?
            .ok_or_else(|| ApplicationError::NotFound(format!("customer {id}")))
    }

    pub async fn upsert(
        &self,
        context: &SyncContext,
        store_id: StoreId,
        request: UpsertCustomerRequest,
    ) -> Result<Customer> {
        let first_name = request.first_name.trim().to_string();
        let last_name1 = request.last_name1.trim().to_string();
        if first_name.is_empty() || last_name1.is_empty() {
            return Err(ApplicationError::Validation(
                "customer name and first surname are required".into(),
            ));
        }
        let mut customer = match request.customer_id {
            Some(id) => self.get(id).await?,
            None => Customer::new(CustomerId::new(), first_name.clone(), last_name1.clone())?,
        };
        customer.first_name = first_name;
        customer.last_name1 = last_name1;
        customer.last_name2 = clean(request.last_name2);
        customer.nif = clean(request.nif);
        if let Some(nif) = &customer.nif
            && !validate_spanish_tax_id(nif)
        {
            return Err(ApplicationError::Validation(format!(
                "invalid Spanish tax id {nif}"
            )));
        }
        customer.phone1 = clean(request.phone);
        customer.email = clean(request.email);
        let has_address = request.street.is_some()
            || request.number.is_some()
            || request.locality.is_some()
            || request.postal_code.is_some()
            || request.province.is_some();
        if has_address {
            customer.address = Address {
                street: clean(request.street).unwrap_or_default(),
                number: clean(request.number).unwrap_or_default(),
                locality: clean(request.locality).unwrap_or_default(),
                postal_code: clean(request.postal_code).unwrap_or_default(),
                province: clean(request.province).unwrap_or_default(),
            };
        }
        customer.active = request.active;

        self.catalog.save_customer(&customer).await?;
        let payload = serde_json::to_value(&customer)
            .map_err(|error| ApplicationError::Serialization(error.to_string()))?;
        self.master_data
            .publish_change(
                context,
                store_id,
                MasterDataKind::Customer,
                customer.id.as_uuid(),
                payload,
                false,
            )
            .await?;
        Ok(customer)
    }
}

fn clean(value: Option<String>) -> Option<String> {
    value
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}
