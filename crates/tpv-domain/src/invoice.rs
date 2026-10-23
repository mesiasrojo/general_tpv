use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use time::OffsetDateTime;

use crate::error::{DomainError, Result};
use crate::ids::{CustomerId, InvoiceId, SaleId, SaleLineId};
use crate::money::{Money, Quantity};
use crate::people::Customer;
use crate::sale::{LineKind, Sale, SaleLine, SaleTotals};
use crate::tax::TaxRate;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "type", content = "data")]
pub enum InvoiceKind {
    Standard,
    Rectificative {
        original_invoice_id: InvoiceId,
        reason: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InvoiceLine {
    pub source_line_id: SaleLineId,
    pub description: String,
    pub quantity: Quantity,
    pub unit_price: Decimal,
    pub discount_bp: u32,
    pub tax_rate: TaxRate,
    pub gross: Money,
    pub tax: Money,
    pub kind: LineKind,
}

impl InvoiceLine {
    pub fn from_sale_line(line: &SaleLine) -> Result<Self> {
        Ok(Self {
            source_line_id: line.id,
            description: line.description.clone(),
            quantity: line.quantity,
            unit_price: line.unit_price,
            discount_bp: line.discount_bp,
            tax_rate: line.tax_rate.clone(),
            gross: line.gross_total()?,
            tax: line.tax_amount()?,
            kind: line.kind,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Invoice {
    pub id: InvoiceId,
    pub sale_id: SaleId,
    pub customer_id: CustomerId,
    pub series: String,
    pub number: u64,
    pub issued_at: OffsetDateTime,
    pub kind: InvoiceKind,
    pub lines: Vec<InvoiceLine>,
    pub totals: SaleTotals,
}

impl Invoice {
    pub fn from_sale(
        sale: &Sale,
        customer: &Customer,
        series: impl Into<String>,
        number: u64,
        issued_at: OffsetDateTime,
    ) -> Result<Self> {
        if !sale.is_confirmed() {
            return Err(DomainError::InvalidState(
                "the sale must be confirmed before invoicing".into(),
            ));
        }
        if sale.invoice_id.is_some() {
            return Err(DomainError::Conflict("sale is already invoiced".into()));
        }
        customer.validate_for_invoice()?;
        if sale.customer_id != Some(customer.id) {
            return Err(DomainError::Validation(
                "customer does not match the sale customer".into(),
            ));
        }
        let series = series.into().trim().to_string();
        if series.is_empty() {
            return Err(DomainError::Validation("invoice series is required".into()));
        }
        if number == 0 {
            return Err(DomainError::Validation(
                "invoice number must be greater than zero".into(),
            ));
        }
        let lines = sale
            .lines
            .iter()
            .map(InvoiceLine::from_sale_line)
            .collect::<Result<Vec<_>>>()?;
        Ok(Self {
            id: InvoiceId::new(),
            sale_id: sale.id,
            customer_id: customer.id,
            series,
            number,
            issued_at,
            kind: InvoiceKind::Standard,
            lines,
            totals: sale.totals()?,
        })
    }

    pub fn rectificative(
        original: &Invoice,
        series: impl Into<String>,
        number: u64,
        issued_at: OffsetDateTime,
        reason: impl Into<String>,
        lines: Vec<InvoiceLine>,
        totals: SaleTotals,
    ) -> Result<Self> {
        if lines.is_empty() {
            return Err(DomainError::Validation(
                "a rectificative invoice requires lines".into(),
            ));
        }
        let reason = reason.into().trim().to_string();
        if reason.is_empty() {
            return Err(DomainError::Validation(
                "a rectificative invoice requires a reason".into(),
            ));
        }
        let series = series.into().trim().to_string();
        if series.is_empty() {
            return Err(DomainError::Validation("invoice series is required".into()));
        }
        if number == 0 {
            return Err(DomainError::Validation(
                "invoice number must be greater than zero".into(),
            ));
        }
        Ok(Self {
            id: InvoiceId::new(),
            sale_id: original.sale_id,
            customer_id: original.customer_id,
            series,
            number,
            issued_at,
            kind: InvoiceKind::Rectificative {
                original_invoice_id: original.id,
                reason,
            },
            lines,
            totals,
        })
    }

    #[must_use]
    pub fn full_number(&self) -> String {
        format!("{}-{:06}", self.series, self.number)
    }

    pub fn validate_totals(&self) -> Result<()> {
        let mut gross = Money::ZERO;
        let mut tax = Money::ZERO;
        for line in &self.lines {
            gross = gross.checked_add(line.gross)?;
            tax = tax.checked_add(line.tax)?;
        }
        if gross != self.totals.gross || tax != self.totals.tax {
            return Err(DomainError::Validation(
                "invoice totals do not match their lines".into(),
            ));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ids::{RegisterId, SellerId, StoreId};
    use crate::money::Quantity;
    use crate::sale::{Payment, PaymentMethod, SaleLine};
    use time::macros::datetime;

    fn now() -> OffsetDateTime {
        datetime!(2026-10-22 19:15:46 UTC)
    }

    fn confirmed_sale() -> Sale {
        let customer = CustomerId::new();
        let mut sale = Sale::new(
            StoreId::new(),
            RegisterId::new(),
            SellerId::new(),
            Some(customer),
            now(),
        );
        sale.add_line(
            SaleLine::new(
                SaleLineId::new(),
                crate::ids::Sku::new("P-1").unwrap(),
                "Product",
                Decimal::new(1210, 2),
                Quantity::one(),
                0,
                TaxRate::standard(),
                LineKind::Sale,
            )
            .unwrap(),
        )
        .unwrap();
        sale.add_payment(
            Payment::new(
                crate::ids::PaymentId::new(),
                PaymentMethod::Cash,
                Money::from_cents(1210),
                Money::from_cents(1210),
                None,
            )
            .unwrap(),
        )
        .unwrap();
        sale.confirm(now()).unwrap();
        sale
    }

    fn customer(id: CustomerId) -> Customer {
        let mut customer = Customer::new(id, "Ana", "García").unwrap();
        customer.nif = Some("12345678Z".into());
        customer
    }

    #[test]
    fn creates_invoice_from_confirmed_sale() {
        let sale = confirmed_sale();
        let customer = customer(sale.customer_id.unwrap());
        let invoice = Invoice::from_sale(&sale, &customer, "FAC-2026", 1, now()).unwrap();
        assert_eq!(invoice.full_number(), "FAC-2026-000001");
        assert_eq!(invoice.totals.gross, Money::from_cents(1210));
        invoice.validate_totals().unwrap();
    }

    #[test]
    fn invoice_requires_valid_customer_nif() {
        let sale = confirmed_sale();
        let mut customer = customer(sale.customer_id.unwrap());
        customer.nif = None;
        assert!(Invoice::from_sale(&sale, &customer, "FAC-2026", 1, now()).is_err());
    }

    #[test]
    fn invoice_requires_confirmed_sale() {
        let mut sale = confirmed_sale();
        sale.state = crate::sale::SaleState::Draft;
        let customer = customer(sale.customer_id.unwrap());
        assert!(Invoice::from_sale(&sale, &customer, "FAC-2026", 1, now()).is_err());
    }
}
