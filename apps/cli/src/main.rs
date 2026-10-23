use std::sync::Arc;

use time::format_description::well_known::Rfc3339;
use tpv_application::{
    AddLineRequest, ApplyPaymentRequest, AuditRepository, AuthService, CashRepository, CashService,
    CatalogRepository, CatalogService, CloseCashRequest, ConfirmSaleRequest, InvoiceRepository,
    InvoiceService, OpenCashRequest, PrintQueueRepository, PrintService, SaleRepository,
    SaleService, SequenceRepository, StartSaleRequest, SyncContext, SyncOutboxRepository,
    UserRepository,
};
use tpv_auth::Argon2PasswordHasher;
use tpv_domain::cash::CashCount;
use tpv_domain::ids::{DeviceId, TenantId};
use tpv_domain::money::Money;
use tpv_domain::sale::PaymentMethod;
use tpv_print::{EscPosRenderer, FilePrinter};
use tpv_storage_sqlite::SqliteStore;
use tpv_sync::SyncClient;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "tpv_cli=info".into()),
        )
        .init();

    let args: Vec<String> = std::env::args().collect();
    let in_memory = args.iter().any(|arg| arg == "--memory");
    let sync_url = args
        .windows(2)
        .find(|window| window[0] == "--sync-url")
        .map(|window| window[1].clone());
    let path = args
        .iter()
        .skip(1)
        .enumerate()
        .find(|(index, arg)| {
            !arg.starts_with("--")
                && args.get(index.saturating_sub(1)).map(String::as_str) != Some("--sync-url")
        })
        .map(|(_, arg)| arg.clone())
        .unwrap_or_else(|| "data/tpv.db".to_string());

    if !in_memory {
        if let Some(parent) = std::path::Path::new(&path).parent() {
            std::fs::create_dir_all(parent)?;
        }
    }

    let store = if in_memory {
        SqliteStore::in_memory().await?
    } else {
        SqliteStore::connect(&path).await?
    };
    let demo = store.seed_demo().await?;
    let store = Arc::new(store);

    let catalog_repo: Arc<dyn CatalogRepository> = store.clone();
    let sale_repo: Arc<dyn SaleRepository> = store.clone();
    let invoice_repo: Arc<dyn InvoiceRepository> = store.clone();
    let cash_repo: Arc<dyn CashRepository> = store.clone();
    let sequence_repo: Arc<dyn SequenceRepository> = store.clone();
    let sync_repo: Arc<dyn SyncOutboxRepository> = store.clone();
    let print_repo: Arc<dyn PrintQueueRepository> = store.clone();
    let user_repo: Arc<dyn UserRepository> = store.clone();
    let audit_repo: Arc<dyn AuditRepository> = store.clone();
    let clock: Arc<dyn tpv_application::Clock> = Arc::new(tpv_application::SystemClock);
    let sync = SyncContext {
        tenant_id: TenantId::new(),
        device_id: DeviceId::new(),
    };

    let auth = AuthService::new(
        user_repo,
        Arc::new(Argon2PasswordHasher),
        audit_repo,
        clock.clone(),
    );
    auth.bootstrap_password("admin", "admin123").await?;
    let session = auth.login("admin", "admin123").await?;
    println!(
        "Sesión iniciada: {} ({:?}) · {}",
        session.user.display_name, session.user.role, session.session_id
    );

    let sale_service = SaleService::new(
        sale_repo.clone(),
        catalog_repo.clone(),
        clock.clone(),
        sequence_repo.clone(),
        "TPV",
        sync.clone(),
    );
    let catalog_service = CatalogService::new(catalog_repo.clone());
    let invoice_service = InvoiceService::new(
        invoice_repo,
        sale_repo.clone(),
        catalog_repo.clone(),
        sequence_repo,
        clock.clone(),
        sync.clone(),
    );
    let cash_service = CashService::new(cash_repo, sale_repo.clone(), clock.clone(), sync.clone());
    let print_service = PrintService::new(
        sale_repo.clone(),
        catalog_repo.clone(),
        print_repo,
        Arc::new(EscPosRenderer::default()),
        clock,
    );

    println!(
        "Base de datos: {}",
        if in_memory { ":memory:" } else { &path }
    );
    println!("Tienda: {}", demo.store_id);
    println!("Caja: {}", demo.register_id);

    let cash = cash_service
        .open(OpenCashRequest {
            store_id: demo.store_id,
            register_id: demo.register_id,
            opened_by: session.user.id,
            opening_float_cents: 10_000,
        })
        .await?;
    println!("Caja abierta con fondo de 100,00 €");

    let mut sale = sale_service
        .start_sale(StartSaleRequest {
            store_id: demo.store_id,
            register_id: demo.register_id,
            seller_id: demo.seller_id,
            customer_id: Some(demo.customer_id),
        })
        .await?;
    sale = sale_service
        .add_line(AddLineRequest {
            sale_id: sale.id,
            sku: "1".into(),
            quantity: "2".into(),
            kind: "sale".into(),
            discount_bp: None,
        })
        .await?;
    sale = sale_service
        .add_line(AddLineRequest {
            sale_id: sale.id,
            sku: "3".into(),
            quantity: "1".into(),
            kind: "sale".into(),
            discount_bp: None,
        })
        .await?;

    let totals = sale.totals()?;
    println!(
        "\nVenta en curso: {} líneas, total {}",
        sale.lines.len(),
        totals.gross.format_eur()
    );
    println!(
        "  Base: {}  IVA: {}",
        totals.net.format_eur(),
        totals.tax.format_eur()
    );
    for breakdown in &totals.tax_breakdown {
        println!(
            "  {} ({}%): base {} IVA {}",
            breakdown.code,
            breakdown.percent_bp as f64 / 100.0,
            breakdown.net.format_eur(),
            breakdown.tax.format_eur()
        );
    }

    sale = sale_service
        .apply_payment(ApplyPaymentRequest {
            sale_id: sale.id,
            method: PaymentMethod::Cash,
            amount_cents: totals.gross.cents(),
            received_cents: totals.gross.cents() + 500,
            reference: None,
        })
        .await?;
    sale = sale_service
        .confirm_sale(ConfirmSaleRequest { sale_id: sale.id })
        .await?;
    println!(
        "\nTicket {} confirmado. Cambio: {}",
        sale.ticket_number.unwrap_or_default(),
        sale.totals()?.change.format_eur()
    );

    let store_info = catalog_service.get_store(demo.store_id).await?;
    let seller = catalog_service
        .list_sellers()
        .await?
        .into_iter()
        .find(|seller| seller.id == demo.seller_id)
        .ok_or("seller not found")?;
    let customer = catalog_service.get_customer(demo.customer_id).await?;
    print_ticket(&sale, &store_info, &seller, Some(&customer));

    let job = print_service.enqueue_ticket(sale.id, false).await?;
    let ticket_path = format!(
        "data/ticket-{:06}.escpos",
        sale.ticket_number.unwrap_or_default()
    );
    let printer = FilePrinter::new(&ticket_path);
    let results = print_service.process_pending(&printer, 10).await?;
    println!(
        "Ticket ESC/POS: {} bytes · {} trabajo(s) procesado(s) · {}",
        job.bytes.len(),
        results.len(),
        ticket_path
    );

    let invoice = invoice_service
        .issue_invoice(tpv_application::IssueInvoiceRequest {
            sale_id: sale.id,
            customer_id: demo.customer_id,
            series: "FAC".into(),
            year: 2026,
        })
        .await?;
    println!("\nFactura emitida: {}", invoice.full_number());
    println!(
        "  Total: {}  IVA: {}",
        invoice.totals.gross.format_eur(),
        invoice.totals.tax.format_eur()
    );

    let counted_cash = Money::from_cents(10_000).checked_add(totals.gross)?;
    let close = cash_service
        .close(CloseCashRequest {
            session_id: cash.id,
            closed_by: session.user.id,
            counted: CashCount::new(counted_cash, Money::ZERO, Money::ZERO),
        })
        .await?;
    println!(
        "\nCierre de caja: esperado {} / contado {} / diferencia {}",
        close.expected.cash.format_eur(),
        close.counted.cash.format_eur(),
        close.differences.cash.format_eur()
    );

    let recent = sale_service.list_recent(demo.store_id, 5).await?;
    println!("\nÚltimas ventas: {}", recent.len());
    for item in recent {
        println!(
            "  {} · {} líneas · {}",
            item.ticket_number.unwrap_or_default(),
            item.lines.len(),
            item.totals()?.gross.format_eur()
        );
    }

    if let Some(url) = sync_url {
        let client = SyncClient::new(url, std::env::var("TPV_CENTRAL_TOKEN").ok());
        let pending = sync_repo.pending_events(100).await?;
        if pending.is_empty() {
            println!("\nSync: no hay eventos pendientes.");
        } else {
            let response = client.push(sync.device_id, pending).await?;
            let mut acknowledged = response.accepted.clone();
            acknowledged.extend(response.duplicates.iter().copied());
            sync_repo.mark_events_sent(&acknowledged).await?;
            println!(
                "\nSync: {} aceptados, {} duplicados, {} rechazados, cursor {}",
                response.accepted.len(),
                response.duplicates.len(),
                response.rejected.len(),
                response.cursor
            );
        }
        let pulled = client.pull(0, Some(demo.store_id.as_uuid())).await?;
        println!("Pull: {} eventos en la central.", pulled.events.len());
    }

    println!("\nDemo completada.");
    Ok(())
}

