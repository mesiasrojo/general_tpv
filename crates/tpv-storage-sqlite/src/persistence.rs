use sqlx::{Sqlite, Transaction};
use tpv_domain::audit::AuditEvent;
use tpv_domain::sale::Sale;
use tpv_domain::sync::SyncEvent;

use crate::error::StorageResult;
use crate::helpers::{enum_to_text, format_time};

pub(crate) async fn enqueue_event_tx(
    tx: &mut Transaction<'_, Sqlite>,
    event: &SyncEvent,
) -> StorageResult<()> {
    let payload = serde_json::to_string(&event.payload)?;
    let envelope = serde_json::to_string(event)?;
    sqlx::query(
        "INSERT INTO sync_outbox
            (event_id, event_type, aggregate_type, aggregate_id, payload_json, envelope_json, created_at, sent_at)
         VALUES (?, ?, ?, ?, ?, ?, ?, NULL)
         ON CONFLICT(event_id) DO NOTHING",
    )
    .bind(event.event_id.to_string())
    .bind(&event.event_type)
    .bind(&event.aggregate_type)
    .bind(event.aggregate_id.to_string())
    .bind(payload)
    .bind(envelope)
    .bind(format_time(event.occurred_at)?)
    .execute(&mut **tx)
    .await?;
    Ok(())
}

pub(crate) async fn insert_audit_tx(
    tx: &mut Transaction<'_, Sqlite>,
    audit: &AuditEvent,
) -> StorageResult<()> {
    let action = enum_to_text(&audit.action)?;
    let details = serde_json::to_string(&audit.details)?;
    sqlx::query(
        "INSERT INTO audit_log
            (id, occurred_at, actor_id, store_id, action, entity_type, entity_id, details_json)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(audit.id.to_string())
    .bind(format_time(audit.occurred_at)?)
    .bind(audit.actor_id.map(|id| id.to_string()))
    .bind(audit.store_id.map(|id| id.to_string()))
    .bind(action)
    .bind(&audit.entity_type)
    .bind(&audit.entity_id)
    .bind(details)
    .execute(&mut **tx)
    .await?;
    Ok(())
}

pub(crate) async fn persist_sale_tx(
    tx: &mut Transaction<'_, Sqlite>,
    sale: &Sale,
) -> StorageResult<()> {
    let totals = sale.totals()?;
    let state = enum_to_text(&sale.state)?;
    sqlx::query(
        "INSERT INTO sales
            (id, store_id, register_id, seller_id, customer_id, price_list_id, state, created_at,
             closed_at, invoice_id, ticket_series, ticket_number, notes, total_gross_cents,
             total_net_cents, total_tax_cents, paid_cents, change_cents)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
         ON CONFLICT(id) DO UPDATE SET
            state = excluded.state,
            closed_at = excluded.closed_at,
            invoice_id = excluded.invoice_id,
            ticket_series = excluded.ticket_series,
            ticket_number = excluded.ticket_number,
            notes = excluded.notes,
            total_gross_cents = excluded.total_gross_cents,
            total_net_cents = excluded.total_net_cents,
            total_tax_cents = excluded.total_tax_cents,
            paid_cents = excluded.paid_cents,
            change_cents = excluded.change_cents",
    )
    .bind(sale.id.to_string())
    .bind(sale.store_id.to_string())
    .bind(sale.register_id.to_string())
    .bind(sale.seller_id.to_string())
    .bind(sale.customer_id.map(|id| id.to_string()))
    .bind(sale.price_list_id.map(|id| id.to_string()))
    .bind(state)
    .bind(format_time(sale.created_at)?)
    .bind(sale.closed_at.map(format_time).transpose()?)
    .bind(sale.invoice_id.map(|id| id.to_string()))
    .bind(sale.ticket_series.as_deref())
    .bind(sale.ticket_number.map(|number| number as i64))
    .bind(sale.notes.as_deref())
    .bind(totals.gross.cents())
    .bind(totals.net.cents())
    .bind(totals.tax.cents())
    .bind(totals.paid.cents())
    .bind(totals.change.cents())
    .execute(&mut **tx)
    .await?;

    sqlx::query("DELETE FROM sale_lines WHERE sale_id = ?")
        .bind(sale.id.to_string())
        .execute(&mut **tx)
        .await?;
    sqlx::query("DELETE FROM payments WHERE sale_id = ?")
        .bind(sale.id.to_string())
        .execute(&mut **tx)
        .await?;

    for (index, line) in sale.lines.iter().enumerate() {
        let kind = enum_to_text(&line.kind)?;
        sqlx::query(
            "INSERT INTO sale_lines
                (id, sale_id, product_id, sku, description, unit_price, quantity,
                 discount_bp, tax_code, tax_percent_bp, kind, line_index)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(line.id.to_string())
        .bind(sale.id.to_string())
        .bind(line.product_id.map(|id| id.to_string()))
        .bind(line.sku.as_str())
        .bind(&line.description)
        .bind(line.unit_price.to_string())
        .bind(line.quantity.as_decimal().to_string())
        .bind(i64::from(line.discount_bp))
        .bind(&line.tax_rate.code)
        .bind(i64::from(line.tax_rate.percent_bp))
        .bind(kind)
        .bind(index as i64)
        .execute(&mut **tx)
        .await?;
    }

    for (index, payment) in sale.payments.iter().enumerate() {
        let method = enum_to_text(&payment.method)?;
        sqlx::query(
            "INSERT INTO payments
                (id, sale_id, method, amount_cents, received_cents, reference, payment_index)
             VALUES (?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(payment.id.to_string())
        .bind(sale.id.to_string())
        .bind(method)
        .bind(payment.amount.cents())
        .bind(payment.received.cents())
        .bind(payment.reference.as_deref())
        .bind(index as i64)
        .execute(&mut **tx)
        .await?;
    }
    Ok(())
}
