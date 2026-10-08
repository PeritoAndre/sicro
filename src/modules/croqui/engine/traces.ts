/**
 * Vestígios paramétricos (frenagem, derrapagem, arrasto…): traçado p0 → p1 com
 * flecha, em px de mundo; larguras e raios em metros. Sem React/Konva.
 */

import type { SicroPoint, SicroTraceObject, TraceSubtype } from "./schema";

export type TraceParamValue = number | string | boolean;

/** `when`: só aparece no Inspector quando o parâmetro `[0]` vale um de `[1]`. */
type ParamWhen = { when?: [string, TraceParamValue[]] };
export type TraceParamSpec = ParamWhen &
  (
    | { k: string; t: "range"; label: string; min: number; max: number; step: number; u: "m" | "%" | "°" | "" | "/m²" | "km/h" }
    | { k: string; t: "seg"; label: string; opts: [TraceParamValue, string][] }
    | { k: string; t: "chk"; label: string }
    | { k: string; t: "txt"; label: string; max?: number }
  );

export interface TraceSpec {
  nome: string;
  sigla: string;
  /** Comprimento ao soltar da prateleira ou clicar sem arrastar (m). */
  len_m: number;
  /** Flecha inicial (m). */
  bend_m: number;
  params: TraceParamSpec[];
  def: Record<string, TraceParamValue>;
}

/** Flecha aparece no Inspector como parâmetro "bend" (m); no objeto é `bend` em px. */
const BEND = (label: string, lim: number): TraceParamSpec => ({ k: "bend", t: "range", label, min: -lim, max: lim, step: 0.05, u: "m" });
const MU: TraceParamSpec = { k: "mu", t: "range", label: "Atrito do pavimento (μ)", min: 0.3, max: 1, step: 0.05, u: "" };

export const TRACE_FLUIDOS: Record<string, [string, string]> = {
  oleo: ["Óleo", "#24272d"],
  arref: ["Arrefecimento", "#2d7a66"],
  comb: ["Combustível", "#6f6230"],
  sangue: ["Sangue", "#7a1414"],
};

