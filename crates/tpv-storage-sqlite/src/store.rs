use std::io::Read;
use std::path::Path;
use std::time::Duration;

use async_trait::async_trait;
use sqlx::sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePoolOptions, SqliteRow};
use sqlx::{Row, SqlitePool};
use tpv_application::{CatalogRepository, Result as AppResult};
use tpv_domain::catalog::Product;
use tpv_domain::ids::{CustomerId, ProductId, SellerId, StoreId};
use tpv_domain::people::{Address, Customer, Seller, Sex, Store};
use tpv_domain::{Sku, TaxRate};
use uuid::Uuid;

use crate::db_key::DatabaseKey;
use crate::error::{StorageError, StorageResult};
use crate::helpers::{enum_from_text, enum_to_text, parse_decimal, parse_uuid};

const SQLITE_HEADER: &[u8; 16] = b"SQLite format 3\0";

#[derive(Clone)]
pub struct SqliteStore {
    pool: SqlitePool,
}

impl SqliteStore {
    pub async fn connect(path: &str, key: &DatabaseKey) -> StorageResult<Self> {
        if let Some(parent) = Path::new(path).parent() {
            std::fs::create_dir_all(parent)?;
        }
        migrate_plaintext_database(path, key).await?;
        Self::open(encrypted_options(path, key), 5).await
    }

    pub async fn in_memory() -> StorageResult<Self> {
        Self::in_memory_with_key(&DatabaseKey::generate()).await
    }

    pub async fn in_memory_with_key(key: &DatabaseKey) -> StorageResult<Self> {
        let options = SqliteConnectOptions::new()
            .filename(":memory:")
            .foreign_keys(true)
            .pragma("key", key.pragma_value());
        Self::open(options, 1).await
    }

    async fn open(options: SqliteConnectOptions, max_connections: u32) -> StorageResult<Self> {
        let pool = open_pool(options, max_connections).await?;
        let store = Self { pool };
        store.migrate().await?;
        Ok(store)
    }

    pub fn pool(&self) -> &SqlitePool {
        &self.pool
    }

    async fn migrate(&self) -> StorageResult<()> {
        sqlx::migrate!("./migrations").run(&self.pool).await?;
        Ok(())
    }
}

fn encrypted_options(path: &str, key: &DatabaseKey) -> SqliteConnectOptions {
    SqliteConnectOptions::new()
        .filename(path)
        .create_if_missing(true)
        .journal_mode(SqliteJournalMode::Wal)
        .foreign_keys(true)
        .busy_timeout(Duration::from_secs(5))
        .pragma("key", key.pragma_value())
}

async fn open_pool(
    options: SqliteConnectOptions,
    max_connections: u32,
) -> StorageResult<SqlitePool> {
    Ok(SqlitePoolOptions::new()
        .max_connections(max_connections)
        .connect_with(options)
        .await?)
}

/// Upgrades a legacy plaintext SQLite database to SQLCipher in place.
///
/// The plaintext file is only removed after the encrypted copy has been
/// exported and successfully reopened with the key.
async fn migrate_plaintext_database(path: &str, key: &DatabaseKey) -> StorageResult<()> {
    let path_ref = Path::new(path);
    if !path_ref.exists() {
        return Ok(());
    }
    let mut file = std::fs::File::open(path_ref)?;
    let mut header = [0_u8; 16];
    let read = file.read(&mut header)?;
    drop(file);
    if read != header.len() || &header != SQLITE_HEADER {
        // Already encrypted, empty, or not a SQLite database.
        return Ok(());
    }

    let encrypted_path = path_ref.with_extension(format!("sqlcipher-{}", Uuid::new_v4()));
    let plaintext_pool = open_pool(
        SqliteConnectOptions::new()
            .filename(path)
            .create_if_missing(true)
            .foreign_keys(true),
        1,
    )
    .await?;
    // Include any committed pages that still live in the WAL file.
    sqlx::query("PRAGMA wal_checkpoint(TRUNCATE)")
        .execute(&plaintext_pool)
        .await?;
    let escaped_path = encrypted_path.to_string_lossy().replace('\'', "''");
    let attach = format!(
        "ATTACH DATABASE '{escaped_path}' AS encrypted KEY \"x'{}'\";",
        key.as_hex()
    );
    sqlx::query(&attach).execute(&plaintext_pool).await?;
    sqlx::query("SELECT sqlcipher_export('encrypted');")
        .execute(&plaintext_pool)
        .await?;
    sqlx::query("DETACH DATABASE encrypted;")
        .execute(&plaintext_pool)
        .await?;
    plaintext_pool.close().await;

    let encrypted_str = encrypted_path.to_str().ok_or_else(|| {
        StorageError::InvalidData("encrypted database path is not valid UTF-8".into())
    })?;
    let verify_pool = open_pool(encrypted_options(encrypted_str, key), 1).await?;
    let row = sqlx::query("SELECT count(*) AS tables FROM sqlite_master")
        .fetch_one(&verify_pool)
        .await?;
    let _: i64 = row.try_get("tables")?;
    verify_pool.close().await;

    std::fs::remove_file(path_ref)?;
    std::fs::rename(&encrypted_path, path_ref)?;
    let _ = std::fs::remove_file(format!("{path}-wal"));
    let _ = std::fs::remove_file(format!("{path}-shm"));
    Ok(())
}

