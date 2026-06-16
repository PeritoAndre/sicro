-- SICRO 2.0 — Calculador de Velocidade (migration 010).
-- Read by src/database/migrations.rs at app start. Keep idempotent
-- (CREATE IF NOT EXISTS) so re-runs are safe. Two additive tables; nothing
-- in earlier migrations is altered.
--
-- Reprodutibilidade pericial é o requisito de projeto desta tabela:
--   * O cálculo SEMPRE persiste a semente (mc_seed) e os sigmas (mc_sigmas_json)
--     usados no Monte Carlo — o laudo precisa reproduzir o número exato.
--   * Cada ponto da trajetória (points_json) referencia um frame COLETADO real
--     (storyboard_frame_id / export_id), herdando actual_timestamp_s e delta_s.

-- --------------------------------------------------------------------------
-- video_speed_calibrations
-- Uma calibração mapeia pixels da imagem para metros no plano da via.
--   method = 'line'  → calibração por segmento de comprimento conhecido (2 pts)
--   method = 'plane' → homografia DLT por 4 correspondências (4 pts)
-- A homografia resultante (3x3 row-major, 9 f64) é congelada aqui.
CREATE TABLE IF NOT EXISTS video_speed_calibrations (
    id                    TEXT PRIMARY KEY,          -- UUID v4
    occurrence_id         TEXT NOT NULL
                            REFERENCES occurrences(id) ON DELETE CASCADE,
    media_hash            TEXT NOT NULL,             -- sha256 do vídeo de origem
    method                TEXT NOT NULL,             -- 'line' | 'plane'
    control_points_json   TEXT NOT NULL DEFAULT '[]',-- [{px,py,world_x_m,world_y_m,label?}]
    reference_source      TEXT NOT NULL,             -- 'campo' | 'norma_viaria' | 'entre_eixos'
    homography_json       TEXT NOT NULL,             -- 9 f64 row-major (imagem px → mundo m)
    residuals_px          REAL,                      -- RMS de reprojeção (NULL se não calculado)
    distortion_model_json TEXT,                      -- reservado p/ distorção de lente; NULL hoje
    author                TEXT NOT NULL,
    created_at            TEXT NOT NULL              -- ISO-8601 / RFC3339
);

CREATE INDEX IF NOT EXISTS idx_video_speed_calibrations_occurrence
    ON video_speed_calibrations(occurrence_id, media_hash);

-- --------------------------------------------------------------------------
-- video_speed_calculations
-- Um cálculo amarra uma calibração a uma trajetória marcada e guarda tanto o
-- ajuste por regressão (vx/vy/SE/IC/R²) quanto a distribuição Monte Carlo.
CREATE TABLE IF NOT EXISTS video_speed_calculations (
    id                    TEXT PRIMARY KEY,          -- UUID v4
    occurrence_id         TEXT NOT NULL
                            REFERENCES occurrences(id) ON DELETE CASCADE,
    media_hash            TEXT NOT NULL,
    calibration_id        TEXT NOT NULL
                            REFERENCES video_speed_calibrations(id) ON DELETE CASCADE,
    -- Cada ponto referencia um frame coletado real (storyboard) p/ herdar tempo:
    -- [{storyboard_frame_id?,export_id?,px,py,u_px,actual_timestamp_s,delta_s?,manual}]
    points_json           TEXT NOT NULL DEFAULT '[]',
    -- Velocidade líquida (sempre presente; regressão por eixo OU média 2-pts):
    velocity_kmh          REAL NOT NULL,             -- |v| = sqrt(vx²+vy²) em km/h
    vx_m_per_s            REAL NOT NULL,
    vy_m_per_s            REAL NOT NULL,
    -- Incerteza do AJUSTE — só existe na regressão (>=3 pontos). No caso de
    -- 2 pontos (média) estas colunas ficam NULL: não há IC estatístico.
    se_m_per_s            REAL,                      -- erro-padrão de |v| (m/s)
    ci_low                REAL,                      -- limite inferior do IC (km/h)
    ci_high               REAL,                      -- limite superior do IC (km/h)
    confidence            REAL,                      -- nível do IC, ex.: 0.95
    r_squared             REAL,
    residuals_json        TEXT NOT NULL DEFAULT '[]',-- [f64] (vazio no caso 2-pts)
    -- Monte Carlo — só existe quando roda (>=3 pontos E calibração de plano,
    -- 4 pontos coplanares). Reprodutibilidade: QUANDO presente, mc_seed e
    -- mc_sigmas_json são SEMPRE gravados para reproduzir o número. No caso
    -- 2-pts ou calibração por linha, ficam NULL.
    mc_seed               INTEGER,                   -- semente RNG resolvida (u64 reinterpretado como i64)
    mc_sigmas_json        TEXT,                      -- {calibration_px,world_m,trajectory_px,time_s}
    mc_n                  INTEGER,                   -- iterações pedidas
    mc_failed             INTEGER,                   -- iterações descartadas (ex.: homografia singular)
    mc_mean_kmh           REAL,
    mc_median_kmh         REAL,
    mc_p2_5_kmh           REAL,
    mc_p97_5_kmh          REAL,
    limitations_json      TEXT NOT NULL DEFAULT '[]',-- [string] ressalvas técnicas
    audit_json            TEXT NOT NULL DEFAULT '{}',-- trilha de auditoria livre
    author                TEXT NOT NULL,
    created_at            TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_video_speed_calculations_occurrence
    ON video_speed_calculations(occurrence_id, media_hash);

CREATE INDEX IF NOT EXISTS idx_video_speed_calculations_calibration
    ON video_speed_calculations(calibration_id);