export const TRACE_SPECS: Record<TraceSubtype, TraceSpec> = {
  frenagem: {
    nome: "Frenagem", sigla: "F", len_m: 14, bend_m: 0,
    params: [
      { k: "rodas", t: "seg", label: "Marcas", opts: [[1, "1 · moto"], [2, "2 · um eixo"], [4, "4 · dois eixos"]] },
      { k: "bitola", t: "range", label: "Bitola", min: 0.8, max: 2.2, step: 0.05, u: "m" },
      { k: "pneu", t: "range", label: "Largura do pneu", min: 0.08, max: 0.35, step: 0.01, u: "m" },
      { k: "sombra", t: "range", label: "Início em sombra", min: 0, max: 0.4, step: 0.01, u: "%" },
      { k: "int", t: "range", label: "Intensidade", min: 0.3, max: 1, step: 0.05, u: "%" },
      BEND("Curvatura (flecha)", 4),
      { k: "abs", t: "chk", label: "ABS: marcas intermitentes e tênues" },
      MU,
    ],
    def: { rodas: 2, bitola: 1.5, pneu: 0.2, sombra: 0.15, int: 0.85, abs: false, mu: 0.7 },
  },
  derrapagem: {
    nome: "Derrapagem em curva", sigla: "D", len_m: 20, bend_m: -1.5,
    params: [
      BEND("Nível da curva (flecha)", 8),
      { k: "abre", t: "range", label: "Abertura entre as marcas", min: 0, max: 2.5, step: 0.05, u: "m" },
      { k: "estria", t: "range", label: "Ângulo das estrias", min: 10, max: 75, step: 1, u: "°" },
      { k: "pneu", t: "range", label: "Largura do pneu", min: 0.1, max: 0.35, step: 0.01, u: "m" },
      { k: "sombra", t: "range", label: "Início em sombra", min: 0, max: 0.4, step: 0.01, u: "%" },
      { k: "int", t: "range", label: "Intensidade", min: 0.3, max: 1, step: 0.05, u: "%" },
      MU,
    ],
    def: { abre: 1.2, estria: 35, pneu: 0.22, sombra: 0.2, int: 0.8, mu: 0.7 },
  },
  arrasto: {
    nome: "Arrasto", sigla: "A", len_m: 6, bend_m: 0,
    params: [
      { k: "larg", t: "range", label: "Largura da faixa", min: 0.2, max: 1.6, step: 0.05, u: "m" },
      { k: "riscos", t: "range", label: "Riscos", min: 4, max: 28, step: 1, u: "" },
      { k: "int", t: "range", label: "Intensidade", min: 0.3, max: 1, step: 0.05, u: "%" },
      BEND("Curvatura (flecha)", 3),
    ],
    def: { larg: 0.7, riscos: 14, int: 0.75 },
  },
  sulcagem: {
    nome: "Sulcagem", sigla: "S", len_m: 1.4, bend_m: 0,
    params: [
      { k: "larg", t: "range", label: "Largura do sulco", min: 0.04, max: 0.4, step: 0.01, u: "m" },
      { k: "int", t: "range", label: "Profundidade (tom)", min: 0.4, max: 1, step: 0.05, u: "%" },
      { k: "lascas", t: "chk", label: "Lascas de pavimento em volta" },
    ],
    def: { larg: 0.14, int: 0.95, lascas: true },
  },
  ranhura: {
    nome: "Ranhuras", sigla: "R", len_m: 5, bend_m: 0,
    params: [
      { k: "linhas", t: "range", label: "Linhas", min: 2, max: 14, step: 1, u: "" },
      { k: "esp", t: "range", label: "Espalhamento", min: 0.1, max: 1.5, step: 0.05, u: "m" },
      { k: "onda", t: "range", label: "Ondulação", min: 0, max: 0.3, step: 0.01, u: "m" },
      { k: "int", t: "range", label: "Intensidade", min: 0.3, max: 1, step: 0.05, u: "%" },
      BEND("Curvatura (flecha)", 3),
    ],
    def: { linhas: 6, esp: 0.5, onda: 0.04, int: 0.8 },
  },
  fluido: {
    nome: "Fluido", sigla: "L", len_m: 2.5, bend_m: 0.3,
    params: [
      { k: "tipo", t: "seg", label: "Tipo", opts: Object.entries(TRACE_FLUIDOS).map(([k, v]) => [k, v[0]]) },
      { k: "raio", t: "range", label: "Raio da poça", min: 0.2, max: 3, step: 0.05, u: "m" },
      BEND("Curvatura do escorrimento", 2),
      { k: "trilha", t: "chk", label: "Escorrimento contínuo" },
      { k: "gotas", t: "chk", label: "Gotas ao longo do trajeto" },
    ],
    def: { tipo: "oleo", raio: 0.9, trilha: true, gotas: true },
  },
  fragmentos: {
    nome: "Fragmentos", sigla: "Fr", len_m: 8, bend_m: 0,
    params: [
      { k: "tipo", t: "seg", label: "Material", opts: [["vidro", "Vidro"], ["plast", "Plástico"], ["misto", "Misto"]] },
      { k: "abert", t: "range", label: "Abertura do leque", min: 10, max: 120, step: 1, u: "°" },
      { k: "dens", t: "range", label: "Densidade", min: 1, max: 40, step: 1, u: "/m²" },
      { k: "cont", t: "chk", label: "Contorno da área" },
    ],
    def: { tipo: "misto", abert: 50, dens: 10, cont: true },
  },
  colisao: {
    nome: "Ponto de colisão", sigla: "PC", len_m: 0, bend_m: 0,
    params: [
      { k: "forma", t: "seg", label: "Forma", opts: [["ponto", "Ponto"], ["area", "Área"]] },
      { k: "raio", t: "range", label: "Raio", min: 0.3, max: 4, step: 0.05, u: "m" },
    ],
    def: { forma: "ponto", raio: 0.6 },
  },
};

export const TRACE_SUBTYPES = Object.keys(TRACE_SPECS) as TraceSubtype[];

export function isTraceSubtype(v: string): v is TraceSubtype {
  return v in TRACE_SPECS;
}

/** Valor do parâmetro com o padrão do tipo. */
export function tp<T extends TraceParamValue>(o: SicroTraceObject, k: string): T {
  const v = o.params?.[k];
  return (v === undefined ? TRACE_SPECS[o.subtype]?.def[k] : v) as T;
}
export const tn = (o: SicroTraceObject, k: string): number => Number(tp(o, k)) || 0;

function newSeed(): number {
  return Math.floor(Math.random() * 1e6) + 1;
}

/** `p0`/`p1` em px de mundo; sem rótulo e sem cota (decisão do dono). */
export function makeTrace(subtype: TraceSubtype, p0: SicroPoint, p1: SicroPoint, pxPerM: number): SicroTraceObject {
  const spec = TRACE_SPECS[subtype];
  return {
    id: `trace_${typeof crypto !== "undefined" && "randomUUID" in crypto ? crypto.randomUUID() : `${Date.now().toString(36)}${Math.random().toString(36).slice(2)}`}`,
    layer_id: "layer_objects",
    kind: "trace",
    subtype,
    p0: { ...p0 },
    p1: subtype === "colisao" ? { ...p0 } : { ...p1 },
    bend: spec.bend_m * pxPerM,
    seed: newSeed(),
    params: { ...spec.def },
    label: "",
    show_measure: false,
    visible: true,
    locked: false,
    category: "vestigios",
  };
}

