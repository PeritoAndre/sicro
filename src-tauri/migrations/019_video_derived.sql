-- SICRO 3.1 — Trechos exportados (migration 019).
--
-- "Exportar trecho" grava uma CÓPIA de parte de um vídeo do caso (o original
-- nunca é tocado) e a registra como vídeo do caso, com hash próprio. Estas
-- colunas ligam a cópia à origem:
--
--   derived_from_hash → SHA-256 do vídeo de origem (NULL = vídeo original)
--   derivation_json   → como foi feito: entrada/saída pedidas e reais, modo
--                        (copy = sem recompressão | reencode), comando ffmpeg
--
-- Aditivo: vídeos existentes ficam com NULL (são originais).

ALTER TABLE video_media ADD COLUMN derived_from_hash TEXT;
ALTER TABLE video_media ADD COLUMN derivation_json TEXT;
