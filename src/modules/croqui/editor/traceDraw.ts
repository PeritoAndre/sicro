/**
 * Desenho dos vestígios em canvas 2D (o mesmo da Bancada de Vestígios).
 * Tudo em metros: o chamador já está em px de mundo e aqui se escala por ppm.
 */

import {
  ENTRE_EIXOS_M,
  TRACE_FLUIDOS,
  armaCornersPx,
  armaFrame,
  armaLenM,
  fanGeom,
  tn,
  tp,
  traceAt,
  traceGeomM,
  traceSpanM,
  traceSteps,
  type SicroPoint,
  type SicroTraceObject,
  type TraceGeom,
} from "../engine";
import type { ParityTema, ParityVestigiosEstilo } from "../engine/road-parity";

export interface TraceDrawOpts {
  ppm: number;
  tema: ParityTema;
  finish: ParityVestigiosEstilo;
  asfalto: string;
}

interface Pal {
  marca: string;
  pc: string;
  traco: string;
  vidro: [string, string];
  plast: string;
  lasca: string;
  /** Sangue fresco e seco. */
  sangue: [string, string];
  /** Arma: metal, contorno, brilho, empunhadura, madeira, borda da madeira, latão, cobre, fundo dos vazios. */
  arma: { metal: string; borda: string; brilho: string; punho: string; madeira: string; madeiraBorda: string; latao: string; cobre: string; fundo: string };
}

const PALETTES: Record<ParityTema, Pal> = {
  tecnico: {
    marca: "#1d1d1d", pc: "#c62828", traco: "#111111", vidro: ["#d4e3ec", "#5f7f92"], plast: "#3a3a3a", lasca: "#8d8d8d",
    sangue: ["#b5121c", "#4b0f12"],
    arma: { metal: "#3a3f47", borda: "#15181c", brilho: "#8d959f", punho: "#232528", madeira: "#7a4a24", madeiraBorda: "#44260f", latao: "#b8963e", cobre: "#9a5e34", fundo: "#ffffff" },
  },
  pb: {
    marca: "#000000", pc: "#000000", traco: "#000000", vidro: ["#ffffff", "#000000"], plast: "#000000", lasca: "#555555",
    sangue: ["#000000", "#000000"],
    arma: { metal: "#ffffff", borda: "#000000", brilho: "#777777", punho: "#ffffff", madeira: "#ffffff", madeiraBorda: "#000000", latao: "#ffffff", cobre: "#ffffff", fundo: "#ffffff" },
  },
  escuro: {
    marca: "#000000", pc: "#ff5a4f", traco: "#f2f2f2", vidro: ["#bcd6e6", "#e8f2f8"], plast: "#9a9a9a", lasca: "#6d6d6d",
    sangue: ["#d8232e", "#6e171b"],
    arma: { metal: "#4a515b", borda: "#e6e6e6", brilho: "#a3abb5", punho: "#2b2e33", madeira: "#8a5a2b", madeiraBorda: "#e6e6e6", latao: "#c9a648", cobre: "#b06f3f", fundo: "#1c1c1c" },
  },
};

export function tracePalette(tema: ParityTema): Pal {
  return PALETTES[tema] ?? PALETTES.tecnico;
}

// ---------------------------------------------------------------------------
// Ruído determinístico pela semente: a forma não muda ao redesenhar.

function hash(n: number): number {
  const s = Math.sin(n * 127.1 + 311.7) * 43758.5453;
  return s - Math.floor(s);
}
function noise1(seed: number, x: number): number {
  const i = Math.floor(x);
  const f = x - i;
  const u = f * f * (3 - 2 * f);
  const a = hash(i + seed * 57.31);
  const b = hash(i + 1 + seed * 57.31);
  return a + (b - a) * u;
}
function fbm(seed: number, x: number): number {
  return (noise1(seed, x) * 0.6 + noise1(seed + 7, x * 2.3) * 0.3 + noise1(seed + 13, x * 5.1) * 0.1) * 2 - 1;
}
export function rng(seed: number): () => number {
  let s = (Math.imul(seed | 0, 2654435761) >>> 0) || 1;
  return () => {
    s ^= s << 13;
    s >>>= 0;
    s ^= s >>> 17;
    s ^= s << 5;
    s >>>= 0;
    return s / 4294967296;
  };
}
export function rgba(hex: string, a: number): string {
  const n = parseInt(hex.slice(1, 7), 16);
  return `rgba(${n >> 16},${(n >> 8) & 255},${n & 255},${Math.max(0, Math.min(1, a))})`;
}
/** Mistura linear de duas cores hex (t = 0 → a, 1 → b). */
export function mixHex(a: string, b: string, t: number): string {
  const A = parseInt(a.slice(1, 7), 16);
  const B = parseInt(b.slice(1, 7), 16);
  const k = Math.max(0, Math.min(1, t));
  const ch = (sh: number) => Math.round(((A >> sh) & 255) * (1 - k) + ((B >> sh) & 255) * k);
  return `#${((ch(16) << 16) | (ch(8) << 8) | ch(0)).toString(16).padStart(6, "0")}`;
}

// ---------------------------------------------------------------------------

type Ctx = CanvasRenderingContext2D;
type Fn = (s: number) => number;

function ribbon(g: TraceGeom, s0: number, s1: number, dFn: Fn, hwFn: Fn, jag: number, seed: number): SicroPoint[] {
  const L: SicroPoint[] = [];
  const R: SicroPoint[] = [];
  for (const s of traceSteps(s0, s1)) {
    const p = traceAt(g, s);
    const d = dFn(s);
    const hw = hwFn(s);
    const j1 = jag * fbm(seed, s * 2.7);
    const j2 = jag * fbm(seed + 5, s * 2.7);
    L.push({ x: p.x + p.nx * (d + hw + j1), y: p.y + p.ny * (d + hw + j1) });
    R.push({ x: p.x + p.nx * (d - hw + j2), y: p.y + p.ny * (d - hw + j2) });
  }
  return L.concat(R.reverse());
}

function offsetLine(g: TraceGeom, s0: number, s1: number, d: number, wob = 0, seed = 0, step = 0.12): SicroPoint[] {
  return traceSteps(s0, s1, step).map((s) => {
    const p = traceAt(g, s);
    const dd = d + (wob ? wob * fbm(seed, s * 1.3) : 0);
    return { x: p.x + p.nx * dd, y: p.y + p.ny * dd };
  });
}

function polyPath(ctx: Ctx, pts: SicroPoint[], close = true): void {
  ctx.beginPath();
  pts.forEach((p, i) => (i ? ctx.lineTo(p.x, p.y) : ctx.moveTo(p.x, p.y)));
  if (close) ctx.closePath();
}

const hatchCache = new Map<string, CanvasPattern | null>();
export function hatch(ctx: Ctx, fg: string, bg: string, pxM: number): CanvasPattern | string {
  const key = `${fg}${bg}${pxM.toFixed(5)}`;
  let p = hatchCache.get(key);
  if (p === undefined) {
    const c = document.createElement("canvas");
    c.width = c.height = 16;
    const x = c.getContext("2d");
    if (x) {
      x.fillStyle = bg;
      x.fillRect(0, 0, 16, 16);
      x.strokeStyle = fg;
      x.lineWidth = 1.5;
      x.beginPath();
      x.moveTo(0, 16);
      x.lineTo(16, 0);
      x.moveTo(-4, 4);
      x.lineTo(4, -4);
      x.moveTo(12, 20);
      x.lineTo(20, 12);
      x.stroke();
    }
    p = ctx.createPattern(c, "repeat");
    // 16 px do ladrilho ≈ 8 px de mundo.
    p?.setTransform(new DOMMatrix().scale(pxM * 0.5));
    if (hatchCache.size > 40) hatchCache.clear();
    hatchCache.set(key, p);
  }
  return p ?? fg;
}

