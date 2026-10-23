#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::path::PathBuf;
use std::sync::Arc;

use serde::Serialize;
use tauri::{Manager, State};
use tpv_application::{
    AddLineRequest, ApplyPaymentRequest, CatalogRepository, CatalogService, CashRepository,
    CashService, CloseCashRequest, ConfirmSaleRequest, InvoiceRepository, InvoiceService,
    OpenCashRequest, PrintQueueRepository, PrintService, SaleRepository, SaleService,
    SequenceRepository, StartSaleRequest, SaleView, SyncContext,
};
use tpv_domain::cash::{CashCount, CashSession};
use tpv_domain::catalog::Product;
use tpv_domain::ids::{CustomerId, DeviceId, RegisterId, SellerId, StoreId, TenantId, UserId};
use tpv_domain::invoice::Invoice;
use tpv_domain::sale::PaymentMethod;
use tpv_print::{DirectoryPrinter, EscPosRenderer};
use tpv_storage_sqlite::{DemoData, SqliteStore};
use time::OffsetDateTime;

#[derive(Clone)]
struct AppState {
    catalog: CatalogService,
    sales: SaleService,
    invoices: InvoiceService,
    cash: CashService,
    print: PrintService,
    print_dir: PathBuf,
    demo: DemoData,
}

#[derive(Debug, Serialize)]
struct AppInfo {
    store_name: String,
    store_id: StoreId,
    register_id: RegisterId,
    seller_id: SellerId,
    customer_id: CustomerId,
    user_id: UserId,
    products: Vec<Product>,
    open_cash: Option<CashSession>,
}

#[tauri::command]
async fn bootstrap(state: State<'_, AppState>) -> Result<AppInfo, String> {
    let state = state.inner().clone();
    let store = state
        .catalog
        .get_store(state.demo.store_id)
        .await
        .map_err(|error| error.to_string())?;
    let products = state
        .catalog
        .list_products()
        .await
        .map_err(|error| error.to_string())?;
    let open_cash = state
        .cash
        .find_open(state.demo.register_id)
        .await
        .map_err(|error| error.to_string())?;
    Ok(AppInfo {
        store_name: store.name,
        store_id: state.demo.store_id,
        register_id: state.demo.register_id,
        seller_id: state.demo.seller_id,
        customer_id: state.demo.customer_id,
        user_id: state.demo.user_id,
        products,
        open_cash,
    })
}

#[tauri::command]
async fn search_products(
    state: State<'_, AppState>,
    query: String,
) -> Result<Vec<tpv_application::ProductSearchResult>, String> {
    state
        .inner()
        .clone()
        .catalog
        .search_products(&query)
        .await
        .map_err(|error| error.to_string())
}

#[tauri::command]
async fn start_sale(state: State<'_, AppState>) -> Result<SaleView, String> {
    let state = state.inner().clone();
    let sale = state
        .sales
        .start_sale(StartSaleRequest {
            store_id: state.demo.store_id,
            register_id: state.demo.register_id,
            seller_id: state.demo.seller_id,
            customer_id: Some(state.demo.customer_id),
        })
        .await
        .map_err(|error| error.to_string())?;
    SaleView::new(sale).map_err(|error| error.to_string())
}

#[tauri::command]
async fn add_line(
    state: State<'_, AppState>,
    sale_id: tpv_domain::ids::SaleId,
    sku: String,
    quantity: String,
    kind: String,
) -> Result<SaleView, String> {
    let sale = state
        .inner()
        .clone()
        .sales
        .add_line(AddLineRequest {
            sale_id,
            sku,
            quantity,
            kind,
            discount_bp: None,
        })
        .await
        .map_err(|error| error.to_string())?;
    SaleView::new(sale).map_err(|error| error.to_string())
}