// ---------------------------------------------------------------------------
// Geometria em metros: Bézier quadrática cuja metade passa a `bend` da corda.

export interface TracePt { x: number; y: number; s: number; tx: number; ty: number; nx: number; ny: number }
export interface TraceGeom {
  pts: TracePt[];
  len: number;
  chord: number;
  /** Normal unitária da corda (lado positivo da flecha). */
  cn: SicroPoint;
  /** Meio da corda e ponto do traçado no meio (alça da flecha). */
  cm: SicroPoint;
  mid: SicroPoint;
}

export function traceGeom(p0: SicroPoint, p1: SicroPoint, bend: number): TraceGeom {
  const dx = p1.x - p0.x;
  const dy = p1.y - p0.y;
  const C = Math.hypot(dx, dy) || 1e-6;
  const nx = -dy / C;
  const ny = dx / C;
  const mx = (p0.x + p1.x) / 2;
  const my = (p0.y + p1.y) / 2;
  const c = { x: mx + nx * 2 * bend, y: my + ny * 2 * bend };
  const n = Math.max(24, Math.min(400, Math.ceil(C * 4)));
  const pts: TracePt[] = [];
  let s = 0;
  let prev: SicroPoint | null = null;
  for (let i = 0; i <= n; i++) {
    const t = i / n;
    const u = 1 - t;
    const x = u * u * p0.x + 2 * u * t * c.x + t * t * p1.x;
    const y = u * u * p0.y + 2 * u * t * c.y + t * t * p1.y;
    let tx = 2 * u * (c.x - p0.x) + 2 * t * (p1.x - c.x);
    let ty = 2 * u * (c.y - p0.y) + 2 * t * (p1.y - c.y);
    const tl = Math.hypot(tx, ty) || 1;
    tx /= tl;
    ty /= tl;
    if (prev) s += Math.hypot(x - prev.x, y - prev.y);
    pts.push({ x, y, s, tx, ty, nx: -ty, ny: tx });
    prev = { x, y };
  }
  return { pts, len: s, chord: C, cn: { x: nx, y: ny }, cm: { x: mx, y: my }, mid: { x: mx + nx * bend, y: my + ny * bend } };
}

/** Geometria do objeto convertida para metros. */
export function traceGeomM(o: SicroTraceObject, pxPerM: number): TraceGeom {
  const k = 1 / Math.max(pxPerM, 1e-6);
  return traceGeom({ x: o.p0.x * k, y: o.p0.y * k }, { x: o.p1.x * k, y: o.p1.y * k }, (o.bend ?? 0) * k);
}

export function traceAt(g: TraceGeom, s: number): TracePt {
  const P = g.pts;
  if (s <= 0) return P[0]!;
  if (s >= g.len) return P[P.length - 1]!;
  let lo = 0;
  let hi = P.length - 1;
  while (hi - lo > 1) {
    const m = (lo + hi) >> 1;
    if (P[m]!.s < s) lo = m;
    else hi = m;
  }
  const a = P[lo]!;
  const b = P[hi]!;
  const f = (s - a.s) / (b.s - a.s || 1);
  const tx = a.tx + (b.tx - a.tx) * f;
  const ty = a.ty + (b.ty - a.ty) * f;
  const tl = Math.hypot(tx, ty) || 1;
  return { x: a.x + (b.x - a.x) * f, y: a.y + (b.y - a.y) * f, s, tx: tx / tl, ty: ty / tl, nx: -ty / tl, ny: tx / tl };
}

export function traceSteps(s0: number, s1: number, d = 0.12): number[] {
  const n = Math.max(2, Math.min(2000, Math.ceil((s1 - s0) / d)));
  const out: number[] = [];
  for (let i = 0; i <= n; i++) out.push(s0 + ((s1 - s0) * i) / n);
  return out;
}

/** Leque dos fragmentos (m): alcance, direção e meia abertura. */
export function fanGeom(o: SicroTraceObject, pxPerM: number): { D: number; ang: number; half: number } {
  const dx = (o.p1.x - o.p0.x) / pxPerM;
  const dy = (o.p1.y - o.p0.y) / pxPerM;
  return { D: Math.hypot(dx, dy), ang: Math.atan2(dy, dx), half: (tn(o, "abert") * Math.PI) / 360 };
}

/** Meia largura lateral ocupada pelo vestígio (m), para seleção e cota. */
export function traceSpanM(o: SicroTraceObject): number {
  switch (o.subtype) {
    case "frenagem":
      return (tn(o, "rodas") === 1 ? 0 : tn(o, "bitola") / 2) + tn(o, "pneu");
    case "derrapagem":
      return tn(o, "abre") + tn(o, "pneu") * 2;
    case "arrasto":
      return tn(o, "larg") / 2;
    case "ranhura":
      return tn(o, "esp") / 2;
    case "sulcagem":
      return tn(o, "larg");
    default:
      return 0.2;
  }
}

