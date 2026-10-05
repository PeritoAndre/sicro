/**
 * Símbolos de arquitetura (mobília, estrutura, externos, pessoas) como primitivas
 * normalizadas: u na largura, v na profundidade (v = 0 é o fundo, que encosta na parede).
 * Um traçador para canvas e outro para SVG usam as mesmas primitivas.
 */

type Fill = "paper" | "tone" | "ink" | "none" | "body";
export type Prim =
  | { t: "rect"; u0: number; v0: number; u1: number; v1: number; fill?: Fill; r?: number; dash?: boolean }
  | { t: "circle"; u: number; v: number; r: number; fill?: Fill; dash?: boolean }
  | { t: "ellipse"; u: number; v: number; ru: number; rv: number; fill?: Fill }
  | { t: "line"; pts: [number, number][]; dash?: boolean; closed?: boolean; fill?: Fill; w?: number }
  | { t: "text"; u: number; v: number; text: string; size: number };

export interface SymbolDef {
  id: string;
  label: string;
  group: string;
  w: number;
  d: number;
  prims: Prim[];
}

const R = (u0: number, v0: number, u1: number, v1: number, fill: Fill = "paper", extra: Partial<Extract<Prim, { t: "rect" }>> = {}): Prim => ({ t: "rect", u0, v0, u1, v1, fill, ...extra });
const C = (u: number, v: number, r: number, fill: Fill = "paper", dash = false): Prim => ({ t: "circle", u, v, r, fill, dash });
const L = (pts: [number, number][], dash = false, extra: Partial<Extract<Prim, { t: "line" }>> = {}): Prim => ({ t: "line", pts, dash, ...extra });
const E = (u: number, v: number, ru: number, rv: number, fill: Fill = "paper"): Prim => ({ t: "ellipse", u, v, ru, rv, fill });

function sofa(splits: number[]): Prim[] {
  return [R(0, 0, 1, 1), R(0.1, 0, 0.9, 0.26, "tone"), R(0, 0, 0.1, 1), R(0.9, 0, 1, 1), ...splits.map((u) => L([[u, 0.26], [u, 1]]))];
}

const steps = (n: number): Prim[] => Array.from({ length: n - 1 }, (_, i) => L([[0, (i + 1) / n], [1, (i + 1) / n]]));

