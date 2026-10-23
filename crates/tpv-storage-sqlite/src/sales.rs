use async_trait::async_trait;
use sqlx::Row;
use sqlx::sqlite::SqliteRow;
use time::OffsetDateTime;
use tpv_application::{InvoiceRepository, Result as AppResult, SaleRepository, SequenceRepository};
use tpv_domain::audit::AuditEvent;
use tpv_domain::cash::CashCount;
use tpv_domain::document::DocumentType;
use tpv_domain::ids::{
    CustomerId, InvoiceId, PaymentId, ProductId, RegisterId, SaleId, SaleLineId, SellerId, StoreId,
};
use tpv_domain::invoice::{Invoice, InvoiceKind, InvoiceLine};
use tpv_domain::money::{Money, Quantity};
use tpv_domain::sale::{Payment, PaymentMethod, Sale, SaleLine, SaleTotals};
use tpv_domain::sync::SyncEvent;
use tpv_domain::{Sku, TaxRate};

use crate::error::{StorageError, StorageResult};
use crate::helpers::{
    enum_from_text, enum_to_text, format_time, i64_to_u64, parse_decimal, parse_optional_time,
    parse_time, parse_uuid,
};
use crate::persistence::{enqueue_event_tx, insert_audit_tx, persist_sale_tx};
use crate::store::SqliteStore;

fn row_to_sale_line(row: &SqliteRow) -> StorageResult<SaleLine> {
    let product_id = row
        .try_get::<Option<String>, _>("product_id")?
        .map(|value| parse_uuid(&value).map(ProductId::from_uuid))
        .transpose()?;
    let tax_rate = TaxRate::new(
        row.try_get::<String, _>("tax_code")?,
        u32::try_from(row.try_get::<i64, _>("tax_percent_bp")?)
            .map_err(|_| StorageError::InvalidData("invalid tax percentage".into()))?,
    )?;
    Ok(SaleLine {
        id: SaleLineId::from_uuid(parse_uuid(&row.try_get::<String, _>("id")?)?),
        product_id,
        sku: Sku::new(row.try_get::<String, _>("sku")?)?,
        description: row.try_get("description")?,
        unit_price: parse_decimal(&row.try_get::<String, _>("unit_price")?)?,
        quantity: Quantity::new(parse_decimal(&row.try_get::<String, _>("quantity")?)?)?,
        discount_bp: u32::try_from(row.try_get::<i64, _>("discount_bp")?)
            .map_err(|_| StorageError::InvalidData("invalid discount".into()))?,
        tax_rate,
        kind: enum_from_text(&row.try_get::<String, _>("kind")?)?,
    })
}

fn row_to_payment(row: &SqliteRow) -> StorageResult<Payment> {
    Ok(Payment {
        id: PaymentId::from_uuid(parse_uuid(&row.try_get::<String, _>("id")?)?),
        method: enum_from_text(&row.try_get::<String, _>("method")?)?,
        amount: Money::from_cents(row.try_get("amount_cents")?),
        received: Money::from_cents(row.try_get("received_cents")?),
        reference: row.try_get("reference")?,
    })
}

impl SqliteStore {
    pub(crate) async fn load_sale(&self, id: SaleId) -> StorageResult<Option<Sale>> {
        let row = sqlx::query("SELECT * FROM sales WHERE id = ?")
            .bind(id.to_string())
            .fetch_optional(self.pool())
            .await?;
        let Some(row) = row else {
            return Ok(None);
        };
        let line_rows =
            sqlx::query("SELECT * FROM sale_lines WHERE sale_id = ? ORDER BY line_index")
                .bind(id.to_string())
                .fetch_all(self.pool())
                .await?;
        let payment_rows =
            sqlx::query("SELECT * FROM payments WHERE sale_id = ? ORDER BY payment_index")
                .bind(id.to_string())
                .fetch_all(self.pool())
                .await?;
        let lines = line_rows
            .iter()
            .map(row_to_sale_line)
            .collect::<StorageResult<Vec<_>>>()?;
        let payments = payment_rows
            .iter()
            .map(row_to_payment)
            .collect::<StorageResult<Vec<_>>>()?;
        let ticket_number = row
            .try_get::<Option<i64>, _>("ticket_number")?
            .map(|value| i64_to_u64(value, "ticket_number"))
            .transpose()?;
        Ok(Some(Sale {
            id: SaleId::from_uuid(parse_uuid(&row.try_get::<String, _>("id")?)?),
            store_id: StoreId::from_uuid(parse_uuid(&row.try_get::<String, _>("store_id")?)?),
            register_id: RegisterId::from_uuid(parse_uuid(
                &row.try_get::<String, _>("register_id")?,
            )?),
            seller_id: SellerId::from_uuid(parse_uuid(&row.try_get::<String, _>("seller_id")?)?),
            customer_id: row
                .try_get::<Option<String>, _>("customer_id")?
                .map(|value| parse_uuid(&value).map(CustomerId::from_uuid))
                .transpose()?,
            lines,
            payments,
            state: enum_from_text(&row.try_get::<String, _>("state")?)?,
            created_at: parse_time(&row.try_get::<String, _>("created_at")?)?,
            closed_at: parse_optional_time(row.try_get("closed_at")?)?,
            invoice_id: row
                .try_get::<Option<String>, _>("invoice_id")?
                .map(|value| parse_uuid(&value).map(InvoiceId::from_uuid))
                .transpose()?,
            ticket_series: row.try_get("ticket_series")?,
            ticket_number,
            notes: row.try_get("notes")?,
        }))
    }

