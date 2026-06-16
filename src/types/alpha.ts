/**
 * Mirror of `src-tauri/src/workspace/{backup,health}.rs` and
 * `src-tauri/src/commands/alpha_commands.rs` (MVP 8 — Consolidação Alpha).
 */

export interface BackupArtifact {
  absolute_path: string;
  relative_path: string;
  filename: string;
  size_bytes: number;
  hash_sha256: string;
  file_count: number;
  created_at: string;
}

export interface DependencyStatus {
  name: string;
  found: boolean;
  path: string | null;
  version_hint: string | null;
}

export interface WorkspaceCounters {
  photos: number;
  audios: number;
  laudos: number;
  croquis: number;
  croqui_exports: number;
  videos: number;
  storyboard_frames: number;
  image_analyses: number;
  image_exports: number;
  laudo_exports: number;
  evidence_links: number;
}

export interface WorkspaceHealth {
  workspace_path: string;
  workspace_id: string;
  occurrence_id: string;
  workspace_size_bytes: number;
  counters: WorkspaceCounters;
  /** "ok" | "warning" | "critical" */
  integrity_overall_status: string;
  files_ok: number;
  files_missing: number;
  broken_links: number;
  unsafe_paths: number;
}

export interface SystemHealthSnapshot {
  generated_at: string;
  app_version: string;
  schema_migrations_applied: number;
  dependencies: DependencyStatus[];
  workspace: WorkspaceHealth | null;
  warnings: string[];
}

export interface HealthReportArtifact {
  relative_path: string;
  generated_at: string;
  overall_status: string;
}

// ---- Backup geral (todos os casos) — W22 ----
// Espelho de `src-tauri/src/workspace/global_backup.rs`.

/** Um caso a entrar no backup geral (origem + rótulo humano). */
export interface GlobalCaseInput {
  workspace_path: string;
  label: string;
}

/** Evento `global-backup-progress` emitido por caso durante a execução. */
export interface GlobalBackupProgress {
  index: number;
  total: number;
  label: string;
  /** "checking" | "backing_up" | "skipped" | "done" | "missing" | "error" */
  phase: string;
}

/** Resultado por caso no relatório do backup geral. */
export interface CaseBackupResult {
  workspace_path: string;
  workspace_id: string | null;
  label: string;
  /** "backed_up" | "skipped_unchanged" | "missing" | "error" */
  status: string;
  filename: string | null;
  size_bytes: number;
  file_count: number;
  error: string | null;
}

/** Relatório final do backup geral incremental. */
export interface GlobalBackupReport {
  destination: string;
  generated_at: string;
  total_cases: number;
  backed_up: number;
  skipped: number;
  missing: number;
  errors: number;
  /** Tamanho de TODO o conjunto no destino. */
  total_size_bytes: number;
  cases: CaseBackupResult[];
}

/** Progresso por caso na restauração de um conjunto de backup. */
export interface RestoreProgress {
  index: number;
  total: number;
  label: string;
  /** "restoring" | "skipped" | "done" | "error" */
  phase: string;
}

/** Resultado por caso na restauração. */
export interface RestoredCase {
  workspace_id: string | null;
  label: string;
  source_filename: string;
  /** "restored" | "skipped_exists" | "error" */
  status: string;
  restored_path: string | null;
  file_count: number;
  error: string | null;
}

/** Relatório final da restauração de um conjunto de backup. */
export interface RestoreReport {
  source: string;
  cases_parent: string;
  restored: number;
  skipped: number;
  errors: number;
  cases: RestoredCase[];
}
