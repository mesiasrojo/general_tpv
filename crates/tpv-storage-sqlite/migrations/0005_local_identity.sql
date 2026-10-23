CREATE TABLE IF NOT EXISTS local_identity (
    id INTEGER PRIMARY KEY CHECK (id = 1),
    tenant_id TEXT NOT NULL,
    device_id TEXT NOT NULL,
    created_at TEXT NOT NULL
);
