/** Ímã do ponteiro: canto existente > parede > alinhamento com cantos > passo de 5 cm. Shift trava 0/45/90°. */
import { add, dist, mul, norm, projectOnSegment, sub, type Pt } from "../model/geom";
import { findNode, findWall } from "../model/walls";
import type { SicroPlantaDoc } from "../model/schema";

export interface SnapGuide {
  a: Pt;
  b: Pt;
}
export interface Snapped {
  p: Pt;
  kind: "node" | "wall" | "align" | "grid" | "free";
  guides: SnapGuide[];
}

export const STEP = 0.05;
const round = (v: number, s = STEP) => Math.round(v / s) * s;

/** `pxTol` em pixels de tela; `pxPerM` = escala atual. */
export function snapPoint(
  doc: SicroPlantaDoc,
  raw: Pt,
  opt: { pxPerM: number; magnet: boolean; from?: Pt | null; ortho?: boolean; exceptNode?: string },
): Snapped {
  const tol = 12 / opt.pxPerM;
  let p = { ...raw };
  // Ângulo travado a partir do ponto anterior.
  if (opt.ortho && opt.from) {
    const v = sub(raw, opt.from);
    const ang = Math.round(Math.atan2(v.y, v.x) / (Math.PI / 4)) * (Math.PI / 4);
    const d = { x: Math.cos(ang), y: Math.sin(ang) };
    const l = Math.max(0, v.x * d.x + v.y * d.y);
    p = add(opt.from, mul(d, opt.magnet ? round(l) : l));
    if (opt.magnet) {
      const n = findNode(doc, p, tol, opt.exceptNode);
      if (n && Math.abs(norm(sub(n, opt.from)).x * d.y - norm(sub(n, opt.from)).y * d.x) < 0.02) return { p: { x: n.x, y: n.y }, kind: "node", guides: [] };
    }
    return { p, kind: "grid", guides: [] };
  }
  if (!opt.magnet) return { p, kind: "free", guides: [] };
  const n = findNode(doc, raw, tol, opt.exceptNode);
  if (n) return { p: { x: n.x, y: n.y }, kind: "node", guides: [] };
  const w = findWall(doc, raw, tol, opt.exceptNode);
  if (w) {
    // Na parede, o ponto anda em passos de 5 cm a partir da ponta a.
    const nodes = new Map(doc.nodes.map((x) => [x.id, x]));
    const A = nodes.get(w.wall.a)!;
    const B = nodes.get(w.wall.b)!;
    const L = dist(A, B);
    const t = Math.max(0, Math.min(1, round(w.t * L) / L));
    const pr = projectOnSegment({ x: A.x + (B.x - A.x) * t, y: A.y + (B.y - A.y) * t }, A, B);
    return { p: pr.point, kind: "wall", guides: [] };
  }
  // Alinhamento com cantos existentes (linhas-guia).
  const guides: SnapGuide[] = [];
  let ax: number | null = null;
  let ay: number | null = null;
  let bx = tol;
  let by = tol;
  let gx: Pt | null = null;
  let gy: Pt | null = null;
  for (const node of doc.nodes) {
    if (node.id === opt.exceptNode) continue;
    const dx = Math.abs(node.x - raw.x);
    const dy = Math.abs(node.y - raw.y);
    if (dx < bx) {
      bx = dx;
      ax = node.x;
      gx = node;
    }
    if (dy < by) {
      by = dy;
      ay = node.y;
      gy = node;
    }
  }
  p = { x: ax ?? round(raw.x), y: ay ?? round(raw.y) };
  if (gx) guides.push({ a: { x: gx.x, y: gx.y }, b: { x: gx.x, y: p.y } });
  if (gy) guides.push({ a: { x: gy.x, y: gy.y }, b: { x: p.x, y: gy.y } });
  return { p, kind: guides.length ? "align" : "grid", guides };
}