interface Env {
  ctx: Ctx;
  pal: Pal;
  opts: TraceDrawOpts;
  /** 1 px de mundo em metros. */
  px: number;
  textura: boolean;
}

function absIntervals(seed: number, a: number, b: number): [number, number][] {
  const r = rng(seed + 99);
  const out: [number, number][] = [];
  let s = a;
  while (s < b) {
    const on = 0.8 + r() * 1.4;
    out.push([s, Math.min(b, s + on)]);
    s += on + 0.35 + r() * 0.7;
  }
  return out;
}

function estrias(env: Env, g: TraceGeom, s0: number, s1: number, dFn: Fn, hwFn: Fn, graus: number, step: number, color: string, lw: number): void {
  const { ctx } = env;
  const th = (graus * Math.PI) / 180;
  ctx.strokeStyle = color;
  ctx.lineWidth = lw;
  for (let s = s0; s <= s1; s += step) {
    const p = traceAt(g, s);
    const d = dFn(s);
    const h = (hwFn(s) * 2.4) / Math.max(Math.sin(th), 0.3);
    const ux = p.tx * Math.cos(th) + p.nx * Math.sin(th);
    const uy = p.ty * Math.cos(th) + p.ny * Math.sin(th);
    const cx = p.x + p.nx * d;
    const cy = p.y + p.ny * d;
    ctx.beginPath();
    ctx.moveTo(cx - ux * h, cy - uy * h);
    ctx.lineTo(cx + ux * h, cy + uy * h);
    ctx.stroke();
  }
}

/** Uma marca de pneu de `a` a `b` (m ao longo do traçado), deslocada `dFn` da linha central. */
function tireMark(env: Env, o: SicroTraceObject, g: TraceGeom, a: number, b: number, dFn: Fn, hwFn: Fn, estria?: number, alphaK = 1): void {
  if (b - a < 0.05) return;
  const { ctx, pal, textura } = env;
  const abs = !!tp(o, "abs");
  const I = tn(o, "int") * (abs ? 0.6 : 1) * alphaK;
  const sombra = tn(o, "sombra");
  const spans = abs ? absIntervals(o.seed, a, b) : [[a, b] as [number, number]];
  const shadowEnd = a + (b - a) * sombra;
  const pa = traceAt(g, a);
  const pb = traceAt(g, b);
  const grad = ctx.createLinearGradient(pa.x, pa.y, pb.x, pb.y);
  grad.addColorStop(0, rgba(pal.marca, I * 0.1));
  grad.addColorStop(Math.max(0.001, Math.min(0.999, sombra)), rgba(pal.marca, I * 0.82));
  grad.addColorStop(1, rgba(pal.marca, I * 0.9));
  const seed = o.seed + Math.round(dFn(a) * 10);
  for (const [s0, s1] of spans) {
    if (textura) {
      const poly = ribbon(g, s0, s1, dFn, hwFn, 0.012, seed);
      ctx.fillStyle = grad;
      polyPath(ctx, poly);
      ctx.fill();
      ctx.save();
      polyPath(ctx, poly);
      ctx.clip();
      // Sulcos e estrias proporcionais ao pneu: de perto continuam finos.
      const lw = Math.max(0.006, hwFn(s0) * 0.16);
      if (estria !== undefined) {
        estrias(env, g, s0, s1, dFn, hwFn, estria, 0.17, rgba(env.opts.asfalto, 0.45), lw);
      } else {
        ctx.strokeStyle = rgba(env.opts.asfalto, 0.45);
        ctx.lineWidth = lw;
        for (const k of [-0.42, -0.14, 0.14, 0.42]) {
          const line = traceSteps(s0, s1).map((s) => {
            const p = traceAt(g, s);
            const off = dFn(s) + hwFn(s) * 2 * k;
            return { x: p.x + p.nx * off, y: p.y + p.ny * off };
          });
          polyPath(ctx, line, false);
          ctx.stroke();
        }
      }
      ctx.restore();
    } else {
      const solid = Math.max(s0, Math.min(s1, shadowEnd));
      if (solid > s0) {
        ctx.strokeStyle = pal.marca;
        ctx.lineWidth = 0.9 * env.px;
        ctx.setLineDash([0.25, 0.18]);
        polyPath(ctx, ribbon(g, s0, solid, dFn, hwFn, 0, 0));
        ctx.stroke();
        ctx.setLineDash([]);
      }
      if (s1 > solid) {
        ctx.fillStyle = pal.marca;
        polyPath(ctx, ribbon(g, solid, s1, dFn, hwFn, 0, 0));
        ctx.fill();
      }
      if (estria !== undefined) {
        ctx.save();
        polyPath(ctx, ribbon(g, s0, s1, dFn, hwFn, 0, 0));
        ctx.clip();
        estrias(env, g, s0, s1, dFn, hwFn, estria, 0.3, env.opts.asfalto, Math.max(0.01, hwFn(s0) * 0.25));
        ctx.restore();
      }
    }
  }
}

function drawFrenagem(env: Env, o: SicroTraceObject, g: TraceGeom): void {
  const L = g.len;
  const b = tn(o, "bitola") / 2;
  const hw = () => tn(o, "pneu") / 2;
  const rodas = tn(o, "rodas");
  if (rodas === 1) return tireMark(env, o, g, 0, L, () => 0, hw);
  if (rodas !== 4) {
    for (const d of [-b, b]) tireMark(env, o, g, 0, L, () => d, hw);
    return;
  }
  const E = Math.min(ENTRE_EIXOS_M, L * 0.45);
  for (const d of [-b, b]) tireMark(env, o, g, E, L, () => d, hw);
  for (const d of [-b + 0.05, b - 0.05]) tireMark(env, o, g, 0, L - E, () => d, hw);
}

function drawDerrapagem(env: Env, o: SicroTraceObject, g: TraceGeom): void {
  const L = g.len || 1;
  const pneu = tn(o, "pneu");
  const hw = (s: number) => (pneu / 2) * (1 + (0.7 * s) / L);
  const estria = tn(o, "estria");
  tireMark(env, o, g, 0, L, () => 0, hw, estria);
  // Marca traseira: abre conforme o veículo gira.
  const dB = (s: number) => tn(o, "abre") * Math.pow(s / L, 1.6) + pneu * 1.2;
  tireMark(env, o, g, L * 0.06, L, dB, hw, estria + 10, 0.88);
}

function drawArrasto(env: Env, o: SicroTraceObject, g: TraceGeom): void {
  const { ctx, pal, textura } = env;
  const L = g.len;
  const w = tn(o, "larg") / 2;
  const I = tn(o, "int");
  const r = rng(o.seed);
  const poly = ribbon(g, 0, L, () => 0, (s) => w * (0.55 + 0.45 * Math.sin(Math.PI * Math.min(1, Math.max(0, s / L))) ** 0.4), textura ? 0.06 : 0, o.seed);
  ctx.fillStyle = rgba(pal.marca, I * (textura ? 0.16 : 0.1));
  polyPath(ctx, poly);
  ctx.fill();
  if (!textura) {
    ctx.strokeStyle = pal.marca;
    ctx.lineWidth = 0.9 * env.px;
    ctx.setLineDash([0.3, 0.2]);
    ctx.stroke();
    ctx.setLineDash([]);
  }
  ctx.strokeStyle = rgba(pal.marca, I * 0.7);
  const n = Math.round(tn(o, "riscos"));
  for (let i = 0; i < n; i++) {
    const d = (r() * 2 - 1) * w * 0.85;
    const a = r() * L * 0.35;
    const b = L - r() * L * 0.3;
    ctx.lineWidth = Math.max(0.01, 0.35 * env.px) * (0.6 + r() * 0.8);
    polyPath(ctx, offsetLine(g, a, b, d, 0.03, o.seed + i), false);
    ctx.stroke();
  }
}

