/**
 * Desenho dos vestígios em canvas 2D (o mesmo da Bancada de Vestígios).
 * Tudo em metros: o chamador já está em px de mundo e aqui se escala por ppm.
 */

import {
  ENTRE_EIXOS_M,
  TRACE_FLUIDOS,
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
}

const PALETTES: Record<ParityTema, Pal> = {
  tecnico: { marca: "#1d1d1d", pc: "#c62828", traco: "#111111", vidro: ["#d4e3ec", "#5f7f92"], plast: "#3a3a3a", lasca: "#8d8d8d" },
  pb: { marca: "#000000", pc: "#000000", traco: "#000000", vidro: ["#ffffff", "#000000"], plast: "#000000", lasca: "#555555" },
  escuro: { marca: "#000000", pc: "#ff5a4f", traco: "#f2f2f2", vidro: ["#bcd6e6", "#e8f2f8"], plast: "#9a9a9a", lasca: "#6d6d6d" },
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
  else {
    const g = traceGeomM(o, ppm);
    if (g.len > 0.01) {
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
  if (o.subtype === "fragmentos") {
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
  if (o.subtype === "fluido") {
    const end = traceAt(g, g.len);
    polys.push(blob(o.seed, end, tn(o, "raio") * 1.1, Math.atan2(end.ty, end.tx)).map(toPx));
  }
  return { line, width: Math.max(10, (traceSpanM(o) + 0.25) * 2 * ppm), polys };
}
