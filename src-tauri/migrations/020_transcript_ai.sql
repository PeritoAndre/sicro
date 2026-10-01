-- SICRO 4.0 — Rascunho da IA na degravação (migration 020).
--
-- Guarda, por trecho, o que veio da transcrição automática (whisper.cpp
-- local) para não se perder ao reabrir o caso:
--
--   ai_json → {"draft": true|false (ainda não revisado pelo perito),
--              "confidence": 0..1 (média do trecho),
--              "words": [{"text","t_start","t_end","p"}] (palavras com tempo
--                        no áudio original e confiança, para ouvir de novo
--                        as duvidosas)}
--
-- Aditivo: trechos existentes ficam com NULL (escritos pelo perito).

ALTER TABLE audio_transcript_segments ADD COLUMN ai_json TEXT;
