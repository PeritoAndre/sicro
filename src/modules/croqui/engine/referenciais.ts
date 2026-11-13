/** Referenciais R1/R2: cor em uso e pé da perpendicular para a cota de um clique. */

import type { SicroLineObject, SicroObject, SicroPoint } from "./schema";

export const COR_R1 = "#dc2626";
export const COR_R2 = "#2563eb";

export function corDoReferencial(linha: SicroLineObject): string {
  return linha.color ?? (linha.subtype === "r2" ? COR_R2 : COR_R1);
}

/** Primeiro R1 (ou R2) visível do croqui. */
export function acharReferencial(objects: ReadonlyArray<SicroObject>, qual: "r1" | "r2"): SicroLineObject | null {
  for (const o of objects) if (o.kind === "line" && o.subtype === qual && o.visible !== false && o.points.length >= 4) return o;
  return null;
}

/**
 * Ponto da linha de onde sai a perpendicular até `p`. Com o pé dentro de algum segmento, o mais próximo;
 * senão, a reta do segmento mais próximo prolongada (a cota continua perpendicular além da ponta do R).
 */
export function peDaPerpendicular(points: number[], p: SicroPoint): SicroPoint {
  let dentro: { pe: SicroPoint; d: number } | null = null;
  let fora: { pe: SicroPoint; d: number } | null = null;
  for (let i = 0; i + 3 < points.length; i += 2) {
    const ax = points[i]!;
    const ay = points[i + 1]!;
    const bx = points[i + 2]!;
    const by = points[i + 3]!;
    const dx = bx - ax;
    const dy = by - ay;
    const len2 = dx * dx + dy * dy;
    if (len2 < 1e-9) continue;
    const t = ((p.x - ax) * dx + (p.y - ay) * dy) / len2;
    const pe = { x: ax + t * dx, y: ay + t * dy };
    const d = Math.hypot(p.x - pe.x, p.y - pe.y);
    if (t >= 0 && t <= 1) {
      if (!dentro || d < dentro.d) dentro = { pe, d };
    } else {
      // Distância ao segmento (ponta mais próxima) decide qual reta prolongar.
      const tc = Math.max(0, Math.min(1, t));
      const dSeg = Math.hypot(p.x - (ax + tc * dx), p.y - (ay + tc * dy));
      if (!fora || dSeg < fora.d) fora = { pe, d: dSeg };
    }
  }
  return (dentro ?? fora)?.pe ?? { x: points[0] ?? p.x, y: points[1] ?? p.y };
}