export const SYMBOLS: SymbolDef[] = [
  // Sala
  { id: "sofa2", label: "Sofá 2 lugares", group: "Sala", w: 1.6, d: 0.85, prims: sofa([0.5]) },
  { id: "sofa3", label: "Sofá 3 lugares", group: "Sala", w: 2.1, d: 0.9, prims: sofa([0.367, 0.633]) },
  { id: "poltrona", label: "Poltrona", group: "Sala", w: 0.8, d: 0.8, prims: [R(0, 0, 1, 1), R(0.16, 0, 0.84, 0.26, "tone"), R(0, 0, 0.16, 1), R(0.84, 0, 1, 1)] },
  { id: "mesa_centro", label: "Mesa de centro", group: "Sala", w: 1.0, d: 0.5, prims: [R(0, 0, 1, 1, "paper", { r: 0.04 })] },
  { id: "rack", label: "Rack e TV", group: "Sala", w: 1.6, d: 0.45, prims: [R(0, 0, 1, 1), R(0.12, 0.12, 0.88, 0.32, "tone")] },
  { id: "estante", label: "Estante", group: "Sala", w: 1.2, d: 0.35, prims: [R(0, 0, 1, 1), L([[0, 0.5], [1, 0.5]], true)] },
  // Jantar
  {
    id: "mesa4",
    label: "Mesa 4 lugares",
    group: "Jantar",
    w: 1.2,
    d: 1.7,
    prims: [R(0.12, 0, 0.45, 0.24), R(0.55, 0, 0.88, 0.24), R(0.12, 0.76, 0.45, 1), R(0.55, 0.76, 0.88, 1), R(0, 0.26, 1, 0.74)],
  },
  {
    id: "mesa_redonda",
    label: "Mesa redonda",
    group: "Jantar",
    w: 1.6,
    d: 1.6,
    prims: [R(0.39, 0, 0.61, 0.16), R(0.39, 0.84, 0.61, 1), R(0, 0.39, 0.16, 0.61), R(0.84, 0.39, 1, 0.61), C(0.5, 0.5, 0.33)],
  },
  { id: "cadeira", label: "Cadeira", group: "Jantar", w: 0.45, d: 0.45, prims: [R(0, 0, 1, 1), R(0, 0, 1, 0.18, "tone")] },
  // Quarto
  {
    id: "cama_casal",
    label: "Cama de casal",
    group: "Quarto",
    w: 1.4,
    d: 1.9,
    prims: [R(0, 0, 1, 1), R(0.06, 0.04, 0.47, 0.18, "paper", { r: 0.04 }), R(0.53, 0.04, 0.94, 0.18, "paper", { r: 0.04 }), L([[0, 0.3], [1, 0.3]]), L([[0, 0.3], [0.18, 0.4], [1, 0.3]], false)],
  },
  {
    id: "cama_solteiro",
    label: "Cama de solteiro",
    group: "Quarto",
    w: 0.9,
    d: 1.9,
    prims: [R(0, 0, 1, 1), R(0.1, 0.04, 0.9, 0.18, "paper", { r: 0.04 }), L([[0, 0.3], [1, 0.3]]), L([[0, 0.3], [0.28, 0.4], [1, 0.3]])],
  },
  { id: "berco", label: "Berço", group: "Quarto", w: 0.7, d: 1.3, prims: [R(0, 0, 1, 1), R(0.08, 0.05, 0.92, 0.95, "none")] },
  { id: "guarda_roupa", label: "Guarda-roupa", group: "Quarto", w: 1.8, d: 0.6, prims: [R(0, 0, 1, 1), L([[0, 0.5], [1, 0.5]], true), L([[0.333, 0.5], [0.333, 1]]), L([[0.667, 0.5], [0.667, 1]])] },
  { id: "comoda", label: "Cômoda", group: "Quarto", w: 1.0, d: 0.5, prims: [R(0, 0, 1, 1), L([[0, 0.22], [1, 0.22]])] },
  { id: "criado", label: "Criado-mudo", group: "Quarto", w: 0.45, d: 0.4, prims: [R(0, 0, 1, 1)] },
  // Cozinha e serviço
  {
    id: "bancada_pia",
    label: "Bancada com pia",
    group: "Cozinha",
    w: 1.8,
    d: 0.6,
    prims: [R(0, 0, 1, 1), R(0.34, 0.14, 0.66, 0.86, "paper", { r: 0.05 }), R(0.37, 0.22, 0.63, 0.8, "none", { r: 0.04 }), C(0.5, 0.08, 0.05, "ink")],
  },
  { id: "fogao", label: "Fogão", group: "Cozinha", w: 0.55, d: 0.6, prims: [R(0, 0, 1, 1), C(0.28, 0.3, 0.16, "none"), C(0.72, 0.3, 0.16, "none"), C(0.28, 0.72, 0.16, "none"), C(0.72, 0.72, 0.16, "none")] },
  { id: "geladeira", label: "Geladeira", group: "Cozinha", w: 0.7, d: 0.7, prims: [R(0, 0, 1, 1), L([[0, 0.86], [1, 0.86]]), { t: "text", u: 0.5, v: 0.5, text: "GEL", size: 0.26 }] },
  { id: "armario", label: "Armário", group: "Cozinha", w: 1.2, d: 0.35, prims: [R(0, 0, 1, 1), L([[0, 0.5], [1, 0.5]], true)] },
  { id: "maquina", label: "Máquina de lavar", group: "Serviço", w: 0.6, d: 0.6, prims: [R(0, 0, 1, 1), C(0.5, 0.56, 0.32, "none")] },
  { id: "tanque", label: "Tanque", group: "Serviço", w: 0.6, d: 0.55, prims: [R(0, 0, 1, 1), R(0.12, 0.16, 0.88, 0.88, "none", { r: 0.04 }), C(0.5, 0.55, 0.05, "ink")] },
  // Banheiro
  { id: "vaso", label: "Vaso sanitário", group: "Banheiro", w: 0.4, d: 0.7, prims: [E(0.5, 0.62, 0.42, 0.36), R(0.04, 0, 0.96, 0.27)] },
  { id: "lavatorio", label: "Lavatório", group: "Banheiro", w: 0.5, d: 0.45, prims: [E(0.5, 0.56, 0.44, 0.42), R(0, 0, 1, 0.22), E(0.5, 0.6, 0.3, 0.26, "none")] },
  { id: "box", label: "Box com chuveiro", group: "Banheiro", w: 0.9, d: 0.9, prims: [R(0, 0, 1, 1), L([[0, 0], [1, 1]]), L([[1, 0], [0, 1]]), C(0.5, 0.5, 0.06, "paper")] },
  { id: "banheira", label: "Banheira", group: "Banheiro", w: 0.75, d: 1.7, prims: [R(0, 0, 1, 1), R(0.09, 0.06, 0.91, 0.94, "none", { r: 0.12 }), C(0.5, 0.12, 0.05, "ink")] },
  // Escritório
  { id: "escrivaninha", label: "Escrivaninha", group: "Escritório", w: 1.2, d: 0.6, prims: [R(0, 0, 1, 1)] },
  { id: "cadeira_escr", label: "Cadeira de escritório", group: "Escritório", w: 0.55, d: 0.55, prims: [C(0.5, 0.55, 0.42), R(0.14, 0.02, 0.86, 0.2, "tone", { r: 0.04 })] },
  // Estrutura
  {
    id: "escada",
    label: "Escada reta",
    group: "Estrutura",
    w: 0.9,
    d: 3.0,
    prims: [R(0, 0, 1, 1), ...steps(12), L([[0.5, 0.93], [0.5, 0.06]], false, { w: 1.4 }), L([[0.4, 0.12], [0.5, 0.05], [0.6, 0.12]], false, { w: 1.4 })],
  },
  { id: "pilar", label: "Pilar", group: "Estrutura", w: 0.25, d: 0.25, prims: [R(0, 0, 1, 1, "ink")] },
  { id: "pilar_redondo", label: "Pilar redondo", group: "Estrutura", w: 0.3, d: 0.3, prims: [C(0.5, 0.5, 0.5, "ink")] },
  // Externo
  {
    id: "carro",
    label: "Carro",
    group: "Externo",
    w: 1.8,
    d: 4.4,
    prims: [
      R(0.03, 0, 0.97, 1, "paper", { r: 0.32 }),
      R(0.12, 0.27, 0.88, 0.37, "tone", { r: 0.06 }),
      R(0.15, 0.74, 0.85, 0.82, "tone", { r: 0.05 }),
      L([[0.12, 0.37], [0.12, 0.74]]),
      L([[0.88, 0.37], [0.88, 0.74]]),
      R(0, 0.3, 0.05, 0.34, "ink"),
      R(0.95, 0.3, 1, 0.34, "ink"),
    ],
  },
  { id: "moto", label: "Moto", group: "Externo", w: 0.7, d: 2.0, prims: [E(0.5, 0.5, 0.2, 0.46), L([[0.08, 0.2], [0.92, 0.2]], false, { w: 1.6 }), C(0.5, 0.42, 0.3, "tone")] },
  {
    id: "arvore",
    label: "Árvore",
    group: "Externo",
    w: 2.0,
    d: 2.0,
    prims: [C(0.5, 0.5, 0.48, "none", true), C(0.5, 0.5, 0.05, "ink"), L([[0.5, 0.1], [0.5, 0.42]]), L([[0.5, 0.58], [0.5, 0.9]]), L([[0.1, 0.5], [0.42, 0.5]]), L([[0.58, 0.5], [0.9, 0.5]])],
  },
];

