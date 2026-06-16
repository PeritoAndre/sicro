-- SICRO 2.0 — exports schema (migration 003).
-- Tracks every file produced by the Export Engine (Spike C).
-- The actual artefact lives in <workspace>/exports/<kind>/ — this table is
-- just the index, so the UI can list "exports of this laudo" without
-- scanning the filesystem.

CREATE TABLE IF NOT EXISTS exports (
    id             TEXT PRIMARY KEY,          -- UUID v4
    occurrence_id  TEXT NOT NULL,
    laudo_id       TEXT NOT NULL,
    kind           TEXT NOT NULL,             -- 'html' | 'pdf' | 'docx'
    relative_path  TEXT NOT NULL,             -- e.g. exports/pdf/laudo_<id>_<ts>.pdf
    file_size      INTEGER NOT NULL DEFAULT 0,
    created_at     TEXT NOT NULL,
    FOREIGN KEY (occurrence_id) REFERENCES occurrences(id) ON DELETE CASCADE,
    FOREIGN KEY (laudo_id)      REFERENCES laudos(id)      ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_exports_laudo_id   ON exports(laudo_id);
CREATE INDEX IF NOT EXISTS idx_exports_created_at ON exports(created_at);
