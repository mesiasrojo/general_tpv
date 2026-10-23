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
