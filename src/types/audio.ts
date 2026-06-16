/**
 * Espelho TS de `AudioMedia` (Rust: `models/audio.rs`). Módulo Áudio, Camada 1.
 *
 * `relative_path` aponta para o WAV de análise (derivado determinístico);
 * `original_relative_path` é o original preservado (quando importado).
 */

export interface AudioMedia {
  id: string;
  occurrence_id: string;
  /** "importado" | "extraido" */
  kind: string;
  original_path: string | null;
  original_relative_path: string | null;
  relative_path: string;
  filename: string;
  sha256: string;
  original_sha256: string | null;
  source_video_sha256: string | null;
  size_bytes: number;
  duration_s: number | null;
  sample_rate: number | null;
  channels: number | null;
  codec: string | null;
  bitrate: number | null;
  raw_probe_json: string;
  warnings_json: string;
  created_at: string;
  updated_at: string;
}

/** Espelho de `AudioMarker` (Rust). Marcador temporal persistido no caso. */
export interface AudioMarker {
  id: string;
  occurrence_id: string;
  audio_sha256: string;
  t_seconds: number;
  label: string;
  created_at: string;
}

/**
 * Espelho de `AudioTranscriptSegment` (Rust). Segmento de degravação MANUAL
 * (trabalho do perito): trecho temporal com rótulo de locutor e texto.
 */
export interface AudioTranscriptSegment {
  id: string;
  occurrence_id: string;
  audio_sha256: string;
  idx: number;
  t_start: number;
  t_end: number | null;
  speaker: string;
  text: string;
  created_at: string;
}

/** Status da ferramenta whisper.cpp (Fase 2 — transcrição local). */
export interface WhisperStatus {
  available: boolean;
  path: string | null;
}

/** Candidato de transcrição (RASCUNHO de máquina) devolvido pelo whisper. */
export interface TranscriptCandidate {
  idx: number;
  t_start: number;
  t_end: number | null;
  speaker: string;
  text: string;
  /** Confiança média (0..1) da IA neste trecho; null se indisponível. */
  confidence: number | null;
}

// W12 (paridade Audacity) — Análise forense (espelho de `audio/analysis.rs`).

/** Medições objetivas de um áudio (pico/RMS/DC/clipping/crista). */
export interface AudioMeasurements {
  duration_s: number;
  sample_rate: number;
  channels: number;
  samples: number;
  peak_linear: number;
  peak_dbfs: number;
  rms_dbfs: number;
  crest_factor_db: number;
  dc_offset: number;
  dc_offset_pct: number;
  clipped_samples: number;
  clipped_runs: number;
  clipped_pct: number;
}

/** Espectro Welch (FFT janelada + média). */
export interface SpectrumResult {
  sample_rate: number;
  fft_size: number;
  window: string;
  freqs_hz: number[];
  mag_db: number[];
  peak_freq_hz: number;
  peak_db: number;
}

/** Curva ENF (Electric Network Frequency) + continuidade. */
export interface EnfResult {
  nominal_hz: number;
  window_s: number;
  step_s: number;
  times_s: number[];
  enf_hz: number[];
  mean_hz: number;
  std_hz: number;
  /** Maior salto frame-a-frame (Hz): descontinuidade = indício de edição. */
  max_jump_hz: number;
}
