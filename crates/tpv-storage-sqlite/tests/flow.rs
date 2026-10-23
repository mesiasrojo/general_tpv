use std::sync::Arc;

use sqlx::Row;
use tpv_application::{
    AddLineRequest, ApplyPaymentRequest, AuditRepository, CashRepository, CashService,
    CloseCashRequest, ConfirmSaleRequest, InvoiceRepository, InvoiceService, OpenCashRequest,
    PrintQueueRepository, PrintService, SaleRepository, SaleService, SequenceRepository,
    StartSaleRequest, SyncContext, SyncOutboxRepository,
};
use tpv_domain::cash::CashCount;
use tpv_domain::document::DocumentType;
use tpv_domain::ids::{DeviceId, TenantId};
use tpv_domain::money::Money;
use tpv_domain::sale::PaymentMethod;
use tpv_print::{EscPosRenderer, FilePrinter};
use tpv_storage_sqlite::SqliteStore;

struct Services {
    store: Arc<SqliteStore>,
    demo: tpv_storage_sqlite::DemoData,
    sales: SaleService,
    invoices: InvoiceService,
    cash: CashService,
    sequence: Arc<dyn SequenceRepository>,
    audit: Arc<dyn AuditRepository>,
    print_service: PrintService,
}

async fn services() -> Services {
    let store = Arc::new(SqliteStore::in_memory().await.unwrap());
    let demo = store.seed_demo().await.unwrap();
    let catalog: Arc<dyn tpv_application::CatalogRepository> = store.clone();
    let sales_repo: Arc<dyn SaleRepository> = store.clone();
    let invoices_repo: Arc<dyn InvoiceRepository> = store.clone();
    let cash_repo: Arc<dyn CashRepository> = store.clone();
    let sequence: Arc<dyn SequenceRepository> = store.clone();
    let audit: Arc<dyn AuditRepository> = store.clone();
    let print_repo: Arc<dyn PrintQueueRepository> = store.clone();
    let clock: Arc<dyn tpv_application::Clock> = Arc::new(tpv_application::SystemClock);
    let sync = SyncContext {
        tenant_id: TenantId::new(),
        device_id: DeviceId::new(),
    };
    let sales = SaleService::new(
        sales_repo.clone(),
        catalog.clone(),
        clock.clone(),
        sequence.clone(),
        "TPV",
        sync.clone(),
    );
    let invoices = InvoiceService::new(
        invoices_repo,
        sales_repo.clone(),
        catalog.clone(),
        sequence.clone(),
        clock.clone(),
        sync.clone(),
    );
    let cash = CashService::new(cash_repo, sales_repo.clone(), clock.clone(), sync);
    let print_service = PrintService::new(
        sales_repo,
        catalog,
        print_repo,
        Arc::new(EscPosRenderer::default()),
        clock,
    );
    Services {
        store,
        demo,
        sales,
        invoices,
        cash,
        sequence,
        audit,
        print_service,
    }
}

#[tokio::test]
async fn confirms_sale_persists_ticket_and_outbox() {
    let services = services().await;
    let mut sale = services
        .sales
        .start_sale(StartSaleRequest {
            store_id: services.demo.store_id,
            register_id: services.demo.register_id,
            seller_id: services.demo.seller_id,
            customer_id: Some(services.demo.customer_id),
        })
        .await
        .unwrap();
    sale = services
        .sales
        .add_line(AddLineRequest {
            sale_id: sale.id,
            sku: "1".into(),
            quantity: "2".into(),
            kind: "sale".into(),
            discount_bp: None,
        })
        .await
        .unwrap();
    sale = services
        .sales
        .add_line(AddLineRequest {
            sale_id: sale.id,
            sku: "3".into(),
            quantity: "1".into(),
            kind: "sale".into(),
            discount_bp: None,
        })
        .await
        .unwrap();
    let totals = sale.totals().unwrap();
    let received = totals.gross.checked_add(Money::from_cents(500)).unwrap();
    sale = services
        .sales
        .apply_payment(ApplyPaymentRequest {
            sale_id: sale.id,
            method: PaymentMethod::Cash,
            amount_cents: totals.gross.cents(),
            received_cents: received.cents(),
            reference: None,
        })
        .await
        .unwrap();
    sale = services
        .sales
        .confirm_sale(ConfirmSaleRequest { sale_id: sale.id })
        .await
        .unwrap();
    assert_eq!(sale.ticket_number, Some(1));
    assert_eq!(sale.ticket_series.as_deref(), Some("TPV"));

    let persisted = SaleRepository::find(services.store.as_ref(), sale.id)
        .await
        .unwrap()
        .expect("sale must be persisted");
    assert_eq!(persisted.state, tpv_domain::sale::SaleState::Paid);
    assert_eq!(persisted.totals().unwrap().gross, totals.gross);
    assert_eq!(persisted.totals().unwrap().change, Money::from_cents(500));

    let outbox: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM sync_outbox")
        .fetch_one(services.store.pool())
        .await
        .unwrap();
    assert_eq!(outbox, 1);
}

