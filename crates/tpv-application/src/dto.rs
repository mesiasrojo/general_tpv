use serde::{Deserialize, Serialize};
use tpv_domain::Money;
use tpv_domain::cash::CashCount;
use tpv_domain::ids::{CustomerId, RegisterId, SaleId, SellerId, StoreId, UserId};
use tpv_domain::sale::PaymentMethod;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncContext {
    pub tenant_id: tpv_domain::ids::TenantId,
    pub device_id: tpv_domain::ids::DeviceId,
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