    async fn list_sale_ids(&self, store_id: StoreId, limit: u32) -> StorageResult<Vec<SaleId>> {
        let rows =
            sqlx::query("SELECT id FROM sales WHERE store_id = ? ORDER BY created_at DESC LIMIT ?")
                .bind(store_id.to_string())
                .bind(i64::from(limit))
                .fetch_all(self.pool())
                .await?;
        rows.iter()
            .map(|row| {
                Ok(SaleId::from_uuid(parse_uuid(
                    &row.try_get::<String, _>("id")?,
                )?))
            })
            .collect()
    }

    async fn load_invoice(&self, id: InvoiceId) -> StorageResult<Option<Invoice>> {
        let row = sqlx::query("SELECT * FROM invoices WHERE id = ?")
            .bind(id.to_string())
            .fetch_optional(self.pool())
            .await?;
        let Some(row) = row else {
            return Ok(None);
        };
        let line_rows =
            sqlx::query("SELECT * FROM invoice_lines WHERE invoice_id = ? ORDER BY line_index")
                .bind(id.to_string())
                .fetch_all(self.pool())
                .await?;
        let lines = line_rows
            .iter()
            .map(|line| {
                let tax_rate = TaxRate::new(
                    line.try_get::<String, _>("tax_code")?,
                    u32::try_from(line.try_get::<i64, _>("tax_percent_bp")?)
                        .map_err(|_| StorageError::InvalidData("invalid tax percentage".into()))?,
                )?;
                Ok(InvoiceLine {
                    source_line_id: SaleLineId::from_uuid(parse_uuid(
                        &line.try_get::<String, _>("source_line_id")?,
                    )?),
                    description: line.try_get("description")?,
                    quantity: Quantity::new(parse_decimal(
                        &line.try_get::<String, _>("quantity")?,
                    )?)?,
                    unit_price: parse_decimal(&line.try_get::<String, _>("unit_price")?)?,
                    discount_bp: u32::try_from(line.try_get::<i64, _>("discount_bp")?)
                        .map_err(|_| StorageError::InvalidData("invalid discount".into()))?,
                    tax_rate,
                    gross: Money::from_cents(line.try_get("gross_cents")?),
                    tax: Money::from_cents(line.try_get("tax_cents")?),
                    kind: enum_from_text(&line.try_get::<String, _>("kind")?)?,
                })
            })
            .collect::<StorageResult<Vec<_>>>()?;
        let kind: InvoiceKind = serde_json::from_str(&row.try_get::<String, _>("kind_json")?)?;
        let totals: SaleTotals = serde_json::from_str(&row.try_get::<String, _>("totals_json")?)?;
        Ok(Some(Invoice {
            id: InvoiceId::from_uuid(parse_uuid(&row.try_get::<String, _>("id")?)?),
            sale_id: SaleId::from_uuid(parse_uuid(&row.try_get::<String, _>("sale_id")?)?),
            customer_id: CustomerId::from_uuid(parse_uuid(
                &row.try_get::<String, _>("customer_id")?,
            )?),
            series: row.try_get("series")?,
            number: i64_to_u64(row.try_get("number")?, "invoice number")?,
            issued_at: parse_time(&row.try_get::<String, _>("issued_at")?)?,
            kind,
            lines,
            totals,
        }))
    }
}