function drawSulcagem(env: Env, o: SicroTraceObject, g: TraceGeom): void {
  const { ctx, pal, textura } = env;
  const L = g.len;
  const w = tn(o, "larg") / 2;
  const r = rng(o.seed);
  const hw = (s: number) => w * Math.pow(Math.max(0, 1 - s / L), 0.55) + 0.004;
  if (tp(o, "lascas") && textura) {
    for (let i = 0; i < 46; i++) {
      const s = r() * L * 0.7;
      const p = traceAt(g, s);
      const d = (r() * 2 - 1) * (w * 2.6 + 0.25 * (1 - s / L));
      const sz = 0.015 + r() * 0.05;
      ctx.fillStyle = r() < 0.5 ? rgba(pal.marca, 0.55) : rgba(pal.lasca, 0.7);
      ctx.save();
      ctx.translate(p.x + p.nx * d, p.y + p.ny * d);
      ctx.rotate(r() * 3);
      ctx.fillRect(-sz / 2, -sz / 2, sz, sz * (0.5 + r()));
      ctx.restore();
    }
  }
  ctx.fillStyle = rgba(pal.marca, tn(o, "int"));
  polyPath(ctx, ribbon(g, 0, L, () => 0, hw, textura ? 0.006 : 0, o.seed));
  ctx.fill();
  if (textura) {
    ctx.strokeStyle = rgba(pal.lasca, 0.55);
    ctx.lineWidth = Math.max(0.006, w * 0.18);
    polyPath(ctx, offsetLine(g, L * 0.05, L * 0.8, 0, 0.01, o.seed), false);
    ctx.stroke();
  }
}

function drawRanhura(env: Env, o: SicroTraceObject, g: TraceGeom): void {
  const { ctx, pal } = env;
  const L = g.len;
  const r = rng(o.seed);
  const n = Math.round(tn(o, "linhas"));
  const esp = tn(o, "esp");
  for (let i = 0; i < n; i++) {
    const d = (n === 1 ? 0 : i / (n - 1) - 0.5) * esp + (r() - 0.5) * 0.04;
    const a = r() * L * 0.25;
    const b = L - r() * L * 0.25;
    ctx.strokeStyle = rgba(pal.marca, tn(o, "int") * (0.55 + r() * 0.45));
    ctx.lineWidth = Math.max(0.012, 0.35 * env.px) * (0.6 + r() * 0.8);
    polyPath(ctx, offsetLine(g, a, b, d, tn(o, "onda"), o.seed + i * 3, 0.08), false);
    ctx.stroke();
  }
}

function blob(seed: number, c: SicroPoint, R: number, dir: number): SicroPoint[] {
  const r = rng(seed + 5);
  const ph = [r() * 6, r() * 6, r() * 6, r() * 6];
  const pts: SicroPoint[] = [];
  const ux = Math.cos(dir);
  const uy = Math.sin(dir);
  for (let i = 0; i < 72; i++) {
    const a = (i / 72) * Math.PI * 2;
    const k = 1 + 0.13 * Math.sin(2 * a + ph[0]!) + 0.09 * Math.sin(3 * a + ph[1]!) + 0.05 * Math.sin(5 * a + ph[2]!) + 0.035 * Math.sin(9 * a + ph[3]!);
    const along = Math.cos(a) * 1.22;
    const across = Math.sin(a) * 0.9;
    pts.push({ x: c.x + (along * ux - across * uy) * R * k, y: c.y + (along * uy + across * ux) * R * k });
  }
  return pts;
}

function drawFluido(env: Env, o: SicroTraceObject, g: TraceGeom): void {
  const { ctx, textura } = env;
  const pb = env.opts.tema === "pb";
  const tipo = String(tp(o, "tipo"));
  const col = pb ? "#000000" : (TRACE_FLUIDOS[tipo] ?? TRACE_FLUIDOS.oleo!)[1];
  const L = g.len;
  const R = tn(o, "raio");
  const r = rng(o.seed);
  const end = traceAt(g, L);
  const dir = Math.atan2(end.ty, end.tx);
  if (tp(o, "trilha") && L > 0.2) {
    ctx.fillStyle = rgba(col, pb ? 0.9 : 0.62);
    polyPath(ctx, ribbon(g, 0, Math.max(0, L - R * 0.6), () => 0, (s) => 0.03 + 0.04 * (s / L), textura ? 0.015 : 0, o.seed));
    ctx.fill();
  }
  if (tp(o, "gotas")) {
    for (let s = 0.15; s < L - R * 0.8; s += 0.22 + r() * 0.25) {
      const p = traceAt(g, s);
      const d = (r() - 0.5) * 0.35;
      const rr = 0.025 + r() * 0.06;
      ctx.beginPath();
      ctx.ellipse(p.x + p.nx * d, p.y + p.ny * d, rr * 1.3, rr, Math.atan2(p.ty, p.tx), 0, Math.PI * 2);
      ctx.fillStyle = rgba(col, 0.75);
      ctx.fill();
    }
  }
  const pool = blob(o.seed, end, R, dir);
  if (pb) {
    ctx.fillStyle = hatch(ctx, "#000000", "#ffffff", env.px);
    polyPath(ctx, pool);
    ctx.fill();
    ctx.strokeStyle = "#000000";
    ctx.lineWidth = 1.2 * env.px;
    ctx.stroke();
  } else if (textura) {
    const gr = ctx.createRadialGradient(end.x - R * 0.25, end.y - R * 0.25, R * 0.05, end.x, end.y, R * 1.3);
    gr.addColorStop(0, rgba(col, 0.45));
    gr.addColorStop(0.7, rgba(col, 0.7));
    gr.addColorStop(1, rgba(col, 0.82));
    ctx.fillStyle = gr;
    polyPath(ctx, pool);
    ctx.fill();
    ctx.strokeStyle = rgba(col, 0.95);
    ctx.lineWidth = Math.max(0.025, env.px);
    ctx.stroke();
    if (tipo === "oleo") {
      ctx.save();
      polyPath(ctx, pool);
      ctx.clip();
      ctx.strokeStyle = "rgba(255,255,255,0.10)";
      ctx.lineWidth = R * 0.12;
      ctx.beginPath();
      ctx.arc(end.x - R * 0.2, end.y - R * 0.25, R * 0.45, 3.6, 5.2);
      ctx.stroke();
      ctx.restore();
    }
    for (let i = 0; i < 5; i++) {
      const a = r() * Math.PI * 2;
      const d = R * (1.15 + r() * 0.35);
      ctx.beginPath();
      ctx.arc(end.x + Math.cos(a) * d, end.y + Math.sin(a) * d, R * (0.04 + r() * 0.07), 0, Math.PI * 2);
      ctx.fillStyle = rgba(col, 0.7);
      ctx.fill();
    }
  } else {
    ctx.fillStyle = rgba(col, 0.18);
    polyPath(ctx, pool);
    ctx.fill();
    ctx.strokeStyle = col;
    ctx.lineWidth = 1.2 * env.px;
    ctx.stroke();
  }
}


// ---------------------------------------------------------------------------
// Sangue: poça, gotejamento, projeção e esfregaço. Cor pela secagem (fresco → seco).

function sangueCor(env: Env, o: SicroTraceObject): string {
  if (env.opts.tema === "pb") return "#000000";
  const [fresco, seco] = env.pal.sangue;
  return mixHex(fresco, seco, tn(o, "tom"));
}

