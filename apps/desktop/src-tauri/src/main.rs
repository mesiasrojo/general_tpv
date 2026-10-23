#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::path::PathBuf;
use std::sync::{Arc, RwLock};

use serde::Serialize;
use tauri::{Manager, State};
use time::OffsetDateTime;
use tpv_application::{
    AddLineRequest, ApplyPaymentRequest, AuthService, CashRepository, CashService,
    CatalogRepository, CatalogService, CloseCashRequest, ConfirmSaleRequest, CreateUserRequest,
    InvoiceRepository, InvoiceService, OpenCashRequest, PrintQueueRepository, PrintService,
    QuarantinedEvent, ResetPasswordRequest, SaleRepository, SaleService, SaleView,
    SequenceRepository, SessionUser, StartSaleRequest, SyncOutboxRepository, SyncService,
    SyncStatus, UpdateUserRequest, UserRepository,
};
use tpv_auth::Argon2PasswordHasher;
use tpv_domain::cash::{CashCount, CashSession};
use tpv_domain::catalog::Product;
use tpv_domain::ids::{CustomerId, RegisterId, SellerId, StoreId, UserId};
use tpv_domain::invoice::Invoice;
use tpv_domain::people::{Role, UserAccount};
use tpv_domain::sale::PaymentMethod;
use tpv_print::{DirectoryPrinter, EscPosRenderer};
use tpv_storage_sqlite::{DemoData, SqliteStore};