impl SqliteStore {
    async fn save_sale_inner(
        &self,
        sale: &Sale,
        event: Option<&SyncEvent>,
        audit: Option<&AuditEvent>,
    ) -> StorageResult<()> {
        let mut tx = self.pool().begin().await?;
        persist_sale_tx(&mut tx, sale).await?;
        if let Some(event) = event {
            enqueue_event_tx(&mut tx, event).await?;
        }
        if let Some(audit) = audit {
            insert_audit_tx(&mut tx, audit).await?;
        }
        tx.commit().await?;
        Ok(())
    }

    async fn payment_totals_inner(
        &self,
        store_id: StoreId,
        register_id: RegisterId,
        since: OffsetDateTime,
    ) -> StorageResult<CashCount> {
        let rows = sqlx::query(
            "SELECT p.method, SUM(p.amount_cents) AS total
             FROM payments p
             JOIN sales s ON s.id = p.sale_id
             WHERE s.store_id = ? AND s.register_id = ?
               AND s.closed_at IS NOT NULL AND s.closed_at >= ?
               AND s.state IN ('paid', 'closed')
             GROUP BY p.method",
        )
        .bind(store_id.to_string())
        .bind(register_id.to_string())
        .bind(format_time(since)?)
        .fetch_all(self.pool())
        .await?;
        let mut totals = CashCount::default();
        for row in rows {
            let method: PaymentMethod = enum_from_text(&row.try_get::<String, _>("method")?)?;
            let amount = Money::from_cents(row.try_get("total")?);
            match method {
                PaymentMethod::Cash => totals.cash = totals.cash.checked_add(amount)?,
                PaymentMethod::Card => totals.card = totals.card.checked_add(amount)?,
                PaymentMethod::Voucher => totals.voucher = totals.voucher.checked_add(amount)?,
                PaymentMethod::Other => {}
            }
        }
        Ok(totals)
    }

    async fn save_invoice_inner(
        &self,
        invoice: &Invoice,
        sale: Option<&Sale>,
        event: &SyncEvent,
        audit: &AuditEvent,
    ) -> StorageResult<()> {
        let mut tx = self.pool().begin().await?;
        let store_id = event.store_id;
        let kind_json = serde_json::to_string(&invoice.kind)?;
        let totals_json = serde_json::to_string(&invoice.totals)?;
        sqlx::query(
            "INSERT INTO invoices
                (id, store_id, sale_id, customer_id, series, number, issued_at, kind_json, totals_json)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(invoice.id.to_string())
        .bind(store_id.to_string())
        .bind(invoice.sale_id.to_string())
        .bind(invoice.customer_id.to_string())
        .bind(&invoice.series)
        .bind(i64::try_from(invoice.number).map_err(|_| {
            StorageError::InvalidData("invoice number exceeds i64".into())
        })?)
        .bind(format_time(invoice.issued_at)?)
        .bind(kind_json)
        .bind(totals_json)
        .execute(&mut *tx)
        .await?;
        for (index, line) in invoice.lines.iter().enumerate() {
            sqlx::query(
                "INSERT INTO invoice_lines
                    (invoice_id, source_line_id, description, quantity, unit_price, discount_bp,
                     tax_code, tax_percent_bp, gross_cents, tax_cents, kind, line_index)
                 VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
            )
            .bind(invoice.id.to_string())
            .bind(line.source_line_id.to_string())
            .bind(&line.description)
            .bind(line.quantity.as_decimal().to_string())
            .bind(line.unit_price.to_string())
            .bind(i64::from(line.discount_bp))
            .bind(&line.tax_rate.code)
            .bind(i64::from(line.tax_rate.percent_bp))
            .bind(line.gross.cents())
            .bind(line.tax.cents())
            .bind(enum_to_text(&line.kind)?)
            .bind(index as i64)
            .execute(&mut *tx)
            .await?;
        }
        if let Some(sale) = sale {
            sqlx::query("UPDATE sales SET invoice_id = ? WHERE id = ?")
                .bind(invoice.id.to_string())
                .bind(sale.id.to_string())
                .execute(&mut *tx)
                .await?;
        }
        enqueue_event_tx(&mut tx, event).await?;
        insert_audit_tx(&mut tx, audit).await?;
        tx.commit().await?;
        Ok(())
    }

    async fn list_invoice_ids(
        &self,
        store_id: StoreId,
        limit: u32,
    ) -> StorageResult<Vec<InvoiceId>> {
        let rows = sqlx::query(
            "SELECT id FROM invoices WHERE store_id = ? ORDER BY issued_at DESC LIMIT ?",
        )
        .bind(store_id.to_string())
        .bind(i64::from(limit))
        .fetch_all(self.pool())
        .await?;
        rows.iter()
            .map(|row| {
                Ok(InvoiceId::from_uuid(parse_uuid(
                    &row.try_get::<String, _>("id")?,
                )?))
            })
            .collect()
    }

    async fn next_number_inner(
        &self,
        store_id: StoreId,
        document_type: DocumentType,
        series: &str,
        year: u16,
    ) -> StorageResult<u64> {
        let mut tx = self.pool().begin().await?;
        let document_type = enum_to_text(&document_type)?;
        let series = series.trim().to_uppercase();
        sqlx::query(
            "INSERT INTO document_sequences
                (store_id, document_type, series, year, next_number)
             VALUES (?, ?, ?, ?, 1)
             ON CONFLICT(store_id, document_type, series, year) DO NOTHING",
        )
        .bind(store_id.to_string())
        .bind(&document_type)
        .bind(&series)
        .bind(i64::from(year))
        .execute(&mut *tx)
        .await?;
        sqlx::query(
            "UPDATE document_sequences SET next_number = next_number + 1
             WHERE store_id = ? AND document_type = ? AND series = ? AND year = ?",
        )
        .bind(store_id.to_string())
        .bind(&document_type)
        .bind(&series)
        .bind(i64::from(year))
        .execute(&mut *tx)
        .await?;
        let allocated: i64 = sqlx::query_scalar(
            "SELECT next_number - 1 FROM document_sequences
             WHERE store_id = ? AND document_type = ? AND series = ? AND year = ?",
        )
        .bind(store_id.to_string())
        .bind(&document_type)
        .bind(&series)
        .bind(i64::from(year))
        .fetch_one(&mut *tx)
        .await?;
        tx.commit().await?;
        i64_to_u64(allocated, "document number")
    }
}

#[async_trait]
impl SaleRepository for SqliteStore {
    async fn save_draft(&self, sale: &Sale) -> AppResult<()> {
        Ok(self.save_sale_inner(sale, None, None).await?)
    }

