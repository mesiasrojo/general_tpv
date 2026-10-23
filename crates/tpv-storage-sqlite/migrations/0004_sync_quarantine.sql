ALTER TABLE sync_outbox ADD COLUMN failed_at TEXT;
ALTER TABLE sync_outbox ADD COLUMN last_error TEXT;
ALTER TABLE sync_outbox ADD COLUMN attempts INTEGER NOT NULL DEFAULT 0;

CREATE INDEX IF NOT EXISTS idx_sync_outbox_quarantine
    ON sync_outbox(failed_at, created_at);
