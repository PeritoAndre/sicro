/**
 * Mirror of `src-tauri/src/models/video_speed.rs`
 * (Calculador de Velocidade — Fase 2, camada de dados).
 *
 * Field names stay snake_case so serde defaults line up with the wire. These
 * interfaces describe the **structured** shape a future Tauri command returns —
 * the Rust repository (de)serializes the `*_json` table columns into these
 * types, so the front-end never sees the raw JSON-text columns.
 */

export type VideoSpeedMethod = "line" | "plane" | "cross_ratio";
export type VideoSpeedReferenceSource = "campo" | "norma_viaria" | "entre_eixos";

/** One pixel↔world correspondence used to fit the homography. */
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
  /** sha256 of the source video. */
  media_hash: string;
  method: VideoSpeedMethod | string;
  control_points: ControlPoint[];
  reference_source: VideoSpeedReferenceSource | string;
  /** Row-major 3×3 homography (image px → world m), length 9. */
  homography: number[];
  /** RMS reprojection residual in pixels (null if not computed). */
  residuals_px: number | null;
  /** Reserved for a future lens-distortion model; null today. */
  distortion_model: Record<string, unknown> | null;
  author: string;
  created_at: string;
}

/**
 * One marked vehicle position, bound to a real collected storyboard frame so
 * it inherits that frame's timestamp (pericial reproducibility).
 */
export interface TrajectoryPoint {
  /** Storyboard frame this point was marked on (the collected frame). */
  storyboard_frame_id?: string | null;
  /** The export (PNG) backing the storyboard frame, when available. */
  export_id?: string | null;
  px: number;
  py: number;
  /** Pixel marking uncertainty (1σ) for this point. */
  u_px: number;
  /** Real frame time inherited from the storyboard frame. */
  actual_timestamp_s: number;
  /** ffmpeg seek error (requested − actual), inherited for audit. */
  delta_s?: number | null;
  /** True when marked by a human (always true in this phase). */
  manual: boolean;
}

/** Monte Carlo per-source sigmas — persisted so the run is reproducible. */
export interface McSigmas {
  calibration_px: number;
  world_m: number;
  trajectory_px: number;
  time_s: number;
}

/**
 * A speed calculation. Uncertainty fields are nullable because not every
 * calculation has them:
 *   - 2 points (average): velocity_kmh/vx/vy present, but NO CI and NO Monte
 *     Carlo — every nullable field is null, and `residuals` is empty.
 *   - ≥3 points with a plane (4-pt) calibration: regression (CI) + Monte Carlo.
 *   - ≥3 points with a line (2-pt) calibration: regression (CI) present, but
 *     Monte Carlo null (MC needs 4 coplanar points).
 */
export interface VideoSpeedCalculation {
  id: string;
  occurrence_id: string;
  media_hash: string;
  /** FK → VideoSpeedCalibration.id */
  calibration_id: string;
  points: TrajectoryPoint[];
  /** |v| = sqrt(vx²+vy²), in km/h (the headline value). */
  velocity_kmh: number;
  vx_m_per_s: number;
  vy_m_per_s: number;
  /** Standard error of |v| in m/s (null for the 2-point case). */
  se_m_per_s: number | null;
  /** Lower confidence-interval bound, in km/h (null for the 2-point case). */
  ci_low: number | null;
  /** Upper confidence-interval bound, in km/h (null for the 2-point case). */
  ci_high: number | null;
  /** Confidence level for [ci_low, ci_high], e.g. 0.95 (null if 2 points). */
  confidence: number | null;
  r_squared: number | null;
  /** 2D residual per point (empty for the 2-point case). */
  residuals: number[];
  /** Exact RNG seed used by the Monte Carlo run (null if MC did not run). */
  mc_seed: number | null;
  /** Exact per-source sigmas used (null if MC did not run). */
  mc_sigmas: McSigmas | null;
  /** Monte Carlo iterations requested (null if MC did not run). */
  mc_n: number | null;
  /** Iterations that failed and were discarded (null if MC did not run). */
  mc_failed: number | null;
  mc_mean_kmh: number | null;
  mc_median_kmh: number | null;
  mc_p2_5_kmh: number | null;
  mc_p97_5_kmh: number | null;
  /** Technical caveats to transcribe into the laudo. */
  limitations: string[];
  /** Free-form audit trail (tool versions, operator notes, etc.). */
  audit: Record<string, unknown> | null;
  author: string;
  created_at: string;
}

// ---------------------------------------------------------------------------
// Command inputs (Phase 3)

export interface CreateSpeedCalibrationInput {
  /** sha256 of a video already registered in this occurrence. */
  media_hash: string;
  /** "line" (2 points) | "plane" (4 points, DLT). */
  method: VideoSpeedMethod | string;
  /** Pixel↔world correspondences (4 for plane, 2 for line). */
  control_points: ControlPoint[];
  reference_source: VideoSpeedReferenceSource | string;
  author?: string | null;
}

export interface ComputeSpeedInput {
  /** Stored calibration used to project pixel→world. */
  calibration_id: string;
  /** Marked trajectory — each point tied to a real collected frame. */
  points: TrajectoryPoint[];
  /** Monte Carlo iterations (>= 10). Ignored for the 2-point case. */
  mc_n?: number | null;
  /** Per-source sigmas for Monte Carlo. Ignored for the 2-point case. */
  mc_sigmas?: McSigmas | null;
  /** Desired CI level (only 0.95 is supported in this phase). */
  confidence?: number | null;
  author?: string | null;
}