export const ENTRE_EIXOS_M = 2.6;
const G = 9.81;

const fmt = (v: number, d = 1) => v.toFixed(d).replace(".", ",");

/** Medidas e contas de referência do Inspector (nunca vão para o PNG). */
export function traceReadouts(o: SicroTraceObject, pxPerM: number): { rows: [string, string][]; formula?: string } {
  if (o.subtype === "colisao") return { rows: [] };
  if (o.subtype === "fragmentos") {
    const f = fanGeom(o, pxPerM);
    return { rows: [["Alcance", `${fmt(f.D)} m`], ["Área do leque", `${fmt(f.half * f.D * f.D)} m²`]] };
  }
  const g = traceGeomM(o, pxPerM);
  const mu = tn(o, "mu");
  if (o.subtype === "frenagem") {
    const d = frenagemDistM(o, g.len);
    const rows: [string, string][] = [["Comprimento das marcas", `${fmt(g.len)} m`]];
    if (tn(o, "rodas") === 4) rows.push(["Descontado o entre-eixos", `${fmt(d)} m`]);
    rows.push(["Velocidade mínima de referência", `${fmt(Math.sqrt(2 * mu * G * d) * 3.6, 0)} km/h`]);
    return { rows, formula: `v = √(2·μ·g·d), μ = ${fmt(mu, 2)}` };
  }
  if (o.subtype === "derrapagem") {
    const C = g.chord;
    const M = Math.abs((o.bend ?? 0) / pxPerM);
    const R = curveRadiusM(C, M);
    const v = Number.isFinite(R) ? Math.sqrt(mu * G * R) * 3.6 : NaN;
    return {
      rows: [
        ["Corda (C)", `${fmt(C)} m`],
        ["Flecha (M)", `${fmt(M, 2)} m`],
        ["Raio", Number.isFinite(R) ? `${fmt(R)} m` : "reta"],
        ["Velocidade crítica de referência", Number.isFinite(v) ? `${fmt(v, 0)} km/h` : "—"],
      ],
      formula: `R = C²/8M + M/2 · v = √(μ·g·R), μ = ${fmt(mu, 2)}`,
    };
  }
  if (o.subtype === "fluido") {
    const r = tn(o, "raio");
    return { rows: [["Escorrimento", `${fmt(g.len)} m`], ["Área da poça ≈", `${fmt(Math.PI * r * r * 1.1)} m²`]] };
  }
  return { rows: [["Comprimento", `${fmt(g.len)} m`]] };
}

/** Distância de frenagem: com 4 marcas, desconta o entre-eixos (marcas traseiras e dianteiras sobrepostas). */
export function frenagemDistM(o: SicroTraceObject, lenM: number): number {
  return tn(o, "rodas") === 4 ? Math.max(0, lenM - Math.min(ENTRE_EIXOS_M, lenM * 0.45)) : lenM;
}

/** Raio pela corda e flecha; reta (∞) com flecha ~0. */
export function curveRadiusM(C: number, M: number): number {
  return M > 0.01 ? (C * C) / (8 * M) + M / 2 : Infinity;
}

/** AABB em px de mundo. */
export function traceBoundsPx(o: SicroTraceObject, pxPerM: number): { x: number; y: number; width: number; height: number } {
  if (o.subtype === "colisao") {
    const r = tn(o, "raio") * pxPerM * 1.25;
    return { x: o.p0.x - r, y: o.p0.y - r, width: 2 * r, height: 2 * r };
  }
  const g = traceGeomM(o, pxPerM);
  const pad = (traceSpanM(o) + (o.subtype === "fluido" ? tn(o, "raio") * 1.3 : 0)) * pxPerM;
  let x0 = Infinity;
  let y0 = Infinity;
  let x1 = -Infinity;
  let y1 = -Infinity;
  for (const p of g.pts) {
    x0 = Math.min(x0, p.x);
    y0 = Math.min(y0, p.y);
    x1 = Math.max(x1, p.x);
    y1 = Math.max(y1, p.y);
  }
  if (o.subtype === "fragmentos") {
    const f = fanGeom(o, pxPerM);
    for (const a of [f.ang - f.half, f.ang, f.ang + f.half]) {
      const x = o.p0.x / pxPerM + Math.cos(a) * f.D;
      const y = o.p0.y / pxPerM + Math.sin(a) * f.D;
      x0 = Math.min(x0, x);
      y0 = Math.min(y0, y);
      x1 = Math.max(x1, x);
      y1 = Math.max(y1, y);
    }
  }
  return { x: x0 * pxPerM - pad, y: y0 * pxPerM - pad, width: (x1 - x0) * pxPerM + 2 * pad, height: (y1 - y0) * pxPerM + 2 * pad };
}