fn print_ticket(
    sale: &tpv_domain::sale::Sale,
    store: &tpv_domain::people::Store,
    seller: &tpv_domain::people::Seller,
    customer: Option<&tpv_domain::people::Customer>,
) {
    let line = "-".repeat(42);
    println!("\n{}", "=".repeat(42));
    println!("{}", store.name);
    println!("{}", store.address.one_line());
    println!(
        "NIF: {}  Tel: {}",
        store.tax_id,
        store.phone.as_deref().unwrap_or("-")
    );
    println!("{line}");
    println!(
        "Ticket: {}-{:06}",
        sale.ticket_series.as_deref().unwrap_or("TPV"),
        sale.ticket_number.unwrap_or_default()
    );
    println!(
        "Fecha: {}",
        sale.closed_at
            .unwrap_or(sale.created_at)
            .format(&Rfc3339)
            .unwrap_or_default()
    );
    println!("Vendedor: {} - {}", seller.code, seller.name);
    if let Some(customer) = customer {
        println!(
            "Cliente: {} ({})",
            customer.full_name(),
            customer.nif.as_deref().unwrap_or("-")
        );
    }
    println!("{line}");
    for sale_line in &sale.lines {
        println!(
            "{} x {:<20} {:>10}",
            sale_line.quantity,
            sale_line.description,
            sale_line
                .gross_total()
                .map(|value| value.format_eur())
                .unwrap_or_default()
        );
    }
    println!("{line}");
    if let Ok(totals) = sale.totals() {
        for breakdown in &totals.tax_breakdown {
            println!(
                "IVA {:.2}%  base {:>10}  cuota {:>10}",
                breakdown.percent_bp as f64 / 100.0,
                breakdown.net.format_eur(),
                breakdown.tax.format_eur()
            );
        }
        println!("TOTAL: {:>28}", totals.gross.format_eur());
        println!("Entregado: {:>23}", totals.received.format_eur());
        println!("Cambio: {:>26}", totals.change.format_eur());
    }
    println!("{line}");
    println!("IVA INCLUIDO");
    println!("GRACIAS POR SU COMPRA");
    println!("{}", "=".repeat(42));
}
