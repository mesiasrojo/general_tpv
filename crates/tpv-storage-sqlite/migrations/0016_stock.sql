-- Existencias por tienda y artículo. La cantidad se guarda como texto decimal
-- para no perder precisión en artículos vendidos por peso. Los movimientos son
-- el libro de auditoría: stock_levels siempre se actualiza en la misma
-- transacción que el movimiento que lo origina.

CREATE TABLE IF NOT EXISTS stock_levels (
    store_id TEXT NOT NULL REFERENCES stores(id) ON DELETE CASCADE,
    product_id TEXT NOT NULL REFERENCES products(id) ON DELETE CASCADE,
    quantity TEXT NOT NULL DEFAULT '0',
    min_quantity TEXT NOT NULL DEFAULT '0',
    updated_at TEXT NOT NULL,
    PRIMARY KEY (store_id, product_id)
);

CREATE INDEX IF NOT EXISTS idx_stock_levels_store_quantity
    ON stock_levels(store_id, quantity);

CREATE TABLE IF NOT EXISTS stock_movements (
    id TEXT PRIMARY KEY,
    store_id TEXT NOT NULL REFERENCES stores(id) ON DELETE CASCADE,
    product_id TEXT NOT NULL REFERENCES products(id) ON DELETE CASCADE,
    kind TEXT NOT NULL,
    quantity TEXT NOT NULL,
    reason TEXT,
    sale_id TEXT REFERENCES sales(id),
    sale_line_id TEXT REFERENCES sale_lines(id),
    occurred_at TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_stock_movements_product_date
    ON stock_movements(store_id, product_id, occurred_at DESC);

-- Una venta no puede descontar dos veces la misma línea aunque se reintente
-- el guardado. SQLite permite múltiples NULL, que es lo que usan los
-- movimientos manuales.
CREATE UNIQUE INDEX IF NOT EXISTS idx_stock_movements_sale_line
    ON stock_movements(sale_line_id)
    WHERE sale_line_id IS NOT NULL;
