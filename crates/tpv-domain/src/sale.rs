use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use time::OffsetDateTime;

use crate::catalog::Product;
use crate::error::{DomainError, Result};
use crate::ids::{
    CustomerId, InvoiceId, PaymentId, ProductId, RegisterId, SaleId, SaleLineId, SellerId, StoreId,
};
use crate::money::{Money, Quantity};
use crate::tax::{TaxBreakdown, TaxRate};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LineKind {
    Sale,
    Return,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PaymentMethod {
    Cash,
    Card,
    Voucher,
    Other,
}

impl PaymentMethod {
    #[must_use]
    pub fn is_cash(self) -> bool {
        matches!(self, Self::Cash)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Payment {
    pub id: PaymentId,
    pub method: PaymentMethod,
    /// Amount applied to the sale.
    pub amount: Money,
    /// Amount handed over by the customer. For non-cash methods it must equal
    /// `amount`; for cash it may be greater and generates change.
    pub received: Money,
    pub reference: Option<String>,
}

impl Payment {
    pub fn new(
        id: PaymentId,
        method: PaymentMethod,
        amount: Money,
        received: Money,
        reference: Option<String>,
    ) -> Result<Self> {
        if !amount.is_positive() {
            return Err(DomainError::Validation(
                "payment amount must be positive".into(),
            ));
        }
        if method.is_cash() {
            if received < amount {
                return Err(DomainError::Validation(
                    "cash received is lower than the payment amount".into(),
                ));
            }
        } else if received != amount {
            return Err(DomainError::Validation(
                "non-cash payments cannot generate change".into(),
            ));
        }
        Ok(Self {
            id,
            method,
            amount,
            received,
            reference: reference.and_then(|value| {
                let trimmed = value.trim().to_string();
                (!trimmed.is_empty()).then_some(trimmed)
            }),
        })
    }

    pub fn cash(id: PaymentId, amount: Money, received: Money) -> Result<Self> {
        Self::new(id, PaymentMethod::Cash, amount, received, None)
    }

    pub fn card(id: PaymentId, amount: Money) -> Result<Self> {
        Self::new(id, PaymentMethod::Card, amount, amount, None)
    }

    pub fn voucher(id: PaymentId, amount: Money) -> Result<Self> {
        Self::new(id, PaymentMethod::Voucher, amount, amount, None)
    }

    pub fn change(&self) -> Result<Money> {
        if self.method.is_cash() {
            self.received.checked_sub(self.amount)
        } else {
            Ok(Money::ZERO)
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SaleLine {
    pub id: SaleLineId,
    pub product_id: Option<ProductId>,
    pub sku: crate::ids::Sku,
    pub description: String,
    /// Tax-inclusive unit price.
    pub unit_price: Decimal,
    pub quantity: Quantity,
    /// Discount in basis points (500 = 5 %).
    pub discount_bp: u32,
    pub tax_rate: TaxRate,
    pub kind: LineKind,
}

impl SaleLine {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        id: SaleLineId,
        sku: crate::ids::Sku,
        description: impl Into<String>,
        unit_price: Decimal,
        quantity: Quantity,
        discount_bp: u32,
        tax_rate: TaxRate,
        kind: LineKind,
    ) -> Result<Self> {
        let description = description.into().trim().to_string();
        if description.is_empty() {
            return Err(DomainError::Validation(
                "line description is required".into(),
            ));
        }
        if unit_price < Decimal::ZERO {
            return Err(DomainError::Validation(
                "unit price cannot be negative".into(),
            ));
        }
        if unit_price.scale() > 4 {
            return Err(DomainError::Validation(
                "unit price supports at most four decimals".into(),
            ));
        }
        if discount_bp > 10_000 {
            return Err(DomainError::InvalidDiscount(discount_bp));
        }
        Ok(Self {
            id,
            product_id: None,
            sku,
            description,
            unit_price,
            quantity,
            discount_bp,
            tax_rate,
            kind,
        })
    }

    pub fn from_product(product: &Product, quantity: Quantity, kind: LineKind) -> Result<Self> {
        let mut line = Self::new(
            SaleLineId::new(),
            product.sku.clone(),
            product.name.clone(),
            product.unit_price,
            quantity,
            0,
            product.tax_rate.clone(),
            kind,
        )?;
        line.product_id = Some(product.id);
        Ok(line)
    }

    pub fn with_discount(mut self, discount_bp: u32) -> Result<Self> {
        if discount_bp > 10_000 {
            return Err(DomainError::InvalidDiscount(discount_bp));
        }
        self.discount_bp = discount_bp;
        Ok(self)
    }

    pub fn with_quantity(mut self, quantity: Quantity) -> Self {
        self.quantity = quantity;
        self
    }

    /// Tax-inclusive total for this line. Returns are negative.
    pub fn gross_total(&self) -> Result<Money> {
        let base = self.unit_price * self.quantity.as_decimal();
        let discount = Decimal::ONE - Decimal::new(i64::from(self.discount_bp), 4);
        let total = Money::from_decimal_round(base * discount)?;
        match self.kind {
            LineKind::Sale => Ok(total),
            LineKind::Return => Ok(-total),
        }
    }

    pub fn tax_amount(&self) -> Result<Money> {
        self.tax_rate.tax_from_gross(self.gross_total()?)
    }

    pub fn net_amount(&self) -> Result<Money> {
        self.gross_total()?.checked_sub(self.tax_amount()?)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SaleState {
    Draft,
    Priced,
    AwaitingPayment,
    Paid,
    Closed,
    Cancelled,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SaleTotals {
    pub gross: Money,
    pub net: Money,
    pub tax: Money,
    pub tax_breakdown: Vec<TaxBreakdown>,
    pub paid: Money,
    pub received: Money,
    pub due: Money,
    pub change: Money,
}

impl SaleTotals {
    #[must_use]
    pub fn is_fully_paid(&self) -> bool {
        self.due.is_zero()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Sale {
    pub id: SaleId,
    pub store_id: StoreId,
    pub register_id: RegisterId,
    pub seller_id: SellerId,
    pub customer_id: Option<CustomerId>,
    pub lines: Vec<SaleLine>,
    pub payments: Vec<Payment>,
    pub state: SaleState,
    pub created_at: OffsetDateTime,
    pub closed_at: Option<OffsetDateTime>,
    pub invoice_id: Option<InvoiceId>,
    pub ticket_series: Option<String>,
    pub ticket_number: Option<u64>,
    pub notes: Option<String>,
}

impl Sale {
    #[must_use]
    pub fn new(
        store_id: StoreId,
        register_id: RegisterId,
        seller_id: SellerId,
        customer_id: Option<CustomerId>,
        now: OffsetDateTime,
    ) -> Self {
        Self {
            id: SaleId::new(),
            store_id,
            register_id,
            seller_id,
            customer_id,
            lines: Vec::new(),
            payments: Vec::new(),
            state: SaleState::Draft,
            created_at: now,
            closed_at: None,
            invoice_id: None,
            ticket_series: None,
            ticket_number: None,
            notes: None,
        }
    }

    /// Assigns the local ticket number. This is called by the application
    /// service immediately before confirmation, using a transactional local
    /// sequence so two tills never share a number.
    pub fn assign_ticket(&mut self, series: impl Into<String>, number: u64) -> Result<()> {
        if self.is_confirmed() {
            return Err(DomainError::InvalidState(
                "a confirmed sale cannot be renumbered".into(),
            ));
        }
        if number == 0 {
            return Err(DomainError::Validation(
                "ticket number must be greater than zero".into(),
            ));
        }
        let series = series.into().trim().to_uppercase();
        if series.is_empty() {
            return Err(DomainError::Validation("ticket series is required".into()));
        }
        self.ticket_series = Some(series);
        self.ticket_number = Some(number);
        Ok(())
    }

    pub fn add_line(&mut self, line: SaleLine) -> Result<()> {
        self.ensure_draft_or_priced()?;
        if line.kind == LineKind::Return {
            let current = self.gross_total()?;
            let new_total = current.checked_add(line.gross_total()?)?;
            if new_total.is_negative() {
                return Err(DomainError::Validation(
                    "the return exceeds the sale total".into(),
                ));
            }
        }
        self.lines.push(line);
        self.state = SaleState::Priced;
        Ok(())
    }

    pub fn remove_line(&mut self, line_id: SaleLineId) -> Result<()> {
        self.ensure_draft_or_priced()?;
        let before = self.lines.len();
        self.lines.retain(|line| line.id != line_id);
        if self.lines.len() == before {
            return Err(DomainError::NotFound("sale line".into()));
        }
        if self.lines.is_empty() {
            self.state = SaleState::Draft;
        }
        Ok(())
    }

    pub fn set_line_quantity(&mut self, line_id: SaleLineId, quantity: Quantity) -> Result<()> {
        self.ensure_draft_or_priced()?;
        let line = self
            .lines
            .iter_mut()
            .find(|line| line.id == line_id)
            .ok_or_else(|| DomainError::NotFound("sale line".into()))?;
        line.quantity = quantity;
        Ok(())
    }

    pub fn add_payment(&mut self, payment: Payment) -> Result<()> {
        self.ensure_unpaid()?;
        let totals = self.totals()?;
        let new_paid = totals.paid.checked_add(payment.amount)?;
        if new_paid > totals.gross {
            return Err(DomainError::PaymentExceedsTotal);
        }
        self.payments.push(payment);
        self.state = SaleState::AwaitingPayment;
        Ok(())
    }

    pub fn clear_payments(&mut self) -> Result<()> {
        self.ensure_unpaid()?;
        self.payments.clear();
        self.state = if self.lines.is_empty() {
            SaleState::Draft
        } else {
            SaleState::Priced
        };
        Ok(())
    }

    pub fn confirm(&mut self, now: OffsetDateTime) -> Result<()> {
        if self.state != SaleState::Draft
            && self.state != SaleState::Priced
            && self.state != SaleState::AwaitingPayment
        {
            return Err(DomainError::InvalidState(
                "sale cannot be confirmed in its current state".into(),
            ));
        }
        if self.lines.is_empty() {
            return Err(DomainError::EmptySale);
        }
        let totals = self.totals()?;
        if totals.gross.is_negative() {
            return Err(DomainError::Validation(
                "a sale cannot have a negative total".into(),
            ));
        }
        if totals.paid != totals.gross {
            return Err(DomainError::PaymentMismatch);
        }
        self.state = SaleState::Paid;
        self.closed_at = Some(now);
        Ok(())
    }

    pub fn mark_closed(&mut self) -> Result<()> {
        if self.state != SaleState::Paid {
            return Err(DomainError::InvalidState("sale is not paid".into()));
        }
        self.state = SaleState::Closed;
        Ok(())
    }

    pub fn cancel(&mut self) -> Result<()> {
        if self.state != SaleState::Draft && self.state != SaleState::Priced {
            return Err(DomainError::InvalidState(
                "only draft sales can be cancelled".into(),
            ));
        }
        self.state = SaleState::Cancelled;
        Ok(())
    }

    pub fn mark_invoiced(&mut self, invoice_id: InvoiceId) -> Result<()> {
        if !self.is_confirmed() {
            return Err(DomainError::InvalidState(
                "the sale must be confirmed before invoicing".into(),
            ));
        }
        if self.invoice_id.is_some() {
            return Err(DomainError::Conflict("sale is already invoiced".into()));
        }
        self.invoice_id = Some(invoice_id);
        Ok(())
    }

    #[must_use]
    pub fn is_confirmed(&self) -> bool {
        matches!(self.state, SaleState::Paid | SaleState::Closed)
    }

    pub fn totals(&self) -> Result<SaleTotals> {
        let mut gross = Money::ZERO;
        let mut by_rate: BTreeMap<String, (TaxRate, Money)> = BTreeMap::new();
        for line in &self.lines {
            let line_gross = line.gross_total()?;
            gross = gross.checked_add(line_gross)?;
            let entry = by_rate
                .entry(line.tax_rate.code.clone())
                .or_insert_with(|| (line.tax_rate.clone(), Money::ZERO));
            entry.1 = entry.1.checked_add(line_gross)?;
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
        let mut paid = Money::ZERO;
        let mut received = Money::ZERO;
        let mut change = Money::ZERO;
        for payment in &self.payments {
            paid = paid.checked_add(payment.amount)?;
            received = received.checked_add(payment.received)?;
            change = change.checked_add(payment.change()?)?;
        }
        let due = gross.checked_sub(paid)?;
        Ok(SaleTotals {
            gross,
            net,
            tax,
            tax_breakdown,
            paid,
            received,
            due,
            change,
        })
    }

    fn gross_total(&self) -> Result<Money> {
        let mut total = Money::ZERO;
        for line in &self.lines {
            total = total.checked_add(line.gross_total()?)?;
        }
        Ok(total)
    }

    fn ensure_unpaid(&self) -> Result<()> {
        if matches!(
            self.state,
            SaleState::Draft | SaleState::Priced | SaleState::AwaitingPayment
        ) {
            Ok(())
        } else {
            Err(DomainError::InvalidState(
                "sale can no longer accept payments".into(),
            ))
        }
    }

    fn ensure_draft_or_priced(&self) -> Result<()> {
        if matches!(self.state, SaleState::Draft | SaleState::Priced) {
            Ok(())
        } else {
            Err(DomainError::InvalidState(
                "sale can no longer be modified".into(),
            ))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ids::{RegisterId, SellerId, StoreId};
    use time::macros::datetime;

    fn now() -> OffsetDateTime {
        datetime!(2026-10-22 19:15:46 UTC)
    }

    fn line(price: i64, qty: i64, rate: TaxRate) -> SaleLine {
        SaleLine::new(
            SaleLineId::new(),
            crate::ids::Sku::new(format!("P-{price}")).unwrap(),
            "Test product",
            Decimal::new(price, 2),
            Quantity::new(Decimal::from(qty)).unwrap(),
            0,
            rate,
            LineKind::Sale,
        )
        .unwrap()
    }

    fn sale_with_lines() -> Sale {
        let mut sale = Sale::new(
            StoreId::new(),
            RegisterId::new(),
            SellerId::new(),
            None,
            now(),
        );
        sale.add_line(line(1210, 1, TaxRate::standard())).unwrap();
        sale.add_line(line(110, 2, TaxRate::standard())).unwrap();
        sale
    }

    #[test]
    fn calculates_tax_inclusive_totals() {
        let sale = sale_with_lines();
        let totals = sale.totals().unwrap();
        assert_eq!(totals.gross, Money::from_cents(1430));
        assert_eq!(totals.net, Money::from_cents(1182));
        assert_eq!(totals.tax, Money::from_cents(248));
        assert_eq!(totals.tax_breakdown.len(), 1);
    }

    #[test]
    fn supports_mixed_payments_and_change() {
        let mut sale = sale_with_lines();
        sale.add_payment(
            Payment::cash(
                PaymentId::new(),
                Money::from_cents(1000),
                Money::from_cents(1570),
            )
            .unwrap(),
        )
        .unwrap();
        sale.add_payment(Payment::card(PaymentId::new(), Money::from_cents(430)).unwrap())
            .unwrap();
        let totals = sale.totals().unwrap();
        assert!(totals.is_fully_paid());
        assert_eq!(totals.change, Money::from_cents(570));
        sale.confirm(now()).unwrap();
        assert_eq!(sale.state, SaleState::Paid);
    }

    #[test]
    fn rejects_unpaid_confirmation() {
        let mut sale = sale_with_lines();
        sale.add_payment(
            Payment::cash(
                PaymentId::new(),
                Money::from_cents(1000),
                Money::from_cents(1000),
            )
            .unwrap(),
        )
        .unwrap();
        assert_eq!(
            sale.confirm(now()).unwrap_err(),
            DomainError::PaymentMismatch
        );
    }

    #[test]
    fn rejects_empty_confirmation() {
        let mut sale = Sale::new(
            StoreId::new(),
            RegisterId::new(),
            SellerId::new(),
            None,
            now(),
        );
        assert_eq!(sale.confirm(now()).unwrap_err(), DomainError::EmptySale);
    }

    #[test]
    fn rejects_returns_larger_than_the_sale() {
        let mut sale = sale_with_lines();
        let return_line = SaleLine::new(
            SaleLineId::new(),
            crate::ids::Sku::new("R-1").unwrap(),
            "Return",
            Decimal::new(5000, 2),
            Quantity::one(),
            0,
            TaxRate::standard(),
            LineKind::Return,
        )
        .unwrap();
        assert!(sale.add_line(return_line).is_err());
    }

    #[test]
    fn confirmed_sale_cannot_be_modified() {
        let mut sale = sale_with_lines();
        sale.add_payment(
            Payment::cash(
                PaymentId::new(),
                Money::from_cents(1430),
                Money::from_cents(1430),
            )
            .unwrap(),
        )
        .unwrap();
        sale.confirm(now()).unwrap();
        let err = sale
            .add_line(line(100, 1, TaxRate::standard()))
            .unwrap_err();
        assert!(matches!(err, DomainError::InvalidState(_)));
    }
}
