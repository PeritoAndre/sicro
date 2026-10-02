/**
 * Erro normalizado dos wrappers de comando: o `invoke` rejeita com o que o
 * Rust devolveu em `Result::Err`, em formato variável.
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
  /** Payload original do Rust. */
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
