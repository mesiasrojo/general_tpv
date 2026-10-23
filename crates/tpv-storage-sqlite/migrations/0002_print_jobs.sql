CREATE TABLE IF NOT EXISTS print_jobs (
    id TEXT PRIMARY KEY,
    sale_id TEXT NOT NULL,
    job_type TEXT NOT NULL,
    document_json TEXT NOT NULL,
    bytes BLOB NOT NULL,
    status TEXT NOT NULL,
    attempts INTEGER NOT NULL DEFAULT 0,
    created_at TEXT NOT NULL,
    printed_at TEXT,
    last_error TEXT
);

CREATE INDEX IF NOT EXISTS idx_print_jobs_status ON print_jobs(status, created_at);
CREATE INDEX IF NOT EXISTS idx_print_jobs_sale ON print_jobs(sale_id, created_at);
