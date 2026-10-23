PRAGMA foreign_keys = ON;

CREATE TABLE IF NOT EXISTS stores (
    id TEXT PRIMARY KEY,
    code TEXT NOT NULL UNIQUE,
    name TEXT NOT NULL,
    tax_id TEXT NOT NULL,
    phone TEXT,
    street TEXT NOT NULL DEFAULT '',
    number TEXT NOT NULL DEFAULT '',
    locality TEXT NOT NULL DEFAULT '',
    postal_code TEXT NOT NULL DEFAULT '',
    province TEXT NOT NULL DEFAULT '',
    active INTEGER NOT NULL DEFAULT 1
);

CREATE TABLE IF NOT EXISTS registers (
    id TEXT PRIMARY KEY,
    store_id TEXT NOT NULL REFERENCES stores(id),
    code TEXT NOT NULL,
    name TEXT NOT NULL,
    active INTEGER NOT NULL DEFAULT 1,
    UNIQUE(store_id, code)
);

CREATE TABLE IF NOT EXISTS users (
    id TEXT PRIMARY KEY,
    username TEXT NOT NULL UNIQUE,
    display_name TEXT NOT NULL,
    role TEXT NOT NULL,
    active INTEGER NOT NULL DEFAULT 1
);

CREATE TABLE IF NOT EXISTS user_stores (
    user_id TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    store_id TEXT NOT NULL REFERENCES stores(id) ON DELETE CASCADE,
    PRIMARY KEY (user_id, store_id)
);

CREATE TABLE IF NOT EXISTS sellers (
    id TEXT PRIMARY KEY,
    code TEXT NOT NULL UNIQUE,
    name TEXT NOT NULL,
    phone1 TEXT,
    phone2 TEXT,
    active INTEGER NOT NULL DEFAULT 1
);

CREATE TABLE IF NOT EXISTS customers (
    id TEXT PRIMARY KEY,
    first_name TEXT NOT NULL,
    last_name1 TEXT NOT NULL,
    last_name2 TEXT,
    sex TEXT,
    phone1 TEXT,
    phone2 TEXT,
    email TEXT,
    street TEXT NOT NULL DEFAULT '',
    number TEXT NOT NULL DEFAULT '',
    locality TEXT NOT NULL DEFAULT '',
    postal_code TEXT NOT NULL DEFAULT '',
    province TEXT NOT NULL DEFAULT '',
    nif TEXT,
    profession TEXT,
    notes TEXT,
    active INTEGER NOT NULL DEFAULT 1
);

CREATE INDEX IF NOT EXISTS idx_customers_name ON customers(last_name1, first_name);
CREATE INDEX IF NOT EXISTS idx_customers_nif ON customers(nif);

CREATE TABLE IF NOT EXISTS products (
    id TEXT PRIMARY KEY,
    sku TEXT NOT NULL UNIQUE,
    barcode TEXT,
    name TEXT NOT NULL,
    family_code TEXT,
    unit_price TEXT NOT NULL,
    tax_code TEXT NOT NULL,
    tax_percent_bp INTEGER NOT NULL,
    active INTEGER NOT NULL DEFAULT 1
);

CREATE INDEX IF NOT EXISTS idx_products_name ON products(name);
CREATE INDEX IF NOT EXISTS idx_products_barcode ON products(barcode);

CREATE TABLE IF NOT EXISTS sales (
    id TEXT PRIMARY KEY,
    store_id TEXT NOT NULL REFERENCES stores(id),
    register_id TEXT NOT NULL,
    seller_id TEXT NOT NULL REFERENCES sellers(id),
    customer_id TEXT,
    state TEXT NOT NULL,
    created_at TEXT NOT NULL,
    closed_at TEXT,
    invoice_id TEXT,
    ticket_series TEXT,
    ticket_number INTEGER,
    notes TEXT,
    total_gross_cents INTEGER NOT NULL DEFAULT 0,
    total_net_cents INTEGER NOT NULL DEFAULT 0,
    total_tax_cents INTEGER NOT NULL DEFAULT 0,
    paid_cents INTEGER NOT NULL DEFAULT 0,
    change_cents INTEGER NOT NULL DEFAULT 0
);

CREATE INDEX IF NOT EXISTS idx_sales_store_created ON sales(store_id, created_at);
CREATE INDEX IF NOT EXISTS idx_sales_register_created ON sales(register_id, created_at);
CREATE UNIQUE INDEX IF NOT EXISTS idx_sales_ticket ON sales(ticket_series, ticket_number) WHERE ticket_number IS NOT NULL;

