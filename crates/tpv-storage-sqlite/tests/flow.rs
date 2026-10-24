use std::sync::Arc;

use rust_decimal::Decimal;
use sqlx::Row;
use time::{Duration, OffsetDateTime};
use tpv_application::{
    AddLineRequest, ApplyPaymentRequest, AuditRepository, AuthService, CashRepository, CashService,
    CloseCashRequest, ConfirmSaleRequest, CreateUserRequest, InvoiceLineSelection,
    InvoiceRepository, InvoiceService, OpenCashRequest, PrintQueueRepository, PrintService,
    RectifyInvoiceLinesRequest, ResetPasswordRequest, SaleQuery, SaleRepository, SaleService,
    SequenceRepository, StartSaleRequest, SyncContext, SyncOutboxRepository, SyncService,
    UpdateUserRequest, UserRepository,
};
use tpv_auth::Argon2PasswordHasher;
use tpv_domain::cash::CashCount;
use tpv_domain::document::DocumentType;
use tpv_domain::ids::{DeviceId, TenantId};
use tpv_domain::invoice::InvoiceKind;
use tpv_domain::money::Money;
use tpv_domain::people::Role;
use tpv_domain::sale::{PaymentMethod, Sale, SaleState};
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
async fn quarantines_failed_events_and_can_requeue_them() {
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
