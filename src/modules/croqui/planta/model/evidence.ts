/** Vestígios: rótulos automáticos e medidas até as paredes (coordenadas retangulares) ou dois cantos. */
import { add, dist, dot, mul, norm, projectOnSegment, sub, type Pt } from "./geom";
import { nodeMap, wallGeometry } from "./walls";
import type { PEvidence, SicroPlantaDoc } from "./schema";

type Doc = SicroPlantaDoc;

export function labelFor(kind: "letra" | "numero", index: number): string {
  if (kind === "numero") return String(index + 1);
  let n = index;
  let s = "";
  do {
    s = String.fromCharCode(65 + (n % 26)) + s;
    n = Math.floor(n / 26) - 1;
  } while (n >= 0);
  return s;
}

/** Próximo rótulo livre no padrão escolhido. */
export function nextEvidenceLabel(doc: Doc): string {
  const used = new Set(doc.evidences.map((e) => e.label));
  for (let i = 0; i < 10000; i++) {
    const l = labelFor(doc.options.label_kind, i);
    if (!used.has(l)) return l;
  }
  return "?";
}

/** Renumera todos na ordem em que foram marcados (ao trocar letra/número). */
export function relabelAll(doc: Doc, kind: "letra" | "numero"): Doc {
  return {
    ...doc,
    options: { ...doc.options, label_kind: kind },
    evidences: doc.evidences.map((e, i) => ({ ...e, label: labelFor(kind, i) })),
  };
}

export interface MeasureLine {
  from: Pt;
  to: Pt;
  value: number;
  /** "parede norte", "canto 2"... */
  ref: string;
}

/** Direção de um vetor em relação ao norte da rosa dos ventos (graus, horário). */
export function cardinal(v: Pt, northDeg: number): "norte" | "leste" | "sul" | "oeste" {
  const a = (Math.atan2(v.x, -v.y) * 180) / Math.PI - northDeg;
  const k = (((Math.round(a / 90) % 4) + 4) % 4) as 0 | 1 | 2 | 3;
  return (["norte", "leste", "sul", "oeste"] as const)[k];
}

/** Duas faces de parede: a mais próxima e a mais próxima quase perpendicular a ela. */
export function wallMeasures(doc: Doc, p: Pt): MeasureLine[] {
  const geo = wallGeometry(doc);
  type Cand = { foot: Pt; d: number; dir: Pt };
  const cands: Cand[] = [];
  for (const g of geo.values()) {
    const [p0, p1, p2, p3] = g.poly as [Pt, Pt, Pt, Pt];
    for (const [a, b] of [
      [p0, p1],
      [p3, p2],
    ] as const) {
      const pr = projectOnSegment(p, a, b);
      if (pr.t < -0.02 || pr.t > 1.02) continue;
      cands.push({ foot: pr.point, d: pr.distance, dir: norm(sub(b, a)) });
    }
  }
  cands.sort((x, y) => x.d - y.d);
  const first = cands[0];
  if (!first) return [];
  const second = cands.find((c) => Math.abs(dot(c.dir, first.dir)) < 0.5);
  const deg = doc.options.compass.deg;
  return [first, second]
    .filter((c): c is Cand => !!c)
    .map((c) => ({ from: p, to: c.foot, value: c.d, ref: `parede ${cardinal(sub(c.foot, p), deg)}` }));
}

/** Triangulação: dois cantos escolhidos, ou os dois mais próximos. */
export function pointMeasures(doc: Doc, e: PEvidence): MeasureLine[] {
  const nodes = nodeMap(doc);
  const p = { x: e.x, y: e.y };
  let ids = e.points.filter((id) => nodes.has(id));
  if (ids.length < 2) {
    ids = [...doc.nodes]
      .sort((a, b) => dist(a, p) - dist(b, p))
      .filter((n, i, arr) => i === 0 || dist(n, arr[0]!) > 0.5)
      .slice(0, 2)
      .map((n) => n.id);
  }
  return ids.map((id, i) => {
    const n = nodes.get(id)!;
    return { from: p, to: { x: n.x, y: n.y }, value: dist(p, n), ref: `canto ${i + 1}` };
  });
}

export function evidenceMeasures(doc: Doc, e: PEvidence): MeasureLine[] {
  return e.measure === "pontos" ? pointMeasures(doc, e) : wallMeasures(doc, { x: e.x, y: e.y });
}

/** Texto da legenda: "2,00 m da parede norte · 1,67 m da parede leste". */
export function measureText(lines: MeasureLine[]): string {
  return lines.map((l) => `${l.value.toFixed(2).replace(".", ",")} m da ${l.ref}`).join(" · ");
}

export function midLabel(l: MeasureLine): Pt {
  return add(l.from, mul(sub(l.to, l.from), 0.5));
}
