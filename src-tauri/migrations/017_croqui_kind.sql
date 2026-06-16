-- SICRO 2.0 — Croqui Corporal (migration 017).
--
-- A umbrella "Croquis" passa a comportar dois sub-sistemas: o croqui VIÁRIO
-- (existente) e o croqui CORPORAL (carta de lesões / morte violenta). Em vez
-- de uma tabela nova, marcamos cada linha com `kind`. Aditivo: croquis
-- existentes assumem 'viario' (DEFAULT), sem migração de dados.
--
--   kind = 'viario'   → arquivo .sicrocroqui (engine viário)
--   kind = 'corporal' → arquivo .sicrocorpo  (engine corporal)
--
-- O backend continua tratando o documento como JSON OPACO; o `kind` só
-- direciona a UI pro editor certo e permite filtrar a lista.

ALTER TABLE croquis ADD COLUMN kind TEXT NOT NULL DEFAULT 'viario';

CREATE INDEX IF NOT EXISTS idx_croquis_kind ON croquis(kind);
