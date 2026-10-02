/** Helpers compartilhados das abas Velocidade e Medições. */

import { convertFileSrc } from "@tauri-apps/api/core";
import type { VideoStoryboardFrame } from "@domain/video";

/** Caminho servível (asset protocol) do PNG de um frame coletado. */
export function frameAssetSrc(
  workspacePath: string,
  frame: VideoStoryboardFrame,
): string | null {
  try {
    const sep = workspacePath.includes("\\") ? "\\" : "/";
    const abs = `${workspacePath}${sep}${frame.output_path.replace(/\//g, sep)}`;
    return convertFileSrc(abs);
  } catch {
    return null;
  }
}

/**
 * Em VFR o seek do ffmpeg pode falhar e gravar `actual_timestamp_s = 0` (PTS não
 * resolvido); `??` não pega 0 e vários frames cairiam no mesmo instante, abortando
 * a regressão. Por isso actual ausente OU não-positivo conta como não confiável.
 */
function hasReliableActual(frame: VideoStoryboardFrame): boolean {
  const a = frame.actual_timestamp_s;
  return typeof a === "number" && Number.isFinite(a) && a > 0;
}

/**
 * Tempo técnico do frame: `actual_timestamp_s` quando confiável; senão o
 * `requested_timestamp_s` (preserva a separação pedida; a ressalva de VFR registra).
 */
export function frameTimestamp(frame: VideoStoryboardFrame): number {
  return hasReliableActual(frame)
    ? (frame.actual_timestamp_s as number)
    : frame.requested_timestamp_s;
}

/** Frame tem timestamp real confiável (actual > 0, não só o solicitado)? */
export function hasActualTimestamp(frame: VideoStoryboardFrame): boolean {
  return hasReliableActual(frame);
}
