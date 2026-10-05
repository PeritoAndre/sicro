/** Cotas externas automáticas: por lado, das bordas de fora até os eixos das paredes internas que chegam lá. */
import { bbox, type Pt } from "./geom";
import { faces, offsetFace } from "./rooms";
import { nodeMap } from "./walls";
import type { SicroPlantaDoc } from "./schema";

export interface DimLine {
  a: Pt;
  b: Pt;
  /** Lado de fora para onde a cota se afasta (unitário). */
  out: Pt;
  /** Afastamento da linha de cota em relação a a/b (m). */
  offset: number;
  value: number;
}

export function autoDims(doc: SicroPlantaDoc, gap = 0.6, step = 0.5): DimLine[] {
  const outer = faces(doc).filter((f) => f.area < -1e-6);
  if (outer.length === 0) return [];
  const outlines = outer.map((f) => offsetFace(doc, f));
  const box = bbox(outlines.flat());
  if (!box) return [];
  const nodes = nodeMap(doc);
  const ring = [...new Set(outer.flatMap((f) => f.nodes))].map((id) => nodes.get(id)!).filter(Boolean);
  const tmax = Math.max(0.1, ...doc.walls.map((w) => w.thickness));
  const edges = outlines.flatMap((poly) => poly.map((p, i) => [p, poly[(i + 1) % poly.length]!] as const));
  const out: DimLine[] = [];
  const uniq = (vs: number[]) => [...vs].sort((x, y) => x - y).filter((v, i, a) => i === 0 || v - a[i - 1]! > 0.02);
  const chain = (ticks: number[], mk: (u: number) => Pt, o: Pt, off: number) => {
    for (let i = 0; i + 1 < ticks.length; i++) {
      const v = ticks[i + 1]! - ticks[i]!;
      if (v > 0.05) out.push({ a: mk(ticks[i]!), b: mk(ticks[i + 1]!), out: o, offset: off, value: v });
    }
  };
  // Cada lado: só os trechos do contorno que estão na borda extrema; marcas nos eixos das paredes que chegam ali.
  const side = (horizontal: boolean, extreme: number, o: Pt, total: boolean) => {
    const along = (p: Pt) => (horizontal ? p.x : p.y);
    const across = (p: Pt) => (horizontal ? p.y : p.x);
    const segs = edges.filter(([a, b]) => Math.abs(across(a) - extreme) < 0.02 && Math.abs(across(b) - extreme) < 0.02 && Math.abs(along(a) - along(b)) > 0.05);
    if (!segs.length) return;
    const lo = Math.min(...segs.flatMap(([a, b]) => [along(a), along(b)]));
    const hi = Math.max(...segs.flatMap(([a, b]) => [along(a), along(b)]));
    const inner = ring.filter((p) => Math.abs(across(p) - extreme) <= tmax * 0.75 + 1e-6 && along(p) > lo + tmax && along(p) < hi - tmax).map(along);
    const ticks = uniq([lo, hi, ...inner]);
    const mk = (u: number): Pt => (horizontal ? { x: u, y: extreme } : { x: extreme, y: u });
    chain(ticks, mk, o, gap);
    if (total && ticks.length > 2) chain([lo, hi], mk, o, gap + step);
  };
  side(true, box.y0, { x: 0, y: -1 }, true);
  side(true, box.y1, { x: 0, y: 1 }, false);
  side(false, box.x0, { x: -1, y: 0 }, true);
  side(false, box.x1, { x: 1, y: 0 }, false);
  return out;
}
