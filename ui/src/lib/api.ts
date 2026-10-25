import { invoke } from '@tauri-apps/api/core';

export type MoneyCents = number;
export type PaymentMethod = 'cash' | 'card' | 'voucher' | 'other';
export type LineKind = 'sale' | 'return';

export interface TaxRate {
  code: string;
  percent_bp: number;
}

export interface TaxBreakdown {
  code: string;
  percent_bp: number;
  gross: MoneyCents;
  net: MoneyCents;
  tax: MoneyCents;
}

export interface SaleTotals {
  gross: MoneyCents;
  net: MoneyCents;
  tax: MoneyCents;
  tax_breakdown: TaxBreakdown[];
  paid: MoneyCents;
  received: MoneyCents;
  due: MoneyCents;
  change: MoneyCents;
}

export interface Product {
  id: string;
  sku: string;
  barcode: string | null;
  name: string;
  family_code: string | null;
  unit_price: string;
  tax_rate: TaxRate;
  active: boolean;
}

export interface SaveProductRequest {
  product_id?: string | null;
  sku: string;
  barcode: string | null;
  name: string;
  family_code: string | null;
  unit_price: string;
  tax_code: string;
  tax_percent_bp: number;
  active: boolean;
}

export interface Family {
  id: string;
  code: string;
  name: string;
  active: boolean;
}

export interface SaveFamilyRequest {
  family_id?: string | null;
  code: string;
  name: string;
  active: boolean;
}

export interface PriceList {
  id: string;
  code: string;
  name: string;
  active: boolean;
}

export interface PriceListItem {
  product_id: string;
  unit_price: string;
}

export interface PriceListSnapshot {
  price_list: PriceList;
  items: PriceListItem[];
}

export interface PriceListItemRequest {
  product_id: string;
  unit_price: string;
}

export interface SavePriceListRequest {
  price_list_id?: string | null;
  code: string;
  name: string;
  items: PriceListItemRequest[];
  active: boolean;
}

export type StockMovementKind =
  | 'initial'
  | 'purchase'
  | 'sale'
  | 'return'
  | 'adjustment_in'
  | 'adjustment_out';

export interface StockLevel {
  store_id: string;
  product_id: string;
  sku: string;
  name: string;
  quantity: string;
  min_quantity: string;
  updated_at: string | null;
}

export interface StockMovement {
  id: string;
  store_id: string;
  product_id: string;
  kind: StockMovementKind;
  quantity: string;
  reason: string | null;
  sale_id: string | null;
  sale_line_id: string | null;
  occurred_at: string;
}

export interface RecordStockMovementRequest {
  product_id: string;
  kind: StockMovementKind;
  quantity: string;
  reason?: string | null;
}

export interface SetStockMinimumRequest {
  product_id: string;
  min_quantity: string;
}

export interface SetStockCountRequest {
  product_id: string;
  counted_quantity: string;
  reason: string;
}

export interface StockCountResult {
  level: StockLevel;
  previous_quantity: string;
  counted_quantity: string;
  difference: string;
  movement: StockMovement | null;
}

export interface StockMovementView {
  id: string;
  store_id: string;
  product_id: string;
  sku: string;
  name: string;
  kind: StockMovementKind;
  quantity: string;
  reason: string | null;
  sale_id: string | null;
  sale_line_id: string | null;
  occurred_at: string;
}

export interface QueryStockMovementsRequest {
  search?: string | null;
  product_id?: string | null;
  kind?: StockMovementKind | null;
  from_ms?: number | null;
  to_ms?: number | null;
  limit?: number;
  offset?: number;
}

export interface StockMovementPage {
  items: StockMovementView[];
  total: number;
  limit: number;
  offset: number;
}

export type StockLevelStatus = 'all' | 'low' | 'restock' | 'out';

export interface StockLevelsQuery {
  search?: string | null;
  status?: StockLevelStatus;
}

export interface Tax {
  id: string;
  code: string;
  name: string;
  percent_bp: number;
  active: boolean;
}

export interface SaveTaxRequest {
  tax_id?: string | null;
  code: string;
  name: string;
  percent_bp: number;
  active: boolean;
}

export interface ProductImportError {
  line: number;
  source_id: string | null;
  message: string;
}

