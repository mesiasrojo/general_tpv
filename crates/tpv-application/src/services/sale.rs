use std::str::FromStr;
use std::sync::Arc;

use rust_decimal::Decimal;
use tpv_domain::audit::{AuditAction, AuditEvent};
use tpv_domain::document::DocumentType;
use tpv_domain::ids::{PaymentId, SaleId, StoreId};
use tpv_domain::money::{Money, Quantity};
use tpv_domain::sale::{LineKind, Payment, Sale, SaleLine};
use tpv_domain::sync::SyncEvent;

use crate::dto::{
    AddLineRequest, ApplyPaymentRequest, ConfirmSaleRequest, SaleHistoryPage, SaleView,
    StartSaleRequest, SyncContext,
};
use crate::error::{ApplicationError, Result};
use crate::ports::{CatalogRepository, Clock, SaleQuery, SaleRepository, SequenceRepository};

#[derive(Clone)]
pub struct SaleService {
    sale_repo: Arc<dyn SaleRepository>,
    catalog_repo: Arc<dyn CatalogRepository>,
    clock: Arc<dyn Clock>,
    sequence_repo: Arc<dyn SequenceRepository>,
    ticket_series: String,
    sync: SyncContext,
}

impl SaleService {
    pub fn new(
        sale_repo: Arc<dyn SaleRepository>,
        catalog_repo: Arc<dyn CatalogRepository>,
        clock: Arc<dyn Clock>,
        sequence_repo: Arc<dyn SequenceRepository>,
        ticket_series: impl Into<String>,
        sync: SyncContext,
    ) -> Self {
        Self {
            sale_repo,
            catalog_repo,
            clock,
            sequence_repo,
            ticket_series: ticket_series.into(),
            sync,
        }
    }

    pub async fn start_sale(&self, request: StartSaleRequest) -> Result<Sale> {
        let sale = Sale::new(
            request.store_id,
            request.register_id,
            request.seller_id,
            request.customer_id,
            self.clock.now(),
        );
        self.sale_repo.save_draft(&sale).await?;
        Ok(sale)
    }

    pub async fn add_line(&self, request: AddLineRequest) -> Result<Sale> {
        let mut sale = self.load(request.sale_id).await?;
        let sku = tpv_domain::Sku::new(request.sku)?;
        let product = self
            .catalog_repo
            .find_product_by_sku(&sku)
            .await?
            .ok_or_else(|| ApplicationError::NotFound(format!("product {sku}")))?;
        let quantity = Decimal::from_str(&request.quantity)
            .map_err(|_| ApplicationError::Validation("invalid quantity".into()))?;
        let quantity = Quantity::new(quantity)?;
        let kind = match request.kind.as_str() {
            "sale" | "venta" | "v" => LineKind::Sale,
            "return" | "devolucion" | "devolución" | "d" => LineKind::Return,
            other => {
                return Err(ApplicationError::Validation(format!(
                    "unknown line kind: {other}"
                )));
            }
        };
        let mut line = SaleLine::from_product(&product, quantity, kind)?;
        if let Some(discount_bp) = request.discount_bp {
            line = line.with_discount(discount_bp)?;
        }
        sale.add_line(line)?;
        self.sale_repo.update_draft(&sale).await?;
        Ok(sale)
    }

    pub async fn apply_payment(&self, request: ApplyPaymentRequest) -> Result<Sale> {
        let mut sale = self.load(request.sale_id).await?;
        let payment = Payment::new(
            PaymentId::new(),
            request.method,
            Money::from_cents(request.amount_cents),
            Money::from_cents(request.received_cents),
            request.reference,
        )?;
        sale.add_payment(payment)?;
        self.sale_repo.update_draft(&sale).await?;
        Ok(sale)
    }

    pub async fn confirm_sale(&self, request: ConfirmSaleRequest) -> Result<Sale> {
        let mut sale = self.load(request.sale_id).await?;
        let now = self.clock.now();
        let year = u16::try_from(now.year()).unwrap_or(2026);
        let number = self
            .sequence_repo
            .next_number(
                sale.store_id,
                DocumentType::Ticket,
                &self.ticket_series,
                year,
            )
            .await?;
        sale.assign_ticket(&self.ticket_series, number)?;
        sale.confirm(now)?;
        let totals = sale.totals()?;
        let payload = serde_json::to_value(&sale)
            .map_err(|error| ApplicationError::Serialization(error.to_string()))?;
        let sequence = u64::try_from(now.unix_timestamp_nanos()).unwrap_or_default();
        let event = SyncEvent::new(
            self.sync.tenant_id,
            sale.store_id,
            self.sync.device_id,
            sequence,
            "sale",
            sale.id.as_uuid(),
            "sale.confirmed",
            1,
            now,
            payload,
        )?;
        let audit = AuditEvent::new(
            now,
            None,
            Some(sale.store_id),
            AuditAction::SaleConfirmed,
            "sale",
            sale.id.to_string(),
            serde_json::json!({
                "seller_id": sale.seller_id,
                "total_cents": totals.gross.cents(),
                "tax_cents": totals.tax.cents(),
                "lines": sale.lines.len(),
                "payments": sale.payments.len(),
            }),
        )?;
        self.sale_repo.confirm_sale(&sale, &event, &audit).await?;
        Ok(sale)
    }

    pub async fn get_sale(&self, sale_id: SaleId) -> Result<Sale> {
        self.load(sale_id).await
    }

    pub async fn list_recent(&self, store_id: StoreId, limit: u32) -> Result<Vec<Sale>> {
        self.sale_repo.list_recent(store_id, limit).await
    }

    /// Consulta el histórico con filtros y devuelve los totales del filtro
    /// completo, no solo de la página visible.
    pub async fn history(&self, mut query: SaleQuery) -> Result<SaleHistoryPage> {
        if let (Some(from), Some(to)) = (query.from, query.to) {
            if from > to {
                return Err(ApplicationError::Validation(
                    "la fecha inicial no puede ser posterior a la final".into(),
                ));
            }
        }
        query.limit = query.limit.clamp(1, 100);
        query.offset = query.offset.min(1_000_000);
        let result = self.sale_repo.query(&query).await?;
        let items = result
            .items
            .into_iter()
            .map(SaleView::new)
            .collect::<Result<Vec<_>>>()?;
        Ok(SaleHistoryPage {
            items,
            total: result.total,
            limit: query.limit,
            offset: query.offset,
            gross: result.summary.gross,
            tax: result.summary.tax,
            paid: result.summary.paid,
        })
    }

    async fn load(&self, sale_id: SaleId) -> Result<Sale> {
        self.sale_repo
            .find(sale_id)
            .await?
            .ok_or_else(|| ApplicationError::NotFound(format!("sale {sale_id}")))
    }
}
