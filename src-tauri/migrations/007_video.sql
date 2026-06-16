-- SICRO 2.0 — Spike F / Video Engine (migration 007).
--
-- Tabelas reaproveitadas conceitualmente do laboratório Python
-- (`SICRO_VIDEO_LAB_RELATORIO.md` §4) e adaptadas ao schema do SICRO
-- Desktop:
--
--   * Todos os registros vivem dentro do workspace `.sicro` de uma
--     ocorrência (FK em `occurrences(id)`).
--   * A mídia é identificada por `sha256` (NOT NULL + UNIQUE por
--     ocorrência); o caminho local pode mudar, o hash não.
--   * Eventos / exports / storyboard se vinculam à mídia pelo hash
--     (`media_hash`), nunca por id local — coerente com a decisão
--     pericial validada no laboratório Python.
--
-- A migration é aditiva. Workspaces antigos não regridem.

-- ---------------------------------------------------------------------------
-- video_media

CREATE TABLE IF NOT EXISTS video_media (
    id                  TEXT PRIMARY KEY,          -- UUID v4
    occurrence_id       TEXT NOT NULL REFERENCES occurrences(id) ON DELETE CASCADE,
    original_path       TEXT,                       -- caminho de origem no disco do usuário
    relative_path       TEXT NOT NULL,              -- videos/originais/<filename> (relativo ao workspace)
    filename            TEXT NOT NULL,
    sha256              TEXT NOT NULL,
    size_bytes          INTEGER NOT NULL DEFAULT 0,
    duration_s          REAL,
    codec               TEXT,
    width               INTEGER,
    height              INTEGER,
    pixel_format        TEXT,
    fps_declared        REAL,
    avg_frame_rate      TEXT,                       -- "30000/1001" — mantemos como string para fidelidade técnica
    r_frame_rate        TEXT,
    time_base           TEXT,
    frame_count         INTEGER,
    bitrate             INTEGER,
    raw_probe_json      TEXT NOT NULL DEFAULT '{}', -- ffprobe -show_format -show_streams -of json verbatim
    warnings_json       TEXT NOT NULL DEFAULT '[]', -- alertas técnicos (VFR, sem frame_count, etc.)
    created_at          TEXT NOT NULL,
    updated_at          TEXT NOT NULL,
    UNIQUE(occurrence_id, sha256)
);

CREATE INDEX IF NOT EXISTS idx_video_media_occurrence_id ON video_media(occurrence_id);
CREATE INDEX IF NOT EXISTS idx_video_media_sha256       ON video_media(sha256);

-- ---------------------------------------------------------------------------
-- video_events

CREATE TABLE IF NOT EXISTS video_events (
    id              TEXT PRIMARY KEY,               -- UUID v4
    occurrence_id   TEXT NOT NULL REFERENCES occurrences(id) ON DELETE CASCADE,
    media_hash      TEXT NOT NULL,                  -- aponta para video_media.sha256 (lógico, não FK rígido)
    timestamp_s     REAL NOT NULL,
    timestamp_label TEXT NOT NULL,                  -- HH:MM:SS.mmm formatado pelo backend
    frame_observed  INTEGER,                        -- índice de frame quando confiável
    pts             INTEGER,
    time_base       TEXT,
    category        TEXT NOT NULL,                  -- colisao | frenagem | impacto | reacao | semaforo | mudanca_faixa | outro
    title           TEXT NOT NULL,
    description     TEXT NOT NULL DEFAULT '',
    reviewed        INTEGER NOT NULL DEFAULT 0,
    source          TEXT NOT NULL DEFAULT 'manual', -- 'manual' | 'imported' | ...
    created_at      TEXT NOT NULL,
    updated_at      TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_video_events_occurrence_id          ON video_events(occurrence_id);
CREATE INDEX IF NOT EXISTS idx_video_events_media_hash_timestamp   ON video_events(media_hash, timestamp_s);

-- ---------------------------------------------------------------------------
-- video_exports
-- Track every PNG/JSON pair produced by the frame collector.

CREATE TABLE IF NOT EXISTS video_exports (
    id                       TEXT PRIMARY KEY,       -- UUID v4
    occurrence_id            TEXT NOT NULL REFERENCES occurrences(id) ON DELETE CASCADE,
    media_hash               TEXT NOT NULL,
    event_id                 TEXT REFERENCES video_events(id) ON DELETE SET NULL,
    type                     TEXT NOT NULL,          -- 'frame_png'
    requested_timestamp_s    REAL NOT NULL,
    actual_timestamp_s       REAL,
    delta_s                  REAL,
    output_path              TEXT NOT NULL,           -- videos/storyboards/frames/<filename>.png (workspace-relative)
    filename                 TEXT NOT NULL,
    sidecar_json_path        TEXT,
    details_json             TEXT NOT NULL DEFAULT '{}',
    created_at               TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_video_exports_occurrence_id ON video_exports(occurrence_id);
CREATE INDEX IF NOT EXISTS idx_video_exports_media_hash    ON video_exports(media_hash);

-- ---------------------------------------------------------------------------
-- video_storyboard_frames

CREATE TABLE IF NOT EXISTS video_storyboard_frames (
    id                          TEXT PRIMARY KEY,    -- UUID v4
    occurrence_id               TEXT NOT NULL REFERENCES occurrences(id) ON DELETE CASCADE,
    media_hash                  TEXT NOT NULL,
    event_id                    TEXT REFERENCES video_events(id) ON DELETE SET NULL,
    export_id                   TEXT REFERENCES video_exports(id) ON DELETE SET NULL,
    title                       TEXT NOT NULL,
    caption                     TEXT NOT NULL DEFAULT '',
    notes                       TEXT NOT NULL DEFAULT '',
    requested_timestamp_s       REAL NOT NULL,
    actual_timestamp_s          REAL,
    delta_s                     REAL,
    observed_frame_index        INTEGER,
    estimated_total_frames      INTEGER,
    frame_index_is_estimated    INTEGER NOT NULL DEFAULT 1,
    pts                         INTEGER,
    time_base                   TEXT,
    output_path                 TEXT NOT NULL,
    sidecar_json_path           TEXT,
    reviewed                    INTEGER NOT NULL DEFAULT 0,
    created_at                  TEXT NOT NULL,
    updated_at                  TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_video_storyboard_occurrence_id ON video_storyboard_frames(occurrence_id);
CREATE INDEX IF NOT EXISTS idx_video_storyboard_media_hash    ON video_storyboard_frames(media_hash);

-- ---------------------------------------------------------------------------
-- video_operation_logs

CREATE TABLE IF NOT EXISTS video_operation_logs (
    id              INTEGER PRIMARY KEY AUTOINCREMENT,
    occurrence_id   TEXT NOT NULL REFERENCES occurrences(id) ON DELETE CASCADE,
    media_hash      TEXT,
    action          TEXT NOT NULL,                  -- media.register | event.create | event.update | event.delete | frame.collect | storyboard_frame.delete
    details_json    TEXT NOT NULL DEFAULT '{}',
    created_at      TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_video_operation_logs_occurrence_id ON video_operation_logs(occurrence_id);
CREATE INDEX IF NOT EXISTS idx_video_operation_logs_media_hash    ON video_operation_logs(media_hash);
