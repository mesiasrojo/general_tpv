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
  active: boolean;
}

export interface SessionUser {
  session_id: string;
  user: UserAccount;
  started_at: string;
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
  listCustomers: () => invoke<Customer[]>('list_customers'),
  saveCustomer: (request: SaveCustomerRequest) =>
    invoke<Customer>('save_customer', { request }),
  listMasterDataConflicts: (limit = 100) =>
    invoke<MasterDataConflict[]>('list_master_data_conflicts', { limit }),
  startSale: (customerId?: string | null) =>
    invoke<SaleView>('start_sale', { customerId: customerId ?? null }),
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
  printTicket: (saleId: string, reprint = false) =>
    invoke<PrintSummary>('print_ticket', { saleId, reprint }),
  printPending: () => invoke<unknown[]>('print_pending'),
  listPrintJobs: () => invoke<PrintJob[]>('list_print_jobs'),
  listSales: () => invoke<SaleView[]>('list_sales'),
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