/** Gota vista de cima: elipse (alongada se em movimento) com coroa de satélites e, se alongada, a cauda à frente. */
function gota(env: Env, r: () => number, x: number, y: number, R: number, ang: number, elong: number, col: string, alpha: number): void {
  const { ctx, textura } = env;
  ctx.fillStyle = rgba(col, alpha);
  ctx.beginPath();
  ctx.ellipse(x, y, R * elong, R, ang, 0, Math.PI * 2);
  ctx.fill();
  // Detalhe só quando a gota tem alguns px: de longe seria ruído.
  if (!textura || R < env.px * 2.5) return;
  const ux = Math.cos(ang);
  const uy = Math.sin(ang);
  if (elong > 1.5) {
    // Cauda apontando para onde a gota ia.
    const d = R * elong * (1.25 + r() * 0.5);
    ctx.beginPath();
    ctx.arc(x + ux * d, y + uy * d, R * (0.18 + r() * 0.14), 0, Math.PI * 2);
    ctx.fill();
  }
  const n = 3 + Math.floor(r() * 4);
  for (let i = 0; i < n; i++) {
    const a = r() * Math.PI * 2;
    const d = R * (1.1 + r() * 0.35) * (1 + (elong - 1) * Math.abs(Math.cos(a - ang)));
    ctx.beginPath();
    ctx.arc(x + Math.cos(a) * d, y + Math.sin(a) * d, R * (0.1 + r() * 0.12), 0, Math.PI * 2);
    ctx.fill();
  }
}

function sanguePoca(env: Env, o: SicroTraceObject, g: TraceGeom, col: string): void {
  const { ctx, textura } = env;
  const pb = env.opts.tema === "pb";
  const L = g.len;
  const R = tn(o, "raio");
  const I = tn(o, "int");
  const tom = tn(o, "tom");
  const r = rng(o.seed);
  const end = traceAt(g, L);
  const dir = Math.atan2(end.ty, end.tx);
  const escuro = mixHex(col, "#000000", 0.35);
  if (tp(o, "trilha") && L > 0.2) {
    ctx.fillStyle = rgba(col, pb ? 0.9 : I * 0.78);
    polyPath(ctx, ribbon(g, 0, Math.max(0, L - R * 0.55), () => 0, (s) => 0.015 + 0.03 * (s / L), textura ? 0.012 : 0, o.seed));
    ctx.fill();
    for (let s = 0.1; s < L - R * 0.8; s += 0.18 + r() * 0.3) {
      const p = traceAt(g, s);
      const d = (r() - 0.5) * 0.25;
      gota(env, r, p.x + p.nx * d, p.y + p.ny * d, 0.012 + r() * 0.025, Math.atan2(p.ty, p.tx), 1.3, col, I * 0.85);
    }
  }
  const pool = blob(o.seed, end, R, dir);
  if (pb) {
    ctx.fillStyle = hatch(ctx, "#000000", "#ffffff", env.px);
    polyPath(ctx, pool);
    ctx.fill();
    ctx.strokeStyle = "#000000";
    ctx.lineWidth = 1.2 * env.px;
    ctx.stroke();
    return;
  }
  if (!textura) {
    ctx.fillStyle = rgba(col, 0.22);
    polyPath(ctx, pool);
    ctx.fill();
    ctx.strokeStyle = col;
    ctx.lineWidth = 1.2 * env.px;
    ctx.stroke();
    return;
  }
  // Halo de soro: sangue fresco separa um anel claro em volta.
  if (tom < 0.75) {
    ctx.fillStyle = rgba(col, 0.12 * (1 - tom) * I);
    polyPath(ctx, blob(o.seed + 1, end, R * 1.14, dir));
    ctx.fill();
  }
  const gr = ctx.createRadialGradient(end.x, end.y, R * 0.1, end.x, end.y, R * 1.25);
  gr.addColorStop(0, rgba(escuro, I * 0.96));
  gr.addColorStop(0.55, rgba(col, I * 0.93));
  gr.addColorStop(1, rgba(mixHex(col, "#000000", 0.2), I * 0.9));
  ctx.fillStyle = gr;
  polyPath(ctx, pool);
  ctx.fill();
  ctx.strokeStyle = rgba(mixHex(col, "#000000", 0.4), I * 0.9);
  ctx.lineWidth = Math.max(0.012, env.px);
  ctx.stroke();
  ctx.save();
  polyPath(ctx, pool);
  ctx.clip();
  if (tom > 0.3) {
    // Coágulos: manchas mais escuras e opacas conforme seca.
    ctx.fillStyle = rgba(mixHex(col, "#1a0505", 0.5), 0.35 * tom * I);
    const n = Math.round(14 + tom * 30);
    for (let i = 0; i < n; i++) {
      const a = r() * Math.PI * 2;
      const d = R * Math.sqrt(r()) * 0.9;
      const rr = R * (0.05 + r() * 0.14);
      ctx.beginPath();
      ctx.ellipse(end.x + Math.cos(a) * d, end.y + Math.sin(a) * d, rr * (1 + r()), rr, r() * 3, 0, Math.PI * 2);
      ctx.fill();
    }
  }
  if (tom < 0.6) {
    // Brilho: sangue fresco é vidrado.
    ctx.strokeStyle = `rgba(255,255,255,${(0.16 * (1 - tom)).toFixed(3)})`;
    ctx.lineWidth = R * 0.1;
    ctx.beginPath();
    ctx.arc(end.x - R * 0.22, end.y - R * 0.28, R * 0.42, 3.5, 5.3);
    ctx.stroke();
  }
  ctx.restore();
  // Satélites na borda.
  const ns = 6 + Math.floor(r() * 6);
  for (let i = 0; i < ns; i++) {
    const a = r() * Math.PI * 2;
    const d = R * (1.12 + r() * 0.4);
    gota(env, r, end.x + Math.cos(a) * d, end.y + Math.sin(a) * d, R * (0.03 + r() * 0.06), a, 1 + r() * 0.8, col, I * 0.85);
  }
}

function sangueGotas(env: Env, o: SicroTraceObject, g: TraceGeom, col: string): void {
  const L = g.len;
  const I = tn(o, "int");
  const R0 = tn(o, "tam") / 2;
  const mov = !!tp(o, "movimento");
  const r = rng(o.seed);
  const step = 1 / Math.max(1, tn(o, "dens"));
  for (let s = step * 0.4; s < L; s += step * (0.6 + r() * 0.8)) {
    const p = traceAt(g, s);
    const d = (r() - 0.5) * 0.32;
    const R = R0 * (0.5 + r() * 0.75);
    gota(env, r, p.x + p.nx * d, p.y + p.ny * d, R, Math.atan2(p.ty, p.tx) + (r() - 0.5) * 0.3, mov ? 1.5 + r() * 0.6 : 1, col, I * (0.8 + r() * 0.2));
  }
}

function sangueProjecao(env: Env, o: SicroTraceObject, col: string): void {
  const { ctx, textura } = env;
  const { D, ang, half } = fanGeom(o, env.opts.ppm);
  const p0 = { x: o.p0.x / env.opts.ppm, y: o.p0.y / env.opts.ppm };
  const I = tn(o, "int");
  const R0 = tn(o, "tam") / 2;
  const r = rng(o.seed);
  if (tp(o, "cont")) {
    ctx.beginPath();
    ctx.moveTo(p0.x, p0.y);
    ctx.arc(p0.x, p0.y, D, ang - half, ang + half);
    ctx.closePath();
    ctx.fillStyle = rgba(col, 0.05);
    ctx.fill();
    ctx.strokeStyle = rgba(env.pal.traco, 0.7);
    ctx.lineWidth = env.px;
    ctx.setLineDash([0.4, 0.3]);
    ctx.stroke();
    ctx.setLineDash([]);
  }
  const n = Math.round(tn(o, "qtd"));
  for (let i = 0; i < n; i++) {
    const u = r();
    const v = r();
    const w = r();
    // Mais gotas perto da origem; o leque estreita junto dela.
    const far = 0.03 + 0.97 * Math.pow(u, 0.7);
    const a = ang + (v * 2 - 1) * half * (0.3 + 0.7 * Math.sqrt(far));
    const R = R0 * (0.25 + 0.75 * w) * (1.05 - 0.5 * far);
    // Longe da origem a gota chega rasante: alongada, com cauda.
    const elong = 1 + 2.6 * far * far + 0.3 * w;
    const x = p0.x + Math.cos(a) * D * far;
    const y = p0.y + Math.sin(a) * D * far;
    if (!textura) {
      ctx.fillStyle = rgba(col, I * 0.85);
      ctx.beginPath();
      ctx.ellipse(x, y, R * elong, R, a, 0, Math.PI * 2);
      ctx.fill();
      continue;
    }
    gota(env, r, x, y, R, a, elong, col, I * (0.9 - 0.3 * far));
  }
}

