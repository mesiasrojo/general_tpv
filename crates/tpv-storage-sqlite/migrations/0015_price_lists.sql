-- Tarifas de precios con overrides explícitos por artículo. El precio de la
-- tarifa es IVA incluido, igual que el precio base del producto. Los artículos
-- sin override conservan su precio de catálogo.

CREATE TABLE IF NOT EXISTS price_lists (
    id TEXT PRIMARY KEY,
    code TEXT NOT NULL UNIQUE,
    name TEXT NOT NULL,
    active INTEGER NOT NULL DEFAULT 1
);

CREATE INDEX IF NOT EXISTS idx_price_lists_name
    ON price_lists(name);

CREATE TABLE IF NOT EXISTS price_list_items (
    price_list_id TEXT NOT NULL REFERENCES price_lists(id) ON DELETE CASCADE,
    product_id TEXT NOT NULL REFERENCES products(id) ON DELETE CASCADE,
    unit_price TEXT NOT NULL,
    PRIMARY KEY (price_list_id, product_id)
);

CREATE INDEX IF NOT EXISTS idx_price_list_items_product
    ON price_list_items(product_id);

-- La tarifa se elige al abrir la venta y queda congelada para las líneas que
-- se añadan después. Las líneas ya guardadas mantienen su precio.
ALTER TABLE sales ADD COLUMN price_list_id TEXT;
