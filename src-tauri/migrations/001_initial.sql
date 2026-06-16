-- SICRO 2.0 — initial schema (migration 001).
-- This file is read by src/database/migrations.rs at app start.
-- Keep it idempotent (CREATE IF NOT EXISTS) so re-runs are safe.

CREATE TABLE IF NOT EXISTS schema_migrations (
    version    TEXT PRIMARY KEY,
    applied_at TEXT NOT NULL
);

-- The occurrence table is the single row that anchors the whole workspace.
-- In Spike A every workspace has exactly one occurrence; nothing in the schema
-- enforces that yet because future work may need to support occurrence merges
-- or revisions.
CREATE TABLE IF NOT EXISTS occurrences (
    id                 TEXT PRIMARY KEY,        -- UUID v4
    numero_bo          TEXT,
    protocolo          TEXT,
    requisicao         TEXT,
    oficio             TEXT,
    delegacia          TEXT,
    tipo_pericia       TEXT,
    natureza           TEXT,
    municipio          TEXT,
    bairro             TEXT,
    logradouro         TEXT,
    referencia         TEXT,
    latitude           REAL,
    longitude          REAL,
    data_fato          TEXT,
    data_acionamento   TEXT,
    data_chegada       TEXT,
    data_encerramento  TEXT,
    peritos            TEXT NOT NULL DEFAULT '[]', -- JSON array of strings
    status             TEXT NOT NULL DEFAULT 'aberta',
    created_at         TEXT NOT NULL,
    updated_at         TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_occurrences_created_at
    ON occurrences(created_at);
CREATE INDEX IF NOT EXISTS idx_occurrences_numero_bo
    ON occurrences(numero_bo);

-- Audit log is intentionally created in Spike A even though only a few rows
-- will be written. The schema is the contract; future modules just plug in.
CREATE TABLE IF NOT EXISTS audit_logs (
    id            INTEGER PRIMARY KEY AUTOINCREMENT,
    occurrence_id TEXT,
    action        TEXT NOT NULL,
    module        TEXT,
    entity_type   TEXT,
    entity_id     TEXT,
    details_json  TEXT,
    created_at    TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_audit_logs_occurrence_id
    ON audit_logs(occurrence_id);
CREATE INDEX IF NOT EXISTS idx_audit_logs_created_at
    ON audit_logs(created_at);