fn row_to_product(row: &SqliteRow) -> StorageResult<Product> {
    let tax_rate = TaxRate::new(
        row.try_get::<String, _>("tax_code")?,
        u32::try_from(row.try_get::<i64, _>("tax_percent_bp")?)
            .map_err(|_| StorageError::InvalidData("invalid tax percentage".into()))?,
    )?;
    Ok(Product {
        id: ProductId::from_uuid(parse_uuid(&row.try_get::<String, _>("id")?)?),
        sku: Sku::new(row.try_get::<String, _>("sku")?)?,
        barcode: row.try_get("barcode")?,
        name: row.try_get("name")?,
        family_code: row.try_get("family_code")?,
        unit_price: parse_decimal(&row.try_get::<String, _>("unit_price")?)?,
        tax_rate,
        active: row.try_get::<i64, _>("active")? != 0,
    })
}

fn row_to_customer(row: &SqliteRow) -> StorageResult<Customer> {
    let sex = row
        .try_get::<Option<String>, _>("sex")?
        .map(|value| enum_from_text::<Sex>(&value))
        .transpose()?;
    Ok(Customer {
        id: CustomerId::from_uuid(parse_uuid(&row.try_get::<String, _>("id")?)?),
        first_name: row.try_get("first_name")?,
        last_name1: row.try_get("last_name1")?,
        last_name2: row.try_get("last_name2")?,
        sex,
        phone1: row.try_get("phone1")?,
        phone2: row.try_get("phone2")?,
        email: row.try_get("email")?,
        address: Address {
            street: row.try_get("street")?,
            number: row.try_get("number")?,
            locality: row.try_get("locality")?,
            postal_code: row.try_get("postal_code")?,
            province: row.try_get("province")?,
        },
        nif: row.try_get("nif")?,
        profession: row.try_get("profession")?,
        notes: row.try_get("notes")?,
        active: row.try_get::<i64, _>("active")? != 0,
    })
}

fn row_to_store(row: &SqliteRow) -> StorageResult<Store> {
    Ok(Store {
        id: StoreId::from_uuid(parse_uuid(&row.try_get::<String, _>("id")?)?),
        code: row.try_get("code")?,
        name: row.try_get("name")?,
        tax_id: row.try_get("tax_id")?,
        phone: row.try_get("phone")?,
        address: Address {
            street: row.try_get("street")?,
            number: row.try_get("number")?,
            locality: row.try_get("locality")?,
            postal_code: row.try_get("postal_code")?,
            province: row.try_get("province")?,
        },
        active: row.try_get::<i64, _>("active")? != 0,
    })
}

fn row_to_seller(row: &SqliteRow) -> StorageResult<Seller> {
    Ok(Seller {
        id: SellerId::from_uuid(parse_uuid(&row.try_get::<String, _>("id")?)?),
        code: row.try_get("code")?,
        name: row.try_get("name")?,
        phone1: row.try_get("phone1")?,
        phone2: row.try_get("phone2")?,
        active: row.try_get::<i64, _>("active")? != 0,
    })
}

impl SqliteStore {
    async fn get_product_by_sku(&self, sku: &Sku) -> StorageResult<Option<Product>> {
        let row = sqlx::query("SELECT * FROM products WHERE sku = ?")
            .bind(sku.as_str())
            .fetch_optional(&self.pool)
            .await?;
        row.as_ref().map(row_to_product).transpose()
    }

    async fn search_product_rows(&self, query: &str, limit: u32) -> StorageResult<Vec<Product>> {
        let pattern = format!("%{}%", query.trim().to_lowercase());
        let rows = sqlx::query(
            "SELECT * FROM products
             WHERE active = 1
               AND (lower(name) LIKE ? OR lower(sku) LIKE ? OR barcode = ?)
             ORDER BY name LIMIT ?",
        )
        .bind(&pattern)
        .bind(&pattern)
        .bind(query.trim())
        .bind(i64::from(limit))
        .fetch_all(&self.pool)
        .await?;
        rows.iter().map(row_to_product).collect()
    }

