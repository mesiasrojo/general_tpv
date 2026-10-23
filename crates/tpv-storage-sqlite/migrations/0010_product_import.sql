-- Importación de artículos desde sistemas anteriores.
-- `product_import_batches` hace que reintentar un fichero ya confirmado sea
-- idempotente. `product_import_sources` conserva la correspondencia entre el
-- identificador del sistema de origen y el producto local.

CREATE TABLE product_import_batches (
    id TEXT PRIMARY KEY,
    source_system TEXT NOT NULL,
    batch_id TEXT NOT NULL,
    total_rows INTEGER NOT NULL,
    inserted INTEGER NOT NULL,
    updated INTEGER NOT NULL,
    unchanged INTEGER NOT NULL,
    committed_at TEXT NOT NULL,
    UNIQUE (source_system, batch_id)
);

CREATE TABLE product_import_sources (
    source_system TEXT NOT NULL,
    source_id TEXT NOT NULL,
    product_id TEXT NOT NULL REFERENCES products(id) ON DELETE CASCADE,
    updated_at TEXT NOT NULL,
    PRIMARY KEY (source_system, source_id)
);

CREATE INDEX idx_product_import_sources_product
    ON product_import_sources(product_id);
