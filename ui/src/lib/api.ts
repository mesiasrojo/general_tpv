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

export interface Invoice {
  id: string;
  sale_id: string;
  customer_id: string;
  series: string;
  number: number;
  issued_at: string;
  kind: unknown;
  lines: unknown[];
  totals: SaleTotals;
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

export interface CashSession {
  id: string;
  store_id: string;
  register_id: string;
  opened_by: string;
  opened_at: string;
  opening_float: MoneyCents;
  movements: unknown[];
  close_history: CashCloseRecord[];
  reopen_history: unknown[];
  state: 'open' | 'closed';
  version: number;
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

export const api = {
  bootstrap: () => invoke<AppInfo>('bootstrap'),
  searchProducts: (query: string) => invoke<ProductSearchResult[]>('search_products', { query }),
  startSale: () => invoke<SaleView>('start_sale'),
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
  listSales: () => invoke<SaleView[]>('list_sales')
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