export interface ProductImportReport {
  source_system: string;
  batch_id: string;
  dry_run: boolean;
  committed: boolean;
  already_imported: boolean;
  total_rows: number;
  valid_rows: number;
  invalid_rows: number;
  inserted: number;
  updated: number;
  unchanged: number;
  errors: ProductImportError[];
}

export interface Customer {
  id: string;
  first_name: string;
  last_name1: string;
  last_name2: string | null;
  phone1: string | null;
  email: string | null;
  nif: string | null;
  address: {
    street: string;
    number: string;
    locality: string;
    postal_code: string;
    province: string;
  };
  active: boolean;
}

export interface SaveCustomerRequest {
  customer_id?: string | null;
  first_name: string;
  last_name1: string;
  last_name2?: string | null;
  nif?: string | null;
  phone?: string | null;
  email?: string | null;
  street?: string | null;
  number?: string | null;
  locality?: string | null;
  postal_code?: string | null;
  province?: string | null;
  active: boolean;
}

export interface MasterDataChange {
  kind: string;
  entity_id: string;
  version: number;
  device_id: string;
  occurred_at: string;
  payload: unknown;
  deleted: boolean;
}

export interface MasterDataConflict {
  id: string;
  tenant_id: string;
  store_id: string;
  kind: string;
  entity_id: string;
  local_version: number;
  incoming: MasterDataChange;
  reason: string;
  outcome: string;
  detected_at: string;
}

export interface ProductSearchResult {
  id: string;
  sku: string;
  name: string;
  price_cents: MoneyCents;
  tax_code: string;
  tax_percent_bp: number;
}

export interface SaleLine {
  id: string;
  product_id: string | null;
  sku: string;
  description: string;
  unit_price: string;
  quantity: string;
  discount_bp: number;
  tax_rate: TaxRate;
  kind: LineKind;
}

export interface Payment {
  id: string;
  method: PaymentMethod;
  amount: MoneyCents;
  received: MoneyCents;
  reference: string | null;
}

export interface Sale {
  id: string;
  store_id: string;
  register_id: string;
  seller_id: string;
  customer_id: string | null;
  lines: SaleLine[];
  payments: Payment[];
  state: string;
  created_at: string;
  closed_at: string | null;
  invoice_id: string | null;
  ticket_series: string | null;
  ticket_number: number | null;
  notes: string | null;
}

export interface SaleView {
  sale: Sale;
  totals: SaleTotals;
}

export interface SaleHistoryRequest {
  limit?: number;
  offset?: number;
  from_ms?: number | null;
  to_ms?: number | null;
  state?: string | null;
  ticket_number?: number | null;
}

export interface SaleHistoryPage {
  items: SaleView[];
  total: number;
  limit: number;
  offset: number;
  gross: MoneyCents;
  tax: MoneyCents;
  paid: MoneyCents;
}

export interface InvoiceLine {
  source_line_id: string;
  description: string;
  quantity: string;
  unit_price: string;
  discount_bp: number;
  tax_rate: TaxRate;
  gross: MoneyCents;
  tax: MoneyCents;
  kind: LineKind;
}

export interface Invoice {
  id: string;
  sale_id: string;
  customer_id: string;
  series: string;
  number: number;
  issued_at: string;
  kind: unknown;
  lines: InvoiceLine[];
  totals: SaleTotals;
}

export interface RefundLineInput {
  source_line_id: string;
  quantity: string;
}

export interface RefundSummary {
  invoice: Invoice;
  cash: CashSession | null;
}

export interface CashCount {
  cash: MoneyCents;
  card: MoneyCents;
  voucher: MoneyCents;
}

export interface CashCloseRecord {
  closed_at: string;
  closed_by: string;
  opening_float: MoneyCents;
  cash_sales: MoneyCents;
  card_sales: MoneyCents;
  voucher_sales: MoneyCents;
  expenses: MoneyCents;
  withdrawals: MoneyCents;
  deposits: MoneyCents;
  expected: CashCount;
  counted: CashCount;
  differences: CashCount;
  version: number;
}

export interface PrintJob {
  id: string;
  sale_id: string;
  job_type: 'ticket' | 'invoice';
  document: unknown;
  bytes: number[];
  status: 'pending' | 'printed' | 'failed';
  attempts: number;
  created_at: string;
  printed_at: string | null;
  last_error: string | null;
}

