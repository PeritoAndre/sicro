/**
 * AABB em coordenadas da stage (world px) para o marquee de seleção.
 * Ignora rotação dos veículos; quem precisa de precisão usa clique direto.
 */

import type { SicroObject } from "../engine";

interface BoundsPx {
  x: number;
  y: number;
  width: number;
  height: number;
}

const ZERO: BoundsPx = { x: 0, y: 0, width: 0, height: 0 };

/** AABB de um objeto em world px; `pxPerM` converte os objetos parity (metros). */
export function getObjectBoundsStagePx(
  obj: SicroObject,
  pxPerM: number,
): BoundsPx {
  switch (obj.kind) {
    case "vehicle": {
      const halfW = obj.width / 2;
      const halfH = obj.height / 2;
      return {
        x: obj.x - halfW,
        y: obj.y - halfH,
        width: obj.width,
        height: obj.height,
      };
    }
    case "marker": {
      const half = obj.size / 2;
      return {
        x: obj.x - half,
        y: obj.y - half,
        width: obj.size,
        height: obj.size,
      };
    }
    case "text": {
      // Sem canvas não há métrica de texto; ~0.6 × font_size por caractere.
      const fontSize = obj.font_size ?? 16;
      const approxW = Math.max(
        fontSize,
        (obj.text?.length ?? 1) * fontSize * 0.6,
      );
      return {
        x: obj.x,
        y: obj.y,
        width: approxW,
        height: fontSize * 1.2,
      };
    }
    case "line": {
      // Array plano [x1, y1, x2, y2, ...]
      const pts = obj.points;
      if (!pts || pts.length < 2) return ZERO;
      let minX = Infinity;
      let maxX = -Infinity;
      let minY = Infinity;
      let maxY = -Infinity;
      for (let i = 0; i + 1 < pts.length; i += 2) {
        const x = pts[i]!;
        const y = pts[i + 1]!;
        if (x < minX) minX = x;
        if (x > maxX) maxX = x;
        if (y < minY) minY = y;
        if (y > maxY) maxY = y;
      }
      return { x: minX, y: minY, width: maxX - minX, height: maxY - minY };
    }
    case "measurement": {
      const minX = Math.min(obj.p1.x, obj.p2.x);
      const maxX = Math.max(obj.p1.x, obj.p2.x);
      const minY = Math.min(obj.p1.y, obj.p2.y);
      const maxY = Math.max(obj.p1.y, obj.p2.y);
      return { x: minX, y: minY, width: maxX - minX, height: maxY - minY };
    }
    case "road_parity": {
      // Pontos de controle em metros. O AABB dos 4 pontos engloba a Bézier
      // (convex hull), com folga de meia largura da via.
      const minXm = Math.min(obj.ax, obj.cx1, obj.cx2, obj.bx);
      const maxXm = Math.max(obj.ax, obj.cx1, obj.cx2, obj.bx);
      const minYm = Math.min(obj.ay, obj.cy1, obj.cy2, obj.by);
      const maxYm = Math.max(obj.ay, obj.cy1, obj.cy2, obj.by);
      const halfWidthM = (obj.largura_m ?? 7) / 2;
      const minX = (minXm - halfWidthM) * pxPerM;
      const maxX = (maxXm + halfWidthM) * pxPerM;
      const minY = (minYm - halfWidthM) * pxPerM;
      const maxY = (maxYm + halfWidthM) * pxPerM;
      return { x: minX, y: minY, width: maxX - minX, height: maxY - minY };
    }
    case "roundabout_parity": {
      // cx, cy e r_m (raio externo) em metros.
      const rM = obj.r_m ?? 10;
      const minX = (obj.cx - rM) * pxPerM;
      const maxX = (obj.cx + rM) * pxPerM;
      const minY = (obj.cy - rM) * pxPerM;
      const maxY = (obj.cy + rM) * pxPerM;
      return { x: minX, y: minY, width: maxX - minX, height: maxY - minY };
    }
    default:
      return ZERO;
  }
}

/** Interseção AABB clássica; retângulo degenerado nunca intersecta. */
export function rectsIntersect(a: BoundsPx, b: BoundsPx): boolean {
  if (a.width === 0 || a.height === 0) return false;
  if (b.width === 0 || b.height === 0) return false;
  return !(
    a.x + a.width < b.x ||
    b.x + b.width < a.x ||
    a.y + a.height < b.y ||
    b.y + b.height < a.y
  );
}

/** Normaliza dois pontos arbitrários num retângulo com width/height >= 0. */
export function rectFromPoints(
  x1: number,
  y1: number,
  x2: number,
  y2: number,
): BoundsPx {
  const minX = Math.min(x1, x2);
  const minY = Math.min(y1, y2);
  return {
    x: minX,
    y: minY,
    width: Math.abs(x2 - x1),
    height: Math.abs(y2 - y1),
  };
}

/**
 * Patch que translada o objeto por (dx, dy) em world px (group-move).
 * Parity guarda metros, então divide por `pxPerM`. `null` para tipo desconhecido.
 */
export function translateObjectPatch(
  obj: SicroObject,
  dx: number,
  dy: number,
  pxPerM: number,
): Partial<SicroObject> | null {
  switch (obj.kind) {
    case "vehicle":
    case "marker":
    case "text":
      return { x: obj.x + dx, y: obj.y + dy };
    case "measurement":
      return {
        p1: { x: obj.p1.x + dx, y: obj.p1.y + dy },
        p2: { x: obj.p2.x + dx, y: obj.p2.y + dy },
      };
    case "line":
      return {
        points: obj.points.map((v, i) =>
          i % 2 === 0 ? v + dx : v + dy,
        ),
      };
    case "road_parity": {
      const dxM = dx / Math.max(pxPerM, 0.0001);
      const dyM = dy / Math.max(pxPerM, 0.0001);
      return {
        ax: obj.ax + dxM,
        ay: obj.ay + dyM,
        bx: obj.bx + dxM,
        by: obj.by + dyM,
        cx1: obj.cx1 + dxM,
        cy1: obj.cy1 + dyM,
        cx2: obj.cx2 + dxM,
        cy2: obj.cy2 + dyM,
      };
    }
    case "roundabout_parity": {
      const dxM = dx / Math.max(pxPerM, 0.0001);
      const dyM = dy / Math.max(pxPerM, 0.0001);
      return { cx: obj.cx + dxM, cy: obj.cy + dyM };
    }
    default:
      return null;
  }
}
