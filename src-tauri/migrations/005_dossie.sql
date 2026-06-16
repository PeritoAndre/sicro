-- SICRO 2.0 — MVP 3 / Dossiê Operacional (migration 005).
--
-- Structured tables for the .sicroapp JSONs that the Spike D importer
-- read but didn't persist beyond `raw_json` blobs. Each row carries:
--
--   - `occurrence_id` (FK) so the Dossiê can list everything per ocorrência;
--   - `import_id`     (FK) for provenance + rehydration;
--   - `original_id`   (string) so we can re-find the mobile row;
--   - structured columns for the fields the perito actually needs to *see*;
--   - `raw_json` with the verbatim mobile payload (forward-compat + audit).
--
-- All tables are additive. Workspaces created before this migration get
-- empty tables until either (a) a fresh import populates them or (b) the
-- `rehydrate_workspace` command re-extracts from the staged package.

-- ---------------------------------------------------------------------------
-- checklist_items
-- Source: `checklist.json` (mobile contract v0.6 — list of objects).

CREATE TABLE IF NOT EXISTS checklist_items (
    id              TEXT PRIMARY KEY,
    occurrence_id   TEXT NOT NULL REFERENCES occurrences(id) ON DELETE CASCADE,
    import_id       TEXT NOT NULL REFERENCES imports(id)     ON DELETE CASCADE,
    original_id     TEXT,
    category        TEXT,
    question        TEXT NOT NULL,
    required        INTEGER NOT NULL DEFAULT 0,
    answer          TEXT NOT NULL DEFAULT 'nao_verificado',
    note            TEXT,
    default_note    TEXT,
    origin          TEXT NOT NULL DEFAULT 'base',
    sort_order      INTEGER NOT NULL DEFAULT 0,
    raw_json        TEXT NOT NULL DEFAULT '{}',
    created_at      TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_checklist_items_occurrence_id
    ON checklist_items(occurrence_id);
CREATE INDEX IF NOT EXISTS idx_checklist_items_required_answer
    ON checklist_items(required, answer);

-- ---------------------------------------------------------------------------
-- entities  (polymorphic: vehicle | victim — future: body, person, suspect, ...)
-- Source: `veiculos.json` + `vitimas.json`.

CREATE TABLE IF NOT EXISTS entities (
    id              TEXT PRIMARY KEY,
    occurrence_id   TEXT NOT NULL REFERENCES occurrences(id) ON DELETE CASCADE,
    import_id       TEXT NOT NULL REFERENCES imports(id)     ON DELETE CASCADE,
    original_id     TEXT,
    type            TEXT NOT NULL,             -- 'vehicle' | 'victim'
    identifier      TEXT,                       -- e.g. 'V1', 'P1'
    label           TEXT,                       -- short summary line
    summary         TEXT,                       -- one-paragraph human description
    photo_ids_json  TEXT NOT NULL DEFAULT '[]', -- JSON array of original photo IDs
    raw_json        TEXT NOT NULL DEFAULT '{}',
    sort_order      INTEGER NOT NULL DEFAULT 0,
    created_at      TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_entities_occurrence_id
    ON entities(occurrence_id);
CREATE INDEX IF NOT EXISTS idx_entities_type
    ON entities(type);

-- ---------------------------------------------------------------------------
-- traces  (vestígios).  Source: `vestigios.json`.

CREATE TABLE IF NOT EXISTS traces (
    id                      TEXT PRIMARY KEY,
    occurrence_id           TEXT NOT NULL REFERENCES occurrences(id) ON DELETE CASCADE,
    import_id               TEXT NOT NULL REFERENCES imports(id)     ON DELETE CASCADE,
    original_id             TEXT,
    identifier              TEXT,                       -- 'E1', 'E2', ...
    type                    TEXT,                       -- 'frenagem' | 'sangue' | ...
    description             TEXT,
    location_description    TEXT,
    length                  REAL,
    width                   REAL,
    unit                    TEXT,
    direction               TEXT,
    note                    TEXT,
    photo_ids_json          TEXT NOT NULL DEFAULT '[]',
    sketch_element_ids_json TEXT NOT NULL DEFAULT '[]',
    raw_json                TEXT NOT NULL DEFAULT '{}',
    sort_order              INTEGER NOT NULL DEFAULT 0,
    created_at              TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_traces_occurrence_id
    ON traces(occurrence_id);

-- ---------------------------------------------------------------------------
-- measurements  (medições).  Source: `medicoes.json`.

CREATE TABLE IF NOT EXISTS measurements (
    id                      TEXT PRIMARY KEY,
    occurrence_id           TEXT NOT NULL REFERENCES occurrences(id) ON DELETE CASCADE,
    import_id               TEXT NOT NULL REFERENCES imports(id)     ON DELETE CASCADE,
    original_id             TEXT,
    label                   TEXT,
    point_a                 TEXT,
    point_b                 TEXT,
    value                   REAL,
    unit                    TEXT,
    method                  TEXT,
    note                    TEXT,
    photo_ids_json          TEXT NOT NULL DEFAULT '[]',
    sketch_element_ids_json TEXT NOT NULL DEFAULT '[]',
    raw_json                TEXT NOT NULL DEFAULT '{}',
    sort_order              INTEGER NOT NULL DEFAULT 0,
    created_at              TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_measurements_occurrence_id
    ON measurements(occurrence_id);

-- ---------------------------------------------------------------------------
-- field_notes  (observações).  Source: `observacoes.json`.

CREATE TABLE IF NOT EXISTS field_notes (
    id               TEXT PRIMARY KEY,
    occurrence_id    TEXT NOT NULL REFERENCES occurrences(id) ON DELETE CASCADE,
    import_id        TEXT NOT NULL REFERENCES imports(id)     ON DELETE CASCADE,
    original_id      TEXT,
    text             TEXT,
    category         TEXT,
    priority         TEXT,
    note_created_at  TEXT,
    note_updated_at  TEXT,
    raw_json         TEXT NOT NULL DEFAULT '{}',
    sort_order       INTEGER NOT NULL DEFAULT 0,
    created_at       TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_field_notes_occurrence_id
    ON field_notes(occurrence_id);

-- ---------------------------------------------------------------------------
-- timeline_events.  Source: `timeline.json`.

CREATE TABLE IF NOT EXISTS timeline_events (
    id              TEXT PRIMARY KEY,
    occurrence_id   TEXT NOT NULL REFERENCES occurrences(id) ON DELETE CASCADE,
    import_id       TEXT NOT NULL REFERENCES imports(id)     ON DELETE CASCADE,
    original_id     TEXT,
    type            TEXT,
    title           TEXT,
    description     TEXT,
    occurred_at     TEXT,
    raw_json        TEXT NOT NULL DEFAULT '{}',
    sort_order      INTEGER NOT NULL DEFAULT 0,
    created_at      TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_timeline_events_occurrence_id
    ON timeline_events(occurrence_id);
CREATE INDEX IF NOT EXISTS idx_timeline_events_occurred_at
    ON timeline_events(occurred_at);

-- ---------------------------------------------------------------------------
-- occurrence_stats.  Source: `estatisticas.json` (snapshot at export time).
-- One row per (occurrence, import) — re-importing overwrites via DELETE+INSERT
-- in the rehydrator.

CREATE TABLE IF NOT EXISTS occurrence_stats (
    id                              TEXT PRIMARY KEY,
    occurrence_id                   TEXT NOT NULL REFERENCES occurrences(id) ON DELETE CASCADE,
    import_id                       TEXT NOT NULL REFERENCES imports(id)     ON DELETE CASCADE,
    duration_seconds                INTEGER,
    photos_count                    INTEGER,
    victims_count                   INTEGER,
    vehicles_count                  INTEGER,
    traces_count                    INTEGER,
    measurements_count              INTEGER,
    notes_count                     INTEGER,
    checklist_items_count           INTEGER,
    answered_checklist_items_count  INTEGER,
    not_applicable_items_count      INTEGER,
    best_gps_accuracy_m             REAL,
    gps_readings_count              INTEGER,
    raw_json                        TEXT NOT NULL DEFAULT '{}',
    created_at                      TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_occurrence_stats_occurrence_id
    ON occurrence_stats(occurrence_id);