export const SYMBOL_BY_ID = new Map(SYMBOLS.map((s) => [s.id, s]));
export const SYMBOL_GROUPS = [...new Set(SYMBOLS.map((s) => s.group))];

/** Pessoas (vista de cima). Em pé: ombros e cabeça. Caída: corpo inteiro, cabeça em v = 0. */
export const PERSON_SIZE = { em_pe: { w: 0.6, d: 0.38 }, caido: { w: 0.62, d: 1.8 } } as const;
export const PERSON_PRIMS: Record<"em_pe" | "caido", Prim[]> = {
  em_pe: [E(0.5, 0.5, 0.5, 0.42, "body"), C(0.5, 0.5, 0.32, "body")],
  caido: [
    R(0.27, 0.62, 0.47, 1, "body", { r: 0.07 }),
    R(0.53, 0.62, 0.73, 1, "body", { r: 0.07 }),
    R(0.02, 0.16, 0.16, 0.5, "body", { r: 0.05 }),
    R(0.84, 0.16, 0.98, 0.5, "body", { r: 0.05 }),
    R(0.18, 0.12, 0.82, 0.64, "body", { r: 0.12 }),
    C(0.5, 0.065, 0.2, "body"),
  ],
};

export interface PrimStyle {
  ink: string;
  paper: string;
  tone: string;
  body: string;
  bodyInk: string;
  /** Espessura do traço em metros. */
  lw: number;
}

const fillOf = (f: Fill | undefined, s: PrimStyle) =>
  f === "none" ? null : f === "ink" ? s.ink : f === "tone" ? s.tone : f === "body" ? s.body : s.paper;