export interface PrintSummary {
  job_id: string;
  file: string;
  bytes: number;
  success: boolean;
}

export type CashMovementKind = 'expense' | 'withdrawal' | 'deposit';

export interface CashMovement {
  id: string;
  kind: CashMovementKind;
  amount: MoneyCents;
  reason: string;
  created_at: string;
}

export interface CashSession {
  id: string;
  store_id: string;
  register_id: string;
  opened_by: string;
  opened_at: string;
  opening_float: MoneyCents;
  movements: CashMovement[];
  close_history: CashCloseRecord[];
  reopen_history: unknown[];
  state: 'open' | 'closed';
  version: number;
}

export type UserRole = 'admin' | 'manager' | 'cashier' | 'auditor';

export interface UserAccount {
  id: string;
  username: string;
  display_name: string;
  role: UserRole;
  store_ids: string[];
  active: boolean;
}

export interface Store {
  id: string;
  code: string;
  name: string;
  tax_id: string;
  phone: string | null;
  address: {
    street: string;
    number: string;
    locality: string;
    postal_code: string;
    province: string;
  };
  active: boolean;
}

export interface Seller {
  id: string;
  code: string;
  name: string;
  phone1: string | null;
  phone2: string | null;
  active: boolean;
}

export interface SaveSellerRequest {
  seller_id?: string | null;
  code: string;
  name: string;
  phone1?: string | null;
  phone2?: string | null;
  active: boolean;
}

export interface SaveStoreRequest {
  store_id?: string | null;
  code: string;
  name: string;
  tax_id: string;
  phone?: string | null;
  street?: string | null;
  number?: string | null;
  locality?: string | null;
  postal_code?: string | null;
  province?: string | null;
  active: boolean;
}

export interface SessionUser {
  session_id: string;
  user: UserAccount;
  started_at: string;
}

export interface TimeEntry {
  id: string;
  user_id: string;
  store_id: string;
  clock_in: string;
  clock_out: string | null;
  correction_reason: string | null;
}

export interface TimeEntryView {
  entry: TimeEntry;
  user_name: string;
}

export interface TimeClockStatus {
  active: TimeEntryView | null;
  recent: TimeEntryView[];
}

export interface Note {
  id: string;
  store_id: string;
  author_id: string;
  title: string;
  body: string;
  created_at: string;
}

export interface NoteRecipientOption {
  user_id: string;
  user_name: string;
}

export interface NoteRecipientView {
  user_id: string;
  user_name: string;
  read_at: string | null;
}

export interface NoteView {
  note: Note;
  author_name: string;
  recipients: NoteRecipientView[];
  viewer_read_at: string | null;
  is_author: boolean;
}

export interface NoteInbox {
  notes: NoteView[];
  unread: number;
  recipients: NoteRecipientOption[];
}

export interface CreateNoteRequest {
  title: string;
  body: string;
  recipient_ids: string[];
}

export interface BackupSummary {
  path: string;
  bytes: number;
}

export interface AppInfo {
  store_name: string;
  store_id: string;
  register_id: string;
  seller_id: string;
  customer_id: string;
  user_id: string;
  products: Product[];
  open_cash: CashSession | null;
}

export interface SyncStatus {
  pending: number;
  sent: number;
  failed: number;
  oldest_pending_at: string | null;
  last_sent_at: string | null;
  last_failed_at: string | null;
  last_error: string | null;
}

export interface SyncSummary {
  accepted: number;
  duplicates: number;
  rejected: number;
  pulled: number;
  users_applied: number;
  master_applied: number;
  master_conflicts: number;
}

export interface QuarantinedEvent {
  event: {
    event_id: string;
    event_type: string;
    aggregate_type: string;
    occurred_at: string;
  };
  attempts: number;
  failed_at: string;
  last_error: string | null;
}

