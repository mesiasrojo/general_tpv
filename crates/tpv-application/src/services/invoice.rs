use std::collections::{BTreeMap, HashSet};
use std::str::FromStr;
use std::sync::Arc;

use rust_decimal::Decimal;
use tpv_domain::audit::{AuditAction, AuditEvent};
use tpv_domain::document::DocumentType;
use tpv_domain::ids::{InvoiceId, StoreId};
use tpv_domain::invoice::{Invoice, InvoiceLine};
use tpv_domain::money::{Money, Quantity};
use tpv_domain::sale::SaleTotals;
use tpv_domain::sync::SyncEvent;
use tpv_domain::tax::{TaxBreakdown, TaxRate};
use uuid::Uuid;

use crate::dto::{
    IssueInvoiceRequest, RectifyInvoiceLinesRequest, RectifyInvoiceRequest, SyncContext,
};
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

    pub async fn rectify_invoice_lines(
        &self,
        request: RectifyInvoiceLinesRequest,
    ) -> Result<Invoice> {
        let original = self.get_invoice(request.invoice_id).await?;
        let sale = self
            .sale_repo
            .find(original.sale_id)
            .await?
            .ok_or_else(|| ApplicationError::NotFound("sale for invoice".into()))?;
        let now = self.clock.now();
        let number = self
            .sequence_repo
            .next_number(
                sale.store_id,
                DocumentType::Rectificative,
                &request.series,
                request.year,
            )
            .await?;
        let mut seen = HashSet::new();
        let mut lines = Vec::with_capacity(request.lines.len());
        for selection in request.lines {
            if !seen.insert(selection.source_line_id) {
                return Err(ApplicationError::Validation(
                    "duplicate invoice line in partial return".into(),
                ));
            }
            let original_line = original
                .lines
                .iter()
                .find(|line| line.source_line_id == selection.source_line_id)
                .ok_or_else(|| {
                    ApplicationError::NotFound(format!("invoice line {}", selection.source_line_id))
                })?;
            let quantity = Decimal::from_str(selection.quantity.trim()).map_err(|_| {
                ApplicationError::Validation(format!(
                    "invalid return quantity {}",
                    selection.quantity
                ))
            })?;
            let quantity = Quantity::new(quantity)?;
            let partial = original_line.partial(quantity)?;
            lines.push(InvoiceLine {
                gross: -partial.gross,
                tax: -partial.tax,
                ..partial
            });
        }
        if lines.is_empty() {
            return Err(ApplicationError::Validation(
                "a partial return requires at least one line".into(),
            ));
        }
        let totals = totals_from_lines(&lines)?;
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
        let event = self.build_event(
            sale.store_id,
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
            Some(sale.store_id),
            AuditAction::InvoiceRectified,
            "invoice",
            invoice.id.to_string(),
            serde_json::json!({
                "original_invoice_id": original.id,
                "number": invoice.full_number(),
                "partial": true,
                "line_count": invoice.lines.len(),
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

fn totals_from_lines(lines: &[InvoiceLine]) -> Result<SaleTotals> {
    let mut gross = Money::ZERO;
    let mut by_rate: BTreeMap<String, (TaxRate, Money)> = BTreeMap::new();
    for line in lines {
        gross = gross.checked_add(line.gross)?;
        let entry = by_rate
            .entry(line.tax_rate.code.clone())
            .or_insert_with(|| (line.tax_rate.clone(), Money::ZERO));
        entry.1 = entry.1.checked_add(line.gross)?;
    }
    let mut tax_breakdown = Vec::with_capacity(by_rate.len());
    for (_, (rate, rate_gross)) in by_rate {
        tax_breakdown.push(TaxBreakdown::from_gross(&rate, rate_gross)?);
    }
    let mut net = Money::ZERO;
    let mut tax = Money::ZERO;
    for entry in &tax_breakdown {
        net = net.checked_add(entry.net)?;
        tax = tax.checked_add(entry.tax)?;
    }
    Ok(SaleTotals {
        gross,
        net,
        tax,
        tax_breakdown,
        paid: Money::ZERO,
        received: Money::ZERO,
        due: Money::ZERO,
        change: Money::ZERO,
    })
}
