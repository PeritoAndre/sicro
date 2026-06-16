-- SICRO 2.0 — Realce de áudio (migration 014).
--
-- Registro de cada realce (auxílio de escuta) aplicado a um áudio. O realce é
-- NÃO-DESTRUTIVO: gera um novo `audio_media` (kind='realce'); aqui guardamos o
-- vínculo origem→saída e a cadeia EXATA de filtros FFmpeg (reproduzível).
-- Aditiva.

CREATE TABLE IF NOT EXISTS audio_enhancements (
    id                  TEXT PRIMARY KEY,             -- UUID v4
    occurrence_id       TEXT NOT NULL REFERENCES occurrences(id) ON DELETE CASCADE,
    source_audio_sha256 TEXT NOT NULL,                -- WAV de análise de origem
    output_audio_sha256 TEXT NOT NULL,                -- WAV realçado gerado
    filters_json        TEXT NOT NULL DEFAULT '{}',   -- {keys:[...], chain:"afftdn,highpass=..."}
    created_at          TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_audio_enh_occurrence ON audio_enhancements(occurrence_id);
