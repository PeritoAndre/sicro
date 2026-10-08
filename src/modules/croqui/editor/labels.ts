/** Rótulo solto dos objetos: posição padrão por tipo e medida de texto (fonte do Konva: Arial). */

import type { SicroObject } from "../engine";
import type { ParityTema } from "../engine/road-parity";

/** Campos do rótulo solto (os objetos parity não os têm no tipo, mas aceitam o patch). */
export interface LabelFields {
  label_dx?: number;
  label_dy?: number;
  label_size?: number;
  label_color?: string | null;
  label_rotation?: number | null;
}

export interface LabelDefaults {
  dx: number;
  dy: number;
  size: number;
  color: string;
  /** Centrado na âncora (veículo, cota) ou ancorado no canto superior esquerdo. */
  center: boolean;
}

/** `outlineTema`: veículo em traço (fundo claro, ou escuro no tema escuro). */
export function labelDefaults(obj: SicroObject, outlineTema?: ParityTema): LabelDefaults {
  switch (obj.kind) {
    case "vehicle": {
      const body = obj.body_type ?? "car";
      const twoWheel = body.startsWith("moto") || body.startsWith("bike");
      // Branco em cima da carroceria; escuro quando o rótulo foi tirado de lá.
      const onBody = obj.label_dx === undefined && obj.label_dy === undefined;
      const onLight = outlineTema !== undefined && outlineTema !== "escuro";
      return { dx: 0, dy: 0, size: twoWheel ? 10 : 12, color: onBody && !onLight ? "#ffffff" : "#111827", center: true };
    }
    case "trace":
    case "fixture":
    case "person":
      return { dx: 0, dy: 0, size: 12, color: "#111827", center: true };
    case "marker":
      return { dx: obj.size / 2 + 6, dy: -obj.size / 2, size: 11, color: obj.color ?? "#1f2937", center: false };
    case "line": {
      const isR = obj.subtype === "r1" || obj.subtype === "r2";
      return { dx: 0, dy: -18, size: isR ? 14 : 12, color: obj.color ?? "#1f2937", center: false };
    }
    case "measurement":
      return { dx: 0, dy: 0, size: 12, color: obj.color ?? "#dc2626", center: true };
    default:
      return { dx: 0, dy: 0, size: 12, color: "#111827", center: false };
  }
}

let measureCtx: CanvasRenderingContext2D | null | undefined;

/** Largura do texto em px (Arial negrito, a fonte que o Konva usa aqui). */
export function textWidthPx(text: string, size: number): number {
  if (measureCtx === undefined) {
    measureCtx =
      typeof document === "undefined" ? null : document.createElement("canvas").getContext("2d");
  }
  if (!measureCtx) return text.length * size * 0.6;
  measureCtx.font = `bold ${size}px Arial`;
  return measureCtx.measureText(text).width;
}
