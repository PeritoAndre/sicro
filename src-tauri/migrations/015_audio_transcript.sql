-- SICRO 2.0 — Degravação assistida MANUAL (migration 015).
--
-- Segmentos de transcrição feitos PELO PERITO (timestamp + locutor + texto). O
-- tool NÃO transcreve nem interpreta nada: é um espaço de trabalho sincronizado
-- com o player. Modelo "replace-all" por áudio (o salvar substitui o conjunto).
-- Aditiva.

CREATE TABLE IF NOT EXISTS audio_transcript_segments (
    id            TEXT PRIMARY KEY,             -- UUID v4
    occurrence_id TEXT NOT NULL REFERENCES occurrences(id) ON DELETE CASCADE,
    audio_sha256  TEXT NOT NULL,                -- WAV de análise ao qual pertence
    idx           INTEGER NOT NULL,             -- ordem do segmento (0..n)
    t_start       REAL NOT NULL,                -- início (s)
    t_end         REAL,                         -- fim (s) — opcional
    speaker       TEXT NOT NULL DEFAULT '',     -- rótulo de locutor (livre, do perito)
    text          TEXT NOT NULL DEFAULT '',     -- transcrição do trecho
    created_at    TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_audio_transcript_audio
    ON audio_transcript_segments(occurrence_id, audio_sha256);
