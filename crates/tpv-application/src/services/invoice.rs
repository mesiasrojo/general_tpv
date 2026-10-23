use std::sync::Arc;

use tpv_domain::audit::{AuditAction, AuditEvent};
use tpv_domain::document::DocumentType;
use tpv_domain::ids::{InvoiceId, StoreId};
use tpv_domain::invoice::{Invoice, InvoiceLine};
use tpv_domain::money::Money;
use tpv_domain::sale::SaleTotals;
use tpv_domain::sync::SyncEvent;
use tpv_domain::tax::TaxBreakdown;
use uuid::Uuid;

use crate::dto::{IssueInvoiceRequest, RectifyInvoiceRequest, SyncContext};
use crate::error::{ApplicationError, Result};
use crate::ports::{
    CatalogRepository, Clock, InvoiceRepository, SaleRepository, SequenceRepository,
};

#[derive(Clone)]
pub struct InvoiceService {
    invoice_repo: Arc<dyn InvoiceRepository>,
    sale_repo: Arc<dyn SaleRepository>,
    catalog_repo: Arc<dyn CatalogRepository>,
    sequence_repo: Arc<dyn SequenceRepository>,
    clock: Arc<dyn Clock>,
    sync: SyncContext,
}

impl InvoiceService {
    pub fn new(
        invoice_repo: Arc<dyn InvoiceRepository>,
        sale_repo: Arc<dyn SaleRepository>,
        catalog_repo: Arc<dyn CatalogRepository>,
        sequence_repo: Arc<dyn SequenceRepository>,
        clock: Arc<dyn Clock>,
        sync: SyncContext,
    ) -> Self {
        Self {
            invoice_repo,
            sale_repo,
            catalog_repo,
            sequence_repo,
            clock,
            sync,
        }
    }

    pub async fn issue_invoice(&self, request: IssueInvoiceRequest) -> Result<Invoice> {
        let mut sale = self
            .sale_repo
            .find(request.sale_id)
            .await?
            .ok_or_else(|| ApplicationError::NotFound(format!("sale {}", request.sale_id)))?;
        let customer = self
            .catalog_repo
            .find_customer(request.customer_id)
            .await?
            .ok_or_else(|| {
                ApplicationError::NotFound(format!("customer {}", request.customer_id))
            })?;
        let now = self.clock.now();
        let number = self
            .sequence_repo
            .next_number(
                sale.store_id,
                DocumentType::Invoice,
                &request.series,
                request.year,
            )
            .await?;
        let invoice = Invoice::from_sale(&sale, &customer, request.series, number, now)?;
        sale.mark_invoiced(invoice.id)?;
        let event = self.build_event(
            sale.store_id,
            "invoice",
            invoice.id.as_uuid(),
            "invoice.issued",
            now,
            serde_json::to_value(&invoice)
                .map_err(|error| ApplicationError::Serialization(error.to_string()))?,
        )?;
        let audit = AuditEvent::new(
            now,
            None,
            Some(sale.store_id),
            AuditAction::InvoiceIssued,
            "invoice",
            invoice.id.to_string(),
            serde_json::json!({
                "sale_id": sale.id,
                "number": invoice.full_number(),
                "total_cents": invoice.totals.gross.cents(),
            }),
        )?;
        self.invoice_repo
            .save_invoice(&invoice, Some(&sale), &event, &audit)
            .await?;
        Ok(invoice)
    }

    pub async fn rectify_invoice(&self, request: RectifyInvoiceRequest) -> Result<Invoice> {
        let original = self
            .invoice_repo
            .find(request.invoice_id)
            .await?
            .ok_or_else(|| ApplicationError::NotFound(format!("invoice {}", request.invoice_id)))?;
        let now = self.clock.now();
        let number = self
            .sequence_repo
            .next_number(
                // The original store is not stored on the invoice in this
                // first slice; use the tenant's first store through the sale.
                self.sale_repo
                    .find(original.sale_id)
                    .await?
                    .ok_or_else(|| ApplicationError::NotFound("sale for invoice".into()))?
                    .store_id,
                DocumentType::Rectificative,
                &request.series,
                request.year,
            )
            .await?;
        let lines = original
            .lines
            .iter()
            .map(|line| InvoiceLine {
                gross: -line.gross,
                tax: -line.tax,
                ..line.clone()
            })
            .collect::<Vec<_>>();
        let totals = SaleTotals {
            gross: -original.totals.gross,
            net: -original.totals.net,
            tax: -original.totals.tax,
            tax_breakdown: original
                .totals
                .tax_breakdown
                .iter()
                .map(|breakdown| TaxBreakdown {
                    gross: -breakdown.gross,
                    net: -breakdown.net,
                    tax: -breakdown.tax,
                    ..breakdown.clone()
                })
                .collect(),
            paid: Money::ZERO,
            received: Money::ZERO,
            due: Money::ZERO,
            change: Money::ZERO,
        };
        let invoice = Invoice::rectificative(
            &original,
            request.series,
            number,
            now,
            request.reason,
            lines,
            totals,
        )?;
        invoice.validate_totals()?;
        let sale = self.sale_repo.find(original.sale_id).await?;
        let store_id = sale
            .as_ref()
            .map(|sale| sale.store_id)
            .ok_or_else(|| ApplicationError::NotFound("sale for invoice".into()))?;
        let event = self.build_event(
            store_id,
            "invoice",
            invoice.id.as_uuid(),
            "invoice.rectified",
            now,
            serde_json::to_value(&invoice)
                .map_err(|error| ApplicationError::Serialization(error.to_string()))?,
        )?;
        let audit = AuditEvent::new(
            now,
            None,
            Some(store_id),
            AuditAction::InvoiceRectified,
            "invoice",
            invoice.id.to_string(),
            serde_json::json!({
                "original_invoice_id": original.id,
                "number": invoice.full_number(),
            }),
        )?;
        self.invoice_repo
            .save_invoice(&invoice, None, &event, &audit)
            .await?;
        Ok(invoice)
    }

    pub async fn get_invoice(&self, id: InvoiceId) -> Result<Invoice> {
        self.invoice_repo
            .find(id)
            .await?
            .ok_or_else(|| ApplicationError::NotFound(format!("invoice {id}")))
    }

    pub async fn list_recent(&self, store_id: StoreId, limit: u32) -> Result<Vec<Invoice>> {
        self.invoice_repo.list_recent(store_id, limit).await
    }

    fn build_event(
        &self,
        store_id: StoreId,
        aggregate_type: &str,
        aggregate_id: Uuid,
        event_type: &str,
        now: time::OffsetDateTime,
        payload: serde_json::Value,
    ) -> Result<SyncEvent> {
        let sequence = u64::try_from(now.unix_timestamp_nanos()).unwrap_or_default();
        Ok(SyncEvent::new(
            self.sync.tenant_id,
            store_id,
            self.sync.device_id,
            sequence,
            aggregate_type,
            aggregate_id,
            event_type,
            1,
            now,
            payload,
        )?)
    }
}
