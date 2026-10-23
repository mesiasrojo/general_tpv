#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::path::{Path, PathBuf};
use std::sync::{Arc, RwLock};

use serde::Serialize;
use tauri::{Manager, State};
use time::OffsetDateTime;
use tpv_application::{
    AddLineRequest, ApplyPaymentRequest, AuthService, BackupRepository, BackupService,
    CashRepository, CashService, CatalogRepository, CatalogService, CloseCashRequest,
    ConfirmSaleRequest, CreateUserRequest, InvoiceRepository, InvoiceService, MasterDataRepository,
    MasterDataService, OpenCashRequest, PrintQueueRepository, PrintService, PrinterPort,
    ProductService, QuarantinedEvent, ResetPasswordRequest, SaleRepository, SaleService, SaleView,
    SequenceRepository, SessionUser, StartSaleRequest, SyncContext, SyncOutboxRepository,
    SyncService, SyncStatus, UpdateUserRequest, UpsertProductRequest, UserRepository,
};
use tpv_auth::Argon2PasswordHasher;
use tpv_domain::cash::{CashCount, CashSession};
use tpv_domain::catalog::Product;
use tpv_domain::ids::{CustomerId, RegisterId, SellerId, StoreId, UserId};
use tpv_domain::invoice::Invoice;
use tpv_domain::master_data::{MasterDataChange, MasterDataConflict};
use tpv_domain::people::{Role, UserAccount, UserProvision};
use tpv_domain::sale::PaymentMethod;
use tpv_print::{
    EscPosRenderer, PrinterProfile, apply_profile_defaults, build_printer, printer_profile,
};
use tpv_storage_sqlite::{DemoData, KeyStore, SqliteStore};
use tpv_sync::{RetryPolicy, SyncClient, SyncWorker};

