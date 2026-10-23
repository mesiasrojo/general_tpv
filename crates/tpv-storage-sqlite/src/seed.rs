use rust_decimal::Decimal;
use tpv_domain::catalog::Product;
use tpv_domain::ids::{CustomerId, RegisterId, SellerId, StoreId, UserId};
use tpv_domain::people::{Address, Customer, Seller, Store};
use tpv_domain::{Sku, TaxRate};
use uuid::Uuid;

use crate::error::StorageResult;
use crate::store::SqliteStore;

#[derive(Debug, Clone, Copy)]
pub struct DemoData {
    pub store_id: StoreId,
    pub register_id: RegisterId,
    pub seller_id: SellerId,
    pub customer_id: CustomerId,
    pub user_id: UserId,
}

impl SqliteStore {
    /// Seeds a realistic demo catalogue and store. Safe to call more than once:
    /// unique codes and SKUs are reused.
    pub async fn seed_demo(&self) -> StorageResult<DemoData> {
        let store_id = self.ensure_store().await?;
        let register_id = self.ensure_register(store_id).await?;
        let seller_id = self.ensure_seller().await?;
        let customer_id = self.ensure_customer().await?;
        let user_id = self.ensure_user(store_id).await?;
        self.ensure_products().await?;
        Ok(DemoData {
            store_id,
            register_id,
            seller_id,
            customer_id,
            user_id,
        })
    }

    async fn ensure_store(&self) -> StorageResult<StoreId> {
        if let Some(id) =
            sqlx::query_scalar::<_, String>("SELECT id FROM stores WHERE code = 'BEN01'")
                .fetch_optional(self.pool())
                .await?
        {
            return Ok(StoreId::from_uuid(Uuid::parse_str(&id).map_err(
                |error| crate::error::StorageError::InvalidData(error.to_string()),
            )?));
        }
        let mut store = Store::new(StoreId::new(), "BEN01", "100%SHOPPING", "B12345674")?;
        store.phone = Some("952574066".into());
        store.address = Address {
            street: "Paseo Marítimo Puerto Marina".into(),
            number: "Local 1".into(),
            locality: "Benalmádena".into(),
            postal_code: "29630".into(),
            province: "Málaga".into(),
        };
        let id = store.id;
        self.persist_store(&store).await?;
        Ok(id)
    }

    async fn ensure_register(&self, store_id: StoreId) -> StorageResult<RegisterId> {
        if let Some(id) = sqlx::query_scalar::<_, String>(
            "SELECT id FROM registers WHERE store_id = ? AND code = 'CAJA1'",
        )
        .bind(store_id.to_string())
        .fetch_optional(self.pool())
        .await?
        {
            return Ok(RegisterId::from_uuid(Uuid::parse_str(&id).map_err(
                |error| crate::error::StorageError::InvalidData(error.to_string()),
            )?));
        }
        let id = RegisterId::new();
        sqlx::query(
            "INSERT INTO registers (id, store_id, code, name) VALUES (?, ?, 'CAJA1', 'Caja 1')",
        )
        .bind(id.to_string())
        .bind(store_id.to_string())
        .execute(self.pool())
        .await?;
        Ok(id)
    }

    async fn ensure_seller(&self) -> StorageResult<SellerId> {
        if let Some(id) = sqlx::query_scalar::<_, String>("SELECT id FROM sellers WHERE code = '1'")
            .fetch_optional(self.pool())
            .await?
        {
            return Ok(SellerId::from_uuid(Uuid::parse_str(&id).map_err(
                |error| crate::error::StorageError::InvalidData(error.to_string()),
            )?));
        }
        let mut seller = Seller::new(SellerId::new(), "1", "Rafi")?;
        seller.phone1 = Some("600000001".into());
        let id = seller.id;
        self.persist_seller(&seller).await?;
        Ok(id)
    }

    async fn ensure_customer(&self) -> StorageResult<CustomerId> {
        if let Some(id) =
            sqlx::query_scalar::<_, String>("SELECT id FROM customers WHERE nif = '12345678Z'")
                .fetch_optional(self.pool())
                .await?
        {
            return Ok(CustomerId::from_uuid(Uuid::parse_str(&id).map_err(
                |error| crate::error::StorageError::InvalidData(error.to_string()),
            )?));
        }
        let mut customer = Customer::new(CustomerId::new(), "Ana", "García")?;
        customer.nif = Some("12345678Z".into());
        customer.phone1 = Some("600000002".into());
        customer.email = Some("ana@example.test".into());
        customer.address = Address {
            street: "Calle Mayor".into(),
            number: "10".into(),
            locality: "Benalmádena".into(),
            postal_code: "29630".into(),
            province: "Málaga".into(),
        };
        let id = customer.id;
        self.persist_customer(&customer).await?;
        Ok(id)
    }

    async fn ensure_user(&self, store_id: StoreId) -> StorageResult<UserId> {
        if let Some(id) =
            sqlx::query_scalar::<_, String>("SELECT id FROM users WHERE username = 'admin'")
                .fetch_optional(self.pool())
                .await?
        {
            return Ok(UserId::from_uuid(Uuid::parse_str(&id).map_err(
                |error| crate::error::StorageError::InvalidData(error.to_string()),
            )?));
        }
        let id = UserId::new();
        sqlx::query(
            "INSERT INTO users (id, username, display_name, role) VALUES (?, 'admin', 'Administrador', 'admin')",
        )
        .bind(id.to_string())
        .execute(self.pool())
        .await?;
        sqlx::query("INSERT INTO user_stores (user_id, store_id) VALUES (?, ?)")
            .bind(id.to_string())
            .bind(store_id.to_string())
            .execute(self.pool())
            .await?;
        Ok(id)
    }

    async fn ensure_products(&self) -> StorageResult<()> {
        let products = [
            (
                "1",
                "Bolsa plástico",
                Decimal::new(10, 2),
                TaxRate::standard(),
            ),
            (
                "2",
                "Agua mineral 50cl",
                Decimal::new(50, 2),
                TaxRate::reduced(),
            ),
            ("3", "Café solo", Decimal::new(120, 2), TaxRate::reduced()),
            (
                "4",
                "Camiseta básica",
                Decimal::new(1200, 2),
                TaxRate::standard(),
            ),
        ];
        for (sku, name, price, tax_rate) in products {
            let existing: Option<String> =
                sqlx::query_scalar("SELECT id FROM products WHERE sku = ?")
                    .bind(sku)
                    .fetch_optional(self.pool())
                    .await?;
            if existing.is_some() {
                continue;
            }
            let product = Product::new(
                tpv_domain::ids::ProductId::new(),
                Sku::new(sku)?,
                name,
                price,
                tax_rate,
            )?;
            self.persist_product(&product).await?;
        }
        Ok(())
    }
}
