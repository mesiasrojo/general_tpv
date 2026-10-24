use serde::{Deserialize, Serialize};
use time::OffsetDateTime;
use tpv_domain::Money;
use tpv_domain::cash::{CashCount, CashMovementKind};
use tpv_domain::ids::{
    CustomerId, FamilyId, ProductId, RegisterId, SaleId, SaleLineId, SellerId, StoreId, TaxId,
    UserId,
};
use tpv_domain::master_data::MasterDataKind;
use tpv_domain::notes::Note;
use tpv_domain::people::Role;
use tpv_domain::sale::PaymentMethod;
use tpv_domain::sync::SyncEvent;
use tpv_domain::timeclock::TimeEntry;
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncContext {
    pub tenant_id: tpv_domain::ids::TenantId,
    pub device_id: tpv_domain::ids::DeviceId,
}

#[derive(Debug, Clone)]
pub struct PublishMasterDataRequest {
    pub context: SyncContext,
    pub store_id: StoreId,
    pub kind: MasterDataKind,
    pub entity_id: Uuid,
    pub payload: serde_json::Value,
    pub deleted: bool,
    pub occurred_at: OffsetDateTime,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpsertProductRequest {
    #[serde(default)]
    pub product_id: Option<ProductId>,
    pub sku: String,
    #[serde(default)]
    pub barcode: Option<String>,
    pub name: String,
    #[serde(default)]
    pub family_code: Option<String>,
    pub unit_price: String,
    pub tax_code: String,
    pub tax_percent_bp: u32,
    #[serde(default = "default_active")]
    pub active: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpsertFamilyRequest {
    #[serde(default)]
    pub family_id: Option<FamilyId>,
    pub code: String,
    pub name: String,
    #[serde(default = "default_active")]
    pub active: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpsertTaxRequest {
    #[serde(default)]
    pub tax_id: Option<TaxId>,
    pub code: String,
    pub name: String,
    pub percent_bp: u32,
    #[serde(default = "default_active")]
    pub active: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpsertSellerRequest {
    #[serde(default)]
    pub seller_id: Option<SellerId>,
    pub code: String,
    pub name: String,
    #[serde(default)]
    pub phone1: Option<String>,
    #[serde(default)]
    pub phone2: Option<String>,
    #[serde(default = "default_active")]
    pub active: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpsertStoreRequest {
    #[serde(default)]
    pub store_id: Option<StoreId>,
    pub code: String,
    pub name: String,
    pub tax_id: String,
    #[serde(default)]
    pub phone: Option<String>,
    #[serde(default)]
    pub street: Option<String>,
    #[serde(default)]
    pub number: Option<String>,
    #[serde(default)]
    pub locality: Option<String>,
    #[serde(default)]
    pub postal_code: Option<String>,
    #[serde(default)]
    pub province: Option<String>,
    #[serde(default = "default_active")]
    pub active: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpsertCustomerRequest {
    #[serde(default)]
    pub customer_id: Option<CustomerId>,
    pub first_name: String,
    pub last_name1: String,
    #[serde(default)]
    pub last_name2: Option<String>,
    #[serde(default)]
    pub nif: Option<String>,
    #[serde(default)]
    pub phone: Option<String>,
    #[serde(default)]
    pub email: Option<String>,
    #[serde(default)]
    pub street: Option<String>,
    #[serde(default)]
    pub number: Option<String>,
    #[serde(default)]
    pub locality: Option<String>,
    #[serde(default)]
    pub postal_code: Option<String>,
    #[serde(default)]
    pub province: Option<String>,
    #[serde(default = "default_active")]
    pub active: bool,
}

/// A time entry enriched with the operator display name for the UI.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TimeEntryView {
    pub entry: TimeEntry,
    pub user_name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TimeClockStatus {
    /// Open entry of the user requesting the status, if any.
    pub active: Option<TimeEntryView>,
    /// Recent entries for the store. Cashiers only see their own entries.
    pub recent: Vec<TimeEntryView>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateNoteRequest {
    pub title: String,
    pub body: String,
    pub recipient_ids: Vec<UserId>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NoteRecipientOption {
    pub user_id: UserId,
    pub user_name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NoteRecipientView {
    pub user_id: UserId,
    pub user_name: String,
    pub read_at: Option<OffsetDateTime>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NoteView {
    pub note: Note,
    pub author_name: String,
    pub recipients: Vec<NoteRecipientView>,
    pub viewer_read_at: Option<OffsetDateTime>,
    pub is_author: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NoteInbox {
    pub notes: Vec<NoteView>,
    /// Notes addressed to the viewer that have not been read yet.
    pub unread: u32,
    /// Active users available as recipients. Empty for non-managers.
    pub recipients: Vec<NoteRecipientOption>,
}

fn default_active() -> bool {
    true
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StartSaleRequest {
    pub store_id: StoreId,
    pub register_id: RegisterId,
    pub seller_id: SellerId,
    pub customer_id: Option<CustomerId>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AddLineRequest {
    pub sale_id: SaleId,
    pub sku: String,
    pub quantity: String,
    pub kind: String,
    pub discount_bp: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApplyPaymentRequest {
    pub sale_id: SaleId,
    pub method: PaymentMethod,
    pub amount_cents: i64,
    pub received_cents: i64,
    pub reference: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConfirmSaleRequest {
    pub sale_id: SaleId,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IssueInvoiceRequest {
    pub sale_id: SaleId,
    pub customer_id: CustomerId,
    pub series: String,
    pub year: u16,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RectifyInvoiceRequest {
    pub invoice_id: tpv_domain::ids::InvoiceId,
    pub series: String,
    pub year: u16,
    pub reason: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InvoiceLineSelection {
    pub source_line_id: SaleLineId,
    pub quantity: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RectifyInvoiceLinesRequest {
    pub invoice_id: tpv_domain::ids::InvoiceId,
    pub series: String,
    pub year: u16,
    pub reason: String,
    #[serde(default)]
    pub refund_reference: Option<String>,
    pub lines: Vec<InvoiceLineSelection>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpenCashRequest {
    pub store_id: StoreId,
    pub register_id: RegisterId,
    pub opened_by: UserId,
    pub opening_float_cents: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CashExpenseRequest {
    pub session_id: tpv_domain::ids::CashSessionId,
    pub amount_cents: i64,
    pub reason: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CashMovementRequest {
    pub session_id: tpv_domain::ids::CashSessionId,
    pub kind: CashMovementKind,
    pub amount_cents: i64,
    pub reason: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CloseCashRequest {
    pub session_id: tpv_domain::ids::CashSessionId,
    pub closed_by: UserId,
    pub counted: CashCount,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReopenCashRequest {
    pub session_id: tpv_domain::ids::CashSessionId,
    pub reopened_by: UserId,
    pub reason: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProductSearchResult {
    pub sku: String,
    pub name: String,
    pub price_cents: i64,
    pub tax_code: String,
    pub tax_percent_bp: u32,
}

impl ProductSearchResult {
    pub fn from_product(product: &tpv_domain::catalog::Product) -> Self {
        Self {
            sku: product.sku.to_string(),
            name: product.name.clone(),
            price_cents: Money::from_decimal_round(product.unit_price)
                .map(|money| money.cents())
                .unwrap_or_default(),
            tax_code: product.tax_rate.code.clone(),
            tax_percent_bp: product.tax_rate.percent_bp,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SaleView {
    pub sale: tpv_domain::sale::Sale,
    pub totals: tpv_domain::sale::SaleTotals,
}

impl SaleView {
    pub fn new(sale: tpv_domain::sale::Sale) -> crate::Result<Self> {
        let totals = sale.totals()?;
        Ok(Self { sale, totals })
    }
}

/// Página de consultas de ventas con totales conciliados del filtro completo.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SaleHistoryPage {
    pub items: Vec<SaleView>,
    pub total: u64,
    pub limit: u32,
    pub offset: u32,
    pub gross: Money,
    pub tax: Money,
    pub paid: Money,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TicketLine {
    pub sku: String,
    pub description: String,
    pub quantity: String,
    pub unit_price: String,
    pub total: Money,
    pub kind: tpv_domain::sale::LineKind,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TicketDocument {
    pub store_name: String,
    pub store_address: String,
    pub store_tax_id: String,
    pub store_phone: Option<String>,
    pub ticket_label: String,
    pub issued_at: time::OffsetDateTime,
    pub seller_code: String,
    pub seller_name: String,
    pub customer_name: Option<String>,
    pub customer_tax_id: Option<String>,
    pub lines: Vec<TicketLine>,
    pub totals: tpv_domain::sale::SaleTotals,
    pub reprint: bool,
    pub footer: Vec<String>,
}

impl TicketDocument {
    pub fn from_sale(
        sale: &tpv_domain::sale::Sale,
        store: &tpv_domain::people::Store,
        seller: &tpv_domain::people::Seller,
        customer: Option<&tpv_domain::people::Customer>,
        reprint: bool,
    ) -> crate::Result<Self> {
        if !sale.is_confirmed() {
            return Err(crate::ApplicationError::Validation(
                "the sale must be confirmed before printing".into(),
            ));
        }
        let number = sale.ticket_number.ok_or_else(|| {
            crate::ApplicationError::Validation("the sale has no ticket number".into())
        })?;
        let series = sale.ticket_series.as_deref().unwrap_or("TPV");
        let lines = sale
            .lines
            .iter()
            .map(|line| {
                Ok(TicketLine {
                    sku: line.sku.to_string(),
                    description: line.description.clone(),
                    quantity: line.quantity.to_string(),
                    unit_price: line.unit_price.to_string(),
                    total: line.gross_total()?,
                    kind: line.kind,
                })
            })
            .collect::<crate::Result<Vec<_>>>()?;
        let mut footer = vec![
            "IVA INCLUIDO".to_string(),
            "GRACIAS POR SU COMPRA".to_string(),
        ];
        if reprint {
            footer.insert(0, "COPIA".to_string());
        }
        Ok(Self {
            store_name: store.name.clone(),
            store_address: store.address.one_line(),
            store_tax_id: store.tax_id.clone(),
            store_phone: store.phone.clone(),
            ticket_label: format!("{series}-{number:06}"),
            issued_at: sale.closed_at.unwrap_or(sale.created_at),
            seller_code: seller.code.clone(),
            seller_name: seller.name.clone(),
            customer_name: customer.map(tpv_domain::people::Customer::full_name),
            customer_tax_id: customer.and_then(|value| value.nif.clone()),
            lines,
            totals: sale.totals()?,
            reprint,
            footer,
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PrintJobType {
    Ticket,
    Invoice,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PrintJobStatus {
    Pending,
    Printed,
    Failed,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PrintJob {
    pub id: tpv_domain::ids::PrintJobId,
    pub sale_id: tpv_domain::ids::SaleId,
    pub job_type: PrintJobType,
    pub document: TicketDocument,
    pub bytes: Vec<u8>,
    pub status: PrintJobStatus,
    pub attempts: u32,
    pub created_at: time::OffsetDateTime,
    pub printed_at: Option<time::OffsetDateTime>,
    pub last_error: Option<String>,
}

impl PrintJob {
    pub fn ticket(
        sale_id: tpv_domain::ids::SaleId,
        document: TicketDocument,
        bytes: Vec<u8>,
        created_at: time::OffsetDateTime,
    ) -> Self {
        Self {
            id: tpv_domain::ids::PrintJobId::new(),
            sale_id,
            job_type: PrintJobType::Ticket,
            document,
            bytes,
            status: PrintJobStatus::Pending,
            attempts: 0,
            created_at,
            printed_at: None,
            last_error: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PrintResult {
    pub job_id: tpv_domain::ids::PrintJobId,
    pub success: bool,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionUser {
    pub session_id: uuid::Uuid,
    pub user: tpv_domain::people::UserAccount,
    pub started_at: time::OffsetDateTime,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SyncStatus {
    pub pending: u32,
    pub sent: u32,
    pub failed: u32,
    pub oldest_pending_at: Option<time::OffsetDateTime>,
    pub last_sent_at: Option<time::OffsetDateTime>,
    pub last_failed_at: Option<time::OffsetDateTime>,
    pub last_error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QuarantinedEvent {
    pub event: SyncEvent,
    pub attempts: u32,
    pub failed_at: time::OffsetDateTime,
    pub last_error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateUserRequest {
    pub username: String,
    pub display_name: String,
    pub role: Role,
    pub store_ids: Vec<StoreId>,
    pub password: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateUserRequest {
    pub user_id: UserId,
    pub display_name: String,
    pub role: Role,
    pub store_ids: Vec<StoreId>,
    pub active: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResetPasswordRequest {
    pub user_id: UserId,
    pub new_password: String,
}
