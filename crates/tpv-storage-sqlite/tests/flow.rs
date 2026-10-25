use std::sync::Arc;

use rust_decimal::Decimal;
use sqlx::Row;
use time::{Duration, OffsetDateTime};
use tpv_application::{
    AddLineRequest, ApplyPaymentRequest, AuditRepository, AuthService, CashRepository, CashService,
    CatalogRepository, CloseCashRequest, ConfirmSaleRequest, CreateUserRequest,
    InvoiceLineSelection, InvoiceRepository, InvoiceService, MasterDataService, OpenCashRequest,
    PriceListItemRequest, PriceListService, PrintQueueRepository, PrintService,
    QueryStockMovementsRequest, RecordStockMovementRequest, RectifyInvoiceLinesRequest,
    ResetPasswordRequest, SaleQuery, SaleRepository, SaleService, SequenceRepository,
    SetStockCountRequest, SetStockMinimumRequest, StartSaleRequest, StockLevelStatus,
    StockLevelsQuery, StockRepository, StockService, SyncContext, SyncOutboxRepository,
    SyncService, UpdateUserRequest, UpsertPriceListRequest, UserRepository,
};
use tpv_auth::Argon2PasswordHasher;
use tpv_domain::audit::{AuditAction, AuditEvent};
use tpv_domain::cash::CashCount;
use tpv_domain::document::DocumentType;
use tpv_domain::ids::{DeviceId, StoreId, TenantId};
use tpv_domain::invoice::InvoiceKind;
use tpv_domain::money::Money;
use tpv_domain::people::{Role, Store};
use tpv_domain::sale::{PaymentMethod, Sale, SaleState};
use tpv_domain::stock::StockMovementKind;
use tpv_domain::sync::SyncEvent;
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
    auth: AuthService,
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
    let user_repo: Arc<dyn UserRepository> = store.clone();
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
    let auth = AuthService::new(
        user_repo,
        Arc::new(Argon2PasswordHasher),
        audit.clone(),
        clock.clone(),
    );
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
        auth,
    }
}