function sangueEsfregaco(env: Env, o: SicroTraceObject, g: TraceGeom, col: string): void {
  const { ctx, textura } = env;
  const pb = env.opts.tema === "pb";
  const L = g.len;
  const w = tn(o, "larg") / 2;
  const I = tn(o, "int");
  const r = rng(o.seed);
  const hw = (s: number) => w * (1 - 0.3 * Math.pow(Math.min(1, s / L), 1.6));
  const poly = ribbon(g, 0, L, () => 0, hw, textura ? 0.035 : 0, o.seed);
  if (pb) {
    ctx.fillStyle = hatch(ctx, "#000000", "#ffffff", env.px);
    polyPath(ctx, poly);
    ctx.fill();
    ctx.strokeStyle = "#000000";
    ctx.lineWidth = env.px;
    ctx.stroke();
    return;
  }
  if (!textura) {
    ctx.fillStyle = rgba(col, 0.2);
    polyPath(ctx, poly);
    ctx.fill();
    ctx.strokeStyle = col;
    ctx.lineWidth = env.px;
    ctx.setLineDash([0.3, 0.2]);
    ctx.stroke();
    ctx.setLineDash([]);
    return;
  }
  const a = traceAt(g, 0);
  const b = traceAt(g, L);
  const gr = ctx.createLinearGradient(a.x, a.y, b.x, b.y);
  gr.addColorStop(0, rgba(col, I * 0.82));
  gr.addColorStop(0.5, rgba(col, I * 0.5));
  gr.addColorStop(1, rgba(col, I * 0.1));
  ctx.fillStyle = gr;
  polyPath(ctx, poly);
  ctx.fill();
  // Estrias no sentido do arrasto, esmaecendo na ponta.
  ctx.save();
  polyPath(ctx, poly);
  ctx.clip();
  const n = Math.round(10 + w * 40);
  const escuro = mixHex(col, "#000000", 0.3);
  for (let i = 0; i < n; i++) {
    const d = (r() * 2 - 1) * w * 0.92;
    const s0 = r() * L * 0.2;
    const s1 = L * (0.5 + r() * 0.5);
    ctx.strokeStyle = rgba(r() < 0.5 ? escuro : col, I * (0.25 + r() * 0.45));
    ctx.lineWidth = Math.max(0.006, 0.3 * env.px) * (0.5 + r() * 1.2);
    polyPath(ctx, offsetLine(g, s0, s1, d, 0.02, o.seed + i * 7, 0.08), false);
    ctx.stroke();
  }
  ctx.restore();
}

function drawSangue(env: Env, o: SicroTraceObject, g: TraceGeom): void {
  const col = sangueCor(env, o);
  switch (tp(o, "tipo")) {
    case "gotas":
      return sangueGotas(env, o, g, col);
    case "projecao":
      return sangueProjecao(env, o, col);
    case "esfregaco":
      return sangueEsfregaco(env, o, g, col);
    default:
      return sanguePoca(env, o, g, col);
  }
}

// ---------------------------------------------------------------------------
// Arma de fogo deitada (perfil lateral), centrada entre p0 e p1, cano para p1.
// Desenho num quadro em que 1 = comprimento da arma e +y = lado da empunhadura.

type Pt = [number, number];
interface ArmaCtx {
  ctx: Ctx;
  A: Pal["arma"];
  /** Largura de linha no quadro normalizado. */
  lw: number;
  /** Textura (gradiente, serrilhas, madeira) ou só silhueta com linhas-chave. */
  tex: boolean;
  pb: boolean;
}

function aPoly(a: ArmaCtx, pts: Pt[], fill: string | CanvasGradient, stroke = a.A.borda, holes: Pt[][] = []): void {
  const { ctx } = a;
  ctx.beginPath();
  pts.forEach(([x, y], i) => (i ? ctx.lineTo(x, y) : ctx.moveTo(x, y)));
  ctx.closePath();
  for (const h of holes) {
    h.forEach(([x, y], i) => (i ? ctx.lineTo(x, y) : ctx.moveTo(x, y)));
    ctx.closePath();
  }
  ctx.fillStyle = fill;
  ctx.fill("evenodd");
  ctx.strokeStyle = stroke;
  ctx.lineWidth = a.lw;
  ctx.lineJoin = "round";
  ctx.stroke();
}

function aRect(a: ArmaCtx, x: number, y: number, w: number, h: number, fill: string | CanvasGradient, r = 0, stroke = a.A.borda): void {
  const { ctx } = a;
  ctx.beginPath();
  ctx.roundRect(x, y, w, h, r);
  ctx.fillStyle = fill;
  ctx.fill();
  ctx.strokeStyle = stroke;
  ctx.lineWidth = a.lw;
  ctx.stroke();
}

function aLine(a: ArmaCtx, pts: Pt[], color: string, lw = a.lw): void {
  const { ctx } = a;
  ctx.beginPath();
  pts.forEach(([x, y], i) => (i ? ctx.lineTo(x, y) : ctx.moveTo(x, y)));
  ctx.strokeStyle = color;
  ctx.lineWidth = lw;
  ctx.stroke();
}

/** Metal: gradiente vertical sutil na textura, chapado no traço. */
function metalFill(a: ArmaCtx, y0: number, y1: number): string | CanvasGradient {
  if (!a.tex || a.pb) return a.A.metal;
  const g = a.ctx.createLinearGradient(0, y0, 0, y1);
  g.addColorStop(0, mixHex(a.A.metal, a.A.brilho, 0.35));
  g.addColorStop(0.45, a.A.metal);
  g.addColorStop(1, mixHex(a.A.metal, a.A.borda, 0.45));
  return g;
}

function woodFill(a: ArmaCtx, y0: number, y1: number): string | CanvasGradient {
  if (!a.tex || a.pb) return a.A.madeira;
  const g = a.ctx.createLinearGradient(0, y0, 0, y1);
  g.addColorStop(0, mixHex(a.A.madeira, "#ffffff", 0.18));
  g.addColorStop(0.5, a.A.madeira);
  g.addColorStop(1, mixHex(a.A.madeira, "#000000", 0.3));
  return g;
}

/** Xadrez da empunhadura: duas famílias de linhas finas, recortadas pelo polígono. */
function checkering(a: ArmaCtx, poly: Pt[], step: number): void {
  const { ctx } = a;
  ctx.save();
  ctx.beginPath();
  poly.forEach(([x, y], i) => (i ? ctx.lineTo(x, y) : ctx.moveTo(x, y)));
  ctx.closePath();
  ctx.clip();
  ctx.strokeStyle = a.pb ? "#000000" : rgba(a.A.brilho, 0.35);
  ctx.lineWidth = a.lw * 0.6;
  ctx.beginPath();
  for (let k = -1.2; k <= 1.2; k += step) {
    ctx.moveTo(k - 0.6, -0.6);
    ctx.lineTo(k + 0.6, 0.6);
    ctx.moveTo(k + 0.6, -0.6);
    ctx.lineTo(k - 0.6, 0.6);
  }
  ctx.stroke();
  ctx.restore();
}

function guardHole(a: ArmaCtx, outer: Pt[], inner: Pt[], fill: string | CanvasGradient): void {
  const { ctx } = a;
  ctx.beginPath();
  outer.forEach(([x, y], i) => (i ? ctx.lineTo(x, y) : ctx.moveTo(x, y)));
  ctx.closePath();
  inner.forEach(([x, y], i) => (i ? ctx.lineTo(x, y) : ctx.moveTo(x, y)));
  ctx.closePath();
  ctx.fillStyle = fill;
  ctx.fill("evenodd");
  ctx.strokeStyle = a.A.borda;
  ctx.lineWidth = a.lw;
  ctx.lineJoin = "round";
  ctx.stroke();
}

