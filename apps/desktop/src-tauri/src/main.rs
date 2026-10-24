#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::path::{Path, PathBuf};
use std::sync::{Arc, RwLock};
use std::time::Duration;

use serde::{Deserialize, Serialize};
use tauri::{Manager, State};
use time::OffsetDateTime;
use tpv_application::{
    AddLineRequest, ApplyPaymentRequest, AuthService, BackupRepository, BackupService,
    CashMovementRequest, CashRepository, CashService, CatalogRepository, CatalogService,
    CloseCashRequest, ConfirmSaleRequest, CreateNoteRequest, CreateUserRequest, CustomerService,
    FamilyService, InvoiceLineSelection, InvoiceRepository, InvoiceService, MasterDataRepository,
    MasterDataService, NoteInbox, NoteRepository, NoteService, OpenCashRequest,
    PrintQueueRepository, PrintService, PrinterPort, ProductImportReport, ProductImportRepository,
    ProductImportService, ProductService, QuarantinedEvent, RectifyInvoiceLinesRequest,
    RectifyInvoiceRequest, ResetPasswordRequest, SaleHistoryPage, SaleQuery, SaleRepository,
    SaleService, SaleView, SellerService, SequenceRepository, SessionUser, StartSaleRequest,
    StoreService, SyncContext, SyncOutboxRepository, SyncService, SyncStatus, TaxService,
    TimeClockRepository, TimeClockService, TimeClockStatus, UpdateUserRequest,
    UpsertCustomerRequest, UpsertFamilyRequest, UpsertProductRequest, UpsertSellerRequest,
    UpsertStoreRequest, UpsertTaxRequest, UserRepository,
};
use tpv_auth::Argon2PasswordHasher;
use tpv_domain::cash::{CashCount, CashMovementKind, CashSession};
use tpv_domain::catalog::{Family, Product};
use tpv_domain::ids::{
    CashSessionId, CustomerId, InvoiceId, NoteId, RegisterId, SellerId, StoreId, UserId,
};
use tpv_domain::invoice::Invoice;
use tpv_domain::master_data::{MasterDataChange, MasterDataConflict};
use tpv_domain::people::{Customer, Role, Seller, Store, UserAccount, UserProvision};
use tpv_domain::sale::{PaymentMethod, SaleState};
use tpv_domain::tax::Tax;
use tpv_print::{
    EscPosRenderer, PrinterProfile, apply_profile_defaults, build_printer, printer_profile,
};
use tpv_storage_sqlite::{DemoData, KeyStore, SqliteStore};
use tpv_sync::{EnrollRequest, RetryPolicy, SyncClient, SyncWorker};