    async fn all_products(&self) -> StorageResult<Vec<Product>> {
        let rows = sqlx::query("SELECT * FROM products WHERE active = 1 ORDER BY name")
            .fetch_all(&self.pool)
            .await?;
        rows.iter().map(row_to_product).collect()
    }

    pub(crate) async fn persist_product(&self, product: &Product) -> StorageResult<()> {
        sqlx::query(
            "INSERT INTO products
                (id, sku, barcode, name, family_code, unit_price, tax_code, tax_percent_bp, active)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)
             ON CONFLICT(id) DO UPDATE SET
                sku = excluded.sku,
                barcode = excluded.barcode,
                name = excluded.name,
                family_code = excluded.family_code,
                unit_price = excluded.unit_price,
                tax_code = excluded.tax_code,
                tax_percent_bp = excluded.tax_percent_bp,
                active = excluded.active",
        )
        .bind(product.id.to_string())
        .bind(product.sku.as_str())
        .bind(product.barcode.as_deref())
        .bind(&product.name)
        .bind(product.family_code.as_deref())
        .bind(product.unit_price.to_string())
        .bind(&product.tax_rate.code)
        .bind(i64::from(product.tax_rate.percent_bp))
        .bind(i64::from(product.active))
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    async fn get_customer_by_id(&self, id: CustomerId) -> StorageResult<Option<Customer>> {
        let row = sqlx::query("SELECT * FROM customers WHERE id = ?")
            .bind(id.to_string())
            .fetch_optional(&self.pool)
            .await?;
        row.as_ref().map(row_to_customer).transpose()
    }

    async fn search_customer_rows(&self, query: &str, limit: u32) -> StorageResult<Vec<Customer>> {
        let pattern = format!("%{}%", query.trim().to_lowercase());
        let rows = sqlx::query(
            "SELECT * FROM customers
             WHERE active = 1
               AND (lower(first_name) LIKE ? OR lower(last_name1) LIKE ?
                    OR lower(last_name2) LIKE ? OR nif = ? OR phone1 LIKE ?)
             ORDER BY last_name1, first_name LIMIT ?",
        )
        .bind(&pattern)
        .bind(&pattern)
        .bind(&pattern)
        .bind(query.trim())
        .bind(&pattern)
        .bind(i64::from(limit))
        .fetch_all(&self.pool)
        .await?;
        rows.iter().map(row_to_customer).collect()
    }

