CREATE TABLE IF NOT EXISTS central_users (
    id UUID PRIMARY KEY,
    tenant_id UUID NOT NULL,
    username TEXT NOT NULL,
    display_name TEXT NOT NULL,
    role TEXT NOT NULL,
    store_ids JSONB NOT NULL DEFAULT '[]'::jsonb,
    password_hash TEXT NOT NULL,
    active BOOLEAN NOT NULL DEFAULT TRUE,
    version BIGINT NOT NULL DEFAULT 1,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE (tenant_id, username)
);

CREATE INDEX IF NOT EXISTS idx_central_users_tenant
    ON central_users (tenant_id, active, username);
