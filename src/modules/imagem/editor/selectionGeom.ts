/**
 * W20 — Geometria pura da seleção (sem React/Konva). Coordenadas em px da
 * imagem original. Reutilizável na máscara (S2) e na cópia de região (S3).
 */
import type {
  SicroImagePoint,
  SicroImageSelection,
} from "../engine/schema";

/** Distância perpendicular do ponto p ao segmento a–b. */
function perpDistance(
  p: SicroImagePoint,
  a: SicroImagePoint,
  b: SicroImagePoint,
): number {
  const dx = b.x - a.x;
  const dy = b.y - a.y;
  const len2 = dx * dx + dy * dy;
  if (len2 === 0) return Math.hypot(p.x - a.x, p.y - a.y);
  let t = ((p.x - a.x) * dx + (p.y - a.y) * dy) / len2;
  t = Math.max(0, Math.min(1, t));
  return Math.hypot(p.x - (a.x + t * dx), p.y - (a.y + t * dy));
}

/**
 * Douglas–Peucker: reduz uma polilinha mantendo o formato (tolerância em px).
 * Usado para o laço (segue o mouse) não virar milhares de pontos.
 */
export function simplifyPath(
  points: SicroImagePoint[],
  tolerance: number,
): SicroImagePoint[] {
  if (points.length <= 2) return points.slice();
  const first = points[0];
  const last = points[points.length - 1];
  if (!first || !last) return points.slice();
  let maxDist = 0;
  let idx = 0;
  for (let i = 1; i < points.length - 1; i++) {
    const pt = points[i];
    if (!pt) continue;
    const d = perpDistance(pt, first, last);
    if (d > maxDist) {
      maxDist = d;
      idx = i;
    }
  }
  if (maxDist > tolerance) {
    const left = simplifyPath(points.slice(0, idx + 1), tolerance);
    const right = simplifyPath(points.slice(idx), tolerance);
    return left.slice(0, -1).concat(right);
  }
  return [first, last];
}

/** Área (px²) de uma seleção — rect, elipse (π·a·b) ou polígono (shoelace). */
export function selectionArea(sel: SicroImageSelection): number {
  if (sel.kind === "polygon") {
    const pts = sel.points ?? [];
    if (pts.length < 3) return 0;
    let acc = 0;
    for (let i = 0, j = pts.length - 1; i < pts.length; j = i++) {
      const pi = pts[i];
      const pj = pts[j];
      if (!pi || !pj) continue;
      acc += pj.x * pi.y - pi.x * pj.y;
    }
    return Math.abs(acc) / 2;
  }
  const w = sel.width ?? 0;
  const h = sel.height ?? 0;
  if (sel.kind === "ellipse") return Math.PI * (w / 2) * (h / 2);
  return w * h;
}

/** Polígono fechado (em px de imagem) que aproxima a seleção, p/ desenho/medida.
 * Elipse vira polígono de `segments` lados. */
export function selectionToPolygon(
  sel: SicroImageSelection,
  segments = 48,
): SicroImagePoint[] {
  if (sel.kind === "polygon") return (sel.points ?? []).slice();
  const x = sel.x ?? 0;
  const y = sel.y ?? 0;
  const w = sel.width ?? 0;
  const h = sel.height ?? 0;
  if (sel.kind === "rect") {
    return [
      { x, y },
      { x: x + w, y },
      { x: x + w, y: y + h },
      { x, y: y + h },
    ];
  }
  // ellipse
  const cx = x + w / 2;
  const cy = y + h / 2;
  const rx = w / 2;
  const ry = h / 2;
  const out: SicroImagePoint[] = [];
  for (let i = 0; i < segments; i++) {
    const a = (i / segments) * Math.PI * 2;
    out.push({ x: cx + Math.cos(a) * rx, y: cy + Math.sin(a) * ry });
  }
  return out;
}