    async fn update_draft(&self, sale: &Sale) -> AppResult<()> {
        Ok(self.save_sale_inner(sale, None, None).await?)
    }

    async fn find(&self, id: SaleId) -> AppResult<Option<Sale>> {
        Ok(self.load_sale(id).await?)
    }

    async fn confirm_sale(
        &self,
        sale: &Sale,
        event: &SyncEvent,
        audit: &AuditEvent,
    ) -> AppResult<()> {
        Ok(self.save_sale_inner(sale, Some(event), Some(audit)).await?)
    }

    async fn list_recent(&self, store_id: StoreId, limit: u32) -> AppResult<Vec<Sale>> {
        let ids = self.list_sale_ids(store_id, limit).await?;
        let mut sales = Vec::with_capacity(ids.len());
        for id in ids {
            if let Some(sale) = self.load_sale(id).await? {
                sales.push(sale);
            }
        }
        Ok(sales)
    }

    async fn payment_totals_since(
        &self,
        store_id: StoreId,
        register_id: RegisterId,
        since: OffsetDateTime,
    ) -> AppResult<CashCount> {
        Ok(self
            .payment_totals_inner(store_id, register_id, since)
            .await?)
    }
}

#[async_trait]
impl InvoiceRepository for SqliteStore {
    async fn save_invoice(
        &self,
        invoice: &Invoice,
        sale: Option<&Sale>,
        event: &SyncEvent,
        audit: &AuditEvent,
    ) -> AppResult<()> {
        Ok(self.save_invoice_inner(invoice, sale, event, audit).await?)
    }

    async fn find(&self, id: InvoiceId) -> AppResult<Option<Invoice>> {
        Ok(self.load_invoice(id).await?)
    }

    async fn list_recent(&self, store_id: StoreId, limit: u32) -> AppResult<Vec<Invoice>> {
        let ids = self.list_invoice_ids(store_id, limit).await?;
        let mut invoices = Vec::with_capacity(ids.len());
        for id in ids {
            if let Some(invoice) = self.load_invoice(id).await? {
                invoices.push(invoice);
            }
        }
        Ok(invoices)
    }
}

#[async_trait]
impl SequenceRepository for SqliteStore {
    async fn next_number(
        &self,
        store_id: StoreId,
        document_type: DocumentType,
        series: &str,
        year: u16,
    ) -> AppResult<u64> {
        Ok(self
            .next_number_inner(store_id, document_type, series, year)
            .await?)
    }
}