#[derive(Clone)]
struct AppState {
    catalog: CatalogService,
    sales: SaleService,
    invoices: InvoiceService,
    cash: CashService,
    print: PrintService,
    sync: SyncService,
    backup: BackupService,
    printer: Arc<dyn PrinterPort>,
    printer_label: String,
    backup_dir: PathBuf,
    auth: AuthService,
    session: Arc<RwLock<Option<SessionUser>>>,
    demo: DemoData,
    store: Arc<SqliteStore>,
    sync_ctx: SyncContext,
    sync_repo: Arc<dyn SyncOutboxRepository>,
    users: Arc<dyn UserRepository>,
    master_data: MasterDataService,
    products: ProductService,
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

#[derive(Debug, Serialize)]
struct BackupSummary {
    path: String,
    bytes: u64,
}

#[derive(Debug, Serialize)]
struct SyncSummary {
    accepted: usize,
    duplicates: usize,
    rejected: usize,
    pulled: usize,
    users_applied: usize,
    master_applied: usize,
    master_conflicts: usize,
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
    let results = state
        .print
        .process_pending(state.printer.as_ref(), 10)
        .await
        .map_err(|error| error.to_string())?;
    Ok(PrintSummary {
        job_id: job.id.to_string(),
        file: state.printer_label.clone(),
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
    state
        .print
        .process_pending(state.printer.as_ref(), 20)
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

#[tauri::command]
async fn list_products(state: State<'_, AppState>) -> Result<Vec<Product>, String> {
    require_session(state.inner())?;
    state
        .inner()
        .products
        .list()
        .await
        .map_err(|error| error.to_string())
}

#[tauri::command]
async fn save_product(
    state: State<'_, AppState>,
    request: UpsertProductRequest,
) -> Result<Product, String> {
    require_manager(state.inner())?;
    let state = state.inner().clone();
    state
        .products
        .upsert(&state.sync_ctx, state.demo.store_id, request)
        .await
        .map_err(|error| error.to_string())
}

#[tauri::command]
async fn list_master_data_conflicts(
    state: State<'_, AppState>,
    limit: Option<u32>,
) -> Result<Vec<MasterDataConflict>, String> {
    require_manager(state.inner())?;
    state
        .inner()
        .master_data
        .list_conflicts(limit.unwrap_or(100))
        .await
        .map_err(|error| error.to_string())
}

#[tauri::command]
async fn backup_now(state: State<'_, AppState>) -> Result<BackupSummary, String> {
    require_manager(state.inner())?;
    let state = state.inner().clone();
    let filename = format!(
        "tpv-backup-{}.db",
        OffsetDateTime::now_utc().unix_timestamp_nanos()
    );
    let destination = state.backup_dir.join(filename);
    let bytes = state
        .backup
        .backup_to(&destination)
        .await
        .map_err(|error| error.to_string())?;
    Ok(BackupSummary {
        path: destination.display().to_string(),
        bytes,
    })
}

#[tauri::command]
async fn sync_now(state: State<'_, AppState>) -> Result<SyncSummary, String> {
    require_manager(state.inner())?;
    let state = state.inner().clone();
    let url = std::env::var("TPV_SYNC_URL")
        .map_err(|_| "TPV_SYNC_URL no configurada en el terminal".to_string())?;
    let token = state
        .store
        .device_token()
        .await
        .map_err(|error| error.to_string())?
        .ok_or_else(|| "terminal no enrolado; ejecuta la CLI con --sync-url".to_string())?;
    let client = SyncClient::new(url, Some(token));
    let worker = SyncWorker::new(client.clone(), RetryPolicy::default());
    let report = worker
        .push_pending(state.sync_repo.as_ref(), state.sync_ctx.device_id, 100)
        .await
        .map_err(|error| error.to_string())?;
    let pulled = client
        .pull(0, Some(state.demo.store_id.as_uuid()))
        .await
        .map_err(|error| error.to_string())?;

    let mut users_applied = 0;
    let mut master_applied = 0;
    let mut master_conflicts = 0;
    for stored in &pulled.events {
        match stored.event.aggregate_type.as_str() {
            "user" if stored.event.event_type == "user.provisioned" => {
                if let Ok(provision) =
                    serde_json::from_value::<UserProvision>(stored.event.payload.clone())
                {
                    if state
                        .users
                        .upsert_provisioned(&provision)
                        .await
                        .map_err(|error| error.to_string())?
                    {
                        users_applied += 1;
                    }
                }
            }
            "master_data" => {
                if let Ok(change) =
                    serde_json::from_value::<MasterDataChange>(stored.event.payload.clone())
                {
                    let result = state
                        .master_data
                        .apply_change(stored.event.tenant_id, stored.event.store_id, &change)
                        .await
                        .map_err(|error| error.to_string())?;
                    if result.applied {
                        master_applied += 1;
                    }
                    if result.conflict.is_some() {
                        master_conflicts += 1;
                    }
                }
            }
            _ => {}
        }
    }
    Ok(SyncSummary {
        accepted: report.accepted,
        duplicates: report.duplicates,
        rejected: report.rejected,
        pulled: pulled.events.len(),
        users_applied,
        master_applied,
        master_conflicts,
    })
}

struct PrinterSettings {
    spec: Option<String>,
    profile: Option<PrinterProfile>,
}

async fn build_app_state(
    store: Arc<SqliteStore>,
    data_dir: &Path,
    demo: DemoData,
    printer_settings: PrinterSettings,
) -> Result<AppState, Box<dyn std::error::Error>> {
    let catalog_repo: Arc<dyn CatalogRepository> = store.clone();
    let sale_repo: Arc<dyn SaleRepository> = store.clone();
    let invoice_repo: Arc<dyn InvoiceRepository> = store.clone();
    let cash_repo: Arc<dyn CashRepository> = store.clone();
    let sequence_repo: Arc<dyn SequenceRepository> = store.clone();
    let print_repo: Arc<dyn PrintQueueRepository> = store.clone();
    let sync_repo: Arc<dyn SyncOutboxRepository> = store.clone();
    let user_repo: Arc<dyn UserRepository> = store.clone();
    let audit_repo: Arc<dyn tpv_application::AuditRepository> = store.clone();
    let backup_repo: Arc<dyn BackupRepository> = store.clone();
    let master_data_repo: Arc<dyn MasterDataRepository> = store.clone();
    let tickets_dir = data_dir.join("tickets");
    let backup_dir = data_dir.join("backups");
    std::fs::create_dir_all(&tickets_dir)?;
    std::fs::create_dir_all(&backup_dir)?;

    let default_spec = format!("dir:{}", tickets_dir.display());
    let configured_spec = printer_settings.spec.as_deref().unwrap_or(&default_spec);
    let printer_spec = if let Some(profile) = printer_settings.profile {
        apply_profile_defaults(configured_spec, profile)
    } else {
        configured_spec.to_string()
    };
    let renderer = printer_settings
        .profile
        .map_or_else(EscPosRenderer::default, EscPosRenderer::for_profile);
    let printer = build_printer(&printer_spec)?;
    let printer_label = printer_spec;

    let clock: Arc<dyn tpv_application::Clock> = Arc::new(tpv_application::SystemClock);
    let sync_ctx = store.sync_context().await?;
    let catalog = CatalogService::new(catalog_repo.clone());
    let sales = SaleService::new(
        sale_repo.clone(),
        catalog_repo.clone(),
        clock.clone(),
        sequence_repo.clone(),
        "TPV",
        sync_ctx.clone(),
    );
    let invoices = InvoiceService::new(
        invoice_repo,
        sale_repo.clone(),
        catalog_repo.clone(),
        sequence_repo,
        clock.clone(),
        sync_ctx.clone(),
    );
    let cash = CashService::new(
        cash_repo,
        sale_repo.clone(),
        clock.clone(),
        sync_ctx.clone(),
    );
    let print = PrintService::new(
        sale_repo,
        catalog_repo.clone(),
        print_repo,
        Arc::new(renderer),
        clock.clone(),
    );
    let sync_service = SyncService::new(sync_repo.clone());
    let backup_service = BackupService::new(backup_repo);
    let master_data = MasterDataService::new(master_data_repo, clock.clone());
    let products = ProductService::new(catalog_repo, master_data.clone());
    let auth = AuthService::new(
        user_repo.clone(),
        Arc::new(Argon2PasswordHasher),
        audit_repo,
        clock,
    );
    auth.bootstrap_password("admin", "admin123").await?;
    Ok(AppState {
        catalog,
        sales,
        invoices,
        cash,
        print,
        sync: sync_service,
        backup: backup_service,
        printer,
        printer_label,
        backup_dir,
        auth,
        session: Arc::new(RwLock::new(None)),
        demo,
        store,
        sync_ctx,
        sync_repo,
        users: user_repo,
        master_data,
        products,
    })
}

fn main() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            let data_dir = app.path().app_data_dir()?;
            std::fs::create_dir_all(&data_dir)?;
            let db_path = data_dir.join("tpv.db");
            let key_store = KeyStore::for_database(&db_path, data_dir.join("tpv.key"));
            let loaded = key_store.load_or_create()?;
            println!("Clave local: {}", loaded.origin.label());
            let key = loaded.key;
            let (store, demo) = tauri::async_runtime::block_on(async {
                let store = SqliteStore::connect(
                    db_path.to_str().ok_or_else(|| {
                        std::io::Error::new(
                            std::io::ErrorKind::InvalidInput,
                            "invalid database path",
                        )
                    })?,
                    &key,
                )
                .await?;
                let demo = store.seed_demo().await?;
                Ok::<_, Box<dyn std::error::Error>>((store, demo))
            })?;
            let selected_profile = match std::env::var("TPV_PRINTER_PROFILE")
                .ok()
                .filter(|value| !value.trim().is_empty())
            {
                Some(name) => Some(printer_profile(&name).ok_or_else(|| {
                    std::io::Error::new(
                        std::io::ErrorKind::InvalidInput,
                        format!("perfil de impresora desconocido: {name}"),
                    )
                })?),
                None => None,
            };
            let printer_spec = std::env::var("TPV_PRINTER").ok();
            let state = tauri::async_runtime::block_on(build_app_state(
                Arc::new(store),
                &data_dir,
                demo,
                PrinterSettings {
                    spec: printer_spec,
                    profile: selected_profile,
                },
            ))?;
            app.manage(state);
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
            list_stores,
            list_products,
            save_product,
            list_master_data_conflicts,
            backup_now,
            sync_now
        ])
        .run(tauri::generate_context!())
        .expect("error while running General TPV");
}

#[cfg(test)]
mod e2e_tests {
    use super::*;
    use serde_json::{Value, json};
    use tauri::ipc::{CallbackFn, InvokeBody};
    use tauri::test::{
        INVOKE_KEY, MockRuntime, get_ipc_response, mock_builder, mock_context, noop_assets,
    };
    use tauri::webview::InvokeRequest;
    use tauri::{App, WebviewWindow};
    use tpv_storage_sqlite::DatabaseKey;

