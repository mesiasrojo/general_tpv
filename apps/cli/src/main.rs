use std::path::Path;
use std::sync::Arc;

use time::format_description::well_known::Rfc3339;
use tpv_application::{
    AddLineRequest, ApplyPaymentRequest, AuditRepository, AuthService, CashRepository, CashService,
    CatalogRepository, CatalogService, CloseCashRequest, ConfirmSaleRequest, InvoiceRepository,
    InvoiceService, MasterDataRepository, MasterDataService, OpenCashRequest, PrintQueueRepository,
    PrintService, SaleRepository, SaleService, SequenceRepository, StartSaleRequest,
    SyncOutboxRepository, SyncService, SyncStatus, UserRepository,
};
use tpv_auth::Argon2PasswordHasher;
use tpv_domain::cash::CashCount;
use tpv_domain::master_data::{MasterDataChange, MasterDataKind};
use tpv_domain::money::Money;
use tpv_domain::sale::PaymentMethod;
use tpv_print::{EscPosRenderer, FilePrinter};
use tpv_storage_sqlite::{DatabaseKey, KeyStore, SqliteStore};
use tpv_sync::{EnrollRequest, RetryPolicy, SyncClient, SyncWorker};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "tpv_cli=info".into()),
        )
        .init();

    let args: Vec<String> = std::env::args().collect();
    let mut in_memory = false;
    let mut retry_failed = false;
    let mut force_enroll = false;
    let mut master_data_demo = false;
    let mut sync_url = None;
    let mut backup_path = None;
    let mut key_file = None;
    let mut export_key_path = None;
    let mut import_key_path = None;
    let mut device_name = "TPV CLI".to_string();
    let mut path = None;
    let mut iter = args.iter().skip(1);
    while let Some(arg) = iter.next() {
        match arg.as_str() {
            "--memory" => in_memory = true,
            "--retry-failed" => retry_failed = true,
            "--enroll" => force_enroll = true,
            "--master-data-demo" => master_data_demo = true,
            "--sync-url" => sync_url = iter.next().cloned(),
            "--backup" => backup_path = iter.next().cloned(),
            "--key-file" => key_file = iter.next().cloned(),
            "--export-key" => export_key_path = iter.next().cloned(),
            "--import-key" => import_key_path = iter.next().cloned(),
            "--device-name" => {
                device_name = iter
                    .next()
                    .cloned()
                    .unwrap_or_else(|| "TPV CLI".to_string());
            }
            other if other.starts_with("--") => {}
            other => {
                if path.is_none() {
                    path = Some(other.to_string());
                }
            }
        }
    }
    let path = path.unwrap_or_else(|| "data/tpv.db".to_string());

    if !in_memory {
        if let Some(parent) = std::path::Path::new(&path).parent() {
            std::fs::create_dir_all(parent)?;
        }
    }

    if import_key_path.is_some() && key_file.is_some() {
        return Err("--import-key y --key-file son incompatibles".into());
    }
    if let Some(import_path) = &import_key_path {
        let imported = DatabaseKey::load(import_path)?.ok_or_else(|| {
            std::io::Error::new(
                std::io::ErrorKind::NotFound,
                format!("no existe la clave {import_path}"),
            )
        })?;
        let key_store = KeyStore::for_database(Path::new(&path), format!("{path}.key"));
        key_store.import_key(&imported)?;
        println!("Clave importada al keychain desde {import_path}");
    }

    let key = if in_memory {
        DatabaseKey::generate()
    } else if let Some(key_file) = &key_file {
        let key = DatabaseKey::load_or_create(key_file)?;
        println!("Clave local: fichero explícito {key_file}");
        key
    } else {
        let key_store = KeyStore::for_database(std::path::Path::new(&path), format!("{path}.key"));
        let loaded = key_store.load_or_create()?;
        println!("Clave local: {}", loaded.origin.label());
        loaded.key
    };
    if let Some(export_path) = &export_key_path {
        key.save_new(export_path)?;
        println!("Clave exportada a {export_path} (permisos 0600; guárdala de forma segura).");
    }
    let store = if in_memory {
        SqliteStore::in_memory_with_key(&key).await?
    } else {
        SqliteStore::connect(&path, &key).await?
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
    let master_data_repo: Arc<dyn MasterDataRepository> = store.clone();
    let clock: Arc<dyn tpv_application::Clock> = Arc::new(tpv_application::SystemClock);
    let sync = store.sync_context().await?;

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
    let master_data_service = MasterDataService::new(master_data_repo, clock.clone());
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
    let sync_service = SyncService::new(sync_repo.clone());

    println!(
        "Base de datos: {} (SQLCipher)",
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

    if master_data_demo {
        let product = catalog_service.get_product("1").await?;
        let payload = serde_json::to_value(&product)?;
        let event = master_data_service
            .publish_change(
                &sync,
                demo.store_id,
                MasterDataKind::Product,
                product.id.as_uuid(),
                payload,
                false,
            )
            .await?;
        println!(
            "\nCambio maestro publicado: producto {} · evento {}",
            product.sku, event.event_id
        );
    }

    let mut sync_status = sync_service.status().await?;
    print_sync_status(&sync_status);

    if retry_failed {
        let requeued = sync_service.retry_all_failed(100).await?;
        println!("Sync: {requeued} evento(s) devueltos a la cola desde cuarentena.");
        sync_status = sync_service.status().await?;
    }

    if let Some(url) = sync_url {
        let admin_token = std::env::var("TPV_CENTRAL_TOKEN").ok();
        let stored_token = if force_enroll {
            None
        } else {
            store.device_token().await?
        };
        let device_token = match stored_token {
            Some(token) => token,
            None => {
                let enrollment_client = SyncClient::new(url.clone(), None);
                let enrollment = enrollment_client
                    .enroll(
                        EnrollRequest {
                            tenant_id: sync.tenant_id.as_uuid(),
                            store_id: demo.store_id.as_uuid(),
                            device_id: sync.device_id.as_uuid(),
                            name: device_name.clone(),
                        },
                        admin_token.as_deref(),
                    )
                    .await?;
                store
                    .save_device_enrollment(&enrollment.token, &device_name)
                    .await?;
                println!(
                    "\nDispositivo enrolado: {} · {} · {}",
                    enrollment.device.device_id, enrollment.device.name, enrollment.device.status
                );
                enrollment.token
            }
        };
        let client = SyncClient::new(url, Some(device_token));
        let worker = SyncWorker::new(client.clone(), RetryPolicy::default());
        let report = worker
            .push_pending(sync_repo.as_ref(), sync.device_id, 100)
            .await?;
        if report.attempts == 0 {
            println!("\nSync: no hay eventos pendientes.");
        } else {
            println!(
                "\nSync: {} aceptados, {} duplicados, {} rechazados, {} intento(s)",
                report.accepted, report.duplicates, report.rejected, report.attempts
            );
        }
        let pulled = client.pull(0, Some(demo.store_id.as_uuid())).await?;
        println!("Pull: {} eventos en la central.", pulled.events.len());
        let mut master_applied = 0;
        let mut master_conflicts = 0;
        for stored in &pulled.events {
            if stored.event.aggregate_type != "master_data" {
                continue;
            }
            match serde_json::from_value::<MasterDataChange>(stored.event.payload.clone()) {
                Ok(change) => {
                    let result = master_data_service
                        .apply_change(stored.event.tenant_id, stored.event.store_id, &change)
                        .await?;
                    if result.applied {
                        master_applied += 1;
                    }
                    if let Some(conflict) = result.conflict {
                        master_conflicts += 1;
                        println!(
                            "  Conflicto maestro: {} {} · {} · versión local {}",
                            conflict.kind.as_str(),
                            conflict.entity_id,
                            conflict.reason.as_str(),
                            conflict.local_version
                        );
                    }
                }
                Err(error) => println!("  Cambio maestro inválido: {error}"),
            }
        }
        if master_applied > 0 || master_conflicts > 0 {
            println!(
                "Maestros: {master_applied} aplicados, {master_conflicts} conflicto(s) registrados."
            );
        }
        sync_status = sync_service.status().await?;
        print_sync_status(&sync_status);
    }

    let quarantine = sync_service.quarantine(5).await?;
    if !quarantine.is_empty() {
        println!(
            "\nCuarentena de sincronización ({} evento(s), mostrando {}):",
            sync_status.failed,
            quarantine.len()
        );
        for entry in &quarantine {
            println!(
                "  {} · {} · {} intento(s) · {} · {}",
                entry.event.event_type,
                entry.failed_at.format(&Rfc3339).unwrap_or_default(),
                entry.attempts,
                entry.event.event_id,
                entry.last_error.as_deref().unwrap_or("-")
            );
        }
        println!("Usa --retry-failed para devolverlos a la cola.");
    }

    if let Some(backup_path) = backup_path {
        let destination = Path::new(&backup_path);
        let bytes = store.backup_to(destination).await?;
        println!(
            "\nBackup cifrado: {} ({bytes} bytes)",
            destination.display()
        );
    }

    println!("\nDemo completada.");
    Ok(())
}

fn print_sync_status(status: &SyncStatus) {
    println!(
        "\nSync local: {} pendiente(s) · {} enviado(s) · {} en cuarentena",
        status.pending, status.sent, status.failed
    );
    if let Some(oldest) = status.oldest_pending_at {
        println!(
            "  Más antiguo pendiente: {}",
            oldest.format(&Rfc3339).unwrap_or_default()
        );
    }
    if let Some(last) = status.last_sent_at {
        println!(
            "  Último envío correcto: {}",
            last.format(&Rfc3339).unwrap_or_default()
        );
    }
    if let Some(error) = &status.last_error {
        println!("  Último error: {error}");
    }
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
