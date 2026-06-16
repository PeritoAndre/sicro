-- SICRO 2.0 — Marcadores de áudio (migration 013).
--
-- Marcadores temporais que o perito coloca no player (timestamp + rótulo),
-- ligados à mídia de áudio pelo hash do WAV de análise (lógico, não FK rígido —
-- coerente com o padrão do módulo Vídeo). Aditiva.

CREATE TABLE IF NOT EXISTS audio_markers (
    id            TEXT PRIMARY KEY,                  -- UUID v4
    occurrence_id TEXT NOT NULL REFERENCES occurrences(id) ON DELETE CASCADE,
    audio_sha256  TEXT NOT NULL,                     -- aponta para audio_media.sha256
    t_seconds     REAL NOT NULL,                     -- posição no áudio (segundos)
    label         TEXT NOT NULL DEFAULT '',
    created_at    TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_audio_markers_occurrence ON audio_markers(occurrence_id);
CREATE INDEX IF NOT EXISTS idx_audio_markers_audio      ON audio_markers(audio_sha256, t_seconds);