async fn confirmed_sale(services: &Services, sku: &str, quantity: &str) -> Sale {
    let mut sale = services
        .sales
        .start_sale(StartSaleRequest {
            store_id: services.demo.store_id,
            register_id: services.demo.register_id,
            seller_id: services.demo.seller_id,
            customer_id: Some(services.demo.customer_id),
            price_list_id: None,
        })
        .await
        .unwrap();
    sale = services
        .sales
        .add_line(AddLineRequest {
            sale_id: sale.id,
            sku: sku.into(),
            quantity: quantity.into(),
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
        .unwrap()
}

#[tokio::test]
async fn sale_queries_filter_paginate_and_summarize() {
    let services = services().await;
    let first = confirmed_sale(&services, "1", "2").await;
    let second = confirmed_sale(&services, "3", "1").await;

    let all = services
        .sales
        .history(SaleQuery {
            store_id: services.demo.store_id,
            limit: 1,
            offset: 0,
            from: None,
            to: None,
            state: Some(SaleState::Paid),
            ticket_number: None,
        })
        .await
        .unwrap();
    assert_eq!(all.total, 2);
    assert_eq!(all.items.len(), 1);
    assert_eq!(all.items[0].sale.id, second.id);

    let expected_gross = first
        .totals()
        .unwrap()
        .gross
        .checked_add(second.totals().unwrap().gross)
        .unwrap();
    assert_eq!(all.gross, expected_gross);

    let page = services
        .sales
        .history(SaleQuery {
            store_id: services.demo.store_id,
            limit: 1,
            offset: 1,
            from: None,
            to: None,
            state: Some(SaleState::Paid),
            ticket_number: None,
        })
        .await
        .unwrap();
    assert_eq!(page.total, 2);
    assert_eq!(page.items[0].sale.id, first.id);

    let by_ticket = services
        .sales
        .history(SaleQuery {
            store_id: services.demo.store_id,
            limit: 10,
            offset: 0,
            from: None,
            to: None,
            state: None,
            ticket_number: second.ticket_number,
        })
        .await
        .unwrap();
    assert_eq!(by_ticket.total, 1);
    assert_eq!(by_ticket.items[0].sale.id, second.id);

    let future = OffsetDateTime::now_utc() + Duration::days(1);
    let empty = services
        .sales
        .history(SaleQuery {
            store_id: services.demo.store_id,
            limit: 10,
            offset: 0,
            from: Some(future),
            to: None,
            state: None,
            ticket_number: None,
        })
        .await
        .unwrap();
    assert_eq!(empty.total, 0);
    assert!(empty.items.is_empty());

    let invalid = services
        .sales
        .history(SaleQuery {
            store_id: services.demo.store_id,
            limit: 10,
            offset: 0,
            from: Some(future),
            to: Some(future - Duration::days(2)),
            state: None,
            ticket_number: None,
        })
        .await;
    assert!(matches!(
        invalid,
        Err(tpv_application::ApplicationError::Validation(_))
    ));
}

#[tokio::test]
async fn local_identity_is_created_once_and_stays_stable() {
    let services = services().await;
    let first = services.store.sync_context().await.unwrap();
    let second = services.store.sync_context().await.unwrap();
    assert_eq!(first.device_id.as_uuid(), second.device_id.as_uuid());
    assert_eq!(first.tenant_id.as_uuid(), second.tenant_id.as_uuid());

    assert!(services.store.device_token().await.unwrap().is_none());
    services
        .store
        .save_device_enrollment("tpvdev_test", "Caja 1")
        .await
        .unwrap();
    assert_eq!(
        services.store.device_token().await.unwrap().as_deref(),
        Some("tpvdev_test")
    );
    services.store.clear_device_enrollment().await.unwrap();
    assert!(services.store.device_token().await.unwrap().is_none());
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
            price_list_id: None,
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
            price_list_id: None,
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

    let partial = services
        .invoices
        .rectify_invoice_lines(RectifyInvoiceLinesRequest {
            invoice_id: invoice.id,
            series: "REC".into(),
            year: 2026,
            reason: "Devolución parcial".into(),
            refund_reference: Some("AUTH-123".into()),
            lines: vec![InvoiceLineSelection {
                source_line_id: invoice.lines[0].source_line_id,
                quantity: "0.5".into(),
            }],
        })
        .await
        .unwrap();
    assert_eq!(partial.full_number(), "REC-000001");
    assert_eq!(partial.lines.len(), 1);
    assert_eq!(partial.lines[0].quantity.as_decimal(), Decimal::new(5, 1));
    assert_eq!(partial.totals.gross, Money::from_cents(-25));
    match &partial.kind {
        InvoiceKind::Rectificative {
            refund_reference, ..
        } => assert_eq!(refund_reference.as_deref(), Some("AUTH-123")),
        InvoiceKind::Standard => panic!("expected a rectificative invoice"),
    }
    partial.validate_totals().unwrap();

    let sku = tpv_domain::Sku::new("2").unwrap();
    let catalog: Arc<dyn CatalogRepository> = services.store.clone();
    let product = catalog.find_product_by_sku(&sku).await.unwrap().unwrap();
    let levels = StockRepository::list_levels(services.store.as_ref(), services.demo.store_id)
        .await
        .unwrap();
    let level = levels
        .into_iter()
        .find(|level| level.product_id == product.id)
        .unwrap();
    assert_eq!(level.quantity, Decimal::new(-5, 1));
    let movements = StockRepository::list_movements(
        services.store.as_ref(),
        services.demo.store_id,
        product.id,
        20,
    )
    .await
    .unwrap();
    let returned = movements
        .iter()
        .find(|movement| movement.kind == StockMovementKind::Return)
        .expect("partial return must replenish stock");
    assert_eq!(returned.quantity.as_decimal(), Decimal::new(5, 1));
    assert_eq!(returned.sale_id, Some(sale.id));
    assert!(returned.sale_line_id.is_none());

    let second_partial = services
        .invoices
        .rectify_invoice_lines(RectifyInvoiceLinesRequest {
            invoice_id: invoice.id,
            series: "REC".into(),
            year: 2026,
            reason: "Segunda devolución parcial".into(),
            refund_reference: Some("AUTH-124".into()),
            lines: vec![InvoiceLineSelection {
                source_line_id: invoice.lines[0].source_line_id,
                quantity: "0.5".into(),
            }],
        })
        .await
        .unwrap();
    assert_eq!(second_partial.full_number(), "REC-000002");
    let levels = StockRepository::list_levels(services.store.as_ref(), services.demo.store_id)
        .await
        .unwrap();
    let level = levels
        .into_iter()
        .find(|level| level.product_id == product.id)
        .unwrap();
    assert_eq!(level.quantity, Decimal::ZERO);
    let movements = StockRepository::list_movements(
        services.store.as_ref(),
        services.demo.store_id,
        product.id,
        20,
    )
    .await
    .unwrap();
    assert_eq!(
        movements
            .iter()
            .filter(|movement| movement.kind == StockMovementKind::Return)
            .count(),
        2
    );

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
            price_list_id: None,
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
            price_list_id: None,
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
async fn quarantines_failed_events_and_can_requeue_them() {
    let services = services().await;
    let mut sale = services
        .sales
        .start_sale(StartSaleRequest {
            store_id: services.demo.store_id,
            register_id: services.demo.register_id,
            seller_id: services.demo.seller_id,
            customer_id: None,
            price_list_id: None,
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

    let sync = SyncService::new(services.store.clone());
    let initial = sync.status().await.unwrap();
    assert_eq!(initial.pending, 1);
    assert_eq!(initial.failed, 0);
    assert_eq!(initial.sent, 0);

    let pending = services.store.pending_events(100).await.unwrap();
    assert_eq!(pending.len(), 1);
    let event_id = pending[0].event_id;
    services
        .store
        .mark_events_failed(&[event_id], "hash inválido")
        .await
        .unwrap();

    assert!(services.store.pending_events(100).await.unwrap().is_empty());
    let failed_status = sync.status().await.unwrap();
    assert_eq!(failed_status.pending, 0);
    assert_eq!(failed_status.failed, 1);
    assert_eq!(failed_status.last_error.as_deref(), Some("hash inválido"));
    assert!(failed_status.last_failed_at.is_some());

    let quarantine = sync.quarantine(10).await.unwrap();
    assert_eq!(quarantine.len(), 1);
    assert_eq!(quarantine[0].event.event_id, event_id);
    assert_eq!(quarantine[0].attempts, 1);
    assert_eq!(quarantine[0].last_error.as_deref(), Some("hash inválido"));

    assert_eq!(sync.retry_failed(&[event_id]).await.unwrap(), 1);
    let requeued = sync.status().await.unwrap();
    assert_eq!(requeued.pending, 1);
    assert_eq!(requeued.failed, 0);
    assert!(requeued.last_error.is_none());
    assert!(sync.quarantine(10).await.unwrap().is_empty());

    services.store.mark_events_sent(&[event_id]).await.unwrap();
    let sent = sync.status().await.unwrap();
    assert_eq!(sent.pending, 0);
    assert_eq!(sent.failed, 0);
    assert_eq!(sent.sent, 1);
    assert!(sent.last_sent_at.is_some());
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
            price_list_id: None,
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

#[tokio::test]
async fn authenticates_admin_and_rejects_bad_password() {
    let services = services().await;
    services
        .auth
        .bootstrap_password("admin", "admin123")
        .await
        .unwrap();
    let session = services.auth.login("admin", "admin123").await.unwrap();
    assert_eq!(session.user.username, "admin");
    assert_eq!(session.user.role, tpv_domain::people::Role::Admin);
    assert!(
        services
            .auth
            .login("admin", "wrong-password")
            .await
            .is_err()
    );
}

#[tokio::test]
async fn password_hash_is_not_stored_in_plain_text() {
    let services = services().await;
    services
        .auth
        .bootstrap_password("admin", "admin123")
        .await
        .unwrap();
    let stored = services
        .store
        .find_by_username("admin")
        .await
        .unwrap()
        .expect("admin must exist");
    let hash = stored.password_hash.expect("hash must exist");
    assert_ne!(hash, "admin123");
    assert!(hash.starts_with("$argon2"));
}

#[tokio::test]
async fn admin_manages_users_and_audits_changes() {
    let services = services().await;
    services
        .auth
        .bootstrap_password("admin", "admin123")
        .await
        .unwrap();
    let admin = services.auth.login("admin", "admin123").await.unwrap();

    let created = services
        .auth
        .create_user(
            &admin,
            CreateUserRequest {
                username: "cajero".into(),
                display_name: "Cajero Uno".into(),
                role: Role::Cashier,
                store_ids: vec![services.demo.store_id],
                password: "cajero123".into(),
            },
        )
        .await
        .unwrap();
    assert_eq!(created.role, Role::Cashier);
    assert_eq!(created.store_ids, vec![services.demo.store_id]);
    assert!(
        services
            .auth
            .list_users(&admin)
            .await
            .unwrap()
            .iter()
            .any(|user| user.id == created.id)
    );

    let updated = services
        .auth
        .update_user(
            &admin,
            UpdateUserRequest {
                user_id: created.id,
                display_name: "Supervisor".into(),
                role: Role::Manager,
                store_ids: vec![services.demo.store_id],
                active: true,
            },
        )
        .await
        .unwrap();
    assert_eq!(updated.role, Role::Manager);
    assert_eq!(updated.display_name, "Supervisor");

    services
        .auth
        .reset_password(
            &admin,
            ResetPasswordRequest {
                user_id: created.id,
                new_password: "nueva1234".into(),
            },
        )
        .await
        .unwrap();
    let logged = services.auth.login("cajero", "nueva1234").await.unwrap();
    assert_eq!(logged.user.id, created.id);

    let deactivated = services
        .auth
        .update_user(
            &admin,
            UpdateUserRequest {
                user_id: created.id,
                display_name: "Supervisor".into(),
                role: Role::Manager,
                store_ids: vec![],
                active: false,
            },
        )
        .await
        .unwrap();
    assert!(!deactivated.active);
    assert!(services.auth.login("cajero", "nueva1234").await.is_err());

    let audits = services.audit.list_recent(None, 50).await.unwrap();
    for action in [
        tpv_domain::audit::AuditAction::UserCreated,
        tpv_domain::audit::AuditAction::UserUpdated,
        tpv_domain::audit::AuditAction::PasswordReset,
        tpv_domain::audit::AuditAction::UserDeactivated,
    ] {
        assert!(audits.iter().any(|event| event.action == action));
    }
    let update_audit = audits
        .iter()
        .find(|event| event.action == tpv_domain::audit::AuditAction::UserUpdated)
        .unwrap();
    assert_eq!(update_audit.actor_id, Some(admin.user.id));
}

#[tokio::test]
async fn cashier_cannot_manage_users() {
    let services = services().await;
    services
        .auth
        .bootstrap_password("admin", "admin123")
        .await
        .unwrap();
    let admin = services.auth.login("admin", "admin123").await.unwrap();
    services
        .auth
        .create_user(
            &admin,
            CreateUserRequest {
                username: "cajero".into(),
                display_name: "Cajero Uno".into(),
                role: Role::Cashier,
                store_ids: vec![services.demo.store_id],
                password: "cajero123".into(),
            },
        )
        .await
        .unwrap();
    let cashier = services.auth.login("cajero", "cajero123").await.unwrap();

    assert!(services.auth.list_users(&cashier).await.is_err());
    assert!(
        services
            .auth
            .create_user(
                &cashier,
                CreateUserRequest {
                    username: "otro".into(),
                    display_name: "Otro".into(),
                    role: Role::Cashier,
                    store_ids: vec![],
                    password: "otro12345".into(),
                },
            )
            .await
            .is_err()
    );
}

#[tokio::test]
async fn cannot_demote_or_deactivate_last_active_admin() {
    let services = services().await;
    services
        .auth
        .bootstrap_password("admin", "admin123")
        .await
        .unwrap();
    let admin = services.auth.login("admin", "admin123").await.unwrap();

    let demote = services
        .auth
        .update_user(
            &admin,
            UpdateUserRequest {
                user_id: admin.user.id,
                display_name: admin.user.display_name.clone(),
                role: Role::Cashier,
                store_ids: vec![],
                active: true,
            },
        )
        .await;
    assert!(matches!(
        demote,
        Err(tpv_application::ApplicationError::Conflict(_))
    ));

    let deactivate = services
        .auth
        .update_user(
            &admin,
            UpdateUserRequest {
                user_id: admin.user.id,
                display_name: admin.user.display_name.clone(),
                role: Role::Admin,
                store_ids: vec![],
                active: false,
            },
        )
        .await;
    assert!(matches!(
        deactivate,
        Err(tpv_application::ApplicationError::Validation(_))
    ));
}

#[tokio::test]
async fn sale_lines_use_the_price_list_override() {
    let services = services().await;
    let catalog: Arc<dyn tpv_application::CatalogRepository> = services.store.clone();
    let master_repo: Arc<dyn tpv_application::MasterDataRepository> = services.store.clone();
    let context = services.store.sync_context().await.unwrap();
    let master_data = MasterDataService::new(master_repo, Arc::new(tpv_application::SystemClock));
    let price_lists = PriceListService::new(catalog.clone(), master_data);

    let sku = tpv_domain::Sku::new("1").unwrap();
    let product = catalog.find_product_by_sku(&sku).await.unwrap().unwrap();
    assert_eq!(product.unit_price, Decimal::new(10, 2));

    let snapshot = price_lists
        .upsert(
            &context,
            services.demo.store_id,
            UpsertPriceListRequest {
                price_list_id: None,
                code: "empleados".into(),
                name: "Empleados".into(),
                items: vec![PriceListItemRequest {
                    product_id: product.id,
                    unit_price: "0,08".into(),
                }],
                active: true,
            },
        )
        .await
        .unwrap();
    assert_eq!(snapshot.price_list.code, "EMPLEADOS");
    assert_eq!(snapshot.items.len(), 1);
    assert_eq!(price_lists.list().await.unwrap().len(), 1);

    let mut sale = services
        .sales
        .start_sale(StartSaleRequest {
            store_id: services.demo.store_id,
            register_id: services.demo.register_id,
            seller_id: services.demo.seller_id,
            customer_id: Some(services.demo.customer_id),
            price_list_id: Some(snapshot.price_list.id),
        })
        .await
        .unwrap();
    assert_eq!(sale.price_list_id, Some(snapshot.price_list.id));

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
    assert_eq!(sale.lines[0].unit_price, Decimal::new(8, 2));
    assert_eq!(sale.totals().unwrap().gross, Money::from_cents(8));

    let inactive = price_lists
        .upsert(
            &context,
            services.demo.store_id,
            UpsertPriceListRequest {
                price_list_id: None,
                code: "antigua".into(),
                name: "Tarifa antigua".into(),
                items: vec![],
                active: false,
            },
        )
        .await
        .unwrap();
    let rejected = services
        .sales
        .start_sale(StartSaleRequest {
            store_id: services.demo.store_id,
            register_id: services.demo.register_id,
            seller_id: services.demo.seller_id,
            customer_id: Some(services.demo.customer_id),
            price_list_id: Some(inactive.price_list.id),
        })
        .await;
    assert!(matches!(
        rejected,
        Err(tpv_application::ApplicationError::Validation(_))
    ));
}

#[tokio::test]
async fn stock_tracks_manual_movements_and_sale_confirmation() {
    let services = services().await;
    let catalog: Arc<dyn tpv_application::CatalogRepository> = services.store.clone();
    let stock_repo: Arc<dyn StockRepository> = services.store.clone();
    let stock = StockService::new(
        stock_repo,
        catalog.clone(),
        Arc::new(tpv_application::SystemClock),
    );
    let other_store = Store::new(StoreId::new(), "ST2", "Tienda secundaria", "B12345674").unwrap();
    catalog.save_store(&other_store).await.unwrap();
    let sku = tpv_domain::Sku::new("1").unwrap();
    let product = catalog.find_product_by_sku(&sku).await.unwrap().unwrap();

    let initial = stock
        .list_levels(services.demo.store_id)
        .await
        .unwrap()
        .into_iter()
        .find(|level| level.product_id == product.id)
        .expect("every product has a stock level");
    assert_eq!(initial.quantity, Decimal::ZERO);
    let other_initial = stock
        .get_level(other_store.id, product.id)
        .await
        .unwrap()
        .expect("every store has an independent stock level");
    assert_eq!(other_initial.quantity, Decimal::ZERO);
    assert_eq!(other_initial.min_quantity, Decimal::ZERO);
    assert_eq!(
        stock.count_low_stock(services.demo.store_id).await.unwrap(),
        0
    );
    assert_eq!(stock.count_low_stock(other_store.id).await.unwrap(), 0);

    stock
        .record_movement(
            services.demo.store_id,
            None,
            RecordStockMovementRequest {
                product_id: product.id,
                kind: StockMovementKind::Purchase,
                quantity: "5".into(),
                reason: Some("Reposición inicial".into()),
            },
        )
        .await
        .unwrap();

    let sale = confirmed_sale(&services, "1", "2").await;
    let after_sale = stock
        .get_level(services.demo.store_id, product.id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(after_sale.quantity, Decimal::new(3, 0));
    let visible = stock
        .levels_for_products(services.demo.store_id, &[product.id])
        .await
        .unwrap();
    assert_eq!(visible.len(), 1);
    assert_eq!(visible[0].quantity, Decimal::new(3, 0));
    assert!(
        stock
            .levels_for_products(services.demo.store_id, &[])
            .await
            .unwrap()
            .is_empty()
    );
    let other_visible = stock
        .levels_for_products(other_store.id, &[product.id])
        .await
        .unwrap();
    assert_eq!(other_visible.len(), 1);
    assert_eq!(other_visible[0].quantity, Decimal::ZERO);
    let other_after_sale = stock
        .get_level(other_store.id, product.id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(other_after_sale.quantity, Decimal::ZERO);

    let movements = stock
        .list_movements(services.demo.store_id, product.id, 20)
        .await
        .unwrap();
    assert_eq!(movements.len(), 2);
    assert_eq!(movements[0].kind, StockMovementKind::Sale);
    assert_eq!(movements[0].quantity.as_decimal(), Decimal::new(2, 0));
    assert_eq!(movements[0].sale_id, Some(sale.id));
    assert!(movements[0].sale_line_id.is_some());
    assert_eq!(
        stock
            .list_movements_for_export(services.demo.store_id, product.id)
            .await
            .unwrap()
            .len(),
        2
    );

    // Replaying the same confirmation must not discount stock twice.
    let replay_event = SyncEvent::new(
        TenantId::new(),
        services.demo.store_id,
        DeviceId::new(),
        99,
        "sale",
        sale.id.as_uuid(),
        "sale.confirmed",
        1,
        sale.closed_at.unwrap(),
        serde_json::to_value(&sale).unwrap(),
    )
    .unwrap();
    let replay_audit = AuditEvent::new(
        sale.closed_at.unwrap(),
        None,
        Some(services.demo.store_id),
        AuditAction::SaleConfirmed,
        "sale",
        sale.id.to_string(),
        serde_json::json!({ "replay": true }),
    )
    .unwrap();
    services
        .store
        .confirm_sale(&sale, &replay_event, &replay_audit)
        .await
        .unwrap();
    let after_replay = stock
        .get_level(services.demo.store_id, product.id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(after_replay.quantity, Decimal::new(3, 0));
    assert_eq!(
        stock
            .list_movements(services.demo.store_id, product.id, 20)
            .await
            .unwrap()
            .len(),
        2
    );

    let audits = services
        .audit
        .list_recent(Some(services.demo.store_id), 30)
        .await
        .unwrap();
    assert!(
        audits
            .iter()
            .any(|event| matches!(event.action, AuditAction::StockMovement))
    );

    // Draft sales never touch stock.
    let mut draft = services
        .sales
        .start_sale(StartSaleRequest {
            store_id: services.demo.store_id,
            register_id: services.demo.register_id,
            seller_id: services.demo.seller_id,
            customer_id: Some(services.demo.customer_id),
            price_list_id: None,
        })
        .await
        .unwrap();
    draft = services
        .sales
        .add_line(AddLineRequest {
            sale_id: draft.id,
            sku: "1".into(),
            quantity: "1".into(),
            kind: "sale".into(),
            discount_bp: None,
        })
        .await
        .unwrap();
    assert_eq!(draft.state, SaleState::Priced);
    let after_draft = stock
        .get_level(services.demo.store_id, product.id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(after_draft.quantity, Decimal::new(3, 0));

    let low = stock
        .set_minimum(
            services.demo.store_id,
            None,
            SetStockMinimumRequest {
                product_id: product.id,
                min_quantity: "4".into(),
            },
        )
        .await
        .unwrap();
    assert!(low.is_low());
    assert_eq!(
        stock.count_low_stock(services.demo.store_id).await.unwrap(),
        1
    );
    assert_eq!(stock.count_low_stock(other_store.id).await.unwrap(), 0);
    let other_after_minimum = stock
        .get_level(other_store.id, product.id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(other_after_minimum.min_quantity, Decimal::ZERO);
    assert_eq!(other_after_minimum.quantity, Decimal::ZERO);
    assert!(
        stock
            .list_movements(other_store.id, product.id, 20)
            .await
            .unwrap()
            .is_empty()
    );
    let audits = services
        .audit
        .list_recent(Some(services.demo.store_id), 30)
        .await
        .unwrap();
    assert!(
        audits
            .iter()
            .any(|event| matches!(event.action, AuditAction::StockMinimumChanged))
    );

    let counted = stock
        .set_count(
            services.demo.store_id,
            None,
            SetStockCountRequest {
                product_id: product.id,
                counted_quantity: "1,5".into(),
                reason: "Conteo físico".into(),
            },
        )
        .await
        .unwrap();
    assert_eq!(counted.previous_quantity, Decimal::new(3, 0));
    assert_eq!(counted.counted_quantity, Decimal::new(15, 1));
    assert_eq!(counted.difference, Decimal::new(-15, 1));
    assert_eq!(counted.level.quantity, Decimal::new(15, 1));
    let count_movement = counted.movement.expect("count must create an adjustment");
    assert_eq!(count_movement.kind, StockMovementKind::AdjustmentOut);
    assert_eq!(count_movement.quantity.as_decimal(), Decimal::new(15, 1));
    assert_eq!(count_movement.reason.as_deref(), Some("Conteo físico"));
    assert_eq!(
        stock
            .list_movements(services.demo.store_id, product.id, 20)
            .await
            .unwrap()
            .len(),
        3
    );

    // Repeating the same count is auditable but does not create a zero move.
    let unchanged = stock
        .set_count(
            services.demo.store_id,
            None,
            SetStockCountRequest {
                product_id: product.id,
                counted_quantity: "1.5".into(),
                reason: "Segundo conteo sin diferencias".into(),
            },
        )
        .await
        .unwrap();
    assert!(unchanged.difference.is_zero());
    assert!(unchanged.movement.is_none());
    assert_eq!(unchanged.level.quantity, Decimal::new(15, 1));
    assert_eq!(
        stock
            .list_movements(services.demo.store_id, product.id, 20)
            .await
            .unwrap()
            .len(),
        3
    );
    let audits = services
        .audit
        .list_recent(Some(services.demo.store_id), 40)
        .await
        .unwrap();
    assert!(
        audits
            .iter()
            .filter(|event| matches!(event.action, AuditAction::StockCount))
            .count()
            >= 2
    );

    let ledger = stock
        .query_movements(
            services.demo.store_id,
            QueryStockMovementsRequest {
                search: None,
                product_id: None,
                kind: None,
                from_ms: None,
                to_ms: None,
                limit: 20,
                offset: 0,
            },
        )
        .await
        .unwrap();
    assert_eq!(ledger.total, 3);
    assert_eq!(ledger.items.len(), 3);
    assert_eq!(ledger.items[0].sku, product.sku.as_str());
    assert_eq!(ledger.items[0].name, product.name);
    assert_eq!(ledger.items[0].kind, StockMovementKind::AdjustmentOut);
    let purchases = stock
        .query_movements(
            services.demo.store_id,
            QueryStockMovementsRequest {
                search: Some(product.sku.to_string()),
                product_id: None,
                kind: Some(StockMovementKind::Purchase),
                from_ms: None,
                to_ms: None,
                limit: 20,
                offset: 0,
            },
        )
        .await
        .unwrap();
    assert_eq!(purchases.total, 1);
    assert_eq!(purchases.items.len(), 1);
    assert_eq!(purchases.items[0].kind, StockMovementKind::Purchase);
    let by_reason = stock
        .query_movements(
            services.demo.store_id,
            QueryStockMovementsRequest {
                search: Some("reposición inicial".into()),
                product_id: None,
                kind: None,
                from_ms: None,
                to_ms: None,
                limit: 20,
                offset: 0,
            },
        )
        .await
        .unwrap();
    assert_eq!(by_reason.total, 1);
    assert_eq!(by_reason.items[0].kind, StockMovementKind::Purchase);
    let second_page = stock
        .query_movements(
            services.demo.store_id,
            QueryStockMovementsRequest {
                search: None,
                product_id: Some(product.id),
                kind: None,
                from_ms: None,
                to_ms: None,
                limit: 1,
                offset: 1,
            },
        )
        .await
        .unwrap();
    assert_eq!(second_page.total, 3);
    assert_eq!(second_page.items.len(), 1);
    assert_eq!(second_page.limit, 1);
    assert_eq!(second_page.offset, 1);
    let other_ledger = stock
        .query_movements(
            other_store.id,
            QueryStockMovementsRequest {
                search: None,
                product_id: None,
                kind: None,
                from_ms: None,
                to_ms: None,
                limit: 20,
                offset: 0,
            },
        )
        .await
        .unwrap();
    assert_eq!(other_ledger.total, 0);
    assert!(other_ledger.items.is_empty());

    let now_ms = OffsetDateTime::now_utc().unix_timestamp() * 1_000;
    let recent_ledger = stock
        .query_movements(
            services.demo.store_id,
            QueryStockMovementsRequest {
                search: None,
                product_id: None,
                kind: None,
                from_ms: Some(now_ms - 3_600_000),
                to_ms: Some(now_ms + 3_600_000),
                limit: 20,
                offset: 0,
            },
        )
        .await
        .unwrap();
    assert_eq!(recent_ledger.total, 3);
    let past_ledger = stock
        .query_movements(
            services.demo.store_id,
            QueryStockMovementsRequest {
                search: None,
                product_id: None,
                kind: None,
                from_ms: Some(now_ms - 7_200_000),
                to_ms: Some(now_ms - 3_600_000),
                limit: 20,
                offset: 0,
            },
        )
        .await
        .unwrap();
    assert_eq!(past_ledger.total, 0);
    let invalid_range = stock
        .query_movements(
            services.demo.store_id,
            QueryStockMovementsRequest {
                search: None,
                product_id: None,
                kind: None,
                from_ms: Some(now_ms + 3_600_000),
                to_ms: Some(now_ms - 3_600_000),
                limit: 20,
                offset: 0,
            },
        )
        .await;
    assert!(matches!(
        invalid_range,
        Err(tpv_application::ApplicationError::Validation(_))
    ));

    let low_export = stock
        .levels_for_export(
            services.demo.store_id,
            StockLevelsQuery {
                search: Some(product.sku.to_string()),
                status: StockLevelStatus::Low,
            },
        )
        .await
        .unwrap();
    assert_eq!(low_export.len(), 1);
    assert_eq!(low_export[0].product_id, product.id);
    let restock_export = stock
        .levels_for_export(
            services.demo.store_id,
            StockLevelsQuery {
                search: Some(product.sku.to_string()),
                status: StockLevelStatus::Restock,
            },
        )
        .await
        .unwrap();
    assert_eq!(restock_export.len(), 1);
    assert_eq!(restock_export[0].restock_quantity(), Decimal::new(25, 1));
    let out_export = stock
        .levels_for_export(
            services.demo.store_id,
            StockLevelsQuery {
                search: Some(product.sku.to_string()),
                status: StockLevelStatus::Out,
            },
        )
        .await
        .unwrap();
    assert!(out_export.is_empty());
    let unmatched_export = stock
        .levels_for_export(
            services.demo.store_id,
            StockLevelsQuery {
                search: Some("no-existe".into()),
                status: StockLevelStatus::All,
            },
        )
        .await
        .unwrap();
    assert!(unmatched_export.is_empty());

    let negative_count = stock
        .set_count(
            services.demo.store_id,
            None,
            SetStockCountRequest {
                product_id: product.id,
                counted_quantity: "-1".into(),
                reason: "Conteo inválido".into(),
            },
        )
        .await;
    assert!(matches!(
        negative_count,
        Err(tpv_application::ApplicationError::Validation(_))
    ));
    let missing_reason = stock
        .set_count(
            services.demo.store_id,
            None,
            SetStockCountRequest {
                product_id: product.id,
                counted_quantity: "2".into(),
                reason: "   ".into(),
            },
        )
        .await;
    assert!(matches!(
        missing_reason,
        Err(tpv_application::ApplicationError::Validation(_))
    ));

    let invalid_import = format!(
        "Codigo;Conteo fisico;Motivo\n{};2,25;Conteo importado\nNO-EXISTE;4;Error\n",
        product.sku.as_str()
    );
    let preview = stock
        .preview_count_import(services.demo.store_id, &invalid_import)
        .await
        .unwrap();
    assert_eq!(preview.total_rows, 2);
    assert_eq!(preview.valid_rows, 1);
    assert_eq!(preview.invalid_rows, 1);
    assert_eq!(preview.adjusted, 1);
    assert!(!preview.committed);
    assert!(!preview.can_commit());
    assert_eq!(preview.errors[0].line, 3);
    assert!(!preview.errors[0].message.is_empty());
    assert_eq!(
        stock
            .get_level(services.demo.store_id, product.id)
            .await
            .unwrap()
            .unwrap()
            .quantity,
        Decimal::new(15, 1)
    );
    let rejected_commit = stock
        .commit_count_import(services.demo.store_id, None, &invalid_import)
        .await
        .unwrap();
    assert!(!rejected_commit.committed);
    assert_eq!(
        stock
            .get_level(services.demo.store_id, product.id)
            .await
            .unwrap()
            .unwrap()
            .quantity,
        Decimal::new(15, 1)
    );

    let valid_import = format!(
        "Codigo;Conteo fisico;Motivo\n{};2,25;Conteo importado\n",
        product.sku.as_str()
    );
    let committed = stock
        .commit_count_import(services.demo.store_id, None, &valid_import)
        .await
        .unwrap();
    assert!(committed.committed);
    assert_eq!(committed.valid_rows, 1);
    assert_eq!(committed.adjusted, 1);
    assert_eq!(committed.unchanged, 0);
    let imported_level = stock
        .get_level(services.demo.store_id, product.id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(imported_level.quantity, Decimal::new(225, 2));
    let imported_movements = stock
        .list_movements(services.demo.store_id, product.id, 20)
        .await
        .unwrap();
    assert_eq!(imported_movements.len(), 4);
    assert_eq!(imported_movements[0].kind, StockMovementKind::AdjustmentIn);
    assert_eq!(
        imported_movements[0].quantity.as_decimal(),
        Decimal::new(75, 2)
    );
    assert_eq!(
        imported_movements[0].reason.as_deref(),
        Some("Conteo importado")
    );
    let repeated_import = stock
        .commit_count_import(services.demo.store_id, None, &valid_import)
        .await
        .unwrap();
    assert!(repeated_import.committed);
    assert_eq!(repeated_import.adjusted, 0);
    assert_eq!(repeated_import.unchanged, 1);
    assert_eq!(
        stock
            .list_movements(services.demo.store_id, product.id, 20)
            .await
            .unwrap()
            .len(),
        4
    );
    assert_eq!(
        stock
            .get_level(other_store.id, product.id)
            .await
            .unwrap()
            .unwrap()
            .quantity,
        Decimal::ZERO
    );

    let rejected = stock
        .record_movement(
            services.demo.store_id,
            None,
            RecordStockMovementRequest {
                product_id: product.id,
                kind: StockMovementKind::Sale,
                quantity: "1".into(),
                reason: None,
            },
        )
        .await;
    assert!(matches!(
        rejected,
        Err(tpv_application::ApplicationError::Validation(_))
    ));
}
