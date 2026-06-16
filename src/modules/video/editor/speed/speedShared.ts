/**
 * Helpers compartilhados do Calculador de Velocidade (UI).
 */

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
 * `actual_timestamp_s` é confiável? Em VFR, o seek do ffmpeg pode falhar e
 * gravar `0` (PTS não resolvido) — note que `0` NÃO é capturado por `?? `,
 * então vários frames acabariam com o mesmo instante 0 e a regressão de
 * velocidade abortaria com "amplitude temporal zero". Tratamos um actual
 * ausente OU não-positivo como não confiável (um frame real em t=0.000 cairia
 * no `requested` ≈ 0, então não há perda prática).
 */
function hasReliableActual(frame: VideoStoryboardFrame): boolean {
  const a = frame.actual_timestamp_s;
  return typeof a === "number" && Number.isFinite(a) && a > 0;
}

/**
 * Tempo técnico do frame: o `actual_timestamp_s` real (entregue pelo ffmpeg)
 * é a fonte de verdade QUANDO confiável; caso contrário cai para o
 * `requested_timestamp_s`, que preserva a separação temporal pedida pelo
 * perito (a ressalva de VFR documenta a aproximação).
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
