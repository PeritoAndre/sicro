-- SICRO 5.2 — Pessoas no vídeo (migration 023).
--
-- Rastreamento automático (YOLOX + ByteTrack) e edição do perito. O vídeo original
-- nunca é alterado: as trilhas são derivadas e ficam só no banco do caso.
--
--   video_people_runs   → uma análise por vídeo (modelo, limiar, passo, tempo gasto)
--   video_people_tracks → uma linha por pessoa: número, nome, cor, caixas automáticas
--                          (amostras_json) e correções do perito (ajustes_json)

CREATE TABLE IF NOT EXISTS video_people_runs (
    occurrence_id     TEXT NOT NULL REFERENCES occurrences(id) ON DELETE CASCADE,
    media_hash        TEXT NOT NULL,
    model             TEXT NOT NULL,
    runtime           TEXT NOT NULL,
    params_json       TEXT NOT NULL,
    width             INTEGER NOT NULL,
    height            INTEGER NOT NULL,
    frame_dur_s       REAL NOT NULL,
    step              INTEGER NOT NULL,
    frames_total      INTEGER NOT NULL,
    frames_analyzed   INTEGER NOT NULL,
    elapsed_s         REAL NOT NULL,
    created_at        TEXT NOT NULL,
    PRIMARY KEY (occurrence_id, media_hash)
);

CREATE TABLE IF NOT EXISTS video_people_tracks (
    id             TEXT PRIMARY KEY,                -- UUID v4
    occurrence_id  TEXT NOT NULL REFERENCES occurrences(id) ON DELETE CASCADE,
    media_hash     TEXT NOT NULL,
    numero         INTEGER NOT NULL,
    nome           TEXT NOT NULL DEFAULT '',
    cor            TEXT NOT NULL,
    estado         TEXT NOT NULL DEFAULT 'auto',    -- 'auto' | 'editado'
    amostras_json  TEXT NOT NULL,                   -- [{t,x,y,w,h}]
    ajustes_json   TEXT NOT NULL DEFAULT '[]',      -- [{t,dx,dy,dw,dh}]
    created_at     TEXT NOT NULL,
    updated_at     TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_video_people_tracks_media
    ON video_people_tracks(occurrence_id, media_hash);
