-- Fichaje local del personal. Cada entrada pertenece a un usuario y una tienda.
-- Solo puede existir un fichaje abierto por usuario y tienda.

CREATE TABLE IF NOT EXISTS time_entries (
    id TEXT PRIMARY KEY,
    user_id TEXT NOT NULL REFERENCES users(id),
    store_id TEXT NOT NULL REFERENCES stores(id),
    clock_in TEXT NOT NULL,
    clock_out TEXT,
    correction_reason TEXT
);

CREATE UNIQUE INDEX IF NOT EXISTS idx_time_entries_open
    ON time_entries(user_id, store_id)
    WHERE clock_out IS NULL;

CREATE INDEX IF NOT EXISTS idx_time_entries_store_in
    ON time_entries(store_id, clock_in DESC);

CREATE INDEX IF NOT EXISTS idx_time_entries_user_in
    ON time_entries(user_id, clock_in DESC);
