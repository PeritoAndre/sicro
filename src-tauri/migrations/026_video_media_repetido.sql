-- O mesmo vídeo pode ser anexado mais de uma vez ao caso (comparar um vídeo com ele mesmo):
-- sai o UNIQUE(occurrence_id, sha256). SQLite não solta constraint, então a tabela é refeita.
CREATE TABLE video_media_novo (
    id                  TEXT PRIMARY KEY,
    occurrence_id       TEXT NOT NULL REFERENCES occurrences(id) ON DELETE CASCADE,
    original_path       TEXT,
    relative_path       TEXT NOT NULL,
    filename            TEXT NOT NULL,
    sha256              TEXT NOT NULL,
    size_bytes          INTEGER NOT NULL DEFAULT 0,
    duration_s          REAL,
    codec               TEXT,
    width               INTEGER,
    height              INTEGER,
    pixel_format        TEXT,
    fps_declared        REAL,
    avg_frame_rate      TEXT,
    r_frame_rate        TEXT,
    time_base           TEXT,
    frame_count         INTEGER,
    bitrate             INTEGER,
    raw_probe_json      TEXT NOT NULL DEFAULT '{}',
    warnings_json       TEXT NOT NULL DEFAULT '[]',
    created_at          TEXT NOT NULL,
    updated_at          TEXT NOT NULL,
    derived_from_hash   TEXT,
    derivation_json     TEXT
);
INSERT INTO video_media_novo (id, occurrence_id, original_path, relative_path, filename, sha256, size_bytes, duration_s, codec, width, height, pixel_format, fps_declared, avg_frame_rate, r_frame_rate, time_base, frame_count, bitrate, raw_probe_json, warnings_json, created_at, updated_at, derived_from_hash, derivation_json)
SELECT id, occurrence_id, original_path, relative_path, filename, sha256, size_bytes, duration_s, codec, width, height, pixel_format, fps_declared, avg_frame_rate, r_frame_rate, time_base, frame_count, bitrate, raw_probe_json, warnings_json, created_at, updated_at, derived_from_hash, derivation_json FROM video_media;
DROP TABLE video_media;
ALTER TABLE video_media_novo RENAME TO video_media;
CREATE INDEX IF NOT EXISTS idx_video_media_occurrence_id ON video_media(occurrence_id);
CREATE INDEX IF NOT EXISTS idx_video_media_sha256       ON video_media(sha256);
