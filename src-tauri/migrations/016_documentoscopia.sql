-- Módulo Documentoscopia — OCR, layout documental, extração de campos, regiões,
-- análise documentoscópica assistida e comparação de documentos.
--
-- Princípios (§13 + metodologia documentoscópica):
--   * O arquivo ORIGINAL nunca é alterado: importação copia para o workspace e
--     gera hash SHA-256. Todo processamento ocorre sobre cópias/derivados.
--   * Determinístico e auditável: cada operação registra log com parâmetros,
--     motor utilizado, versão e hashes.
--   * Coordenadas (bbox) são NORMALIZADAS (0..1) em relação à página, para que a
--     sobreposição visual funcione em qualquer zoom/resolução de renderização.

CREATE TABLE IF NOT EXISTS doc_documents (
    id                TEXT PRIMARY KEY,
    occurrence_id     TEXT NOT NULL,
    title             TEXT NOT NULL,
    original_filename TEXT NOT NULL,
    relative_path     TEXT NOT NULL,                 -- cópia preservada (workspace-relativa)
    file_type         TEXT NOT NULL,                 -- "pdf" | "image"
    extension         TEXT NOT NULL,
    doc_type          TEXT NOT NULL DEFAULT 'outro', -- cnh|rg|crlv|contrato|recibo|...
    sha256            TEXT NOT NULL,
    size_bytes        INTEGER NOT NULL,
    page_count        INTEGER NOT NULL DEFAULT 0,
    has_text_layer    INTEGER NOT NULL DEFAULT 0,    -- PDF com camada textual pesquisável
    status            TEXT NOT NULL DEFAULT 'importado', -- importado|ocr_pendente|ocr_concluido|revisado
    metadata_json     TEXT NOT NULL DEFAULT '{}',
    notes             TEXT NOT NULL DEFAULT '',
    imported_by       TEXT,
    created_at        TEXT NOT NULL,
    updated_at        TEXT NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_doc_documents_occ ON doc_documents(occurrence_id);
CREATE UNIQUE INDEX IF NOT EXISTS idx_doc_documents_occ_sha
    ON doc_documents(occurrence_id, sha256);

CREATE TABLE IF NOT EXISTS doc_pages (
    id             TEXT PRIMARY KEY,
    document_id    TEXT NOT NULL,
    page_number    INTEGER NOT NULL,
    width          INTEGER NOT NULL DEFAULT 0,
    height         INTEGER NOT NULL DEFAULT 0,
    rotation       INTEGER NOT NULL DEFAULT 0,
    dpi            INTEGER,
    rendered_path  TEXT,
    thumbnail_path TEXT,
    created_at     TEXT NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_doc_pages_doc ON doc_pages(document_id);

CREATE TABLE IF NOT EXISTS doc_ocr_runs (
    id              TEXT PRIMARY KEY,
    document_id     TEXT NOT NULL,
    page_number     INTEGER,                  -- NULL = documento inteiro
    engine          TEXT NOT NULL,            -- "mock" | "pdf_text_layer" | "paddleocr" | ...
    engine_version  TEXT NOT NULL DEFAULT '',
    language        TEXT NOT NULL DEFAULT 'pt',
    mode            TEXT NOT NULL,            -- full_document|page|region
    status          TEXT NOT NULL DEFAULT 'concluido',
    avg_confidence  REAL,
    block_count     INTEGER NOT NULL DEFAULT 0,
    parameters_json TEXT NOT NULL DEFAULT '{}',
    created_at      TEXT NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_doc_ocr_runs_doc ON doc_ocr_runs(document_id);

CREATE TABLE IF NOT EXISTS doc_text_blocks (
    id             TEXT PRIMARY KEY,
    ocr_run_id     TEXT NOT NULL,
    document_id    TEXT NOT NULL,
    page_number    INTEGER NOT NULL DEFAULT 1,
    text           TEXT NOT NULL,
    confidence     REAL,
    bbox_x         REAL NOT NULL DEFAULT 0,  -- normalizado 0..1
    bbox_y         REAL NOT NULL DEFAULT 0,
    bbox_w         REAL NOT NULL DEFAULT 0,
    bbox_h         REAL NOT NULL DEFAULT 0,
    block_type     TEXT NOT NULL DEFAULT 'paragraph', -- paragraph|line|word|table|key_value|header|footer|signature|stamp|image|unknown
    reading_order  INTEGER NOT NULL DEFAULT 0,
    corrected_text TEXT,                      -- revisão humana (preserva o original em `text`)
    reviewed       INTEGER NOT NULL DEFAULT 0,
    created_at     TEXT NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_doc_text_blocks_run ON doc_text_blocks(ocr_run_id);
CREATE INDEX IF NOT EXISTS idx_doc_text_blocks_doc ON doc_text_blocks(document_id);

CREATE TABLE IF NOT EXISTS doc_fields (
    id              TEXT PRIMARY KEY,
    document_id     TEXT NOT NULL,
    page_number     INTEGER,
    field_name      TEXT NOT NULL,
    field_value     TEXT NOT NULL,
    confidence      REAL,
    source          TEXT NOT NULL DEFAULT 'heuristica', -- heuristica|manual|ocr
    bbox_x          REAL,
    bbox_y          REAL,
    bbox_w          REAL,
    bbox_h          REAL,
    reviewed        INTEGER NOT NULL DEFAULT 0,
    corrected_value TEXT,
    created_at      TEXT NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_doc_fields_doc ON doc_fields(document_id);

CREATE TABLE IF NOT EXISTS doc_regions (
    id           TEXT PRIMARY KEY,
    document_id  TEXT NOT NULL,
    page_number  INTEGER NOT NULL DEFAULT 1,
    region_type  TEXT NOT NULL, -- text|table|signature|stamp|qrcode|barcode|image|suspect_area|annotation|manual_selection
    bbox_x       REAL NOT NULL DEFAULT 0,
    bbox_y       REAL NOT NULL DEFAULT 0,
    bbox_w       REAL NOT NULL DEFAULT 0,
    bbox_h       REAL NOT NULL DEFAULT 0,
    label        TEXT NOT NULL DEFAULT '',
    confidence   REAL,
    notes        TEXT NOT NULL DEFAULT '',
    created_at   TEXT NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_doc_regions_doc ON doc_regions(document_id);

CREATE TABLE IF NOT EXISTS doc_analyses (
    id              TEXT PRIMARY KEY,
    document_id     TEXT NOT NULL,
    analysis_type   TEXT NOT NULL,            -- ela|noise|metadata|integrity|...
    status          TEXT NOT NULL DEFAULT 'concluido',
    parameters_json TEXT NOT NULL DEFAULT '{}',
    result_json     TEXT NOT NULL DEFAULT '{}',
    summary         TEXT NOT NULL DEFAULT '',
    created_at      TEXT NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_doc_analyses_doc ON doc_analyses(document_id);

CREATE TABLE IF NOT EXISTS doc_comparisons (
    id                     TEXT PRIMARY KEY,
    occurrence_id          TEXT NOT NULL,
    questioned_document_id TEXT NOT NULL,
    reference_document_id  TEXT NOT NULL,
    comparison_type        TEXT NOT NULL DEFAULT 'visual', -- visual|fields|layout|text|dimensions
    results_json           TEXT NOT NULL DEFAULT '{}',
    summary                TEXT NOT NULL DEFAULT '',
    created_at             TEXT NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_doc_comparisons_occ ON doc_comparisons(occurrence_id);

CREATE TABLE IF NOT EXISTS doc_logs (
    id              TEXT PRIMARY KEY,
    document_id     TEXT,
    occurrence_id   TEXT NOT NULL,
    action          TEXT NOT NULL,
    parameters_json TEXT NOT NULL DEFAULT '{}',
    result          TEXT NOT NULL DEFAULT '',
    source_hash     TEXT,
    output_hash     TEXT,
    actor           TEXT,
    created_at      TEXT NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_doc_logs_doc ON doc_logs(document_id);
CREATE INDEX IF NOT EXISTS idx_doc_logs_occ ON doc_logs(occurrence_id);