function armaPistola(a: ArmaCtx): void {
  const { ctx, A, lw, tex } = a;
  // Armação com guarda-mato vazado e empunhadura inclinada.
  const frame: Pt[] = [
    [-0.46, -0.2], [0.5, -0.2], [0.5, -0.1], [0.46, -0.06], [0.15, -0.06], [0.15, 0.08], [0.09, 0.15], [-0.02, 0.15], [-0.08, 0.1],
    [-0.19, 0.335], [-0.21, 0.355], [-0.5, 0.355], [-0.46, 0.22], [-0.41, 0.0], [-0.5, -0.12], [-0.5, -0.19],
  ];
  const hole: Pt[] = [[0.11, -0.02], [0.11, 0.07], [0.06, 0.115], [-0.01, 0.115], [-0.05, 0.07], [-0.05, -0.02]];
  guardHole(a, frame, hole, a.pb || !tex ? A.punho : mixHex(A.punho, A.metal, 0.4));
  if (tex) {
    checkering(a, [[-0.41, 0.02], [-0.1, 0.02], [-0.19, 0.335], [-0.46, 0.335]], 0.045);
    aRect(a, -0.5, 0.33, 0.3, 0.03, A.punho, 0.004);
  }
  // Gatilho.
  aLine(a, [[0.045, -0.01], [0.04, 0.04], [0.02, 0.085]], A.borda, lw * 1.6);
  // Ferrolho com serrilhas, janela de ejeção e alças de mira.
  aRect(a, -0.48, -0.36, 0.98, 0.165, metalFill(a, -0.36, -0.2), 0.022);
  if (tex) {
    for (let i = 0; i < 7; i++) aLine(a, [[-0.44 + i * 0.02, -0.33], [-0.44 + i * 0.02, -0.225]], rgba(A.borda, 0.7), lw * 0.8);
    aRect(a, 0.03, -0.345, 0.17, 0.07, a.pb ? A.fundo : mixHex(A.metal, A.borda, 0.6), 0.008);
    aLine(a, [[-0.3, -0.335], [0.44, -0.335]], rgba(A.brilho, a.pb ? 0 : 0.5), lw);
  }
  aRect(a, -0.46, -0.4, 0.05, 0.04, A.metal, 0.004);
  aRect(a, 0.42, -0.39, 0.03, 0.03, A.metal, 0.002);
  ctx.lineJoin = "round";
}

function armaRevolver(a: ArmaCtx): void {
  const { A, lw, tex } = a;
  // Empunhadura (madeira ou borracha) atrás, por baixo da armação.
  const grip: Pt[] = [[-0.3, 0.0], [-0.12, 0.05], [-0.1, 0.1], [-0.2, 0.3], [-0.42, 0.3], [-0.4, 0.15], [-0.36, 0.03]];
  aPoly(a, grip, woodFill(a, 0.0, 0.3), A.madeiraBorda);
  if (tex && !a.pb) {
    a.ctx.beginPath();
    a.ctx.arc(-0.27, 0.17, 0.035, 0, Math.PI * 2);
    a.ctx.fillStyle = A.latao;
    a.ctx.fill();
    a.ctx.strokeStyle = A.madeiraBorda;
    a.ctx.lineWidth = lw;
    a.ctx.stroke();
  }
  // Cão.
  aPoly(a, [[-0.33, -0.31], [-0.27, -0.29], [-0.28, -0.21], [-0.36, -0.24]], A.metal);
  // Armação com guarda-mato vazado.
  const frame: Pt[] = [[-0.32, -0.27], [0.08, -0.27], [0.08, 0.06], [0.06, 0.14], [0.0, 0.2], [-0.1, 0.2], [-0.16, 0.14], [-0.16, 0.06], [-0.32, 0.03]];
  const hole: Pt[] = [[0.03, 0.07], [0.03, 0.13], [-0.01, 0.165], [-0.09, 0.165], [-0.12, 0.13], [-0.12, 0.07]];
  guardHole(a, frame, hole, metalFill(a, -0.27, 0.2));
  // Gatilho.
  aLine(a, [[-0.04, 0.07], [-0.05, 0.12], [-0.07, 0.15]], A.borda, lw * 1.6);
  // Tambor com canaletas.
  aRect(a, -0.27, -0.235, 0.3, 0.26, metalFill(a, -0.235, 0.025), 0.07);
  if (tex) {
    for (const y of [-0.16, -0.08, 0.0]) aLine(a, [[-0.25, y], [0.01, y]], rgba(A.borda, 0.55), lw * 0.9);
    aLine(a, [[-0.24, -0.215], [0.0, -0.215]], rgba(A.brilho, a.pb ? 0 : 0.5), lw);
  }
  // Cano, vareta ejetora e massa de mira.
  aRect(a, 0.06, -0.215, 0.44, 0.115, metalFill(a, -0.215, -0.1), 0.012);
  aRect(a, 0.1, -0.1, 0.32, 0.06, metalFill(a, -0.1, -0.04), 0.02);
  aPoly(a, [[0.42, -0.215], [0.47, -0.215], [0.47, -0.26], [0.43, -0.26]], A.metal);
  if (tex) aLine(a, [[0.08, -0.2], [0.48, -0.2]], rgba(A.brilho, a.pb ? 0 : 0.5), lw);
}

function armaEspingarda(a: ArmaCtx): void {
  const { A, lw, tex } = a;
  // Coronha de madeira com chapa de borracha.
  const stock: Pt[] = [[-0.24, -0.095], [-0.24, 0.0], [-0.3, 0.03], [-0.49, 0.09], [-0.49, -0.07], [-0.3, -0.1]];
  aPoly(a, stock, woodFill(a, -0.1, 0.09), A.madeiraBorda);
  aRect(a, -0.5, -0.075, 0.018, 0.17, A.punho, 0.004);
  // Cano e tubo do carregador.
  aRect(a, -0.08, -0.088, 0.58, 0.032, metalFill(a, -0.088, -0.056), 0.004);
  aRect(a, -0.08, -0.056, 0.48, 0.026, metalFill(a, -0.056, -0.03), 0.006);
  aRect(a, 0.485, -0.1, 0.012, 0.012, A.metal, 0.002);
  // Telha (bomba) com ranhuras.
  aRect(a, 0.12, -0.1, 0.18, 0.1, woodFill(a, -0.1, 0.0), 0.02, A.madeiraBorda);
  if (tex) for (let i = 1; i < 6; i++) aLine(a, [[0.12 + i * 0.03, -0.095], [0.12 + i * 0.03, -0.005]], rgba(A.madeiraBorda, 0.6), lw * 0.9);
  // Caixa da culatra com janela de ejeção e guarda-mato.
  aRect(a, -0.24, -0.1, 0.18, 0.1, metalFill(a, -0.1, 0.0), 0.012);
  if (tex) aRect(a, -0.17, -0.082, 0.085, 0.04, a.pb ? A.fundo : mixHex(A.metal, A.borda, 0.6), 0.006);
  guardHole(a, [[-0.21, 0.0], [-0.09, 0.0], [-0.09, 0.055], [-0.2, 0.055]], [[-0.18, 0.012], [-0.11, 0.012], [-0.11, 0.043], [-0.18, 0.043]], A.metal);
  aLine(a, [[-0.145, 0.012], [-0.15, 0.04]], A.borda, lw * 1.4);
}