export const api = {
  login: (username: string, password: string) =>
    invoke<SessionUser>('login', { username, password }),
  logout: () => invoke<void>('logout'),
  currentUser: () => invoke<SessionUser | null>('current_user'),
  bootstrap: () => invoke<AppInfo>('bootstrap'),
  searchProducts: (query: string) => invoke<ProductSearchResult[]>('search_products', { query }),
  listProducts: () => invoke<Product[]>('list_products'),
  saveProduct: (request: SaveProductRequest) => invoke<Product>('save_product', { request }),
  listFamilies: () => invoke<Family[]>('list_families'),
  saveFamily: (request: SaveFamilyRequest) => invoke<Family>('save_family', { request }),
  listTaxes: () => invoke<Tax[]>('list_taxes'),
  saveTax: (request: SaveTaxRequest) => invoke<Tax>('save_tax', { request }),
  listPriceLists: () => invoke<PriceListSnapshot[]>('list_price_lists'),
  savePriceList: (request: SavePriceListRequest) =>
    invoke<PriceListSnapshot>('save_price_list', { request }),
  listStockLevels: () => invoke<StockLevel[]>('list_stock_levels'),
  exportStockCsv: (request: StockLevelsQuery) =>
    invoke<string | null>('export_stock_csv', { request }),
  exportStockReplenishmentCsv: (request: StockLevelsQuery) =>
    invoke<string | null>('export_stock_replenishment_csv', { request }),
  exportStockCountSheetCsv: (request: StockLevelsQuery) =>
    invoke<string | null>('export_stock_count_sheet_csv', { request }),
  exportStockMovementsCsv: (productId: string) =>
    invoke<string | null>('export_stock_movements_csv', { productId }),
  listStockLevelsForProducts: (productIds: string[]) =>
    invoke<StockLevel[]>('list_stock_levels_for_products', { productIds }),
  countLowStock: () => invoke<number>('count_low_stock'),
  listStockMovements: (productId: string, limit = 100) =>
    invoke<StockMovement[]>('list_stock_movements', { productId, limit }),
  queryStockMovements: (request: QueryStockMovementsRequest) =>
    invoke<StockMovementPage>('query_stock_movements', { request }),
  exportStockLedgerCsv: (request: QueryStockMovementsRequest) =>
    invoke<string | null>('export_stock_ledger_csv', { request }),
  recordStockMovement: (request: RecordStockMovementRequest) =>
    invoke<StockMovement>('record_stock_movement', { request }),
  setStockMinimum: (request: SetStockMinimumRequest) =>
    invoke<StockLevel>('set_stock_minimum', { request }),
  setStockCount: (request: SetStockCountRequest) =>
    invoke<StockCountResult>('set_stock_count', { request }),
  listSellers: () => invoke<Seller[]>('list_sellers'),
  saveSeller: (request: SaveSellerRequest) => invoke<Seller>('save_seller', { request }),
  saveStore: (request: SaveStoreRequest) => invoke<Store>('save_store', { request }),
  previewProductImport: (sourceSystem: string, batchId: string | null, csv: string) =>
    invoke<ProductImportReport>('preview_product_import', { sourceSystem, batchId, csv }),
  commitProductImport: (sourceSystem: string, batchId: string | null, csv: string) =>
    invoke<ProductImportReport>('commit_product_import', { sourceSystem, batchId, csv }),
  listCustomers: () => invoke<Customer[]>('list_customers'),
  saveCustomer: (request: SaveCustomerRequest) =>
    invoke<Customer>('save_customer', { request }),
  listMasterDataConflicts: (limit = 100) =>
    invoke<MasterDataConflict[]>('list_master_data_conflicts', { limit }),
  startSale: (customerId?: string | null, priceListId?: string | null) =>
    invoke<SaleView>('start_sale', {
      customerId: customerId ?? null,
      priceListId: priceListId ?? null
    }),
  addLine: (saleId: string, sku: string, quantity: string, kind: LineKind) =>
    invoke<SaleView>('add_line', { saleId, sku, quantity, kind }),
  applyPayment: (
    saleId: string,
    method: PaymentMethod,
    amountCents: MoneyCents,
    receivedCents: MoneyCents
  ) =>
    invoke<SaleView>('apply_payment', {
      saleId,
      method,
      amountCents,
      receivedCents
    }),
  confirmSale: (saleId: string) => invoke<SaleView>('confirm_sale', { saleId }),
  issueInvoice: (saleId: string) => invoke<Invoice>('issue_invoice', { saleId }),
  rectifyInvoice: (invoiceId: string, reason: string) =>
    invoke<Invoice>('rectify_invoice', { invoiceId, reason }),
  getInvoice: (invoiceId: string) => invoke<Invoice>('get_invoice', { invoiceId }),
  refundInvoiceLines: (
    invoiceId: string,
    reason: string,
    refundMethod: 'cash' | 'card',
    refundReference: string | null,
    lines: RefundLineInput[],
    sessionId: string | null
  ) =>
    invoke<RefundSummary>('refund_invoice_lines', {
      invoiceId,
      reason,
      refundMethod,
      refundReference,
      lines,
      sessionId
    }),
  openCash: () => invoke<CashSession>('open_cash'),
  closeCash: (
    sessionId: string,
    countedCashCents: MoneyCents,
    countedCardCents: MoneyCents,
    countedVoucherCents: MoneyCents
  ) =>
    invoke<CashCloseRecord>('close_cash', {
      sessionId,
      countedCashCents,
      countedCardCents,
      countedVoucherCents
    }),
  cashStatus: () => invoke<CashSession | null>('cash_status'),
  cashMovement: (
    sessionId: string,
    kind: CashMovementKind,
    amountCents: number,
    reason: string
  ) => invoke<CashSession>('cash_movement', { sessionId, kind, amountCents, reason }),
  timeClockStatus: () => invoke<TimeClockStatus>('time_clock_status'),
  timeClockIn: () => invoke<TimeClockStatus>('time_clock_in'),
  timeClockOut: () => invoke<TimeClockStatus>('time_clock_out'),
  noteList: () => invoke<NoteInbox>('note_list'),
  noteCreate: (request: CreateNoteRequest) => invoke<NoteInbox>('note_create', { request }),
  noteRead: (noteId: string) => invoke<NoteInbox>('note_read', { noteId }),
  printTicket: (saleId: string, reprint = false) =>
    invoke<PrintSummary>('print_ticket', { saleId, reprint }),
  printPending: () => invoke<unknown[]>('print_pending'),
  listPrintJobs: () => invoke<PrintJob[]>('list_print_jobs'),
  listSales: () => invoke<SaleView[]>('list_sales'),
  querySales: (request: SaleHistoryRequest) =>
    invoke<SaleHistoryPage>('query_sales', { request }),
  syncStatus: () => invoke<SyncStatus>('sync_status'),
  syncNow: () => invoke<SyncSummary>('sync_now'),
  listFailedSyncEvents: (limit = 20) =>
    invoke<QuarantinedEvent[]>('list_failed_sync_events', { limit }),
  retrySyncEvents: (eventIds: string[]) =>
    invoke<number>('retry_sync_events', { eventIds }),
  retryAllSyncEvents: () => invoke<number>('retry_all_sync_events'),
  listUsers: () => invoke<UserAccount[]>('list_users'),
  createUser: (
    username: string,
    displayName: string,
    role: UserRole,
    storeIds: string[],
    password: string
  ) =>
    invoke<UserAccount>('create_user', {
      username,
      displayName,
      role,
      storeIds,
      password
    }),
  updateUser: (
    userId: string,
    displayName: string,
    role: UserRole,
    storeIds: string[],
    active: boolean
  ) =>
    invoke<UserAccount>('update_user', {
      userId,
      displayName,
      role,
      storeIds,
      active
    }),
  resetPassword: (userId: string, newPassword: string) =>
    invoke<void>('reset_password', { userId, newPassword }),
  listStores: () => invoke<Store[]>('list_stores'),
  backupNow: () => invoke<BackupSummary>('backup_now')
};

export function formatMoney(cents: MoneyCents): string {
  return `${(cents / 100).toFixed(2)} €`;
}

export function taxPercent(basisPoints: number): string {
  return (basisPoints / 100).toFixed(2).replace(/\.00$/, '');
}

export function stateLabel(state: string): string {
  switch (state) {
    case 'draft':
      return 'Borrador';
    case 'priced':
      return 'Precios';
    case 'awaiting_payment':
      return 'Pendiente de pago';
    case 'paid':
      return 'Cobrada';
    case 'closed':
      return 'Cerrada';
    case 'cancelled':
      return 'Cancelada';
    default:
      return state;
  }
}
