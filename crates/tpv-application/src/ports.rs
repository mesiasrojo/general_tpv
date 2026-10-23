use async_trait::async_trait;
use time::OffsetDateTime;
use tpv_domain::Sku;
use tpv_domain::audit::AuditEvent;
use tpv_domain::cash::{CashCount, CashSession};
use tpv_domain::catalog::Product;
use tpv_domain::document::DocumentType;
use tpv_domain::ids::{CashSessionId, CustomerId, InvoiceId, RegisterId, SaleId, StoreId};
use tpv_domain::invoice::Invoice;
use tpv_domain::people::{Customer, Seller, Store};
use tpv_domain::sale::Sale;
use tpv_domain::sync::SyncEvent;

use crate::error::Result;

/// Deterministic clock port. Tests can supply a fixed implementation.
pub trait Clock: Send + Sync {
    fn now(&self) -> OffsetDateTime;
}

#[derive(Debug, Clone, Copy, Default)]
pub struct SystemClock;

impl Clock for SystemClock {
    fn now(&self) -> OffsetDateTime {
        OffsetDateTime::now_utc()
    }
}

#[async_trait]
pub trait CatalogRepository: Send + Sync {
    async fn find_product_by_sku(&self, sku: &Sku) -> Result<Option<Product>>;
    async fn search_products(&self, query: &str, limit: u32) -> Result<Vec<Product>>;
    async fn list_products(&self) -> Result<Vec<Product>>;
    async fn save_product(&self, product: &Product) -> Result<()>;

    async fn find_customer(&self, id: CustomerId) -> Result<Option<Customer>>;
    async fn search_customers(&self, query: &str, limit: u32) -> Result<Vec<Customer>>;
    async fn save_customer(&self, customer: &Customer) -> Result<()>;

    async fn list_stores(&self) -> Result<Vec<Store>>;
    async fn find_store(&self, id: StoreId) -> Result<Option<Store>>;
    async fn save_store(&self, store: &Store) -> Result<()>;

    async fn list_sellers(&self) -> Result<Vec<Seller>>;
    async fn find_seller(&self, id: tpv_domain::ids::SellerId) -> Result<Option<Seller>>;
    async fn save_seller(&self, seller: &Seller) -> Result<()>;
}

#[async_trait]
pub trait SaleRepository: Send + Sync {
    async fn save_draft(&self, sale: &Sale) -> Result<()>;
    async fn update_draft(&self, sale: &Sale) -> Result<()>;
    async fn find(&self, id: SaleId) -> Result<Option<Sale>>;
    async fn confirm_sale(&self, sale: &Sale, event: &SyncEvent, audit: &AuditEvent) -> Result<()>;
    async fn list_recent(&self, store_id: StoreId, limit: u32) -> Result<Vec<Sale>>;
    async fn payment_totals_since(
        &self,
        store_id: StoreId,
        register_id: RegisterId,
        since: OffsetDateTime,
    ) -> Result<CashCount>;
}

#[async_trait]
pub trait InvoiceRepository: Send + Sync {
    async fn save_invoice(
        &self,
        invoice: &Invoice,
        sale: Option<&Sale>,
        event: &SyncEvent,
        audit: &AuditEvent,
    ) -> Result<()>;
    async fn find(&self, id: InvoiceId) -> Result<Option<Invoice>>;
    async fn list_recent(&self, store_id: StoreId, limit: u32) -> Result<Vec<Invoice>>;
}

#[async_trait]
pub trait SequenceRepository: Send + Sync {
    async fn next_number(
        &self,
        store_id: StoreId,
        document_type: DocumentType,
        series: &str,
        year: u16,
    ) -> Result<u64>;
}

#[async_trait]
pub trait CashRepository: Send + Sync {
    async fn save(
        &self,
        session: &CashSession,
        event: Option<&SyncEvent>,
        audit: &AuditEvent,
    ) -> Result<()>;
    async fn find(&self, id: CashSessionId) -> Result<Option<CashSession>>;
    async fn find_open(&self, register_id: RegisterId) -> Result<Option<CashSession>>;
    async fn list_recent(&self, store_id: StoreId, limit: u32) -> Result<Vec<CashSession>>;
}

#[async_trait]
pub trait AuditRepository: Send + Sync {
    async fn append(&self, audit: &AuditEvent) -> Result<()>;
    async fn list_recent(&self, store_id: Option<StoreId>, limit: u32) -> Result<Vec<AuditEvent>>;
}

#[async_trait]
pub trait SyncOutboxRepository: Send + Sync {
    async fn pending_events(&self, limit: u32) -> Result<Vec<SyncEvent>>;
    async fn mark_events_sent(&self, event_ids: &[uuid::Uuid]) -> Result<()>;
}

pub trait TicketRenderer: Send + Sync {
    fn render_ticket(&self, document: &crate::dto::TicketDocument) -> Result<Vec<u8>>;
}

#[async_trait]
pub trait PrinterPort: Send + Sync {
    async fn print(&self, job: &crate::dto::PrintJob) -> Result<()>;
}

#[async_trait]
pub trait PrintQueueRepository: Send + Sync {
    async fn enqueue(&self, job: &crate::dto::PrintJob) -> Result<()>;
    async fn pending(&self, limit: u32) -> Result<Vec<crate::dto::PrintJob>>;
    async fn mark_printed(&self, id: tpv_domain::ids::PrintJobId) -> Result<()>;
    async fn mark_failed(&self, id: tpv_domain::ids::PrintJobId, error: &str) -> Result<()>;
    async fn list_recent(&self, limit: u32) -> Result<Vec<crate::dto::PrintJob>>;
}

#[derive(Debug, Clone)]
pub struct StoredUser {
    pub account: tpv_domain::people::UserAccount,
    pub password_hash: Option<String>,
    pub failed_attempts: u32,
    pub locked_until: Option<time::OffsetDateTime>,
    pub last_login_at: Option<time::OffsetDateTime>,
}

pub trait PasswordHasher: Send + Sync {
    fn hash(&self, password: &str) -> Result<String>;
    fn verify(&self, password: &str, hash: &str) -> Result<bool>;
}

#[async_trait]
pub trait UserRepository: Send + Sync {
    async fn find_by_username(&self, username: &str) -> Result<Option<StoredUser>>;
    async fn find_by_id(&self, id: tpv_domain::ids::UserId) -> Result<Option<StoredUser>>;
    async fn create_user(
        &self,
        user: &tpv_domain::people::UserAccount,
        password_hash: &str,
    ) -> Result<()>;
    async fn set_password_hash(
        &self,
        id: tpv_domain::ids::UserId,
        password_hash: &str,
    ) -> Result<()>;
    async fn record_login(
        &self,
        id: tpv_domain::ids::UserId,
        failed_attempts: u32,
        locked_until: Option<time::OffsetDateTime>,
        last_login_at: Option<time::OffsetDateTime>,
    ) -> Result<()>;
    async fn list_users(&self) -> Result<Vec<tpv_domain::people::UserAccount>>;
}
