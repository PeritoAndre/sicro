-- SICRO 2.0 — MVP 4 / Integração Evidência → Laudo (migration 008).
--
-- Tabela de vínculos entre evidências e laudos. Quando o perito insere
-- uma foto/croqui/frame/dado no laudo, uma linha é gravada aqui — assim
-- a UI consegue listar "este laudo cita as seguintes evidências" sem
-- parsear o `.sicrodoc`, e auditoria fica trivial.
--
-- Os atributos da evidência inserida (caminho relativo, hash, etc.)
-- continuam no próprio nó do `.sicrodoc` — esta tabela é índice/log,
-- não a fonte da verdade.

CREATE TABLE IF NOT EXISTS evidence_links (
    id                  TEXT PRIMARY KEY,           -- UUID v4
    occurrence_id       TEXT NOT NULL REFERENCES occurrences(id) ON DELETE CASCADE,
    target_type         TEXT NOT NULL,              -- 'laudo' (reservado para futuras superfícies)
    target_id           TEXT NOT NULL,              -- laudo_id quando target_type='laudo'
    relation_type       TEXT NOT NULL,              -- 'inserted_in_laudo'
    source_kind         TEXT NOT NULL,              -- 'photo' | 'croqui' | 'video_frame' | 'video_storyboard' | 'occurrence_field' | 'checklist_table' | 'traces_table' | 'measurements_table' | 'field_note'
    /* Identificadores opcionais conforme o source_kind. NULL quando não aplicável. */
    media_asset_id              TEXT,
    croqui_id                   TEXT,
    video_media_hash            TEXT,
    video_event_id              TEXT,
    video_storyboard_frame_id   TEXT,
    field_note_id               TEXT,
    /* Caminho relativo do asset principal (PNG/JPG/etc.) — pode ser NULL para inserts puramente textuais. */
    relative_path       TEXT,
    /* Hash do asset principal quando disponível (foto: SHA-256 do JPG; croqui: SHA do PNG; vídeo: media_hash). */
    source_hash         TEXT,
    /* Bloco JSON livre — categoria da foto, timestamp do frame, etc. */
    metadata_json       TEXT NOT NULL DEFAULT '{}',
    created_at          TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_evidence_links_target
    ON evidence_links(target_type, target_id);
CREATE INDEX IF NOT EXISTS idx_evidence_links_occurrence
    ON evidence_links(occurrence_id);
CREATE INDEX IF NOT EXISTS idx_evidence_links_source_kind
    ON evidence_links(source_kind);
