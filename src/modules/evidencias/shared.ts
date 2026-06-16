/**
 * Shared helpers for the Evidências module.
 */

import { convertFileSrc } from "@tauri-apps/api/core";
import type { IntegrityStatus } from "@domain/evidence_registry";

export function statusClass(
  status: IntegrityStatus,
  styles: Record<string, string>,
): string {
  switch (status) {
    case "ok":
      return `${styles.statusPill} ${styles.statusOk}`;
    case "missing_file":
    case "missing_sidecar":
    case "broken_link":
      return `${styles.statusPill} ${styles.statusWarn}`;
    case "hash_mismatch":
    case "unsafe_path":
      return `${styles.statusPill} ${styles.statusCrit}`;
    default:
      return `${styles.statusPill} ${styles.statusUnknown}`;
  }
}

export function statusLabel(status: IntegrityStatus): string {
  switch (status) {
    case "ok":
      return "ok";
    case "missing_file":
      return "ausente";
    case "missing_sidecar":
      return "sidecar ausente";
    case "broken_link":
      return "link quebrado";
    case "hash_mismatch":
      return "hash divergente";
    case "unsafe_path":
      return "path inseguro";
    case "unknown":
      return "—";
  }
}

/** Build an asset URL the WebView can load (tauri:// protocol). */
export function assetUrl(
  workspacePath: string,
  relativePath: string,
): string | null {
  if (!workspacePath || !relativePath) return null;
  const sep = workspacePath.includes("\\") ? "\\" : "/";
  const trimmed = workspacePath.replace(/[\\/]+$/, "");
  const normRel = sep === "\\"
    ? relativePath.replace(/\//g, "\\")
    : relativePath.replace(/\\/g, "/");
  try {
    return convertFileSrc(`${trimmed}${sep}${normRel}`);
  } catch {
    return null;
  }
}

export function prettyBytes(n: number | null | undefined): string {
  if (n == null) return "—";
  if (n < 1024) return `${n} B`;
  if (n < 1024 * 1024) return `${(n / 1024).toFixed(1)} KB`;
  if (n < 1024 * 1024 * 1024) return `${(n / 1024 / 1024).toFixed(1)} MB`;
  return `${(n / 1024 / 1024 / 1024).toFixed(2)} GB`;
}

export function shortHash(h: string | null | undefined, n = 10): string {
  if (!h) return "—";
  return h.length <= n ? h : `${h.slice(0, n)}…`;
}

export function kindLabel(kind: string): string {
  switch (kind) {
    case "photo":
      return "Foto";
    case "croqui":
      return "Croqui";
    case "croqui_export":
      return "Croqui (PNG)";
    case "video":
      return "Vídeo";
    case "audio":
      return "Áudio";
    case "video_frame":
      return "Frame";
    case "storyboard_frame":
      return "Frame";
    case "laudo":
      return "Laudo";
    case "laudo_export":
      return "Export laudo";
    case "image_analysis":
      return "Imagem (análise)";
    case "image_export":
      return "Imagem (export)";
    case "document":
      return "Documento";
    case "imported_package":
      return "Pacote importado";
    default:
      return kind;
  }
}