CREATE TABLE IF NOT EXISTS sale_lines (
    id TEXT PRIMARY KEY,
    sale_id TEXT NOT NULL REFERENCES sales(id) ON DELETE CASCADE,
    product_id TEXT,
    sku TEXT NOT NULL,
    description TEXT NOT NULL,
    unit_price TEXT NOT NULL,
    quantity TEXT NOT NULL,
    discount_bp INTEGER NOT NULL DEFAULT 0,
    tax_code TEXT NOT NULL,
    tax_percent_bp INTEGER NOT NULL,
    kind TEXT NOT NULL,
    line_index INTEGER NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_sale_lines_sale ON sale_lines(sale_id, line_index);

CREATE TABLE IF NOT EXISTS payments (
    id TEXT PRIMARY KEY,
    sale_id TEXT NOT NULL REFERENCES sales(id) ON DELETE CASCADE,
    method TEXT NOT NULL,
    amount_cents INTEGER NOT NULL,
    received_cents INTEGER NOT NULL,
    reference TEXT,
    payment_index INTEGER NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_payments_sale ON payments(sale_id, payment_index);

CREATE TABLE IF NOT EXISTS invoices (
    id TEXT PRIMARY KEY,
    store_id TEXT NOT NULL REFERENCES stores(id),
    sale_id TEXT NOT NULL REFERENCES sales(id),
    customer_id TEXT NOT NULL REFERENCES customers(id),
    series TEXT NOT NULL,
    number INTEGER NOT NULL,
    issued_at TEXT NOT NULL,
    kind_json TEXT NOT NULL,
    totals_json TEXT NOT NULL,
    UNIQUE(series, number)
);

CREATE INDEX IF NOT EXISTS idx_invoices_store_issued ON invoices(store_id, issued_at);

CREATE TABLE IF NOT EXISTS invoice_lines (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    invoice_id TEXT NOT NULL REFERENCES invoices(id) ON DELETE CASCADE,
    source_line_id TEXT NOT NULL,
    description TEXT NOT NULL,
    quantity TEXT NOT NULL,
    unit_price TEXT NOT NULL,
    discount_bp INTEGER NOT NULL DEFAULT 0,
    tax_code TEXT NOT NULL,
    tax_percent_bp INTEGER NOT NULL,
    gross_cents INTEGER NOT NULL,
    tax_cents INTEGER NOT NULL,
    kind TEXT NOT NULL,
    line_index INTEGER NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_invoice_lines_invoice ON invoice_lines(invoice_id, line_index);

CREATE TABLE IF NOT EXISTS document_sequences (
    store_id TEXT NOT NULL,
    document_type TEXT NOT NULL,
    series TEXT NOT NULL,
    year INTEGER NOT NULL,
    next_number INTEGER NOT NULL,
    PRIMARY KEY (store_id, document_type, series, year)
);

CREATE TABLE IF NOT EXISTS cash_sessions (
    id TEXT PRIMARY KEY,
    store_id TEXT NOT NULL REFERENCES stores(id),
    register_id TEXT NOT NULL,
    opened_by TEXT NOT NULL,
    opened_at TEXT NOT NULL,
    opening_float_cents INTEGER NOT NULL,
    state TEXT NOT NULL,
    version INTEGER NOT NULL,
    movements_json TEXT NOT NULL,
    close_history_json TEXT NOT NULL,
    reopen_history_json TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_cash_sessions_register ON cash_sessions(register_id, opened_at);

CREATE TABLE IF NOT EXISTS sync_outbox (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    event_id TEXT NOT NULL UNIQUE,
    event_type TEXT NOT NULL,
    aggregate_type TEXT NOT NULL,
    aggregate_id TEXT NOT NULL,
    payload_json TEXT NOT NULL,
    envelope_json TEXT NOT NULL,
    created_at TEXT NOT NULL,
    sent_at TEXT
);

CREATE INDEX IF NOT EXISTS idx_sync_outbox_pending ON sync_outbox(sent_at, id);

CREATE TABLE IF NOT EXISTS audit_log (
    id TEXT PRIMARY KEY,
    occurred_at TEXT NOT NULL,
    actor_id TEXT,
    store_id TEXT,
    action TEXT NOT NULL,
    entity_type TEXT NOT NULL,
    entity_id TEXT NOT NULL,
    details_json TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_audit_store_time ON audit_log(store_id, occurred_at);
