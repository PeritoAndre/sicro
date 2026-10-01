-- SICRO 4.0 — Separação de locutores (migration 021).
--
-- "Quem fala quando" num áudio, calculado LOCALMENTE (sherpa-onnx: segmentação
-- pyannote 3.0 + assinatura de voz WeSpeaker). Uma linha por áudio (rodar de
-- novo substitui). É apoio à degravação, não identificação de pessoa:
--
--   turns_json  → [{"t_start","t_end","speaker"}]  (speaker = 1..N, ordem de
--                 primeira fala)
--   names_json  → nomes dados pelo perito ["Entrevistador", …] (índice =
--                 locutor − 1; vazio = "Locutor N")
--   params_json → programa e versão, modelos, nº de locutores informado ou
--                 limiar do modo automático

CREATE TABLE IF NOT EXISTS audio_diarizations (
    id            TEXT PRIMARY KEY,
    occurrence_id TEXT NOT NULL REFERENCES occurrences(id) ON DELETE CASCADE,
    audio_sha256  TEXT NOT NULL,
    turns_json    TEXT NOT NULL,
    names_json    TEXT NOT NULL DEFAULT '[]',
    params_json   TEXT NOT NULL,
    created_at    TEXT NOT NULL
);

CREATE UNIQUE INDEX IF NOT EXISTS idx_audio_diarizations_audio
    ON audio_diarizations(occurrence_id, audio_sha256);
