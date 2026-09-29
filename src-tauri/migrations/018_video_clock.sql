-- SICRO 3.0 — Relógio da câmera (migration 018).
--
-- Vínculo entre o tempo da MÍDIA e o relógio que a câmera imprime na imagem:
-- o perito informa "aos 00:05.000 do vídeo, o relógio da câmera marca
-- 03:36:05". A partir daí o player mostra o horário da câmera em qualquer
-- instante (tempo_mídia − media_time_s + clock_seconds) e dá para sincronizar
-- duas câmeras pelo horário. Um vínculo por vídeo (upsert). Aditivo.
--
-- clock_seconds: horário da câmera naquele instante, em segundos desde 00:00.
-- clock_label:   exatamente o que o perito digitou (rastreabilidade).

CREATE TABLE IF NOT EXISTS video_clock_calibrations (
    id              TEXT PRIMARY KEY,               -- UUID v4
    occurrence_id   TEXT NOT NULL REFERENCES occurrences(id) ON DELETE CASCADE,
    media_hash      TEXT NOT NULL,
    media_time_s    REAL NOT NULL,
    clock_seconds   REAL NOT NULL,
    clock_date      TEXT,                           -- AAAA-MM-DD, se a câmera mostra
    clock_label     TEXT NOT NULL,
    note            TEXT NOT NULL DEFAULT '',
    created_at      TEXT NOT NULL,
    updated_at      TEXT NOT NULL,
    UNIQUE(occurrence_id, media_hash)
);

CREATE INDEX IF NOT EXISTS idx_video_clock_occurrence ON video_clock_calibrations(occurrence_id);
