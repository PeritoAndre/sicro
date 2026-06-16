/**
 * Mirror of `src-tauri/src/models/video_distance.rs`
 * (Medição de Distância por fotogrametria — camada de dados).
 *
 * Field names stay snake_case so serde defaults line up with the wire. The
 * measurement CONSUMES an existing `VideoSpeedCalibration` (via
 * `calibration_id`) — it never recalibrates the scene.
 *
 * Unlike speed, a 2-point distance has NO regression confidence interval: the
 * ONLY source of uncertainty is the Monte Carlo. Without σ, only the pointwise
 * `distance_m` is produced and every `mc_*` field is null.
 */

/** Monte Carlo per-source sigmas for distance — NO time sigma (no time here). */
export interface McSigmasDistance {
  /** σ of calibration-point marking (px). */
  calibration_px: number;
  /** σ of the calibration's real-dimension measurement (m). */
  world_m: number;
  /** σ of marking the TWO measured points (px). */
  measure_px: number;
}

export interface VideoDistanceMeasurement {
  id: string;
  occurrence_id: string;
  /** sha256 of the source video (inherited from the calibration). */
  media_hash: string;
  /** FK → VideoSpeedCalibration.id (the scene geometry consumed). */
  calibration_id: string;
  p1_px: number;
  p1_py: number;
  p2_px: number;
  p2_py: number;
  /** Pointwise distance in meters (always present). */
  distance_m: number;
  /** Exact RNG seed used by the Monte Carlo run (null if MC did not run). */
  mc_seed: number | null;
  /** Exact per-source sigmas used (null if MC did not run). */
  mc_sigmas: McSigmasDistance | null;
  /** Monte Carlo iterations requested (null if MC did not run). */
  mc_n: number | null;
  /** Iterations that failed and were discarded (null if MC did not run). */
  mc_failed: number | null;
  mc_mean_m: number | null;
  mc_median_m: number | null;
  mc_p2_5_m: number | null;
  mc_p97_5_m: number | null;
  /** Technical caveats to transcribe into the laudo. */
  limitations: string[];
  /** Free-form audit trail. */
  audit: Record<string, unknown> | null;
  author: string;
  created_at: string;
}

// ---------------------------------------------------------------------------
// Command input

export interface CreateDistanceMeasurementInput {
  /** Stored calibration used to project pixel→world (defines the media). */
  calibration_id: string;
  p1_px: number;
  p1_py: number;
  p2_px: number;
  p2_py: number;
  /** Monte Carlo iterations (>= 10). Ignored if `mc_sigmas` absent/zero. */
  mc_n?: number | null;
  /** Per-source sigmas for Monte Carlo. Without them, only the pointwise distance. */
  mc_sigmas?: McSigmasDistance | null;
  author?: string | null;
}
