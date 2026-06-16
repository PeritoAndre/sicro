-- SICRO 2.0 — Módulo Áudio (migration 012).
--
-- Camada 1: registro de áudio com cadeia de custódia, espelhando o módulo
-- Vídeo (007). Cada áudio vive dentro do workspace `.sicro` da ocorrência.
--
--   * O ORIGINAL importado é preservado (audio/originais/) e hasheado — é a
--     evidência. O WAV de análise (audio/wav/) é um DERIVADO determinístico
--     (FFmpeg, PCM 16-bit), também hasheado, usado pelo player/forma de onda.
--   * Quando o áudio é EXTRAÍDO de um vídeo, não há original de áudio separado:
--     a origem é o vídeo (source_video_sha256); só o WAV derivado é guardado.
--
-- Aditiva. Workspaces antigos não regridem.

CREATE TABLE IF NOT EXISTS audio_media (
    id                     TEXT PRIMARY KEY,           -- UUID v4
    occurrence_id          TEXT NOT NULL REFERENCES occurrences(id) ON DELETE CASCADE,
    kind                   TEXT NOT NULL,              -- 'importado' | 'extraido'
    original_path          TEXT,                       -- caminho de origem no disco do usuário (ou do vídeo)
    original_relative_path TEXT,                       -- original preservado (workspace-relative), quando importado
    relative_path          TEXT NOT NULL,              -- WAV de análise (audio/wav/<file>.wav, workspace-relative)
    filename               TEXT NOT NULL,
    sha256                 TEXT NOT NULL,              -- hash do WAV de análise (chave de dedupe)
    original_sha256        TEXT,                       -- hash do original (quando importado)
    source_video_sha256    TEXT,                       -- vídeo de origem (quando extraído)
    size_bytes             INTEGER NOT NULL DEFAULT 0,
    duration_s             REAL,
    sample_rate            INTEGER,
    channels               INTEGER,
    codec                  TEXT,
    bitrate                INTEGER,
    raw_probe_json         TEXT NOT NULL DEFAULT '{}', -- ffprobe -show_format -show_streams verbatim
    warnings_json          TEXT NOT NULL DEFAULT '[]',
    created_at             TEXT NOT NULL,
    updated_at             TEXT NOT NULL,
    UNIQUE(occurrence_id, sha256)
);

CREATE INDEX IF NOT EXISTS idx_audio_media_occurrence_id ON audio_media(occurrence_id);
CREATE INDEX IF NOT EXISTS idx_audio_media_sha256        ON audio_media(sha256);

CREATE TABLE IF NOT EXISTS audio_operation_logs (
    id              INTEGER PRIMARY KEY AUTOINCREMENT,
    occurrence_id   TEXT NOT NULL REFERENCES occurrences(id) ON DELETE CASCADE,
    media_hash      TEXT,
    action          TEXT NOT NULL,                     -- audio.import | audio.extract | ...
    details_json    TEXT NOT NULL DEFAULT '{}',
    created_at      TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_audio_operation_logs_occurrence_id ON audio_operation_logs(occurrence_id);