#[tauri::command]
async fn apply_payment(
    state: State<'_, AppState>,
    sale_id: tpv_domain::ids::SaleId,
    method: String,
    amount_cents: i64,
    received_cents: i64,
) -> Result<SaleView, String> {
    let method = match method.as_str() {
        "cash" | "efectivo" => PaymentMethod::Cash,
        "card" | "tarjeta" => PaymentMethod::Card,
        "voucher" | "vale" => PaymentMethod::Voucher,
        _ => PaymentMethod::Other,
    };
    let sale = state
        .inner()
        .clone()
        .sales
        .apply_payment(ApplyPaymentRequest {
            sale_id,
            method,
            amount_cents,
            received_cents,
            reference: None,
        })
        .await
        .map_err(|error| error.to_string())?;
    SaleView::new(sale).map_err(|error| error.to_string())
}

#[tauri::command]
async fn confirm_sale(
    state: State<'_, AppState>,
    sale_id: tpv_domain::ids::SaleId,
) -> Result<SaleView, String> {
    let sale = state
        .inner()
        .clone()
        .sales
        .confirm_sale(ConfirmSaleRequest { sale_id })
        .await
        .map_err(|error| error.to_string())?;
    SaleView::new(sale).map_err(|error| error.to_string())
}

#[tauri::command]
async fn issue_invoice(
    state: State<'_, AppState>,
    sale_id: tpv_domain::ids::SaleId,
) -> Result<Invoice, String> {
    let state = state.inner().clone();
    let year = u16::try_from(OffsetDateTime::now_utc().year()).unwrap_or(2026);
    state
        .invoices
        .issue_invoice(tpv_application::IssueInvoiceRequest {
            sale_id,
            customer_id: state.demo.customer_id,
            series: "FAC".into(),
            year,
        })
        .await
        .map_err(|error| error.to_string())
}

#[tauri::command]
async fn open_cash(state: State<'_, AppState>) -> Result<CashSession, String> {
    let state = state.inner().clone();
    if let Some(session) = state
        .cash
        .find_open(state.demo.register_id)
        .await
        .map_err(|error| error.to_string())?
    {
        return Ok(session);
    }
    state
        .cash
        .open(OpenCashRequest {
            store_id: state.demo.store_id,
            register_id: state.demo.register_id,
            opened_by: state.demo.user_id,
            opening_float_cents: 10_000,
        })
        .await
        .map_err(|error| error.to_string())
}

#[tauri::command]
async fn close_cash(
    state: State<'_, AppState>,
    session_id: tpv_domain::ids::CashSessionId,
    counted_cash_cents: i64,
    counted_card_cents: i64,
    counted_voucher_cents: i64,
) -> Result<tpv_domain::cash::CashCloseRecord, String> {
    let state = state.inner().clone();
    state
        .cash
        .close(CloseCashRequest {
            session_id,
            closed_by: state.demo.user_id,
            counted: CashCount::new(
                tpv_domain::Money::from_cents(counted_cash_cents),
                tpv_domain::Money::from_cents(counted_card_cents),
                tpv_domain::Money::from_cents(counted_voucher_cents),
            ),
        })
        .await
        .map_err(|error| error.to_string())
}

#[tauri::command]
async fn cash_status(state: State<'_, AppState>) -> Result<Option<CashSession>, String> {
    let state = state.inner().clone();
    state
        .cash
        .find_open(state.demo.register_id)
        .await
        .map_err(|error| error.to_string())
}

#[derive(Debug, Serialize)]
struct PrintSummary {
    job_id: String,
    file: String,
    bytes: usize,
    success: bool,
}

#[tauri::command]
async fn print_ticket(
    state: State<'_, AppState>,
    sale_id: tpv_domain::ids::SaleId,
    reprint: bool,
) -> Result<PrintSummary, String> {
    let state = state.inner().clone();
    let job = state
        .print
        .enqueue_ticket(sale_id, reprint)
        .await
        .map_err(|error| error.to_string())?;
    let printer = DirectoryPrinter::new(state.print_dir.clone());
    let results = state
        .print
        .process_pending(&printer, 10)
        .await
        .map_err(|error| error.to_string())?;
    Ok(PrintSummary {
        job_id: job.id.to_string(),
        file: state
            .print_dir
            .join(format!("{}.escpos", job.id))
            .display()
            .to_string(),
        bytes: job.bytes.len(),
        success: results.iter().all(|result| result.success),
    })
}