function armaCarabina(a: ArmaCtx): void {
  const { A, lw, tex } = a;
  // Coronha inteiriça de madeira, do fuste à chapa.
  const stock: Pt[] = [
    [0.3, -0.045], [0.3, 0.0], [-0.08, 0.0], [-0.17, 0.035], [-0.3, 0.06], [-0.49, 0.09], [-0.49, -0.07], [-0.32, -0.085], [-0.2, -0.06], [-0.1, -0.06],
  ];
  aPoly(a, stock, woodFill(a, -0.085, 0.09), A.madeiraBorda);
  aRect(a, -0.5, -0.075, 0.016, 0.17, A.punho, 0.004);
  if (tex) aLine(a, [[-0.3, 0.07], [-0.48, 0.08]], rgba(A.madeiraBorda, 0.5), lw * 0.8);
  // Cano e ação com ferrolho de manejo.
  aRect(a, 0.08, -0.075, 0.42, 0.03, metalFill(a, -0.075, -0.045), 0.004);
  aRect(a, -0.2, -0.09, 0.26, 0.05, metalFill(a, -0.09, -0.04), 0.01);
  aLine(a, [[-0.1, -0.05], [-0.08, 0.0]], A.borda, lw * 2.2);
  a.ctx.beginPath();
  a.ctx.arc(-0.075, 0.012, 0.022, 0, Math.PI * 2);
  a.ctx.fillStyle = A.metal;
  a.ctx.fill();
  a.ctx.strokeStyle = A.borda;
  a.ctx.lineWidth = lw;
  a.ctx.stroke();
  if (tex) aRect(a, -0.17, -0.078, 0.07, 0.03, a.pb ? A.fundo : mixHex(A.metal, A.borda, 0.6), 0.004);
  guardHole(a, [[-0.16, 0.0], [-0.03, 0.0], [-0.03, 0.05], [-0.15, 0.05]], [[-0.13, 0.012], [-0.05, 0.012], [-0.05, 0.038], [-0.13, 0.038]], A.metal);
  aLine(a, [[-0.085, 0.012], [-0.09, 0.036]], A.borda, lw * 1.4);
}

function armaFuzil(a: ArmaCtx): void {
  const { A, lw, tex } = a;
  // Coronha retrátil e tubo.
  aRect(a, -0.42, -0.075, 0.24, 0.045, metalFill(a, -0.075, -0.03), 0.01);
  aPoly(a, [[-0.5, -0.1], [-0.5, 0.1], [-0.46, 0.1], [-0.3, 0.005], [-0.3, -0.03], [-0.42, -0.03], [-0.42, -0.1]], metalFill(a, -0.1, 0.1));
  // Empunhadura e carregador curvo.
  const grip: Pt[] = [[-0.2, 0.05], [-0.11, 0.05], [-0.14, 0.2], [-0.23, 0.2]];
  aPoly(a, grip, A.punho);
  if (tex) checkering(a, grip, 0.03);
  aPoly(a, [[-0.05, 0.05], [0.04, 0.05], [0.09, 0.22], [0.01, 0.25], [-0.03, 0.23]], metalFill(a, 0.05, 0.25));
  if (tex) aLine(a, [[-0.02, 0.1], [0.05, 0.1]], rgba(A.borda, 0.6), lw);
  // Caixa da culatra (superior e inferior) e guarda-mato.
  aRect(a, -0.2, -0.1, 0.24, 0.08, metalFill(a, -0.1, -0.02), 0.008);
  aRect(a, -0.2, -0.02, 0.24, 0.07, metalFill(a, -0.02, 0.05), 0.008);
  guardHole(a, [[-0.11, 0.05], [-0.05, 0.05], [-0.05, 0.105], [-0.11, 0.105]], [[-0.095, 0.06], [-0.065, 0.06], [-0.065, 0.092], [-0.095, 0.092]], A.metal);
  aLine(a, [[-0.08, 0.06], [-0.085, 0.09]], A.borda, lw * 1.4);
  if (tex) {
    aRect(a, -0.06, -0.085, 0.075, 0.035, a.pb ? A.fundo : mixHex(A.metal, A.borda, 0.6), 0.004);
    aLine(a, [[-0.2, -0.06], [-0.22, -0.06]], A.borda, lw * 2);
  }
  // Alça de mira / asa de transporte.
  aRect(a, -0.17, -0.15, 0.1, 0.05, metalFill(a, -0.15, -0.1), 0.006);
  // Guarda-mão com respiros, cano, massa de mira e quebra-chamas.
  aRect(a, 0.04, -0.1, 0.24, 0.1, metalFill(a, -0.1, 0.0), 0.02);
  if (tex) {
    for (let i = 0; i < 5; i++) {
      a.ctx.beginPath();
      a.ctx.arc(0.08 + i * 0.045, -0.05, 0.012, 0, Math.PI * 2);
      a.ctx.fillStyle = a.pb ? A.fundo : mixHex(A.metal, A.borda, 0.7);
      a.ctx.fill();
      a.ctx.strokeStyle = A.borda;
      a.ctx.lineWidth = lw * 0.8;
      a.ctx.stroke();
    }
  }
  aRect(a, 0.28, -0.07, 0.2, 0.028, metalFill(a, -0.07, -0.042), 0.003);
  aPoly(a, [[0.27, -0.07], [0.31, -0.07], [0.3, -0.14], [0.28, -0.14]], A.metal);
  aRect(a, 0.45, -0.08, 0.05, 0.048, A.metal, 0.004);
}

function armaEstojo(a: ArmaCtx): void {
  const { A, lw, tex } = a;
  const fill = tex && !a.pb ? (() => { const g = a.ctx.createLinearGradient(0, -0.2, 0, 0.2); g.addColorStop(0, mixHex(A.latao, "#ffffff", 0.35)); g.addColorStop(0.5, A.latao); g.addColorStop(1, mixHex(A.latao, "#000000", 0.4)); return g; })() : A.latao;
  // Corpo, aro e boca aberta.
  aRect(a, -0.46, -0.18, 0.96, 0.36, fill, 0.01);
  aRect(a, -0.5, -0.2, 0.06, 0.4, fill, 0.01);
  aLine(a, [[-0.42, -0.18], [-0.42, 0.18]], A.borda, lw * 0.8);
  aLine(a, [[0.44, -0.18], [0.44, 0.18]], A.borda, lw * 0.8);
}

function armaProjetil(a: ArmaCtx): void {
  const { ctx, A, lw, tex } = a;
  const fill = tex && !a.pb ? (() => { const g = ctx.createLinearGradient(0, -0.3, 0, 0.3); g.addColorStop(0, mixHex(A.cobre, "#ffffff", 0.35)); g.addColorStop(0.5, A.cobre); g.addColorStop(1, mixHex(A.cobre, "#000000", 0.4)); return g; })() : A.cobre;
  // Base cilíndrica e ogiva.
  ctx.beginPath();
  ctx.moveTo(-0.5, -0.3);
  ctx.lineTo(-0.05, -0.3);
  ctx.quadraticCurveTo(0.35, -0.28, 0.5, 0);
  ctx.quadraticCurveTo(0.35, 0.28, -0.05, 0.3);
  ctx.lineTo(-0.5, 0.3);
  ctx.closePath();
  ctx.fillStyle = fill;
  ctx.fill();
  ctx.strokeStyle = A.borda;
  ctx.lineWidth = lw;
  ctx.stroke();
  aLine(a, [[-0.05, -0.3], [-0.05, 0.3]], rgba(A.borda, 0.6), lw * 0.8);
  if (tex) aLine(a, [[-0.4, -0.2], [0.1, -0.17]], rgba("#ffffff", a.pb ? 0 : 0.35), lw * 1.2);
}

function drawArma(env: Env, o: SicroTraceObject): void {
  const { ctx } = env;
  const f = armaFrame(o, env.opts.ppm);
  const L = armaLenM(o);
  const pb = env.opts.tema === "pb";
  const a: ArmaCtx = { ctx, A: env.pal.arma, lw: Math.max(env.px / L, 0.004), tex: env.textura, pb };
  ctx.save();
  ctx.translate(f.c.x, f.c.y);
  ctx.rotate(Math.atan2(f.uy, f.ux));
  if (tp(o, "lado") === "esq") ctx.scale(1, -1);
  ctx.scale(L, L);
  ctx.lineJoin = "round";
  ctx.lineCap = "round";
  switch (tp(o, "modelo")) {
    case "revolver":
      armaRevolver(a);
      break;
    case "espingarda":
      armaEspingarda(a);
      break;
    case "carabina":
      armaCarabina(a);
      break;
    case "fuzil":
      armaFuzil(a);
      break;
    case "estojo":
      armaEstojo(a);
      break;
    case "projetil":
      armaProjetil(a);
      break;
    default:
      armaPistola(a);
  }
  ctx.restore();
}

