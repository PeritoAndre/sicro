-- SICRO 2.0 — Medição de Distância por fotogrametria (migration 011).
-- Read by src/database/migrations.rs at app start. Keep idempotent
-- (CREATE IF NOT EXISTS) so re-runs are safe. One additive table; nothing
-- in earlier migrations is altered.
--
-- A medição CONSOME uma calibração já criada pelo Calculador de Velocidade
-- (video_speed_calibrations) — ela nunca recalibra a cena. A mesma 3×3
-- (homography_json) que dá velocidade projeta os 2 pontos para metros, e a
-- distância euclidiana entre eles é o resultado pontual.
--
-- DIFERENÇA crucial em relação à velocidade: distância de 2 pontos NÃO tem
-- intervalo de confiança de regressão. A ÚNICA fonte de incerteza é o Monte
-- Carlo. Sem σ informado pelo perito ⇒ só a distância pontual, sem intervalo
-- (todo o bloco mc_* fica NULL). Reprodutibilidade pericial: QUANDO o MC roda,
-- mc_seed e mc_sigmas_json são SEMPRE gravados para reproduzir o número.

CREATE TABLE IF NOT EXISTS video_distance_measurements (
    id                 TEXT PRIMARY KEY,             -- UUID v4
    occurrence_id      TEXT NOT NULL
                         REFERENCES occurrences(id) ON DELETE CASCADE,
    media_hash         TEXT NOT NULL,                -- sha256 do vídeo de origem
    calibration_id     TEXT NOT NULL
                         REFERENCES video_speed_calibrations(id) ON DELETE CASCADE,
    -- Os dois pontos medidos, em pixel nativo do frame calibrado.
    p1_px              REAL NOT NULL,
    p1_py              REAL NOT NULL,
    p2_px              REAL NOT NULL,
    p2_py              REAL NOT NULL,
    -- Distância pontual (m): |project(p2) − project(p1)|. Sempre presente.
    distance_m         REAL NOT NULL,
    -- Bloco de incerteza — TUDO nullable: sem σ não há Monte Carlo (e não há
    -- IC de regressão para 2 pontos). QUANDO o MC roda, mc_seed e
    -- mc_sigmas_json são gravados para reprodutibilidade.
    mc_seed            INTEGER,                      -- semente RNG (u64 reinterpretado como i64)
    mc_sigmas_json     TEXT,                         -- {calibration_px, world_m, measure_px}
    mc_n               INTEGER,                      -- iterações pedidas
    mc_failed          INTEGER,                      -- iterações descartadas (calibração singular)
    mc_mean_m          REAL,
    mc_median_m        REAL,
    mc_p2_5_m          REAL,
    mc_p97_5_m         REAL,
    limitations_json   TEXT NOT NULL DEFAULT '[]',   -- [string] ressalvas técnicas
    audit_json         TEXT NOT NULL DEFAULT '{}',   -- trilha de auditoria livre
    author             TEXT NOT NULL,
    created_at         TEXT NOT NULL                 -- ISO-8601 / RFC3339
);

CREATE INDEX IF NOT EXISTS idx_video_distance_measurements_occurrence
    ON video_distance_measurements(occurrence_id, media_hash);

CREATE INDEX IF NOT EXISTS idx_video_distance_measurements_calibration
    ON video_distance_measurements(calibration_id);
