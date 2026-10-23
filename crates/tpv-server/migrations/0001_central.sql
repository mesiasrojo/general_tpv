CREATE TABLE IF NOT EXISTS central_events (
    seq BIGSERIAL PRIMARY KEY,
    event_id UUID NOT NULL UNIQUE,
    tenant_id UUID NOT NULL,
    store_id UUID NOT NULL,
    device_id UUID NOT NULL,
    device_seq BIGINT NOT NULL,
    aggregate_type TEXT NOT NULL,
    aggregate_id UUID NOT NULL,
    event_type TEXT NOT NULL,
    schema_version INTEGER NOT NULL,
    occurred_at TIMESTAMPTZ NOT NULL,
    payload JSONB NOT NULL,
    envelope JSONB NOT NULL,
    hash TEXT NOT NULL,
    received_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_central_events_store_seq
    ON central_events(store_id, seq);

CREATE INDEX IF NOT EXISTS idx_central_events_type_seq
    ON central_events(aggregate_type, seq);

CREATE INDEX IF NOT EXISTS idx_central_events_device
    ON central_events(device_id, device_seq);
