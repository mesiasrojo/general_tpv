-- Notas internas dirigidas al personal de la tienda.
-- Cada nota tiene un autor, un contenido y uno o varios destinatarios con acuse de lectura.

CREATE TABLE IF NOT EXISTS notes (
    id TEXT PRIMARY KEY,
    store_id TEXT NOT NULL REFERENCES stores(id),
    author_id TEXT NOT NULL REFERENCES users(id),
    title TEXT NOT NULL,
    body TEXT NOT NULL,
    created_at TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_notes_store_created
    ON notes(store_id, created_at DESC);

CREATE TABLE IF NOT EXISTS note_recipients (
    note_id TEXT NOT NULL REFERENCES notes(id) ON DELETE CASCADE,
    user_id TEXT NOT NULL REFERENCES users(id),
    read_at TEXT,
    PRIMARY KEY (note_id, user_id)
);

CREATE INDEX IF NOT EXISTS idx_note_recipients_user
    ON note_recipients(user_id, read_at);
