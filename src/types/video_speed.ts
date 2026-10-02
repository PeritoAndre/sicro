/**
 * Espelha `src-tauri/src/models/video_speed.rs` (snake_case = wire do serde).
 * O repositório Rust já desserializa as colunas `*_json`; o front nunca vê o texto cru.
 */

export type VideoSpeedMethod = "line" | "plane" | "cross_ratio";
export type VideoSpeedReferenceSource = "campo" | "norma_viaria" | "entre_eixos";

/** Correspondência pixel↔mundo usada para ajustar a homografia. */
export interface ControlPoint {
  px: number;
  py: number;
  world_x_m: number;
  world_y_m: number;
  label?: string | null;
}

export interface VideoSpeedCalibration {
  id: string;
  occurrence_id: string;
  media_hash: string;
  method: VideoSpeedMethod | string;
  control_points: ControlPoint[];
  reference_source: VideoSpeedReferenceSource | string;
  /** 3×3 row-major (px da imagem → m), 9 valores. */
  homography: number[];
  /** RMS de reprojeção em px (null se não calculado). */
  residuals_px: number | null;
  /** Reservado para modelo de distorção de lente; null hoje. */
  distortion_model: Record<string, unknown> | null;
  author: string;
  created_at: string;
}

/** Posição marcada num quadro coletado do storyboard, herdando o timestamp real dele. */
export interface TrajectoryPoint {
  storyboard_frame_id?: string | null;
  export_id?: string | null;
  px: number;
  py: number;
  /** Incerteza de marcação (1σ, px). */
  u_px: number;
  actual_timestamp_s: number;
  /** Erro de seek do ffmpeg (pedido − real), para auditoria. */
  delta_s?: number | null;
  manual: boolean;
}

/** Persistidos para o Monte Carlo ser reproduzível. */
export interface McSigmas {
  calibration_px: number;
  world_m: number;
  trajectory_px: number;
  time_s: number;
}

/**
 * Campos de incerteza são nulos quando não se aplicam: 2 pontos = média sem IC nem
 * Monte Carlo; ≥3 pontos com plano (4 pts) = regressão + MC; ≥3 com linha (2 pts) =
 * regressão sem MC (MC exige 4 pontos coplanares).
 */
export interface VideoSpeedCalculation {
  id: string;
  occurrence_id: string;
  media_hash: string;
  calibration_id: string;
  points: TrajectoryPoint[];
  /** |v| em km/h (o valor principal). */
  velocity_kmh: number;
  vx_m_per_s: number;
  vy_m_per_s: number;
  /** Erro-padrão de |v| em m/s. */
  se_m_per_s: number | null;
  /** IC em km/h. */
  ci_low: number | null;
  ci_high: number | null;
  /** Nível do IC, ex.: 0.95. */
  confidence: number | null;
  r_squared: number | null;
  /** Resíduo 2D por ponto (vazio com 2 pontos). */
  residuals: number[];
  /** Seed exata do Monte Carlo (null se não rodou). */
  mc_seed: number | null;
  mc_sigmas: McSigmas | null;
  mc_n: number | null;
  /** Iterações descartadas. */
  mc_failed: number | null;
  mc_mean_kmh: number | null;
  mc_median_kmh: number | null;
  mc_p2_5_kmh: number | null;
  mc_p97_5_kmh: number | null;
  /** Ressalvas técnicas a transcrever no laudo. */
  limitations: string[];
  audit: Record<string, unknown> | null;
  author: string;
  created_at: string;
}

// ---------------------------------------------------------------------------
// Entradas dos comandos

export interface CreateSpeedCalibrationInput {
  media_hash: string;
  /** "line" (2 pontos) | "plane" (4 pontos, DLT). */
  method: VideoSpeedMethod | string;
  control_points: ControlPoint[];
  reference_source: VideoSpeedReferenceSource | string;
  author?: string | null;
}

export interface ComputeSpeedInput {
  calibration_id: string;
  points: TrajectoryPoint[];
  /** >= 10; ignorado com 2 pontos. */
  mc_n?: number | null;
  mc_sigmas?: McSigmas | null;
  /** Só 0.95 é suportado. */
  confidence?: number | null;
  author?: string | null;
}