#[tokio::test]
async fn issues_invoice_and_rejects_second_invoice() {
    let services = services().await;
    let mut sale = services
        .sales
        .start_sale(StartSaleRequest {
            store_id: services.demo.store_id,
            register_id: services.demo.register_id,
            seller_id: services.demo.seller_id,
            customer_id: Some(services.demo.customer_id),
        })
        .await
        .unwrap();
    sale = services
        .sales
        .add_line(AddLineRequest {
            sale_id: sale.id,
            sku: "2".into(),
            quantity: "1".into(),
            kind: "sale".into(),
            discount_bp: None,
        })
        .await
        .unwrap();
    let totals = sale.totals().unwrap();
    sale = services
        .sales
        .apply_payment(ApplyPaymentRequest {
            sale_id: sale.id,
            method: PaymentMethod::Card,
            amount_cents: totals.gross.cents(),
            received_cents: totals.gross.cents(),
            reference: None,
        })
        .await
        .unwrap();
    sale = services
        .sales
        .confirm_sale(ConfirmSaleRequest { sale_id: sale.id })
        .await
        .unwrap();

    let invoice = services
        .invoices
        .issue_invoice(tpv_application::IssueInvoiceRequest {
            sale_id: sale.id,
            customer_id: services.demo.customer_id,
            series: "FAC".into(),
            year: 2026,
        })
        .await
        .unwrap();
    assert_eq!(invoice.number, 1);
    assert_eq!(invoice.full_number(), "FAC-000001");
    assert_eq!(invoice.totals.gross, totals.gross);

    let stored = InvoiceRepository::find(services.store.as_ref(), invoice.id)
        .await
        .unwrap()
        .expect("invoice must be persisted");
    assert_eq!(stored.lines.len(), invoice.lines.len());
    assert!(
        services
            .invoices
            .issue_invoice(tpv_application::IssueInvoiceRequest {
                sale_id: sale.id,
                customer_id: services.demo.customer_id,
                series: "FAC".into(),
                year: 2026,
            })
            .await
            .is_err()
    );
}

#[tokio::test]
async fn closes_cash_with_expected_and_counted_amounts() {
    let services = services().await;
    let cash = services
        .cash
        .open(OpenCashRequest {
            store_id: services.demo.store_id,
            register_id: services.demo.register_id,
            opened_by: services.demo.user_id,
            opening_float_cents: 10_000,
        })
        .await
        .unwrap();

    let mut sale = services
        .sales
        .start_sale(StartSaleRequest {
            store_id: services.demo.store_id,
            register_id: services.demo.register_id,
            seller_id: services.demo.seller_id,
            customer_id: None,
        })
        .await
        .unwrap();
    sale = services
        .sales
        .add_line(AddLineRequest {
            sale_id: sale.id,
            sku: "4".into(),
            quantity: "1".into(),
            kind: "sale".into(),
            discount_bp: None,
        })
        .await
        .unwrap();
    let totals = sale.totals().unwrap();
    sale = services
        .sales
        .apply_payment(ApplyPaymentRequest {
            sale_id: sale.id,
            method: PaymentMethod::Cash,
            amount_cents: totals.gross.cents(),
            received_cents: totals.gross.cents(),
            reference: None,
        })
        .await
        .unwrap();
    services
        .sales
        .confirm_sale(ConfirmSaleRequest { sale_id: sale.id })
        .await
        .unwrap();

    let counted = Money::from_cents(10_000).checked_add(totals.gross).unwrap();
    let record = services
        .cash
        .close(CloseCashRequest {
            session_id: cash.id,
            closed_by: services.demo.user_id,
            counted: CashCount::new(counted, Money::ZERO, Money::ZERO),
        })
        .await
        .unwrap();
    assert_eq!(record.expected.cash, counted);
    assert_eq!(record.differences.cash, Money::ZERO);

    let stored = CashRepository::find(services.store.as_ref(), cash.id)
        .await
        .unwrap()
        .expect("cash session must exist");
    assert_eq!(stored.state, tpv_domain::cash::CashSessionState::Closed);
    assert_eq!(stored.close_history.len(), 1);
}

