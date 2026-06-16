-- SICRO 2.0 — Spike E / Croqui Engine (migration 006).
--
-- Same shape as Spike B's `laudos`: each croqui is a row pointing at a
-- `.sicrocroqui` file on disk (JSON envelope). The PNG export is tracked
-- via the existing `exports` table — no new export kind needed at the
-- schema level (kind = 'png' would be added later if necessary).

CREATE TABLE IF NOT EXISTS croquis (
    id              TEXT PRIMARY KEY,             -- UUID v4
    occurrence_id   TEXT NOT NULL REFERENCES occurrences(id) ON DELETE CASCADE,
    title           TEXT NOT NULL,
    relative_path   TEXT NOT NULL,                 -- croquis/croqui_<id>.sicrocroqui
    status          TEXT NOT NULL DEFAULT 'draft', -- 'draft' | 'ready'
    schema_version  TEXT NOT NULL DEFAULT '0.1',
    last_export_relative_path TEXT,                -- croquis/exports/croqui_<id>_<ts>.png
    created_at      TEXT NOT NULL,
    updated_at      TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_croquis_occurrence_id ON croquis(occurrence_id);
CREATE INDEX IF NOT EXISTS idx_croquis_updated_at   ON croquis(updated_at);
