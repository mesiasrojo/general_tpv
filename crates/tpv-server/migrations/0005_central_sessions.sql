CREATE TABLE IF NOT EXISTS central_sessions (
    token_hash TEXT PRIMARY KEY,
    user_id UUID NOT NULL,
    tenant_id UUID NOT NULL,
    username TEXT NOT NULL,
    display_name TEXT NOT NULL,
    role TEXT NOT NULL,
    expires_at TIMESTAMPTZ NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_central_sessions_expiry
    ON central_sessions (expires_at);