/** Traça as primitivas centradas na origem, num retângulo w × d (metros). */
export function drawPrims(ctx: CanvasRenderingContext2D, prims: Prim[], w: number, d: number, s: PrimStyle, isBody = false) {
  const X = (u: number) => (u - 0.5) * w;
  const Y = (v: number) => (v - 0.5) * d;
  const m = Math.min(w, d);
  ctx.lineJoin = "round";
  ctx.lineCap = "round";
  for (const p of prims) {
    const stroke = isBody ? s.bodyInk : s.ink;
    ctx.strokeStyle = stroke;
    ctx.lineWidth = s.lw;
    ctx.setLineDash([]);
    ctx.beginPath();
    if (p.t === "rect") {
      const x = X(p.u0);
      const y = Y(p.v0);
      const rw = X(p.u1) - x;
      const rh = Y(p.v1) - y;
      const r = Math.min((p.r ?? 0) * m, rw / 2, rh / 2);
      if (r > 0 && "roundRect" in ctx) (ctx as CanvasRenderingContext2D & { roundRect: (...a: number[]) => void }).roundRect(x, y, rw, rh, r);
      else ctx.rect(x, y, rw, rh);
      if (p.dash) ctx.setLineDash([s.lw * 4, s.lw * 3]);
    } else if (p.t === "circle") {
      ctx.arc(X(p.u), Y(p.v), p.r * m, 0, Math.PI * 2);
      if (p.dash) ctx.setLineDash([s.lw * 4, s.lw * 3]);
    } else if (p.t === "ellipse") {
      ctx.ellipse(X(p.u), Y(p.v), p.ru * w, p.rv * d, 0, 0, Math.PI * 2);
    } else if (p.t === "line") {
      p.pts.forEach(([u, v], i) => (i === 0 ? ctx.moveTo(X(u), Y(v)) : ctx.lineTo(X(u), Y(v))));
      if (p.closed) ctx.closePath();
      if (p.dash) ctx.setLineDash([s.lw * 4, s.lw * 3]);
      if (p.w) ctx.lineWidth = s.lw * p.w;
    } else {
      ctx.fillStyle = s.ink;
      ctx.font = `600 ${p.size * m}px "JetBrains Mono", monospace`;
      ctx.textAlign = "center";
      ctx.textBaseline = "middle";
      ctx.fillText(p.text, X(p.u), Y(p.v));
      continue;
    }
    const f = p.t === "line" ? (p.fill ? fillOf(p.fill, s) : null) : fillOf(p.fill, s);
    if (f) {
      ctx.fillStyle = f;
      ctx.fill();
    }
    ctx.stroke();
  }
  ctx.setLineDash([]);
}

/** Mesmas primitivas em SVG (miniaturas da paleta e exportação vetorial futura). */
export function primsSvg(prims: Prim[], w: number, d: number, s: PrimStyle, isBody = false): string {
  const X = (u: number) => +((u - 0.5) * w).toFixed(4);
  const Y = (v: number) => +((v - 0.5) * d).toFixed(4);
  const m = Math.min(w, d);
  const out: string[] = [];
  const stroke = isBody ? s.bodyInk : s.ink;
  const dash = `stroke-dasharray="${s.lw * 4} ${s.lw * 3}"`;
  for (const p of prims) {
    const st = `stroke="${stroke}" stroke-width="${s.lw}" stroke-linejoin="round" stroke-linecap="round"`;
    if (p.t === "rect") {
      const f = fillOf(p.fill, s) ?? "none";
      const r = Math.min((p.r ?? 0) * m, ((p.u1 - p.u0) * w) / 2, ((p.v1 - p.v0) * d) / 2);
      out.push(`<rect x="${X(p.u0)}" y="${Y(p.v0)}" width="${+((p.u1 - p.u0) * w).toFixed(4)}" height="${+((p.v1 - p.v0) * d).toFixed(4)}" rx="${+r.toFixed(4)}" fill="${f}" ${st} ${p.dash ? dash : ""}/>`);
    } else if (p.t === "circle") {
      out.push(`<circle cx="${X(p.u)}" cy="${Y(p.v)}" r="${+(p.r * m).toFixed(4)}" fill="${fillOf(p.fill, s) ?? "none"}" ${st} ${p.dash ? dash : ""}/>`);
    } else if (p.t === "ellipse") {
      out.push(`<ellipse cx="${X(p.u)}" cy="${Y(p.v)}" rx="${+(p.ru * w).toFixed(4)}" ry="${+(p.rv * d).toFixed(4)}" fill="${fillOf(p.fill, s) ?? "none"}" ${st}/>`);
    } else if (p.t === "line") {
      const pts = p.pts.map(([u, v]) => `${X(u)},${Y(v)}`).join(" ");
      out.push(`<${p.closed ? "polygon" : "polyline"} points="${pts}" fill="${p.fill ? fillOf(p.fill, s) ?? "none" : "none"}" ${st.replace(`stroke-width="${s.lw}"`, `stroke-width="${s.lw * (p.w ?? 1)}"`)} ${p.dash ? dash : ""}/>`);
    } else {
      out.push(`<text x="${X(p.u)}" y="${Y(p.v)}" font-size="${+(p.size * m).toFixed(4)}" font-family="JetBrains Mono, monospace" font-weight="600" text-anchor="middle" dominant-baseline="central" fill="${s.ink}">${p.text}</text>`);
    }
  }
  return out.join("");
}
