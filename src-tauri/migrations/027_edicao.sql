-- Edição profissional (migration 027).
--
--   video_edit_outputs → cada exportação final da Edição. Sem media_hash de propósito:
--                        a saída é evidência produzida e fica quando um vídeo de origem sai.
--   video_media        → proveniência do recebimento (formato do gravador, desvio do relógio).

CREATE TABLE IF NOT EXISTS video_edit_outputs (
    id                  TEXT PRIMARY KEY,             -- UUID v4 (saida_id)
    occurrence_id       TEXT NOT NULL REFERENCES occurrences(id) ON DELETE CASCADE,
    projeto_id          TEXT NOT NULL,
    titulo              TEXT NOT NULL,
    pasta               TEXT NOT NULL,                -- relativa ao caso
    arquivo             TEXT NOT NULL,                -- relativo ao caso
    sha256              TEXT NOT NULL,                -- do arquivo
    composto_sha256     TEXT,                         -- quadros do grafo antes de codificar
    decodificado_sha256 TEXT,                         -- quadros do arquivo entregue, decodificado
    audio_sha256        TEXT,                         -- PCM do composto
    tamanho             INTEGER NOT NULL,
    duracao_s           REAL NOT NULL,
    n_quadros           INTEGER NOT NULL,
    predefinicao        TEXT NOT NULL,
    codificador         TEXT NOT NULL,
    tecnico_rel         TEXT,
    retrato_rel         TEXT NOT NULL,                -- Sistema/videos/edicao/<id>/exportacoes/<saida_id>
    projeto_sha256      TEXT NOT NULL,
    autor_json          TEXT NOT NULL DEFAULT '{}',
    refeita_de          TEXT,                         -- saida_id da exportação refeita
    estado              TEXT NOT NULL DEFAULT 'a_conferir'
                        CHECK (estado IN ('a_conferir', 'concluida')),
    exige_conferencia   INTEGER NOT NULL DEFAULT 0,   -- 1 = tem ocultação ou silêncio: pede conferência no arquivo
    conferida_por       TEXT,
    conferida_em        TEXT,
    criado_em           TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_video_edit_outputs_projeto
    ON video_edit_outputs(occurrence_id, projeto_id);

ALTER TABLE video_media ADD COLUMN recebido_nativo  INTEGER;
ALTER TABLE video_media ADD COLUMN recebido_obs     TEXT;
ALTER TABLE video_media ADD COLUMN relogio_desvio_s REAL;
ALTER TABLE video_media ADD COLUMN gravador         TEXT;
