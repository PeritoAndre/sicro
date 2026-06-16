-- MVP 7 — Editor de Imagem Pericial.
--
-- Três tabelas:
--   * image_analyses        — uma linha por sessão `.sicroimage`;
--   * image_exports         — uma linha por imagem derivada (PNG/JPG +
--                              sidecar JSON) exportada da sessão;
--   * image_operation_logs  — log textual das operações da sessão
--                              (criação, ajuste, anotação, export...).
--
-- O `.sicroimage` continua na pasta `imagens/analises/`. Os derivados
-- vão para `imagens/exports/` junto com o sidecar `_sidecar.json` e
-- são referenciados aqui.
--
-- Nenhum campo de hash é obrigatório (algumas imagens vindas do Dossiê
-- já carregam SHA-256; outras, importadas localmente, são hasheadas
-- só no ato da criação da análise).
--
-- Compatibilidade: aditiva. Nenhuma tabela existente é alterada.

CREATE TABLE IF NOT EXISTS image_analyses (
    id TEXT PRIMARY KEY,
    occurrence_id TEXT NOT NULL REFERENCES occurrences(id) ON DELETE CASCADE,
    title TEXT NOT NULL,
    -- Origem: 'photo' | 'video_frame' | 'evidence' | 'local_import'
    source_kind TEXT NOT NULL,
    -- Source id na tabela de origem (media_asset_id, storyboard_frame_id,
    -- evidence_registry synthetic id ou file UUID local).
    source_id TEXT,
    -- Caminho relativo da imagem original (sempre dentro do workspace).
    original_relative_path TEXT NOT NULL,
    -- SHA-256 da imagem original (quando conhecida).
    original_hash_sha256 TEXT,
    -- Caminho relativo do arquivo `.sicroimage` (sessão).
    analysis_relative_path TEXT NOT NULL,
    -- Caminho relativo da última imagem derivada exportada.
    last_export_relative_path TEXT,
    status TEXT NOT NULL DEFAULT 'draft',
    metadata_json TEXT NOT NULL DEFAULT '{}',
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_image_analyses_occurrence
    ON image_analyses(occurrence_id);
CREATE INDEX IF NOT EXISTS idx_image_analyses_source
    ON image_analyses(source_kind, source_id);

CREATE TABLE IF NOT EXISTS image_exports (
    id TEXT PRIMARY KEY,
    occurrence_id TEXT NOT NULL REFERENCES occurrences(id) ON DELETE CASCADE,
    image_analysis_id TEXT NOT NULL REFERENCES image_analyses(id) ON DELETE CASCADE,
    output_relative_path TEXT NOT NULL,
    sidecar_relative_path TEXT,
    hash_sha256 TEXT,
    width INTEGER,
    height INTEGER,
    format TEXT NOT NULL,
    created_at TEXT NOT NULL,
    operation_summary_json TEXT NOT NULL DEFAULT '{}'
);

CREATE INDEX IF NOT EXISTS idx_image_exports_occurrence
    ON image_exports(occurrence_id);
CREATE INDEX IF NOT EXISTS idx_image_exports_analysis
    ON image_exports(image_analysis_id);

CREATE TABLE IF NOT EXISTS image_operation_logs (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    occurrence_id TEXT NOT NULL,
    image_analysis_id TEXT NOT NULL,
    action TEXT NOT NULL,
    details_json TEXT NOT NULL DEFAULT '{}',
    created_at TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_image_logs_analysis
    ON image_operation_logs(image_analysis_id, created_at DESC);
