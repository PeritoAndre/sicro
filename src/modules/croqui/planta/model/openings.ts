/** Portas, janelas e vãos presos a uma parede: posição pelo eixo, encaixe e geometria do símbolo. */
import { add, dot, mul, projectOnSegment, sub, type Pt } from "./geom";
import { nodeMap, wallFrame, type WallGeom } from "./walls";
import { OPENING_DEFAULTS, uid, type OpeningKind, type POpening, type SicroPlantaDoc } from "./schema";

type Doc = SicroPlantaDoc;

/** Faixa livre da parede para aberturas (entre as quinas). */
export function freeSpan(g: WallGeom): { lo: number; hi: number } {
  return { lo: g.marginA, hi: g.frame.L - g.marginB };
}

export function clampT0(g: WallGeom, t0: number, width: number): number {
  const { lo, hi } = freeSpan(g);
  if (hi - lo <= width) return lo + Math.max(0, (hi - lo - width) / 2);
  return Math.max(lo, Math.min(hi - width, t0));
}

/** Retângulo do vão (atravessa a espessura toda). */
export function openingQuad(g: WallGeom, o: Pick<POpening, "t0" | "width">, thickness: number): Pt[] {
  const { A, dir, nrm } = g.frame;
  const s = add(A, mul(dir, o.t0));
  const e = add(A, mul(dir, o.t0 + o.width));
  const h = thickness / 2;
  return [add(s, mul(nrm, h)), add(e, mul(nrm, h)), add(e, mul(nrm, -h)), add(s, mul(nrm, -h))];
}

export interface WallSnap {
  wall: string;
  /** Distância pelo eixo, a partir do nó a. */
  along: number;
  /** De que lado do eixo está o ponteiro (1 = lado perp de a→b). */
  side: 1 | -1;
  distance: number;
}

/** Parede mais próxima do ponteiro (para encaixar uma abertura). */
export function snapToWall(doc: Doc, p: Pt, tol: number): WallSnap | null {
  const nodes = nodeMap(doc);
  let best: WallSnap | null = null;
  for (const w of doc.walls) {
    const f = wallFrame(w, nodes);
    if (!f || f.L < 0.2) continue;
    const pr = projectOnSegment(p, f.A, f.B);
    if (pr.t < 0 || pr.t > 1 || pr.distance > tol + w.thickness / 2) continue;
    if (!best || pr.distance < best.distance) {
      best = { wall: w.id, along: pr.t * f.L, side: dot(sub(p, f.A), f.nrm) >= 0 ? 1 : -1, distance: pr.distance };
    }
  }
  return best;
}

export function newOpening(kind: OpeningKind, wall: string, t0: number, side: 1 | -1): POpening {
  const d = OPENING_DEFAULTS[kind];
  return { id: uid("o"), wall, kind, t0, width: d.width, sill: d.sill, height: d.height, hinge: "start", side };
}

/** Distâncias livres do vão até as quinas (o que se mede com trena no local). */
export function openingGaps(g: WallGeom, o: Pick<POpening, "t0" | "width">): { before: number; after: number } {
  const { lo, hi } = freeSpan(g);
  return { before: Math.max(0, o.t0 - lo), after: Math.max(0, hi - (o.t0 + o.width)) };
}

/** Peças do desenho de cada tipo, em metros (o renderizador só traça). */
export interface OpeningDrawing {
  /** Retângulo branco que "corta" a parede. */
  gap: Pt[];
  lines: Pt[][];
  dashed: Pt[][];
  arcs: { c: Pt; r: number; a0: number; a1: number; ccw: boolean }[];
}

export function openingDrawing(g: WallGeom, o: POpening, thickness: number): OpeningDrawing {
  const { A, dir, nrm } = g.frame;
  const h = thickness / 2;
  const at = (t: number, k: number) => add(add(A, mul(dir, t)), mul(nrm, k));
  const s = o.t0;
  const e = o.t0 + o.width;
  const gap = openingQuad(g, o, thickness);
  const out: OpeningDrawing = { gap, lines: [], dashed: [], arcs: [] };
  const sideK = o.side; // lado para onde abre
  const face = h * sideK;
  const ang = (v: Pt) => Math.atan2(v.y, v.x);
  if (o.kind === "janela" || o.kind === "basculante") {
    out.lines.push([at(s, -h), at(e, -h)], [at(s, h), at(e, h)], [at(s, -h), at(s, h)], [at(e, -h), at(e, h)]);
    out.lines.push([at(s, -h / 3), at(e, -h / 3)], [at(s, h / 3), at(e, h / 3)]);
    if (o.kind === "basculante") out.lines.push([at(s, -h), at(e, h)]);
  } else if (o.kind === "vao") {
    out.dashed.push([at(s, -h), at(e, -h)], [at(s, h), at(e, h)]);
  } else if (o.kind === "correr") {
    const m = (s + e) / 2;
    const k = h * 0.35;
    out.lines.push([at(s, -k), at(m + 0.05, -k)], [at(m - 0.05, k), at(e, k)]);
    out.lines.push([at(s, -h), at(s, h)], [at(e, -h), at(e, h)]);
  } else {
    // Porta: folha perpendicular à parede a partir da dobradiça e arco até o outro batente.
    const leaves: { hinge: number; other: number; w: number }[] =
      o.kind === "porta_dupla"
        ? [
            { hinge: s, other: (s + e) / 2, w: o.width / 2 },
            { hinge: e, other: (s + e) / 2, w: o.width / 2 },
          ]
        : o.hinge === "start"
          ? [{ hinge: s, other: e, w: o.width }]
          : [{ hinge: e, other: s, w: o.width }];
    for (const l of leaves) {
      const hp = at(l.hinge, face);
      const tip = add(hp, mul(nrm, sideK * l.w));
      const op = at(l.other, face);
      out.lines.push([hp, tip]);
      const a0 = ang(sub(tip, hp));
      const a1 = ang(sub(op, hp));
      let d = a1 - a0;
      while (d > Math.PI) d -= 2 * Math.PI;
      while (d < -Math.PI) d += 2 * Math.PI;
      out.arcs.push({ c: hp, r: l.w, a0, a1, ccw: d < 0 });
    }
  }
  return out;
}

/** Vão no meio de um trecho, centrado no ponto do eixo. */
export function centeredT0(along: number, width: number) {
  return along - width / 2;
}

export function openingCenter(g: WallGeom, o: Pick<POpening, "t0" | "width">): Pt {
  return add(g.frame.A, mul(g.frame.dir, o.t0 + o.width / 2));
}