    pub(crate) async fn persist_customer(&self, customer: &Customer) -> StorageResult<()> {
        sqlx::query(
            "INSERT INTO customers
                (id, first_name, last_name1, last_name2, sex, phone1, phone2, email,
                 street, number, locality, postal_code, province, nif, profession, notes, active)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
             ON CONFLICT(id) DO UPDATE SET
                first_name = excluded.first_name,
                last_name1 = excluded.last_name1,
                last_name2 = excluded.last_name2,
                sex = excluded.sex,
                phone1 = excluded.phone1,
                phone2 = excluded.phone2,
                email = excluded.email,
                street = excluded.street,
                number = excluded.number,
                locality = excluded.locality,
                postal_code = excluded.postal_code,
                province = excluded.province,
                nif = excluded.nif,
                profession = excluded.profession,
                notes = excluded.notes,
                active = excluded.active",
        )
        .bind(customer.id.to_string())
        .bind(&customer.first_name)
        .bind(&customer.last_name1)
        .bind(customer.last_name2.as_deref())
        .bind(customer.sex.as_ref().map(enum_to_text).transpose()?)
        .bind(customer.phone1.as_deref())
        .bind(customer.phone2.as_deref())
        .bind(customer.email.as_deref())
        .bind(&customer.address.street)
        .bind(&customer.address.number)
        .bind(&customer.address.locality)
        .bind(&customer.address.postal_code)
        .bind(&customer.address.province)
        .bind(customer.nif.as_deref())
        .bind(customer.profession.as_deref())
        .bind(customer.notes.as_deref())
        .bind(i64::from(customer.active))
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    async fn all_stores(&self) -> StorageResult<Vec<Store>> {
        let rows = sqlx::query("SELECT * FROM stores WHERE active = 1 ORDER BY name")
            .fetch_all(&self.pool)
            .await?;
        rows.iter().map(row_to_store).collect()
    }

    async fn get_store_by_id(&self, id: StoreId) -> StorageResult<Option<Store>> {
        let row = sqlx::query("SELECT * FROM stores WHERE id = ?")
            .bind(id.to_string())
            .fetch_optional(&self.pool)
            .await?;
        row.as_ref().map(row_to_store).transpose()
    }

    pub(crate) async fn persist_store(&self, store: &Store) -> StorageResult<()> {
        sqlx::query(
            "INSERT INTO stores
                (id, code, name, tax_id, phone, street, number, locality, postal_code, province, active)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
             ON CONFLICT(id) DO UPDATE SET
                code = excluded.code,
                name = excluded.name,
                tax_id = excluded.tax_id,
                phone = excluded.phone,
                street = excluded.street,
                number = excluded.number,
                locality = excluded.locality,
                postal_code = excluded.postal_code,
                province = excluded.province,
                active = excluded.active",
        )
        .bind(store.id.to_string())
        .bind(&store.code)
        .bind(&store.name)
        .bind(&store.tax_id)
        .bind(store.phone.as_deref())
        .bind(&store.address.street)
        .bind(&store.address.number)
        .bind(&store.address.locality)
        .bind(&store.address.postal_code)
        .bind(&store.address.province)
        .bind(i64::from(store.active))
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    async fn all_sellers(&self) -> StorageResult<Vec<Seller>> {
        let rows = sqlx::query("SELECT * FROM sellers WHERE active = 1 ORDER BY name")
            .fetch_all(&self.pool)
            .await?;
        rows.iter().map(row_to_seller).collect()
    }

    async fn get_seller_by_id(&self, id: SellerId) -> StorageResult<Option<Seller>> {
        let row = sqlx::query("SELECT * FROM sellers WHERE id = ?")
            .bind(id.to_string())
            .fetch_optional(&self.pool)
            .await?;
        row.as_ref().map(row_to_seller).transpose()
    }

    pub(crate) async fn persist_seller(&self, seller: &Seller) -> StorageResult<()> {
        sqlx::query(
            "INSERT INTO sellers (id, code, name, phone1, phone2, active)
             VALUES (?, ?, ?, ?, ?, ?)
             ON CONFLICT(id) DO UPDATE SET
                code = excluded.code,
                name = excluded.name,
                phone1 = excluded.phone1,
                phone2 = excluded.phone2,
                active = excluded.active",
        )
        .bind(seller.id.to_string())
        .bind(&seller.code)
        .bind(&seller.name)
        .bind(seller.phone1.as_deref())
        .bind(seller.phone2.as_deref())
        .bind(i64::from(seller.active))
        .execute(&self.pool)
        .await?;
        Ok(())
    }
}

#[async_trait]
impl CatalogRepository for SqliteStore {
    async fn find_product_by_sku(&self, sku: &Sku) -> AppResult<Option<Product>> {
        Ok(self.get_product_by_sku(sku).await?)
    }

    async fn search_products(&self, query: &str, limit: u32) -> AppResult<Vec<Product>> {
        Ok(self.search_product_rows(query, limit).await?)
    }

    async fn list_products(&self) -> AppResult<Vec<Product>> {
        Ok(self.all_products().await?)
    }

    async fn save_product(&self, product: &Product) -> AppResult<()> {
        self.persist_product(product).await?;
        Ok(())
    }

    async fn find_customer(&self, id: CustomerId) -> AppResult<Option<Customer>> {
        Ok(self.get_customer_by_id(id).await?)
    }

    async fn search_customers(&self, query: &str, limit: u32) -> AppResult<Vec<Customer>> {
        Ok(self.search_customer_rows(query, limit).await?)
    }

    async fn save_customer(&self, customer: &Customer) -> AppResult<()> {
        self.persist_customer(customer).await?;
        Ok(())
    }

    async fn list_stores(&self) -> AppResult<Vec<Store>> {
        Ok(self.all_stores().await?)
    }

    async fn find_store(&self, id: StoreId) -> AppResult<Option<Store>> {
        Ok(self.get_store_by_id(id).await?)
    }

    async fn save_store(&self, store: &Store) -> AppResult<()> {
        self.persist_store(store).await?;
        Ok(())
    }

    async fn list_sellers(&self) -> AppResult<Vec<Seller>> {
        Ok(self.all_sellers().await?)
    }

    async fn find_seller(&self, id: SellerId) -> AppResult<Option<Seller>> {
        Ok(self.get_seller_by_id(id).await?)
    }

    async fn save_seller(&self, seller: &Seller) -> AppResult<()> {
        self.persist_seller(seller).await?;
        Ok(())
    }
}
