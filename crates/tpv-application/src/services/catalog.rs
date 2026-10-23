use std::sync::Arc;

use tpv_domain::catalog::Product;
use tpv_domain::ids::{CustomerId, StoreId};
use tpv_domain::people::{Customer, Seller, Store};

use crate::dto::ProductSearchResult;
use crate::error::{ApplicationError, Result};
use crate::ports::CatalogRepository;

#[derive(Clone)]
pub struct CatalogService {
    repo: Arc<dyn CatalogRepository>,
}

impl CatalogService {
    pub fn new(repo: Arc<dyn CatalogRepository>) -> Self {
        Self { repo }
    }

    pub async fn search_products(&self, query: &str) -> Result<Vec<ProductSearchResult>> {
        let products = self.repo.search_products(query, 50).await?;
        Ok(products
            .iter()
            .map(ProductSearchResult::from_product)
            .collect())
    }

    pub async fn list_products(&self) -> Result<Vec<Product>> {
        self.repo.list_products().await
    }

    pub async fn get_product(&self, sku: &str) -> Result<Product> {
        let sku = tpv_domain::Sku::new(sku)?;
        self.repo
            .find_product_by_sku(&sku)
            .await?
            .ok_or_else(|| ApplicationError::NotFound(format!("product {sku}")))
    }

    pub async fn upsert_product(&self, product: &Product) -> Result<()> {
        self.repo.save_product(product).await
    }

    pub async fn search_customers(&self, query: &str) -> Result<Vec<Customer>> {
        self.repo.search_customers(query, 50).await
    }

    pub async fn get_customer(&self, id: CustomerId) -> Result<Customer> {
        self.repo
            .find_customer(id)
            .await?
            .ok_or_else(|| ApplicationError::NotFound(format!("customer {id}")))
    }

    pub async fn upsert_customer(&self, customer: &Customer) -> Result<()> {
        self.repo.save_customer(customer).await
    }

    pub async fn list_stores(&self) -> Result<Vec<Store>> {
        self.repo.list_stores().await
    }

    pub async fn get_store(&self, id: StoreId) -> Result<Store> {
        self.repo
            .find_store(id)
            .await?
            .ok_or_else(|| ApplicationError::NotFound(format!("store {id}")))
    }

    pub async fn upsert_store(&self, store: &Store) -> Result<()> {
        self.repo.save_store(store).await
    }

    pub async fn list_sellers(&self) -> Result<Vec<Seller>> {
        self.repo.list_sellers().await
    }

    pub async fn upsert_seller(&self, seller: &Seller) -> Result<()> {
        self.repo.save_seller(seller).await
    }
}
