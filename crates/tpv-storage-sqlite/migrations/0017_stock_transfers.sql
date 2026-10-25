-- Documents the two stock movements generated when inventory is moved
-- between stores. Quantities are stored as decimal text like the stock ledger.
CREATE TABLE IF NOT EXISTS stock_transfers (
    id TEXT PRIMARY KEY,
    from_store_id TEXT NOT NULL REFERENCES stores(id) ON DELETE CASCADE,
    to_store_id TEXT NOT NULL REFERENCES stores(id) ON DELETE CASCADE,
    product_id TEXT NOT NULL REFERENCES products(id) ON DELETE CASCADE,
    quantity TEXT NOT NULL,
    reason TEXT,
    out_movement_id TEXT NOT NULL REFERENCES stock_movements(id),
    in_movement_id TEXT NOT NULL REFERENCES stock_movements(id),
    occurred_at TEXT NOT NULL,
    CHECK (from_store_id <> to_store_id)
);

CREATE INDEX IF NOT EXISTS idx_stock_transfers_from
    ON stock_transfers(from_store_id, occurred_at DESC);

CREATE INDEX IF NOT EXISTS idx_stock_transfers_to
    ON stock_transfers(to_store_id, occurred_at DESC);