function drawFragmentos(env: Env, o: SicroTraceObject): void {
  const { ctx, pal } = env;
  const { D, ang, half } = fanGeom(o, env.opts.ppm);
  const p0 = { x: o.p0.x / env.opts.ppm, y: o.p0.y / env.opts.ppm };
  const r = rng(o.seed);
  const n = Math.min(900, Math.round(tn(o, "dens") * half * D * D * 0.35));
  if (tp(o, "cont")) {
    ctx.beginPath();
    ctx.moveTo(p0.x, p0.y);
    ctx.arc(p0.x, p0.y, D, ang - half, ang + half);
    ctx.closePath();
    ctx.fillStyle = rgba(pal.marca, 0.04);
    ctx.fill();
    ctx.strokeStyle = rgba(pal.traco, 0.7);
    ctx.lineWidth = env.px;
    ctx.setLineDash([0.4, 0.3]);
    ctx.stroke();
    ctx.setLineDash([]);
  }
  const tipo = String(tp(o, "tipo"));
  for (let i = 0; i < n; i++) {
    const u = r();
    const v = r();
    const w = r();
    const sz0 = r();
    const rot = r() * 3;
    const kind = tipo === "misto" ? (w < 0.62 ? "vidro" : "plast") : tipo;
    const rr = D * (0.08 + 0.92 * Math.sqrt(u));
    const a = ang + (v * 2 - 1) * half * (0.35 + 0.65 * Math.sqrt(u));
    ctx.save();
    ctx.translate(p0.x + Math.cos(a) * rr, p0.y + Math.sin(a) * rr);
    ctx.rotate(rot);
    if (kind === "vidro") {
      const s = 0.04 + sz0 * 0.08;
      ctx.fillStyle = pal.vidro[0];
      ctx.strokeStyle = pal.vidro[1];
      ctx.lineWidth = Math.max(0.006, 0.2 * env.px);
      ctx.beginPath();
      ctx.rect(-s / 2, -s / 2, s, s * 0.8);
      ctx.fill();
      ctx.stroke();
    } else {
      const s = 0.06 + sz0 * 0.14;
      ctx.fillStyle = pal.plast;
      ctx.beginPath();
      ctx.moveTo(-s / 2, -s / 3);
      ctx.lineTo(s / 2, -s / 2);
      ctx.lineTo(s / 3, s / 2);
      ctx.lineTo(-s / 3, s / 3);
      ctx.closePath();
      ctx.fill();
    }
    ctx.restore();
  }
}

function drawColisao(env: Env, o: SicroTraceObject): void {
  const { ctx, pal } = env;
  const c = { x: o.p0.x / env.opts.ppm, y: o.p0.y / env.opts.ppm };
  const R = tn(o, "raio");
  const area = tp(o, "forma") === "area";
  if (area) {
    ctx.beginPath();
    ctx.ellipse(c.x, c.y, R * 1.25, R, 0, 0, Math.PI * 2);
    ctx.fillStyle = env.opts.tema === "pb" ? hatch(ctx, "#000000", "#ffffff", env.px) : rgba(pal.pc, 0.12);
    ctx.fill();
    ctx.strokeStyle = pal.pc;
    ctx.lineWidth = 1.4 * env.px;
    ctx.setLineDash([0.4, 0.25]);
    ctx.stroke();
    ctx.setLineDash([]);
  }
  const r = area ? Math.min(R * 0.45, 0.45) : R;
  ctx.strokeStyle = pal.pc;
  ctx.lineWidth = Math.max(2 * env.px, r * 0.12);
  ctx.beginPath();
  ctx.arc(c.x, c.y, r, 0, Math.PI * 2);
  ctx.stroke();
  const k = r * 0.62;
  ctx.beginPath();
  ctx.moveTo(c.x - k, c.y - k);
  ctx.lineTo(c.x + k, c.y + k);
  ctx.moveTo(c.x - k, c.y + k);
  ctx.lineTo(c.x + k, c.y - k);
  ctx.stroke();
}

/** Desenha o vestígio; `ctx` em px de mundo. */
export function drawTrace(ctx: Ctx, o: SicroTraceObject, opts: TraceDrawOpts): void {
  const ppm = Math.max(opts.ppm, 1e-6);
  const env: Env = { ctx, pal: tracePalette(opts.tema), opts: { ...opts, ppm }, px: 1 / ppm, textura: opts.finish !== "traco" };
  ctx.save();
  ctx.scale(ppm, ppm);
  ctx.lineJoin = "round";
  if (o.subtype === "colisao") drawColisao(env, o);
  else if (o.subtype === "fragmentos") drawFragmentos(env, o);
  else if (o.subtype === "arma") drawArma(env, o);
  else {
    const g = traceGeomM(o, ppm);
    // A poça de sangue existe mesmo sem escorrimento (p0 = p1).
    if (g.len > 0.01 || (o.subtype === "sangue" && tp(o, "tipo") === "poca")) {
      switch (o.subtype) {
        case "frenagem":
          drawFrenagem(env, o, g);
          break;
        case "derrapagem":
          drawDerrapagem(env, o, g);
          break;
        case "arrasto":
          drawArrasto(env, o, g);
          break;
        case "sulcagem":
          drawSulcagem(env, o, g);
          break;
        case "ranhura":
          drawRanhura(env, o, g);
          break;
        case "fluido":
          drawFluido(env, o, g);
          break;
        case "sangue":
          drawSangue(env, o, g);
          break;
      }
    }
  }
  ctx.restore();
}

/** Área clicável em px de mundo: linha central larga + polígonos (poça, leque, círculo). */
export function traceHitShape(o: SicroTraceObject, ppm: number): { line: SicroPoint[]; width: number; polys: SicroPoint[][] } {
  const toPx = (p: SicroPoint) => ({ x: p.x * ppm, y: p.y * ppm });
  if (o.subtype === "colisao") {
    const R = tn(o, "raio") * ppm * (tp(o, "forma") === "area" ? 1.25 : 1);
    const poly = Array.from({ length: 24 }, (_, i) => {
      const a = (i / 24) * Math.PI * 2;
      return { x: o.p0.x + Math.cos(a) * R, y: o.p0.y + Math.sin(a) * R };
    });
    return { line: [], width: 0, polys: [poly] };
  }
  if (o.subtype === "arma") {
    return { line: [], width: 0, polys: [armaCornersPx(o, ppm, 1.15)] };
  }
  if (o.subtype === "fragmentos" || (o.subtype === "sangue" && tp(o, "tipo") === "projecao")) {
    const { D, ang, half } = fanGeom(o, ppm);
    const poly: SicroPoint[] = [{ ...o.p0 }];
    for (let i = 0; i <= 16; i++) {
      const a = ang - half + (2 * half * i) / 16;
      poly.push({ x: o.p0.x + Math.cos(a) * D * ppm, y: o.p0.y + Math.sin(a) * D * ppm });
    }
    return { line: [], width: 0, polys: [poly] };
  }
  const g = traceGeomM(o, ppm);
  const line = g.pts.filter((_, i) => i % 2 === 0 || i === g.pts.length - 1).map(toPx);
  const polys: SicroPoint[][] = [];
  if (o.subtype === "fluido" || (o.subtype === "sangue" && tp(o, "tipo") === "poca")) {
    const end = traceAt(g, g.len);
    polys.push(blob(o.seed, end, tn(o, "raio") * 1.1, Math.atan2(end.ty, end.tx)).map(toPx));
  }
  return { line, width: Math.max(10, (traceSpanM(o) + 0.25) * 2 * ppm), polys };
}
