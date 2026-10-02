/**
 * Espelha `src-tauri/src/models/video_distance.rs` (snake_case = wire do serde).
 * A medição CONSOME uma `VideoSpeedCalibration` existente; nunca recalibra.
 * Distância de 2 pontos não tem IC de regressão: sem σ, só `distance_m` e `mc_*` null.
 */

/** Sem σ de tempo: distância não envolve tempo. */
export interface McSigmasDistance {
  /** σ da marcação dos pontos de calibração (px). */
  calibration_px: number;
  /** σ da medida real da calibração (m). */
  world_m: number;
  /** σ da marcação dos DOIS pontos medidos (px). */
  measure_px: number;
}

export interface VideoDistanceMeasurement {
  id: string;
  occurrence_id: string;
  /** Herdado da calibração. */
  media_hash: string;
  calibration_id: string;
  p1_px: number;
  p1_py: number;
  p2_px: number;
  p2_py: number;
  /** Distância pontual em metros (sempre presente). */
  distance_m: number;
  /** Seed exata do Monte Carlo (null se não rodou). */
  mc_seed: number | null;
  mc_sigmas: McSigmasDistance | null;
  mc_n: number | null;
  /** Iterações descartadas. */
  mc_failed: number | null;
  mc_mean_m: number | null;
  mc_median_m: number | null;
  mc_p2_5_m: number | null;
  mc_p97_5_m: number | null;
  /** Ressalvas técnicas a transcrever no laudo. */
  limitations: string[];
  audit: Record<string, unknown> | null;
  author: string;
  created_at: string;
}

// ---------------------------------------------------------------------------
// Entrada do comando

export interface CreateDistanceMeasurementInput {
  /** Define a mídia e a projeção pixel→mundo. */
  calibration_id: string;
  p1_px: number;
  p1_py: number;
  p2_px: number;
  p2_py: number;
  /** >= 10; ignorado sem `mc_sigmas`. */
  mc_n?: number | null;
  mc_sigmas?: McSigmasDistance | null;
  author?: string | null;
}