#[tauri::command]
async fn print_pending(state: State<'_, AppState>) -> Result<Vec<tpv_application::PrintResult>, String> {
    let state = state.inner().clone();
    let printer = DirectoryPrinter::new(state.print_dir.clone());
    state
        .print
        .process_pending(&printer, 20)
        .await
        .map_err(|error| error.to_string())
}

#[tauri::command]
async fn list_print_jobs(state: State<'_, AppState>) -> Result<Vec<tpv_application::PrintJob>, String> {
    state
        .inner()
        .clone()
        .print
        .list_recent(20)
        .await
        .map_err(|error| error.to_string())
}

#[tauri::command]
async fn list_sales(state: State<'_, AppState>) -> Result<Vec<SaleView>, String> {
    let state = state.inner().clone();
    let sales = state
        .sales
        .list_recent(state.demo.store_id, 50)
        .await
        .map_err(|error| error.to_string())?;
    sales
        .into_iter()
        .map(|sale| SaleView::new(sale).map_err(|error| error.to_string()))
        .collect()
}

fn main() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            let data_dir = app.path().app_data_dir()?;
            std::fs::create_dir_all(&data_dir)?;
            let db_path = data_dir.join("tpv.db");
            let (store, demo) = tauri::async_runtime::block_on(async {
                let store = SqliteStore::connect(
                    db_path.to_str().ok_or_else(|| {
                        std::io::Error::new(
                            std::io::ErrorKind::InvalidInput,
                            "invalid database path",
                        )
                    })?,
                )
                .await?;
                let demo = store.seed_demo().await?;
                Ok::<_, Box<dyn std::error::Error>>((store, demo))
            })?;
            let store = Arc::new(store);
            let catalog_repo: Arc<dyn CatalogRepository> = store.clone();
            let sale_repo: Arc<dyn SaleRepository> = store.clone();
            let invoice_repo: Arc<dyn InvoiceRepository> = store.clone();
            let cash_repo: Arc<dyn CashRepository> = store.clone();
            let sequence_repo: Arc<dyn SequenceRepository> = store.clone();
            let print_repo: Arc<dyn PrintQueueRepository> = store.clone();
            let tickets_dir = data_dir.join("tickets");
            std::fs::create_dir_all(&tickets_dir)?;
            let clock: Arc<dyn tpv_application::Clock> = Arc::new(tpv_application::SystemClock);
            let sync = SyncContext {
                tenant_id: TenantId::new(),
                device_id: DeviceId::new(),
            };
            let catalog = CatalogService::new(catalog_repo.clone());
            let sales = SaleService::new(
                sale_repo.clone(),
                catalog_repo.clone(),
                clock.clone(),
                sequence_repo.clone(),
                "TPV",
                sync.clone(),
            );
            let invoices = InvoiceService::new(
                invoice_repo,
                sale_repo.clone(),
                catalog_repo.clone(),
                sequence_repo,
                clock.clone(),
                sync.clone(),
            );
            let cash = CashService::new(cash_repo, sale_repo.clone(), clock.clone(), sync);
            let print = PrintService::new(
                sale_repo,
                catalog_repo,
                print_repo,
                Arc::new(EscPosRenderer::default()),
                clock,
            );
            app.manage(AppState {
                catalog,
                sales,
                invoices,
                cash,
                print,
                print_dir: tickets_dir,
                demo,
            });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            bootstrap,
            search_products,
            start_sale,
            add_line,
            apply_payment,
            confirm_sale,
            issue_invoice,
            open_cash,
            close_cash,
            cash_status,
            print_ticket,
            print_pending,
            list_print_jobs,
            list_sales
        ])
        .run(tauri::generate_context!())
        .expect("error while running General TPV");
}