    fn build_test_app() -> (App<MockRuntime>, WebviewWindow<MockRuntime>, PathBuf) {
        let data_dir = std::env::temp_dir().join(format!("tpv-e2e-{}", uuid::Uuid::new_v4()));
        let state = tauri::async_runtime::block_on(async {
            let store = SqliteStore::in_memory_with_key(&DatabaseKey::generate())
                .await
                .expect("in-memory store");
            let demo = store.seed_demo().await.expect("demo data");
            build_app_state(
                Arc::new(store),
                &data_dir,
                demo,
                PrinterSettings {
                    spec: Some("null".into()),
                    profile: None,
                },
            )
            .await
            .expect("application state")
        });
        let app = mock_builder()
            .manage(state)
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
                list_stores,
                list_products,
                save_product,
                list_master_data_conflicts,
                backup_now,
                sync_now
            ])
            .build(mock_context(noop_assets()))
            .expect("mock app");
        let webview = tauri::WebviewWindowBuilder::new(&app, "main", Default::default())
            .build()
            .expect("mock webview");
        (app, webview, data_dir)
    }

    fn invoke(
        webview: &WebviewWindow<MockRuntime>,
        cmd: &str,
        args: Value,
    ) -> Result<Value, Value> {
        get_ipc_response(
            webview,
            InvokeRequest {
                cmd: cmd.into(),
                callback: CallbackFn(0),
                error: CallbackFn(1),
                url: "tauri://localhost".parse().expect("valid URL"),
                body: InvokeBody::Json(args),
                headers: Default::default(),
                invoke_key: INVOKE_KEY.to_string(),
            },
        )
        .map(|body| body.deserialize::<Value>().expect("JSON response"))
    }

    #[test]
    fn sale_flow_runs_through_the_tauri_command_layer() {
        let (app, webview, data_dir) = build_test_app();

        assert_eq!(
            invoke(&webview, "current_user", json!({})).unwrap(),
            Value::Null
        );
        assert!(invoke(&webview, "bootstrap", json!({})).is_err());

        let session = invoke(
            &webview,
            "login",
            json!({ "username": "admin", "password": "admin123" }),
        )
        .expect("admin login");
        assert_eq!(session["user"]["role"], "admin");

        let info = invoke(&webview, "bootstrap", json!({})).expect("bootstrap");
        assert_eq!(info["store_name"], "100%SHOPPING");
        let store_id = info["store_id"].as_str().expect("store id").to_string();

        invoke(&webview, "open_cash", json!({})).expect("open cash");
        let sale = invoke(&webview, "start_sale", json!({})).expect("start sale");
        let sale_id = sale["sale"]["id"].as_str().expect("sale id").to_string();

        let sale = invoke(
            &webview,
            "add_line",
            json!({
                "saleId": sale_id,
                "sku": "3",
                "quantity": "2",
                "kind": "sale"
            }),
        )
        .expect("add line");
        assert_eq!(sale["totals"]["gross"], 240);

        let sale = invoke(
            &webview,
            "apply_payment",
            json!({
                "saleId": sale_id,
                "method": "cash",
                "amountCents": 240,
                "receivedCents": 500
            }),
        )
        .expect("apply payment");
        assert_eq!(sale["totals"]["change"], 260);

        let sale =
            invoke(&webview, "confirm_sale", json!({ "saleId": sale_id })).expect("confirm sale");
        assert_eq!(sale["sale"]["state"], "paid");
        assert!(sale["sale"]["ticket_number"].is_number());

        let print = invoke(
            &webview,
            "print_ticket",
            json!({ "saleId": sale_id, "reprint": false }),
        )
        .expect("print ticket");
        assert_eq!(print["success"], true);
        assert_eq!(print["file"], "null");

        invoke(&webview, "issue_invoice", json!({ "saleId": sale_id })).expect("issue invoice");
        let sales = invoke(&webview, "list_sales", json!({})).expect("list sales");
        assert_eq!(sales.as_array().expect("sales array").len(), 1);

        let cash = invoke(&webview, "cash_status", json!({})).expect("cash status");
        assert!(cash.is_object());
        assert_eq!(cash["state"], "open");

        let conflicts =
            invoke(&webview, "list_master_data_conflicts", json!({})).expect("conflicts");
        assert_eq!(conflicts.as_array().expect("conflicts array").len(), 0);

        let product = invoke(
            &webview,
            "save_product",
            json!({
                "request": {
                    "sku": "UI-1",
                    "barcode": "8412345678901",
                    "name": "Producto UI",
                    "family_code": "UI",
                    "unit_price": "9.95",
                    "tax_code": "IVA21",
                    "tax_percent_bp": 2100,
                    "active": true
                }
            }),
        )
        .expect("save product");
        assert_eq!(product["sku"], "UI-1");
        assert_eq!(product["name"], "Producto UI");
        assert_eq!(product["unit_price"], "9.95");
        let product_id = product["id"].as_str().expect("product id").to_string();

        let products = invoke(&webview, "list_products", json!({})).expect("list products");
        assert!(
            products
                .as_array()
                .expect("products array")
                .iter()
                .any(|item| item["id"] == product_id)
        );

        let updated = invoke(
            &webview,
            "save_product",
            json!({
                "request": {
                    "product_id": product_id,
                    "sku": "UI-1",
                    "barcode": "8412345678901",
                    "name": "Producto UI actualizado",
                    "family_code": "UI",
                    "unit_price": "10.95",
                    "tax_code": "IVA21",
                    "tax_percent_bp": 2100,
                    "active": true
                }
            }),
        )
        .expect("update product");
        assert_eq!(updated["name"], "Producto UI actualizado");
        assert_eq!(updated["unit_price"], "10.95");

        let backup = invoke(&webview, "backup_now", json!({})).expect("backup");
        assert!(backup["bytes"].as_u64().unwrap_or(0) > 0);
        assert!(Path::new(backup["path"].as_str().expect("backup path")).exists());

        let cashier = invoke(
            &webview,
            "create_user",
            json!({
                "username": "cajero1",
                "displayName": "Cajero Uno",
                "role": "cashier",
                "storeIds": [store_id],
                "password": "cajero123"
            }),
        )
        .expect("create cashier");
        assert_eq!(cashier["role"], "cashier");

        invoke(&webview, "logout", json!({})).expect("logout admin");
        invoke(
            &webview,
            "login",
            json!({ "username": "cajero1", "password": "cajero123" }),
        )
        .expect("cashier login");
        let forbidden =
            invoke(&webview, "list_users", json!({})).expect_err("cashier cannot list users");
        assert!(
            forbidden
                .as_str()
                .unwrap_or_default()
                .contains("gestionar usuarios")
        );

        drop(webview);
        drop(app);
        let _ = std::fs::remove_dir_all(data_dir);
    }
}
