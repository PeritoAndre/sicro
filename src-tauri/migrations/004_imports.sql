-- SICRO 2.0 — Spike D / Importador .sicroapp (migration 004).
--
-- Tables introduced by the .sicroapp importer. They live inside each
-- workspace's SQLite (alongside `occurrences`, `laudos`, `exports`).
--
-- Resolution order:
--   1. `imports`        — one row per .sicroapp brought into the workspace.
--   2. `occurrences`    — the row created from the package (Spike A already
--                         had this table; this migration only adds
--                         additive columns to track the package origin).
--   3. `media_assets`   — per-file index of binaries extracted to media/.
--   4. `evidence_items` — domain-level evidence (photo, etc.) pointing
--                         at one media asset; reused in future modules.
--
-- The migration is intentionally additive: it only adds tables/columns,
-- never renames or drops. Spikes A/B/C continue to work unchanged.

-- ---------------------------------------------------------------------------
-- imports

CREATE TABLE IF NOT EXISTS imports (
    id                     TEXT PRIMARY KEY,        -- UUID v4, Desktop side
    package_relative_path  TEXT NOT NULL,           -- imports/<id>/original_package.sicroapp
    original_filename      TEXT,                    -- as picked from disk
    package_sha256         TEXT NOT NULL,           -- SHA-256 of the whole .sicroapp on disk
    format                 TEXT NOT NULL,           -- 'sicroapp' | 'sicrocampo'
    schema_version         TEXT NOT NULL,           -- raw `manifest.versao` value
    app_name               TEXT,                    -- when future manifests provide it
    app_version            TEXT,
    mobile_occurrence_id   TEXT,                    -- manifest.ocorrencia.id
    status                 TEXT NOT NULL,           -- 'imported' | 'imported_with_warnings' | 'failed'
    warnings_json          TEXT NOT NULL DEFAULT '[]',
    errors_json            TEXT NOT NULL DEFAULT '[]',
    raw_manifest_json      TEXT NOT NULL,
    imported_at            TEXT NOT NULL
);

CREATE UNIQUE INDEX IF NOT EXISTS idx_imports_package_sha256
    ON imports(package_sha256);
CREATE INDEX IF NOT EXISTS idx_imports_mobile_occurrence_id
    ON imports(mobile_occurrence_id);
CREATE INDEX IF NOT EXISTS idx_imports_imported_at
    ON imports(imported_at);

-- ---------------------------------------------------------------------------
-- occurrences — additive columns for imported packages.
-- Older Spike A workspaces had no notion of provenance; importing a .sicroapp
-- creates a row with these populated, hand-created rows leave them NULL.

ALTER TABLE occurrences ADD COLUMN import_id            TEXT REFERENCES imports(id);
ALTER TABLE occurrences ADD COLUMN original_mobile_id   TEXT;
ALTER TABLE occurrences ADD COLUMN primary_accuracy_m   REAL;
ALTER TABLE occurrences ADD COLUMN resultado            TEXT;
ALTER TABLE occurrences ADD COLUMN raw_case_json        TEXT;
ALTER TABLE occurrences ADD COLUMN raw_metadata_json    TEXT;
ALTER TABLE occurrences ADD COLUMN raw_location_json    TEXT;

CREATE INDEX IF NOT EXISTS idx_occurrences_import_id
    ON occurrences(import_id);

-- ---------------------------------------------------------------------------
-- media_assets

CREATE TABLE IF NOT EXISTS media_assets (
    id                     TEXT PRIMARY KEY,        -- UUID v4, Desktop side
    import_id              TEXT NOT NULL REFERENCES imports(id) ON DELETE CASCADE,
    occurrence_id          TEXT NOT NULL REFERENCES occurrences(id) ON DELETE CASCADE,
    original_id            TEXT,                    -- foto_<microseconds> from mobile
    type                   TEXT NOT NULL,           -- 'photo' | future: 'video' | 'audio' | 'attachment'
    relative_path          TEXT NOT NULL,           -- media/photos/<filename> inside workspace
    original_package_path  TEXT,                    -- e.g. 'fotos/foto_123.jpg' inside the .sicroapp
    original_filename      TEXT,
    mime_type              TEXT,
    size_bytes             INTEGER NOT NULL DEFAULT 0,
    sha256                 TEXT,
    captured_at            TEXT,
    imported_at            TEXT NOT NULL,
    category               TEXT,
    caption                TEXT,
    raw_json               TEXT NOT NULL DEFAULT '{}'
);

CREATE INDEX IF NOT EXISTS idx_media_assets_occurrence_id
    ON media_assets(occurrence_id);
CREATE INDEX IF NOT EXISTS idx_media_assets_import_id
    ON media_assets(import_id);
CREATE INDEX IF NOT EXISTS idx_media_assets_original_id
    ON media_assets(original_id);

-- ---------------------------------------------------------------------------
-- evidence_items

CREATE TABLE IF NOT EXISTS evidence_items (
    id                 TEXT PRIMARY KEY,             -- UUID v4
    occurrence_id      TEXT NOT NULL REFERENCES occurrences(id) ON DELETE CASCADE,
    media_asset_id     TEXT REFERENCES media_assets(id) ON DELETE SET NULL,
    type               TEXT NOT NULL,                -- 'photo' (Spike D); future: 'trace', 'measurement', ...
    title              TEXT,
    description        TEXT,
    source_module      TEXT,                         -- 'photos' | 'traces' | ...
    captured_at        TEXT,
    metadata_json      TEXT NOT NULL DEFAULT '{}',
    created_at         TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_evidence_items_occurrence_id
    ON evidence_items(occurrence_id);
CREATE INDEX IF NOT EXISTS idx_evidence_items_media_asset_id
    ON evidence_items(media_asset_id);
CREATE INDEX IF NOT EXISTS idx_evidence_items_type
    ON evidence_items(type);