#[derive(Clone)]
struct AppState {
    catalog: CatalogService,
    sales: SaleService,
    invoices: InvoiceService,
    cash: CashService,
    print: PrintService,
    sync: SyncService,
    print_dir: PathBuf,
    auth: AuthService,
    session: Arc<RwLock<Option<SessionUser>>>,
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

fn require_session(state: &AppState) -> Result<SessionUser, String> {
    state
        .session
        .read()
        .map_err(|_| "session lock poisoned".to_string())?
        .clone()
        .ok_or_else(|| "sesión no iniciada".to_string())
}

fn require_sell(state: &AppState) -> Result<SessionUser, String> {
    let session = require_session(state)?;
    if session.user.role.can_sell() {
        Ok(session)
    } else {
        Err("el usuario no tiene permiso para vender".to_string())
    }
}

fn require_manager(state: &AppState) -> Result<SessionUser, String> {
    let session = require_session(state)?;
    if session.user.role.can_manage_master_data() {
        Ok(session)
    } else {
        Err("el usuario no tiene permiso de responsable".to_string())
    }
}

fn require_manage_users(state: &AppState) -> Result<SessionUser, String> {
    let session = require_session(state)?;
    if session.user.role.can_manage_users() {
        Ok(session)
    } else {
        Err("el usuario no tiene permiso para gestionar usuarios".to_string())
    }
}

#[tauri::command]
async fn login(
    state: State<'_, AppState>,
    username: String,
    password: String,
) -> Result<SessionUser, String> {
    let session = state
        .inner()
        .auth
        .login(&username, &password)
        .await
        .map_err(|error| error.to_string())?;
    *state
        .inner()
        .session
        .write()
        .map_err(|_| "session lock poisoned".to_string())? = Some(session.clone());
    Ok(session)
}

#[tauri::command]
async fn logout(state: State<'_, AppState>) -> Result<(), String> {
    *state
        .inner()
        .session
        .write()
        .map_err(|_| "session lock poisoned".to_string())? = None;
    Ok(())
}

#[tauri::command]
async fn current_user(state: State<'_, AppState>) -> Result<Option<SessionUser>, String> {
    Ok(state
        .inner()
        .session
        .read()
        .map_err(|_| "session lock poisoned".to_string())?
        .clone())
}

#[tauri::command]
async fn bootstrap(state: State<'_, AppState>) -> Result<AppInfo, String> {
    require_session(state.inner())?;
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
    require_sell(state.inner())?;
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
    require_sell(state.inner())?;
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
    require_sell(state.inner())?;
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
    require_sell(state.inner())?;
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
    require_sell(state.inner())?;
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
    require_sell(state.inner())?;
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
    require_sell(state.inner())?;
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
    require_sell(state.inner())?;
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
    require_sell(state.inner())?;
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
    require_sell(state.inner())?;
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
async fn print_pending(
    state: State<'_, AppState>,
) -> Result<Vec<tpv_application::PrintResult>, String> {
    require_sell(state.inner())?;
    let state = state.inner().clone();
    let printer = DirectoryPrinter::new(state.print_dir.clone());
    state
        .print
        .process_pending(&printer, 20)
        .await
        .map_err(|error| error.to_string())
}

#[tauri::command]
async fn list_print_jobs(
    state: State<'_, AppState>,
) -> Result<Vec<tpv_application::PrintJob>, String> {
    require_session(state.inner())?;
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
    require_session(state.inner())?;
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

#[tauri::command]
async fn sync_status(state: State<'_, AppState>) -> Result<SyncStatus, String> {
    require_session(state.inner())?;
    state
        .inner()
        .sync
        .status()
        .await
        .map_err(|error| error.to_string())
}

#[tauri::command]
async fn list_failed_sync_events(
    state: State<'_, AppState>,
    limit: Option<u32>,
) -> Result<Vec<QuarantinedEvent>, String> {
    require_session(state.inner())?;
    state
        .inner()
        .sync
        .quarantine(limit.unwrap_or(20))
        .await
        .map_err(|error| error.to_string())
}

#[tauri::command]
async fn retry_sync_events(
    state: State<'_, AppState>,
    event_ids: Vec<uuid::Uuid>,
) -> Result<u64, String> {
    require_manager(state.inner())?;
    state
        .inner()
        .sync
        .retry_failed(&event_ids)
        .await
        .map_err(|error| error.to_string())
}

#[tauri::command]
async fn retry_all_sync_events(state: State<'_, AppState>) -> Result<u64, String> {
    require_manager(state.inner())?;
    state
        .inner()
        .sync
        .retry_all_failed(100)
        .await
        .map_err(|error| error.to_string())
}

#[tauri::command]
async fn list_users(state: State<'_, AppState>) -> Result<Vec<UserAccount>, String> {
    let session = require_manage_users(state.inner())?;
    state
        .inner()
        .auth
        .list_users(&session)
        .await
        .map_err(|error| error.to_string())
}

#[tauri::command]
async fn create_user(
    state: State<'_, AppState>,
    username: String,
    display_name: String,
    role: Role,
    store_ids: Vec<StoreId>,
    password: String,
) -> Result<UserAccount, String> {
    let session = require_manage_users(state.inner())?;
    state
        .inner()
        .auth
        .create_user(
            &session,
            CreateUserRequest {
                username,
                display_name,
                role,
                store_ids,
                password,
            },
        )
        .await
        .map_err(|error| error.to_string())
}

#[tauri::command]
async fn update_user(
    state: State<'_, AppState>,
    user_id: UserId,
    display_name: String,
    role: Role,
    store_ids: Vec<StoreId>,
    active: bool,
) -> Result<UserAccount, String> {
    let session = require_manage_users(state.inner())?;
    state
        .inner()
        .auth
        .update_user(
            &session,
            UpdateUserRequest {
                user_id,
                display_name,
                role,
                store_ids,
                active,
            },
        )
        .await
        .map_err(|error| error.to_string())
}

#[tauri::command]
async fn reset_password(
    state: State<'_, AppState>,
    user_id: UserId,
    new_password: String,
) -> Result<(), String> {
    let session = require_manage_users(state.inner())?;
    state
        .inner()
        .auth
        .reset_password(
            &session,
            ResetPasswordRequest {
                user_id,
                new_password,
            },
        )
        .await
        .map_err(|error| error.to_string())
}

#[tauri::command]
async fn list_stores(state: State<'_, AppState>) -> Result<Vec<tpv_domain::people::Store>, String> {
    require_manage_users(state.inner())?;
    state
        .inner()
        .catalog
        .list_stores()
        .await
        .map_err(|error| error.to_string())
}

fn main() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            let data_dir = app.path().app_data_dir()?;
            std::fs::create_dir_all(&data_dir)?;
            let db_path = data_dir.join("tpv.db");
            let (store, demo) = tauri::async_runtime::block_on(async {
                let store = SqliteStore::connect(db_path.to_str().ok_or_else(|| {
                    std::io::Error::new(std::io::ErrorKind::InvalidInput, "invalid database path")
                })?)
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
            let sync_repo: Arc<dyn SyncOutboxRepository> = store.clone();
            let user_repo: Arc<dyn UserRepository> = store.clone();
            let audit_repo: Arc<dyn tpv_application::AuditRepository> = store.clone();
            let tickets_dir = data_dir.join("tickets");
            std::fs::create_dir_all(&tickets_dir)?;
            let clock: Arc<dyn tpv_application::Clock> = Arc::new(tpv_application::SystemClock);
            let sync = tauri::async_runtime::block_on(store.sync_context())?;
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
                clock.clone(),
            );
            let sync_service = SyncService::new(sync_repo);
            let auth =
                AuthService::new(user_repo, Arc::new(Argon2PasswordHasher), audit_repo, clock);
            tauri::async_runtime::block_on(auth.bootstrap_password("admin", "admin123"))?;
            app.manage(AppState {
                catalog,
                sales,
                invoices,
                cash,
                print,
                sync: sync_service,
                print_dir: tickets_dir,
                auth,
                session: Arc::new(RwLock::new(None)),
                demo,
            });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            login,
            logout,
            current_user,
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
            list_sales,
            sync_status,
            list_failed_sync_events,
            retry_sync_events,
            retry_all_sync_events,
            list_users,
            create_user,
            update_user,
            reset_password,
            list_stores
        ])
        .run(tauri::generate_context!())
        .expect("error while running General TPV");
}
