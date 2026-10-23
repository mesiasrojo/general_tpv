CREATE TABLE IF NOT EXISTS central_master_data (
    tenant_id UUID NOT NULL,
    store_id UUID NOT NULL,
    kind TEXT NOT NULL,
    entity_id UUID NOT NULL,
    version BIGINT NOT NULL,
    change JSONB NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    PRIMARY KEY (tenant_id, store_id, kind, entity_id)
);

CREATE TABLE IF NOT EXISTS central_master_conflicts (
    id UUID PRIMARY KEY,
    tenant_id UUID NOT NULL,
    store_id UUID NOT NULL,
    kind TEXT NOT NULL,
    entity_id UUID NOT NULL,
    local_version BIGINT NOT NULL,
    incoming JSONB NOT NULL,
    reason TEXT NOT NULL,
    outcome TEXT NOT NULL,
    detected_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_central_master_conflicts_time
    ON central_master_conflicts(detected_at DESC);
