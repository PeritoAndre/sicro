/**
 * Normalized error type returned by command wrappers.
 *
 * Tauri's `invoke` rejects with whatever the Rust side returned through
 * `Result::Err`. We standardize that on the front-end so the UI can render
 * a consistent message without inspecting unknown shapes.
 */

export type SicroErrorKind =
  | "workspace"
  | "database"
  | "filesystem"
  | "validation"
  | "io"
  | "unknown";

export interface SicroError {
  kind: SicroErrorKind;
  message: string;
  /** Original payload from Rust, useful for debugging. */
  raw?: unknown;
}

export function toSicroError(err: unknown): SicroError {
  if (err && typeof err === "object" && "kind" in err && "message" in err) {
    const obj = err as { kind: unknown; message: unknown };
    if (typeof obj.kind === "string" && typeof obj.message === "string") {
      return {
        kind: (obj.kind as SicroErrorKind) ?? "unknown",
        message: obj.message,
        raw: err,
      };
    }
  }
  if (typeof err === "string") {
    return { kind: "unknown", message: err, raw: err };
  }
  if (err instanceof Error) {
    return { kind: "unknown", message: err.message, raw: err };
  }
  return { kind: "unknown", message: "Erro desconhecido.", raw: err };
}