#[tokio::test]
async fn allocates_sequence_numbers_exactly_once() {
    let services = services().await;
    let first = services
        .sequence
        .next_number(services.demo.store_id, DocumentType::Ticket, "TPV", 2026)
        .await
        .unwrap();
    let second = services
        .sequence
        .next_number(services.demo.store_id, DocumentType::Ticket, "TPV", 2026)
        .await
        .unwrap();
    assert_eq!(first, 1);
    assert_eq!(second, 2);

    let row = sqlx::query("SELECT next_number FROM document_sequences WHERE series = 'TPV'")
        .fetch_one(services.store.pool())
        .await
        .unwrap();
    assert_eq!(row.get::<i64, _>("next_number"), 3);
}

#[tokio::test]
async fn writes_audit_trail() {
    let services = services().await;
    let cash = services
        .cash
        .open(OpenCashRequest {
            store_id: services.demo.store_id,
            register_id: services.demo.register_id,
            opened_by: services.demo.user_id,
            opening_float_cents: 0,
        })
        .await
        .unwrap();
    assert_eq!(cash.state, tpv_domain::cash::CashSessionState::Open);
    let events = services
        .audit
        .list_recent(Some(services.demo.store_id), 10)
        .await
        .unwrap();
    assert!(!events.is_empty());
    assert!(
        events
            .iter()
            .any(|event| matches!(event.action, tpv_domain::audit::AuditAction::CashOpened))
    );
}

#[tokio::test]
async fn outbox_can_be_read_and_marked_as_sent() {
    let services = services().await;
    let mut sale = services
        .sales
        .start_sale(StartSaleRequest {
            store_id: services.demo.store_id,
            register_id: services.demo.register_id,
            seller_id: services.demo.seller_id,
            customer_id: None,
        })
        .await
        .unwrap();
    sale = services
        .sales
        .add_line(AddLineRequest {
            sale_id: sale.id,
            sku: "3".into(),
            quantity: "1".into(),
            kind: "sale".into(),
            discount_bp: None,
        })
        .await
        .unwrap();
    let totals = sale.totals().unwrap();
    sale = services
        .sales
        .apply_payment(ApplyPaymentRequest {
            sale_id: sale.id,
            method: PaymentMethod::Cash,
            amount_cents: totals.gross.cents(),
            received_cents: totals.gross.cents(),
            reference: None,
        })
        .await
        .unwrap();
    services
        .sales
        .confirm_sale(ConfirmSaleRequest { sale_id: sale.id })
        .await
        .unwrap();

    let pending = services.store.pending_events(100).await.unwrap();
    assert_eq!(pending.len(), 1);
    assert!(pending[0].verify_hash());
    let ids = pending
        .iter()
        .map(|event| event.event_id)
        .collect::<Vec<_>>();
    services.store.mark_events_sent(&ids).await.unwrap();
    assert!(services.store.pending_events(100).await.unwrap().is_empty());
}

#[tokio::test]
async fn prints_ticket_through_persistent_queue() {
    let services = services().await;
    let mut sale = services
        .sales
        .start_sale(StartSaleRequest {
            store_id: services.demo.store_id,
            register_id: services.demo.register_id,
            seller_id: services.demo.seller_id,
            customer_id: Some(services.demo.customer_id),
        })
        .await
        .unwrap();
    sale = services
        .sales
        .add_line(AddLineRequest {
            sale_id: sale.id,
            sku: "1".into(),
            quantity: "1".into(),
            kind: "sale".into(),
            discount_bp: None,
        })
        .await
        .unwrap();
    let totals = sale.totals().unwrap();
    sale = services
        .sales
        .apply_payment(ApplyPaymentRequest {
            sale_id: sale.id,
            method: PaymentMethod::Cash,
            amount_cents: totals.gross.cents(),
            received_cents: totals.gross.cents(),
            reference: None,
        })
        .await
        .unwrap();
    sale = services
        .sales
        .confirm_sale(ConfirmSaleRequest { sale_id: sale.id })
        .await
        .unwrap();

    let job = services
        .print_service
        .enqueue_ticket(sale.id, false)
        .await
        .unwrap();
    assert!(!job.bytes.is_empty());
    assert_eq!(services.store.pending(10).await.unwrap().len(), 1);

    let path = std::env::temp_dir().join(format!("tpv-flow-ticket-{}.escpos", std::process::id()));
    let printer = FilePrinter::new(&path);
    let results = services
        .print_service
        .process_pending(&printer, 10)
        .await
        .unwrap();
    assert_eq!(results.len(), 1);
    assert!(results[0].success);
    assert!(path.exists());
    assert!(services.store.pending(10).await.unwrap().is_empty());
    let recent = PrintQueueRepository::list_recent(services.store.as_ref(), 10)
        .await
        .unwrap();
    assert_eq!(recent.len(), 1);
    assert_eq!(recent[0].status, tpv_application::PrintJobStatus::Printed);
    let _ = std::fs::remove_file(path);
}
