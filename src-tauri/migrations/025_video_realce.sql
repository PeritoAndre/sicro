-- SICRO 5.3 — Leitura e realce de imagem no vídeo (migration 025).
--
-- Realce é apoio de leitura, não prova: o vídeo original nunca é alterado. Tudo que o
-- realce gera é arquivo novo, com SHA-256, parâmetros e aviso gravados.
--
--   video_lens_profiles → correção de lente (k1, k2, centro) salva por vídeo
--   video_realce_results → cada saída do realce (média de quadros, super-resolução,
--                           retificação, quadro corrigido, vídeo estabilizado ou corrigido)

CREATE TABLE IF NOT EXISTS video_lens_profiles (
    occurrence_id  TEXT NOT NULL REFERENCES occurrences(id) ON DELETE CASCADE,
    media_hash     TEXT NOT NULL,
    nome           TEXT NOT NULL DEFAULT '',
    k1             REAL NOT NULL,
    k2             REAL NOT NULL,
    cx             REAL NOT NULL DEFAULT 0.5,
    cy             REAL NOT NULL DEFAULT 0.5,
    updated_at     TEXT NOT NULL,
    PRIMARY KEY (occurrence_id, media_hash)
);

CREATE TABLE IF NOT EXISTS video_realce_results (
    id              TEXT PRIMARY KEY,                -- UUID v4
    occurrence_id   TEXT NOT NULL REFERENCES occurrences(id) ON DELETE CASCADE,
    media_hash      TEXT NOT NULL,                   -- vídeo de origem
    tipo            TEXT NOT NULL,                   -- media | media_quadro | superres | retificada | quadro_lente | estabilizado | lente_video
    caminho         TEXT NOT NULL,                   -- relativo ao caso
    sha256          TEXT NOT NULL,
    tamanho         INTEGER NOT NULL,
    largura         INTEGER,
    altura          INTEGER,
    parametros_json TEXT NOT NULL,
    aviso           TEXT NOT NULL DEFAULT '',
    created_at      TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_video_realce_results_media
    ON video_realce_results(occurrence_id, media_hash);
