CREATE TABLE IF NOT EXISTS master_data_state (
    tenant_id TEXT NOT NULL,
    store_id TEXT NOT NULL,
    kind TEXT NOT NULL,
    entity_id TEXT NOT NULL,
    version INTEGER NOT NULL,
    change_json TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    PRIMARY KEY (tenant_id, store_id, kind, entity_id)
);

CREATE TABLE IF NOT EXISTS master_data_conflicts (
    id TEXT PRIMARY KEY,
    tenant_id TEXT NOT NULL,
    store_id TEXT NOT NULL,
    kind TEXT NOT NULL,
    entity_id TEXT NOT NULL,
    local_version INTEGER NOT NULL,
    incoming_json TEXT NOT NULL,
    reason TEXT NOT NULL,
    outcome TEXT NOT NULL,
    detected_at TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_master_data_conflicts_time
    ON master_data_conflicts(detected_at DESC);
