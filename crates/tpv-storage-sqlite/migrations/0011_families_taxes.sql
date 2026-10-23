-- Maestros editables de familias e impuestos.
-- Los productos siguen guardando el código de familia e IVA para conservar
-- el histórico aunque un maestro se desactive.

CREATE TABLE IF NOT EXISTS families (
    id TEXT PRIMARY KEY,
    code TEXT NOT NULL UNIQUE,
    name TEXT NOT NULL,
    active INTEGER NOT NULL DEFAULT 1
);

CREATE INDEX IF NOT EXISTS idx_families_name ON families(name);

CREATE TABLE IF NOT EXISTS taxes (
    id TEXT PRIMARY KEY,
    code TEXT NOT NULL UNIQUE,
    name TEXT NOT NULL,
    percent_bp INTEGER NOT NULL,
    active INTEGER NOT NULL DEFAULT 1
);

CREATE INDEX IF NOT EXISTS idx_taxes_name ON taxes(name);
