-- SICRO 5.2.1 — Rastreio: classe do objeto e origem da trilha (migration 024).
--
--   classe → classe COCO do detector (0 = pessoa); -1 = mancha marcada à mão
--   origem → 'auto' (detector + ByteTrack) | 'manual' (mancha seguida por correlação)
--
-- As trilhas da 5.2 ficam como pessoas detectadas automaticamente.

ALTER TABLE video_people_tracks ADD COLUMN classe INTEGER NOT NULL DEFAULT 0;
ALTER TABLE video_people_tracks ADD COLUMN origem TEXT NOT NULL DEFAULT 'auto';
