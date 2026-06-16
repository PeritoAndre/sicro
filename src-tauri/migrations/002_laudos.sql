-- SICRO 2.0 — laudos schema (migration 002).
-- Adds the laudos table required by Spike B (Document Engine).
-- Each laudo row points to a .sicrodoc file inside laudos/ in the workspace.

CREATE TABLE IF NOT EXISTS laudos (
    id                TEXT PRIMARY KEY,                  -- UUID v4
    occurrence_id     TEXT NOT NULL,
    title             TEXT NOT NULL,
    template_id       TEXT NOT NULL DEFAULT 'documento_livre',
    relative_path     TEXT NOT NULL,                     -- e.g. laudos/laudo_<id>.sicrodoc
    status            TEXT NOT NULL DEFAULT 'rascunho',  -- rascunho|revisado|exportado|assinado|arquivado
    created_at        TEXT NOT NULL,
    updated_at        TEXT NOT NULL,
    last_export_pdf   TEXT,
    last_export_docx  TEXT,
    FOREIGN KEY (occurrence_id) REFERENCES occurrences(id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_laudos_occurrence_id ON laudos(occurrence_id);
CREATE INDEX IF NOT EXISTS idx_laudos_updated_at   ON laudos(updated_at);