#[derive(Clone)]
struct AppState {
    catalog: CatalogService,
    customers: CustomerService,
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
    families: FamilyService,
    taxes: TaxService,
    sellers: SellerService,
    stores: StoreService,
    imports: ProductImportService,
    notes: NoteService,
    time_clock: TimeClockService,
    sync_lock: Arc<tokio::sync::Mutex<()>>,
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
async fn start_sale(
    state: State<'_, AppState>,
    customer_id: Option<CustomerId>,
) -> Result<SaleView, String> {
    require_sell(state.inner())?;
    let state = state.inner().clone();
    let sale = state
        .sales
        .start_sale(StartSaleRequest {
            store_id: state.demo.store_id,
            register_id: state.demo.register_id,
            seller_id: state.demo.seller_id,
            customer_id: customer_id.or(Some(state.demo.customer_id)),
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
    let sale = state
        .sales
        .get_sale(sale_id)
        .await
        .map_err(|error| error.to_string())?;
    let customer_id = sale.customer_id.unwrap_or(state.demo.customer_id);
    let year = u16::try_from(OffsetDateTime::now_utc().year()).unwrap_or(2026);
    state
        .invoices
        .issue_invoice(tpv_application::IssueInvoiceRequest {
            sale_id,
            customer_id,
            series: "FAC".into(),
            year,
        })
        .await
        .map_err(|error| error.to_string())
}

#[tauri::command]
async fn rectify_invoice(
    state: State<'_, AppState>,
    invoice_id: InvoiceId,
    reason: String,
) -> Result<Invoice, String> {
    require_manager(state.inner())?;
    let reason = reason.trim().to_string();
    if reason.is_empty() {
        return Err("la rectificación requiere un motivo".into());
    }
    let year = u16::try_from(OffsetDateTime::now_utc().year()).unwrap_or(2026);
    let state = state.inner().clone();
    state
        .invoices
        .rectify_invoice(RectifyInvoiceRequest {
            invoice_id,
            series: "REC".into(),
            year,
            reason,
        })
        .await
        .map_err(|error| error.to_string())
}

#[derive(Debug, Serialize)]
struct RefundSummary {
    invoice: Invoice,
    cash: Option<CashSession>,
}

#[tauri::command]
async fn get_invoice(state: State<'_, AppState>, invoice_id: InvoiceId) -> Result<Invoice, String> {
    require_manager(state.inner())?;
    let state = state.inner().clone();
    state
        .invoices
        .get_invoice(invoice_id)
        .await
        .map_err(|error| error.to_string())
}

#[tauri::command]
async fn refund_invoice_lines(
    state: State<'_, AppState>,
    invoice_id: InvoiceId,
    reason: String,
    refund_method: String,
    refund_reference: Option<String>,
    lines: Vec<InvoiceLineSelection>,
    session_id: Option<CashSessionId>,
) -> Result<RefundSummary, String> {
    require_manager(state.inner())?;
    let reason = reason.trim().to_string();
    if reason.is_empty() {
        return Err("la devolución requiere un motivo".into());
    }
    let refund_method = refund_method.trim().to_lowercase();
    if refund_method != "cash" && refund_method != "card" {
        return Err("la forma de reembolso debe ser cash o card".into());
    }
    let refund_reference = refund_reference
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty());
    if refund_method == "card" && refund_reference.is_none() {
        return Err("el reembolso con tarjeta requiere la referencia del justificante".into());
    }
    if refund_method == "cash" {
        let session_id =
            session_id.ok_or_else(|| "abre la caja para reembolsar en efectivo".to_string())?;
        let open = state
            .cash
            .find_open(state.demo.register_id)
            .await
            .map_err(|error| error.to_string())?
            .ok_or_else(|| "no hay una caja abierta".to_string())?;
        if open.id != session_id {
            return Err("la sesión de caja indicada no está abierta".into());
        }
    }
    let year = u16::try_from(OffsetDateTime::now_utc().year()).unwrap_or(2026);
    let state = state.inner().clone();
    let invoice = state
        .invoices
        .rectify_invoice_lines(RectifyInvoiceLinesRequest {
            invoice_id,
            series: "REC".into(),
            year,
            reason,
            refund_reference,
            lines,
        })
        .await
        .map_err(|error| error.to_string())?;
    let cash = if refund_method == "cash" {
        let amount_cents = -invoice.totals.gross.cents();
        if amount_cents <= 0 {
            return Err("la devolución no tiene importe positivo".into());
        }
        let movement_reason = format!("Devolución {}", invoice.full_number());
        let movement = state
            .cash
            .movement(CashMovementRequest {
                session_id: session_id.expect("cash session was validated"),
                kind: CashMovementKind::Expense,
                amount_cents,
                reason: movement_reason,
            })
            .await
            .map_err(|error| error.to_string())?;
        Some(movement)
    } else {
        None
    };
    Ok(RefundSummary { invoice, cash })
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

#[tauri::command]
async fn cash_movement(
    state: State<'_, AppState>,
    session_id: CashSessionId,
    kind: CashMovementKind,
    amount_cents: i64,
    reason: String,
) -> Result<CashSession, String> {
    let current = require_sell(state.inner())?;
    if kind != CashMovementKind::Expense && !current.user.role.can_reopen_cash() {
        return Err("solo un responsable puede registrar retiradas o ingresos".into());
    }
    let state = state.inner().clone();
    state
        .cash
        .movement(CashMovementRequest {
            session_id,
            kind,
            amount_cents,
            reason,
        })
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

#[derive(Debug, Default, Deserialize)]
struct SaleHistoryRequest {
    #[serde(default)]
    limit: Option<u32>,
    #[serde(default)]
    offset: Option<u32>,
    #[serde(default)]
    from_ms: Option<i64>,
    #[serde(default)]
    to_ms: Option<i64>,
    #[serde(default)]
    state: Option<String>,
    #[serde(default)]
    ticket_number: Option<u64>,
}

fn millis_to_time(value: i64) -> Result<OffsetDateTime, String> {
    let nanos = i128::from(value)
        .checked_mul(1_000_000)
        .ok_or_else(|| "fecha fuera de rango".to_string())?;
    OffsetDateTime::from_unix_timestamp_nanos(nanos).map_err(|_| "fecha fuera de rango".to_string())
}

fn parse_sale_state(value: Option<String>) -> Result<Option<SaleState>, String> {
    let Some(value) = value.map(|text| text.trim().to_ascii_lowercase()) else {
        return Ok(None);
    };
    if value.is_empty() || value == "all" {
        return Ok(None);
    }
    serde_json::from_value(serde_json::Value::String(value.clone()))
        .map(Some)
        .map_err(|_| format!("estado de venta no válido: {value}"))
}

#[tauri::command]
async fn query_sales(
    state: State<'_, AppState>,
    request: SaleHistoryRequest,
) -> Result<SaleHistoryPage, String> {
    require_session(state.inner())?;
    let state = state.inner().clone();
    let from = request.from_ms.map(millis_to_time).transpose()?;
    let to = request.to_ms.map(millis_to_time).transpose()?;
    state
        .sales
        .history(SaleQuery {
            store_id: state.demo.store_id,
            limit: request.limit.unwrap_or(25),
            offset: request.offset.unwrap_or(0),
            from,
            to,
            state: parse_sale_state(request.state)?,
            ticket_number: request.ticket_number.filter(|value| *value > 0),
        })
        .await
        .map_err(|error| error.to_string())
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
async fn list_stores(state: State<'_, AppState>) -> Result<Vec<Store>, String> {
    require_manager(state.inner())?;
    state
        .inner()
        .stores
        .list()
        .await
        .map_err(|error| error.to_string())
}

#[tauri::command]
async fn list_sellers(state: State<'_, AppState>) -> Result<Vec<Seller>, String> {
    require_session(state.inner())?;
    state
        .inner()
        .sellers
        .list()
        .await
        .map_err(|error| error.to_string())
}

#[tauri::command]
async fn save_seller(
    state: State<'_, AppState>,
    request: UpsertSellerRequest,
) -> Result<Seller, String> {
    require_manager(state.inner())?;
    let state = state.inner().clone();
    state
        .sellers
        .upsert(&state.sync_ctx, state.demo.store_id, request)
        .await
        .map_err(|error| error.to_string())
}

#[tauri::command]
async fn save_store(
    state: State<'_, AppState>,
    request: UpsertStoreRequest,
) -> Result<Store, String> {
    require_manager(state.inner())?;
    let state = state.inner().clone();
    state
        .stores
        .upsert(&state.sync_ctx, state.demo.store_id, request)
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
async fn list_customers(state: State<'_, AppState>) -> Result<Vec<Customer>, String> {
    require_session(state.inner())?;
    state
        .inner()
        .customers
        .list()
        .await
        .map_err(|error| error.to_string())
}

#[tauri::command]
async fn save_customer(
    state: State<'_, AppState>,
    request: UpsertCustomerRequest,
) -> Result<Customer, String> {
    require_manager(state.inner())?;
    let state = state.inner().clone();
    state
        .customers
        .upsert(&state.sync_ctx, state.demo.store_id, request)
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
async fn list_families(state: State<'_, AppState>) -> Result<Vec<Family>, String> {
    require_session(state.inner())?;
    state
        .inner()
        .families
        .list()
        .await
        .map_err(|error| error.to_string())
}

#[tauri::command]
async fn save_family(
    state: State<'_, AppState>,
    request: UpsertFamilyRequest,
) -> Result<Family, String> {
    require_manager(state.inner())?;
    let state = state.inner().clone();
    state
        .families
        .upsert(&state.sync_ctx, state.demo.store_id, request)
        .await
        .map_err(|error| error.to_string())
}

#[tauri::command]
async fn list_taxes(state: State<'_, AppState>) -> Result<Vec<Tax>, String> {
    require_session(state.inner())?;
    state
        .inner()
        .taxes
        .list()
        .await
        .map_err(|error| error.to_string())
}

#[tauri::command]
async fn save_tax(state: State<'_, AppState>, request: UpsertTaxRequest) -> Result<Tax, String> {
    require_manager(state.inner())?;
    let state = state.inner().clone();
    state
        .taxes
        .upsert(&state.sync_ctx, state.demo.store_id, request)
        .await
        .map_err(|error| error.to_string())
}

#[tauri::command]
async fn time_clock_status(state: State<'_, AppState>) -> Result<TimeClockStatus, String> {
    let session = require_session(state.inner())?;
    load_time_clock_status(state.inner(), &session).await
}

#[tauri::command]
async fn time_clock_in(state: State<'_, AppState>) -> Result<TimeClockStatus, String> {
    let session = require_session(state.inner())?;
    let state = state.inner().clone();
    state
        .time_clock
        .clock_in(session.user.id, state.demo.store_id)
        .await
        .map_err(|error| error.to_string())?;
    load_time_clock_status(&state, &session).await
}

#[tauri::command]
async fn time_clock_out(state: State<'_, AppState>) -> Result<TimeClockStatus, String> {
    let session = require_session(state.inner())?;
    let state = state.inner().clone();
    state
        .time_clock
        .clock_out(session.user.id, state.demo.store_id)
        .await
        .map_err(|error| error.to_string())?;
    load_time_clock_status(&state, &session).await
}

async fn load_time_clock_status(
    state: &AppState,
    session: &SessionUser,
) -> Result<TimeClockStatus, String> {
    state
        .time_clock
        .status(
            state.demo.store_id,
            session.user.id,
            session.user.role.can_manage_master_data(),
            50,
        )
        .await
        .map_err(|error| error.to_string())
}

#[tauri::command]
async fn note_list(state: State<'_, AppState>) -> Result<NoteInbox, String> {
    let session = require_session(state.inner())?;
    load_note_inbox(state.inner(), &session).await
}

#[tauri::command]
async fn note_create(
    state: State<'_, AppState>,
    request: CreateNoteRequest,
) -> Result<NoteInbox, String> {
    let session = require_manager(state.inner())?;
    let state = state.inner().clone();
    state
        .notes
        .create(state.demo.store_id, session.user.id, request)
        .await
        .map_err(|error| error.to_string())?;
    load_note_inbox(&state, &session).await
}

#[tauri::command]
async fn note_read(state: State<'_, AppState>, note_id: NoteId) -> Result<NoteInbox, String> {
    let session = require_session(state.inner())?;
    let state = state.inner().clone();
    state
        .notes
        .read(
            note_id,
            state.demo.store_id,
            session.user.id,
            session.user.role.can_manage_master_data(),
        )
        .await
        .map_err(|error| error.to_string())
}

async fn load_note_inbox(state: &AppState, session: &SessionUser) -> Result<NoteInbox, String> {
    state
        .notes
        .inbox(
            state.demo.store_id,
            session.user.id,
            session.user.role.can_manage_master_data(),
            100,
        )
        .await
        .map_err(|error| error.to_string())
}

#[tauri::command]
async fn preview_product_import(
    state: State<'_, AppState>,
    source_system: String,
    batch_id: Option<String>,
    csv: String,
) -> Result<ProductImportReport, String> {
    require_manager(state.inner())?;
    let state = state.inner().clone();
    let plan = state
        .imports
        .prepare(&source_system, batch_id.as_deref(), &csv)
        .map_err(|error| error.to_string())?;
    state
        .imports
        .preview(&plan)
        .await
        .map_err(|error| error.to_string())
}

#[tauri::command]
async fn commit_product_import(
    state: State<'_, AppState>,
    source_system: String,
    batch_id: Option<String>,
    csv: String,
) -> Result<ProductImportReport, String> {
    require_manager(state.inner())?;
    let state = state.inner().clone();
    let plan = state
        .imports
        .prepare(&source_system, batch_id.as_deref(), &csv)
        .map_err(|error| error.to_string())?;
    state
        .imports
        .commit(&state.sync_ctx, state.demo.store_id, &plan)
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
    let url = std::env::var("TPV_SYNC_URL")
        .map_err(|_| "TPV_SYNC_URL no configurada en el terminal".to_string())?;
    perform_sync(state.inner(), &url).await
}

async fn ensure_enrolled(state: &AppState, url: &str) -> Result<String, String> {
    if let Some(token) = state
        .store
        .device_token()
        .await
        .map_err(|error| error.to_string())?
    {
        return Ok(token);
    }
    let admin_token = std::env::var("TPV_CENTRAL_TOKEN").ok();
    let device_name = std::env::var("TPV_DEVICE_NAME").unwrap_or_else(|_| "Caja Tauri".to_string());
    let client = SyncClient::new(url.to_string(), None);
    let enrollment = client
        .enroll(
            EnrollRequest {
                tenant_id: state.sync_ctx.tenant_id.as_uuid(),
                store_id: state.demo.store_id.as_uuid(),
                device_id: state.sync_ctx.device_id.as_uuid(),
                name: device_name,
            },
            admin_token.as_deref(),
        )
        .await
        .map_err(|error| format!("no se pudo enrolar el terminal: {error}"))?;
    state
        .store
        .save_device_enrollment(&enrollment.token, &enrollment.device.name)
        .await
        .map_err(|error| error.to_string())?;
    state
        .store
        .save_pull_cursor(0)
        .await
        .map_err(|error| error.to_string())?;
    println!(
        "Dispositivo enrolado automáticamente: {} · {}",
        enrollment.device.device_id, enrollment.device.status
    );
    Ok(enrollment.token)
}

async fn perform_sync(state: &AppState, url: &str) -> Result<SyncSummary, String> {
    let _guard = state.sync_lock.lock().await;
    let token = ensure_enrolled(state, url).await?;
    let client = SyncClient::new(url.to_string(), Some(token));
    let worker = SyncWorker::new(client.clone(), RetryPolicy::default());
    let report = worker
        .push_pending(state.sync_repo.as_ref(), state.sync_ctx.device_id, 100)
        .await
        .map_err(|error| error.to_string())?;

    let mut cursor = state
        .store
        .pull_cursor()
        .await
        .map_err(|error| error.to_string())?;
    let mut pulled_events = 0;
    let mut users_applied = 0;
    let mut master_applied = 0;
    let mut master_conflicts = 0;
    loop {
        let pulled = client
            .pull(cursor, Some(state.demo.store_id.as_uuid()))
            .await
            .map_err(|error| error.to_string())?;
        let count = pulled.events.len();
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
                        let invalidate_session = state.session.read().is_ok_and(|session| {
                            session.as_ref().is_some_and(|current| {
                                current.user.id == provision.user.id
                                    && (current.user.role != provision.user.role
                                        || current.user.active != provision.user.active
                                        || current.user.store_ids != provision.user.store_ids
                                        || current.user.username != provision.user.username)
                            })
                        });
                        if invalidate_session {
                            if let Ok(mut session) = state.session.write() {
                                *session = None;
                            }
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
        state
            .store
            .save_pull_cursor(pulled.next_cursor)
            .await
            .map_err(|error| error.to_string())?;
        pulled_events += count;
        if count < 1000 || pulled.next_cursor == cursor {
            break;
        }
        cursor = pulled.next_cursor;
    }

    Ok(SyncSummary {
        accepted: report.accepted,
        duplicates: report.duplicates,
        rejected: report.rejected,
        pulled: pulled_events,
        users_applied,
        master_applied,
        master_conflicts,
    })
}

fn sync_interval_secs(value: Option<&str>) -> u64 {
    value
        .and_then(|value| value.trim().parse::<u64>().ok())
        .filter(|value| *value > 0)
        .unwrap_or(60)
        .max(5)
}

async fn sync_loop(state: AppState) {
    let Ok(url) = std::env::var("TPV_SYNC_URL") else {
        return;
    };
    let interval = sync_interval_secs(std::env::var("TPV_SYNC_INTERVAL_SECS").ok().as_deref());
    println!("Sincronización automática cada {interval}s contra {url}");
    loop {
        if let Err(error) = perform_sync(&state, &url).await {
            eprintln!("Sync automático: {error}");
        }
        tokio::time::sleep(Duration::from_secs(interval)).await;
    }
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
    let import_repo: Arc<dyn ProductImportRepository> = store.clone();
    let time_clock_repo: Arc<dyn TimeClockRepository> = store.clone();
    let note_repo: Arc<dyn NoteRepository> = store.clone();
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
    let products = ProductService::new(catalog_repo.clone(), master_data.clone());
    let families = FamilyService::new(catalog_repo.clone(), master_data.clone());
    let taxes = TaxService::new(catalog_repo.clone(), master_data.clone());
    let sellers = SellerService::new(catalog_repo.clone(), master_data.clone());
    let stores = StoreService::new(catalog_repo.clone(), master_data.clone());
    let imports = ProductImportService::new(import_repo);
    let time_clock = TimeClockService::new(time_clock_repo, user_repo.clone(), clock.clone());
    let notes = NoteService::new(note_repo, user_repo.clone(), clock.clone());
    let customers = CustomerService::new(catalog_repo, master_data.clone());
    let auth = AuthService::new(
        user_repo.clone(),
        Arc::new(Argon2PasswordHasher),
        audit_repo,
        clock,
    );
    auth.bootstrap_password("admin", "admin123").await?;
    Ok(AppState {
        catalog,
        customers,
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
        families,
        taxes,
        sellers,
        stores,
        imports,
        notes,
        time_clock,
        sync_lock: Arc::new(tokio::sync::Mutex::new(())),
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
            let sync_state = state.clone();
            tauri::async_runtime::spawn(sync_loop(sync_state));
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
            rectify_invoice,
            get_invoice,
            refund_invoice_lines,
            open_cash,
            close_cash,
            cash_status,
            cash_movement,
            print_ticket,
            print_pending,
            list_print_jobs,
            list_sales,
            query_sales,
            sync_status,
            list_failed_sync_events,
            retry_sync_events,
            retry_all_sync_events,
            list_users,
            create_user,
            update_user,
            reset_password,
            list_stores,
            list_sellers,
            save_seller,
            save_store,
            list_products,
            save_product,
            list_families,
            save_family,
            list_taxes,
            save_tax,
            time_clock_status,
            time_clock_in,
            time_clock_out,
            note_list,
            note_create,
            note_read,
            preview_product_import,
            commit_product_import,
            list_customers,
            save_customer,
            list_master_data_conflicts,
            backup_now,
            sync_now
        ])
        .run(tauri::generate_context!())
        .expect("error while running General TPV");
}

#[cfg(test)]
mod config_tests {
    use super::sync_interval_secs;

    #[test]
    fn sync_interval_uses_default_and_minimum() {
        assert_eq!(sync_interval_secs(None), 60);
        assert_eq!(sync_interval_secs(Some("")), 60);
        assert_eq!(sync_interval_secs(Some("invalid")), 60);
        assert_eq!(sync_interval_secs(Some("0")), 60);
        assert_eq!(sync_interval_secs(Some("1")), 5);
        assert_eq!(sync_interval_secs(Some("120")), 120);
    }
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
                rectify_invoice,
                get_invoice,
                refund_invoice_lines,
                open_cash,
                close_cash,
                cash_status,
                cash_movement,
                print_ticket,
                print_pending,
                list_print_jobs,
                list_sales,
                query_sales,
                sync_status,
                list_failed_sync_events,
                retry_sync_events,
                retry_all_sync_events,
                list_users,
                create_user,
                update_user,
                reset_password,
                list_stores,
                list_sellers,
                save_seller,
                save_store,
                list_products,
                save_product,
                list_families,
                save_family,
                list_taxes,
                save_tax,
                time_clock_status,
                time_clock_in,
                time_clock_out,
                note_list,
                note_create,
                note_read,
                preview_product_import,
                commit_product_import,
                list_customers,
                save_customer,
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

        let customer = invoke(
            &webview,
            "save_customer",
            json!({
                "request": {
                    "first_name": "Cliente",
                    "last_name1": "E2E",
                    "nif": "12345678Z",
                    "phone": "600123123",
                    "active": true
                }
            }),
        )
        .expect("save customer");
        let customer_id = customer["id"].as_str().expect("customer id").to_string();
        assert_eq!(customer["nif"], "12345678Z");
        let customers = invoke(&webview, "list_customers", json!({})).expect("list customers");
        assert!(
            customers
                .as_array()
                .expect("customers array")
                .iter()
                .any(|item| item["id"] == customer_id)
        );

        let opened = invoke(&webview, "open_cash", json!({})).expect("open cash");
        let cash_id = opened["id"].as_str().expect("cash session id").to_string();
        let cash = invoke(
            &webview,
            "cash_movement",
            json!({
                "sessionId": cash_id,
                "kind": "expense",
                "amountCents": 500,
                "reason": "Compra de hielo"
            }),
        )
        .expect("cash expense");
        assert_eq!(cash["movements"][0]["kind"], "expense");
        assert_eq!(cash["movements"][0]["amount"], 500);
        let sale = invoke(&webview, "start_sale", json!({ "customerId": customer_id }))
            .expect("start sale");
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

        let invoice =
            invoke(&webview, "issue_invoice", json!({ "saleId": sale_id })).expect("issue invoice");
        let invoice_id = invoice["id"].as_str().expect("invoice id").to_string();
        assert_eq!(invoice["kind"]["type"], "standard");
        assert_eq!(invoice["customer_id"], customer_id);
        let line_id = invoice["lines"][0]["source_line_id"]
            .as_str()
            .expect("invoice line id")
            .to_string();
        let fetched = invoke(&webview, "get_invoice", json!({ "invoiceId": invoice_id }))
            .expect("get invoice");
        assert_eq!(fetched["lines"].as_array().expect("invoice lines").len(), 1);
        let forbidden = invoke(
            &webview,
            "refund_invoice_lines",
            json!({
                "invoiceId": invoice_id,
                "reason": "Reembolso tarjeta sin referencia",
                "refundMethod": "card",
                "refundReference": null,
                "sessionId": null,
                "lines": [{
                    "source_line_id": line_id,
                    "quantity": "1"
                }]
            }),
        )
        .expect_err("card refund requires a reference");
        assert!(
            forbidden
                .as_str()
                .unwrap_or_default()
                .contains("referencia")
        );
        let refund = invoke(
            &webview,
            "refund_invoice_lines",
            json!({
                "invoiceId": invoice_id,
                "reason": "Devolución parcial",
                "refundMethod": "cash",
                "refundReference": null,
                "sessionId": cash_id,
                "lines": [{
                    "source_line_id": line_id,
                    "quantity": "1"
                }]
            }),
        )
        .expect("partial refund");
        assert_eq!(refund["invoice"]["kind"]["type"], "rectificative");
        assert_eq!(
            refund["invoice"]["kind"]["data"]["reason"],
            "Devolución parcial"
        );
        assert_eq!(refund["invoice"]["totals"]["gross"], -120);
        let movements = refund["cash"]["movements"]
            .as_array()
            .expect("cash movements");
        let movement = movements.last().expect("refund movement");
        assert_eq!(movement["kind"], "expense");
        assert_eq!(movement["amount"], 120);
        assert!(
            movement["reason"]
                .as_str()
                .unwrap_or_default()
                .contains("REC-000001")
        );
        let sales = invoke(&webview, "list_sales", json!({})).expect("list sales");
        assert_eq!(sales.as_array().expect("sales array").len(), 1);
        let ticket_number = sales[0]["sale"]["ticket_number"]
            .as_u64()
            .expect("ticket number");

        let history = invoke(
            &webview,
            "query_sales",
            json!({
                "request": {
                    "limit": 10,
                    "offset": 0,
                    "state": "paid",
                    "ticket_number": ticket_number
                }
            }),
        )
        .expect("query sales");
        assert_eq!(history["total"], 1);
        assert_eq!(history["items"].as_array().expect("history items").len(), 1);
        assert!(history["gross"].as_i64().unwrap_or_default() > 0);

        let no_results = invoke(
            &webview,
            "query_sales",
            json!({
                "request": {
                    "limit": 10,
                    "offset": 0,
                    "ticket_number": ticket_number + 1
                }
            }),
        )
        .expect("query empty sales");
        assert_eq!(no_results["total"], 0);

        let cash = invoke(&webview, "cash_status", json!({})).expect("cash status");
        assert!(cash.is_object());
        assert_eq!(cash["state"], "open");

        let conflicts =
            invoke(&webview, "list_master_data_conflicts", json!({})).expect("conflicts");
        assert_eq!(conflicts.as_array().expect("conflicts array").len(), 0);

        let defaults = invoke(&webview, "list_taxes", json!({})).expect("default taxes");
        assert!(
            defaults
                .as_array()
                .expect("taxes array")
                .iter()
                .any(|item| item["code"] == "IVA21")
        );

        let family = invoke(
            &webview,
            "save_family",
            json!({"request": {"code": "UI", "name": "Pruebas UI", "active": true}}),
        )
        .expect("save family");
        assert_eq!(family["code"], "UI");
        assert_eq!(family["active"], true);

        let tax = invoke(
            &webview,
            "save_tax",
            json!({"request": {"code": "UI5", "name": "Especial 5%", "percent_bp": 500, "active": true}}),
        )
        .expect("save tax");
        assert_eq!(tax["code"], "UI5");
        assert_eq!(tax["percent_bp"], 500);

        let families = invoke(&webview, "list_families", json!({})).expect("list families");
        assert!(
            families
                .as_array()
                .expect("families array")
                .iter()
                .any(|item| item["code"] == "UI")
        );
        let taxes = invoke(&webview, "list_taxes", json!({})).expect("list taxes");
        assert!(
            taxes
                .as_array()
                .expect("taxes array")
                .iter()
                .any(|item| item["code"] == "UI5")
        );

        let seller = invoke(
            &webview,
            "save_seller",
            json!({
                "request": {
                    "code": "UI-S",
                    "name": "Vendedor UI",
                    "phone1": "600123123",
                    "active": true
                }
            }),
        )
        .expect("save seller");
        assert_eq!(seller["code"], "UI-S");
        assert_eq!(seller["name"], "Vendedor UI");
        assert_eq!(seller["phone1"], "600123123");
        let sellers = invoke(&webview, "list_sellers", json!({})).expect("list sellers");
        assert!(
            sellers
                .as_array()
                .expect("sellers array")
                .iter()
                .any(|item| item["code"] == "UI-S")
        );

        let store_master = invoke(
            &webview,
            "save_store",
            json!({
                "request": {
                    "code": "UI-STORE",
                    "name": "Tienda UI",
                    "tax_id": "B12345674",
                    "phone": "952123123",
                    "street": "Calle UI",
                    "number": "7",
                    "locality": "Benalmádena",
                    "postal_code": "29630",
                    "province": "Málaga",
                    "active": true
                }
            }),
        )
        .expect("save store");
        assert_eq!(store_master["code"], "UI-STORE");
        assert_eq!(store_master["name"], "Tienda UI");
        assert_eq!(store_master["tax_id"], "B12345674");
        let store_master_id = store_master["id"]
            .as_str()
            .expect("store master id")
            .to_string();
        let store_list = invoke(&webview, "list_stores", json!({})).expect("list stores");
        assert!(
            store_list
                .as_array()
                .expect("stores array")
                .iter()
                .any(|item| item["id"] == store_master_id && item["code"] == "UI-STORE")
        );

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
                    "tax_code": "UI5",
                    "tax_percent_bp": 500,
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
                    "tax_code": "UI5",
                    "tax_percent_bp": 500,
                    "active": true
                }
            }),
        )
        .expect("update product");
        assert_eq!(updated["name"], "Producto UI actualizado");
        assert_eq!(updated["unit_price"], "10.95");

        let import_csv = "source_id,sku,name,unit_price,tax_percent_bp\n\
                          legacy-1,IMP-1,Producto importado,3.25,2100\n";
        let preview = invoke(
            &webview,
            "preview_product_import",
            json!({
                "sourceSystem": "legacy",
                "batchId": null,
                "csv": import_csv
            }),
        )
        .expect("preview product import");
        assert_eq!(preview["valid_rows"], 1);
        assert_eq!(preview["inserted"], 1);
        assert_eq!(preview["committed"], false);

        let imported = invoke(
            &webview,
            "commit_product_import",
            json!({
                "sourceSystem": "legacy",
                "batchId": null,
                "csv": import_csv
            }),
        )
        .expect("commit product import");
        assert_eq!(imported["committed"], true);
        assert_eq!(imported["inserted"], 1);
        assert_eq!(
            imported["inserted"].as_u64().unwrap_or(0)
                + imported["updated"].as_u64().unwrap_or(0)
                + imported["unchanged"].as_u64().unwrap_or(0),
            imported["valid_rows"].as_u64().unwrap_or(0)
        );

        let replay = invoke(
            &webview,
            "commit_product_import",
            json!({
                "sourceSystem": "legacy",
                "batchId": null,
                "csv": import_csv
            }),
        )
        .expect("replay product import");
        assert_eq!(replay["already_imported"], true);
        assert_eq!(replay["inserted"], 1);

        let products = invoke(&webview, "list_products", json!({})).expect("imported products");
        assert!(
            products
                .as_array()
                .expect("products array")
                .iter()
                .any(|item| item["sku"] == "IMP-1")
        );

        let time_clock =
            invoke(&webview, "time_clock_status", json!({})).expect("time clock status");
        assert!(time_clock["active"].is_null());
        assert_eq!(
            time_clock["recent"]
                .as_array()
                .expect("recent time entries")
                .len(),
            0
        );

        let clocked_in = invoke(&webview, "time_clock_in", json!({})).expect("clock in");
        assert_eq!(clocked_in["active"]["user_name"], "Administrador");
        assert!(clocked_in["active"]["entry"]["clock_out"].is_null());
        assert!(invoke(&webview, "time_clock_in", json!({})).is_err());

        let clocked_out = invoke(&webview, "time_clock_out", json!({})).expect("clock out");
        assert!(clocked_out["active"].is_null());
        assert_eq!(
            clocked_out["recent"]
                .as_array()
                .expect("recent time entries")
                .len(),
            1
        );
        assert!(!clocked_out["recent"][0]["entry"]["clock_out"].is_null());

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
        let cashier_id = cashier["id"].as_str().expect("cashier id").to_string();

        let empty_notes = invoke(&webview, "note_list", json!({})).expect("empty notes");
        assert_eq!(
            empty_notes["notes"].as_array().expect("notes array").len(),
            0
        );
        assert_eq!(empty_notes["unread"], 0);
        assert!(
            empty_notes["recipients"]
                .as_array()
                .expect("note recipients")
                .iter()
                .any(|recipient| recipient["user_name"] == "Cajero Uno")
        );

        let created_notes = invoke(
            &webview,
            "note_create",
            json!({
                "request": {
                    "title": "Cambio de turno",
                    "body": "Abrir caja a las 08:30.",
                    "recipient_ids": [cashier_id]
                }
            }),
        )
        .expect("create note");
        assert_eq!(
            created_notes["notes"]
                .as_array()
                .expect("created notes")
                .len(),
            1
        );
        let note_id = created_notes["notes"][0]["note"]["id"]
            .as_str()
            .expect("note id")
            .to_string();
        assert_eq!(created_notes["notes"][0]["author_name"], "Administrador");

        invoke(&webview, "logout", json!({})).expect("logout admin");
        invoke(
            &webview,
            "login",
            json!({ "username": "cajero1", "password": "cajero123" }),
        )
        .expect("cashier login");
        let cashier_notes = invoke(&webview, "note_list", json!({})).expect("cashier notes");
        assert_eq!(cashier_notes["unread"], 1);
        assert_eq!(cashier_notes["notes"][0]["note"]["id"], note_id);
        assert!(cashier_notes["notes"][0]["viewer_read_at"].is_null());
        assert_eq!(
            cashier_notes["recipients"]
                .as_array()
                .expect("cashier note recipients")
                .len(),
            0
        );
        let read_notes =
            invoke(&webview, "note_read", json!({ "noteId": note_id })).expect("read note");
        assert_eq!(read_notes["unread"], 0);
        assert!(!read_notes["notes"][0]["viewer_read_at"].is_null());
        let forbidden = invoke(
            &webview,
            "note_create",
            json!({
                "request": {
                    "title": "No permitido",
                    "body": "El cajero no puede crear notas.",
                    "recipient_ids": [cashier_id]
                }
            }),
        )
        .expect_err("cashier cannot create notes");
        assert!(
            forbidden
                .as_str()
                .unwrap_or_default()
                .contains("responsable")
        );
        let forbidden =
            invoke(&webview, "list_users", json!({})).expect_err("cashier cannot list users");
        assert!(
            forbidden
                .as_str()
                .unwrap_or_default()
                .contains("gestionar usuarios")
        );
        let forbidden = invoke(
            &webview,
            "save_customer",
            json!({
                "request": {
                    "first_name": "Cliente",
                    "last_name1": "Cajero",
                    "active": true
                }
            }),
        )
        .expect_err("cashier cannot save customers");
        assert!(
            forbidden
                .as_str()
                .unwrap_or_default()
                .contains("responsable")
        );
        let forbidden = invoke(
            &webview,
            "rectify_invoice",
            json!({
                "invoiceId": invoice_id,
                "reason": "Intento de cajero"
            }),
        )
        .expect_err("cashier cannot rectify invoices");
        assert!(
            forbidden
                .as_str()
                .unwrap_or_default()
                .contains("responsable")
        );
        let forbidden = invoke(
            &webview,
            "refund_invoice_lines",
            json!({
                "invoiceId": invoice_id,
                "reason": "Intento de cajero",
                "refundMethod": "cash",
                "refundReference": null,
                "sessionId": cash_id,
                "lines": [{
                    "source_line_id": line_id,
                    "quantity": "1"
                }]
            }),
        )
        .expect_err("cashier cannot partially refund invoices");
        assert!(
            forbidden
                .as_str()
                .unwrap_or_default()
                .contains("responsable")
        );
        let forbidden = invoke(
            &webview,
            "cash_movement",
            json!({
                "sessionId": cash_id,
                "kind": "withdrawal",
                "amountCents": 100,
                "reason": "Intento de cajero"
            }),
        )
        .expect_err("cashier cannot withdraw cash");
        assert!(
            forbidden
                .as_str()
                .unwrap_or_default()
                .contains("responsable")
        );

        drop(webview);
        drop(app);
        let _ = std::fs::remove_dir_all(data_dir);
    }
}
