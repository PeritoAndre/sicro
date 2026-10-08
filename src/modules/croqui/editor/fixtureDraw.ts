/**
 * Desenho da sinalização e do entorno em canvas 2D, em metros (o chamador está
 * em px de mundo). Cores e traços seguem o tema das vias.
 */

import {
  FIXTURE_SPECS,
  fixtureDir,
  fixtureExtentM,
  fixtureGeomM,
  fn,
  fp,
  traceAt,
  traceSteps,
  type SicroFixtureObject,
  type SicroPoint,
  type TraceGeom,
} from "../engine";
import { parityTemaColors, type ParityStyle } from "../engine/road-parity";
import { hatch, rgba } from "./traceDraw";

type Ctx = CanvasRenderingContext2D;

interface FPal {
  linha: string;
  fundo: string;
  vermelho: string;
  azul: string;
  amareloPlaca: string;
  sinal: [string, string, string];
  apagado: string;
  caixa: string;
  copa: string;
  copaLinha: string;
  tronco: string;
  camera: string;
  defensa: string;
  concreto: string;
  lombada: string;
  lampada: string;
  contornoBranca: string | null;
  pb: boolean;
}

const PALS: Record<ParityStyle["tema"], FPal> = {
  tecnico: {
    linha: "#111111", fundo: "#ffffff", vermelho: "#c62828", azul: "#1e5aa8", amareloPlaca: "#f2c200",
    sinal: ["#d32f2f", "#f2b705", "#1f9d48"], apagado: "#4a4f55", caixa: "#1f2329",
    copa: "#d9e8cf", copaLinha: "#4f6f43", tronco: "#8b6b4a", camera: "#2563eb",
    defensa: "#6b6b6b", concreto: "#d9d9d9", lombada: "#d2d2d2", lampada: "#fff3b8", contornoBranca: "#bdbdbd", pb: false,
  },
  pb: {
    linha: "#000000", fundo: "#ffffff", vermelho: "#000000", azul: "#000000", amareloPlaca: "#ffffff",
    sinal: ["#000000", "#000000", "#000000"], apagado: "#ffffff", caixa: "#ffffff",
    copa: "#ffffff", copaLinha: "#000000", tronco: "#ffffff", camera: "#000000",
    defensa: "#000000", concreto: "#ffffff", lombada: "#ffffff", lampada: "#ffffff", contornoBranca: null, pb: true,
  },
  escuro: {
    linha: "#f2f2f2", fundo: "#2a2f36", vermelho: "#ff5a4f", azul: "#3b7dd8", amareloPlaca: "#f5c518",
    sinal: ["#ff4d4d", "#f5c518", "#34c759"], apagado: "#3a3f45", caixa: "#0e1116",
    copa: "#3a6535", copaLinha: "#8fbf7f", tronco: "#7a5a3a", camera: "#60a5fa",
    defensa: "#bdbdbd", concreto: "#5a5a5a", lombada: "#2b2b2b", lampada: "#c9b458", contornoBranca: null, pb: false,
  },
};

export interface FixtureDrawOpts {
  ppm: number;
  style: ParityStyle;
}

interface Env {
  ctx: Ctx;
  pal: FPal;
  st: ParityStyle;
  ppm: number;
  /** 1 px de mundo em metros. */
  px: number;
}

const v = (x: number, y: number): SicroPoint => ({ x, y });

/** Referencial local: x = frente (p0 → p1), y = direita. */
function frame(o: SicroFixtureObject, ppm: number) {
  const d = fixtureDir(o, ppm);
  const c = v(o.p0.x / ppm, o.p0.y / ppm);
  const L = (x: number, y: number): SicroPoint => v(c.x + d.ux * x - d.uy * y, c.y + d.uy * x + d.ux * y);
  return { c, ux: d.ux, uy: d.uy, dist: d.dist, L, ang: Math.atan2(d.uy, d.ux) };
}

function poly(ctx: Ctx, pts: SicroPoint[], close = true): void {
  ctx.beginPath();
  pts.forEach((p, i) => (i ? ctx.lineTo(p.x, p.y) : ctx.moveTo(p.x, p.y)));
  if (close) ctx.closePath();
}

function circle(ctx: Ctx, c: SicroPoint, r: number): void {
  ctx.beginPath();
  ctx.arc(c.x, c.y, r, 0, Math.PI * 2);
}

/** Texto em px de mundo (fontes < 1 px quebram no canvas). Sempre reto. */
function text(env: Env, t: string, c: SicroPoint, sizeM: number, color: string, weight = "bold"): void {
  const { ctx, ppm } = env;
  ctx.save();
  ctx.scale(1 / ppm, 1 / ppm);
  ctx.font = `${weight} ${sizeM * ppm}px Arial, sans-serif`;
  ctx.textAlign = "center";
  ctx.textBaseline = "middle";
  ctx.fillStyle = color;
  ctx.fillText(t, c.x * ppm, c.y * ppm);
  ctx.restore();
}

function ribbon(g: TraceGeom, hw: number, s0 = 0, s1 = g.len): SicroPoint[] {
  const A: SicroPoint[] = [];
  const B: SicroPoint[] = [];
  for (const s of traceSteps(s0, s1)) {
    const p = traceAt(g, s);
    A.push(v(p.x + p.nx * hw, p.y + p.ny * hw));
    B.push(v(p.x - p.nx * hw, p.y - p.ny * hw));
  }
  return A.concat(B.reverse());
}

function centerline(g: TraceGeom, off = 0): SicroPoint[] {
  return traceSteps(0, g.len).map((s) => {
    const p = traceAt(g, s);
    return v(p.x + p.nx * off, p.y + p.ny * off);
  });
}

// ---------------------------------------------------------------------------
// Sinalização

function signIcon(env: Env, o: SicroFixtureObject, c: SicroPoint, S: number): void {
  const { ctx, pal } = env;
  const modelo = String(fp(o, "modelo"));
  const texto = String(fp(o, "texto") ?? "").trim();
  const ring = Math.max(S * 0.09, 1.5 * env.px);
  ctx.lineJoin = "round";
  if (modelo === "pare") {
    const pts = Array.from({ length: 8 }, (_, i) => {
      const a = (i / 8) * Math.PI * 2 + Math.PI / 8;
      return v(c.x + Math.cos(a) * S * 0.5, c.y + Math.sin(a) * S * 0.5);
    });
    poly(ctx, pts);
    ctx.fillStyle = pal.vermelho;
    ctx.fill();
    ctx.strokeStyle = pal.pb ? pal.linha : "#ffffff";
    ctx.lineWidth = S * 0.05;
    ctx.stroke();
    text(env, "PARE", c, S * 0.27, "#ffffff");
    return;
  }
  if (modelo === "pref") {
    poly(ctx, [v(c.x - S * 0.5, c.y - S * 0.38), v(c.x + S * 0.5, c.y - S * 0.38), v(c.x, c.y + S * 0.5)]);
    ctx.fillStyle = pal.fundo === "#2a2f36" ? "#ffffff" : pal.fundo;
    ctx.fill();
    ctx.strokeStyle = pal.vermelho;
    ctx.lineWidth = ring * 1.3;
    ctx.stroke();
    return;
  }
  if (modelo === "vel" || modelo === "proib_est") {
    circle(ctx, c, S * 0.5 - ring / 2);
    ctx.fillStyle = "#ffffff";
    ctx.fill();
    ctx.strokeStyle = pal.vermelho;
    ctx.lineWidth = ring;
    ctx.stroke();
    if (modelo === "vel") {
      text(env, String(Math.round(fn(o, "vel"))), c, S * 0.36, "#111111");
    } else {
      text(env, "E", c, S * 0.46, "#111111");
      ctx.strokeStyle = pal.vermelho;
      ctx.lineWidth = ring;
      ctx.beginPath();
      const k = S * 0.33;
      ctx.moveTo(c.x - k, c.y - k);
      ctx.lineTo(c.x + k, c.y + k);
      ctx.stroke();
    }
    return;
  }
  if (modelo === "adv") {
    poly(ctx, [v(c.x, c.y - S * 0.5), v(c.x + S * 0.5, c.y), v(c.x, c.y + S * 0.5), v(c.x - S * 0.5, c.y)]);
    ctx.fillStyle = pal.amareloPlaca;
    ctx.fill();
    ctx.strokeStyle = "#111111";
    ctx.lineWidth = S * 0.05;
    ctx.stroke();
    const t = texto || "!";
    text(env, t, c, t.length > 3 ? S * 0.16 : S * 0.36, "#111111");
    return;
  }
  // Indicação: retângulo azul com orla branca.
  const w = S;
  const h = S * 0.6;
  ctx.beginPath();
  ctx.roundRect(c.x - w / 2, c.y - h / 2, w, h, S * 0.06);
  ctx.fillStyle = pal.pb ? "#ffffff" : pal.azul;
  ctx.fill();
  ctx.strokeStyle = pal.pb ? "#000000" : "#ffffff";
  ctx.lineWidth = S * 0.04;
  ctx.stroke();
  if (texto) text(env, texto.slice(0, 24), c, Math.min(S * 0.2, (w * 1.6) / Math.max(texto.length, 1)), pal.pb ? "#000000" : "#ffffff");
}

function drawPlaca(env: Env, o: SicroFixtureObject): void {
  const { ctx, pal, px } = env;
  const F = frame(o, env.ppm);
  const S = fn(o, "tam");
  // Vista de cima: poste e face da placa voltada para a alça (trânsito).
  const pw = Math.max(0.6, S * 0.45);
  ctx.strokeStyle = pal.linha;
  ctx.lineWidth = Math.max(0.04, 1.8 * px);
  ctx.lineCap = "butt";
  poly(ctx, [F.L(0.05, -pw / 2), F.L(0.05, pw / 2)], false);
  ctx.stroke();
  circle(ctx, F.c, Math.max(0.06, 1.8 * px));
  ctx.fillStyle = pal.linha;
  ctx.fill();
  signIcon(env, o, F.L(-(S * 0.5 + 0.35), 0), S);
}

function drawSemaforo(env: Env, o: SicroFixtureObject): void {
  const { ctx, pal, px } = env;
  const F = frame(o, env.ppm);
  const S = fn(o, "tam");
  const braco = !!fp(o, "braco") && F.dist > 0.3;
  const head = braco ? v(o.p1.x / env.ppm, o.p1.y / env.ppm) : F.L(0.5 + S * 0.2, 0);
  ctx.strokeStyle = pal.linha;
  ctx.lineCap = "round";
  if (braco) {
    ctx.lineWidth = Math.max(0.08, 2 * px);
    poly(ctx, [F.c, head], false);
    ctx.stroke();
  }
  circle(ctx, F.c, Math.max(0.12, 2.2 * px));
  ctx.fillStyle = pal.linha;
  ctx.fill();
  const ped = fp(o, "grupo") === "ped";
  const n = ped ? 2 : 3;
  const W = S * 0.36;
  const H = S * (ped ? 0.7 : 1);
  ctx.beginPath();
  ctx.roundRect(head.x - W / 2, head.y - H / 2, W, H, W * 0.25);
  ctx.fillStyle = pal.caixa;
  ctx.fill();
  ctx.strokeStyle = pal.linha;
  ctx.lineWidth = Math.max(0.02, px);
  ctx.stroke();
  const fase = String(fp(o, "fase"));
  const cores = ped ? [pal.sinal[0], pal.sinal[2]] : pal.sinal;
  const nomes = ped ? ["vermelho", "verde"] : ["vermelho", "amarelo", "verde"];
  for (let i = 0; i < n; i++) {
    const cy = head.y - H / 2 + (H / n) * (i + 0.5);
    const lit = fase === "nenhuma" || fase === nomes[i];
    circle(ctx, v(head.x, cy), W * 0.32);
    ctx.fillStyle = pal.pb ? (lit && fase !== "nenhuma" ? "#000000" : "#ffffff") : lit ? cores[i]! : pal.apagado;
    ctx.fill();
    if (pal.pb) {
      ctx.strokeStyle = "#000000";
      ctx.lineWidth = Math.max(0.015, 0.8 * px);
      ctx.stroke();
    }
  }
}

function brancaFill(env: Env, pts: SicroPoint[]): void {
  const { ctx, pal } = env;
  poly(ctx, pts);
  ctx.fillStyle = env.st.branca;
  ctx.fill();
  if (pal.contornoBranca) {
    ctx.strokeStyle = pal.contornoBranca;
    ctx.lineWidth = 0.6 * env.px;
    ctx.stroke();
  }
}

function drawFaixa(env: Env, o: SicroFixtureObject): void {
  const g = fixtureGeomM({ ...o, bend: 0 }, env.ppm);
  const hw = fn(o, "largura") / 2;
  const listra = fn(o, "listra");
  const passo = listra + fn(o, "espaco");
  const n = Math.max(1, Math.floor((g.len + fn(o, "espaco")) / passo));
  // Centraliza as listras na travessia.
  const start = (g.len - (n * passo - fn(o, "espaco"))) / 2;
  for (let i = 0; i < n; i++) {
    const s0 = start + i * passo;
    const a = traceAt(g, s0);
    const b = traceAt(g, s0 + listra);
    brancaFill(env, [
      v(a.x + a.nx * hw, a.y + a.ny * hw),
      v(b.x + b.nx * hw, b.y + b.ny * hw),
      v(b.x - b.nx * hw, b.y - b.ny * hw),
      v(a.x - a.nx * hw, a.y - a.ny * hw),
    ]);
  }
}

function drawRetencao(env: Env, o: SicroFixtureObject): void {
  brancaFill(env, ribbon(fixtureGeomM({ ...o, bend: 0 }, env.ppm), fn(o, "esp") / 2));
}

/** Clareia (k > 0) ou escurece (k < 0) uma cor #rrggbb. */
function shade(hex: string, k: number): string {
  const n = parseInt(hex.slice(1, 7), 16);
  const ch = [n >> 16, (n >> 8) & 255, n & 255].map((c) => Math.round(k >= 0 ? c + (255 - c) * k : c * (1 + k)));
  return `rgb(${ch[0]},${ch[1]},${ch[2]})`;
}

/** Lombada de asfalto: corpo com volume (luz numa rampa, sombra na outra) e sombra projetada. */
function drawLombada(env: Env, o: SicroFixtureObject): void {
  const { ctx, pal, px } = env;
  const g = fixtureGeomM({ ...o, bend: 0 }, env.ppm);
  const C = fp(o, "tipo") === "B" ? 1.5 : 3.7;
  const A = env.st.asfalto;
  const a = traceAt(g, 0);
  const e0 = v(a.x - a.nx * (C / 2), a.y - a.ny * (C / 2));
  const e1 = v(a.x + a.nx * (C / 2), a.y + a.ny * (C / 2));
  const rect = ribbon(g, C / 2);
  if (pal.pb) {
    poly(ctx, rect);
    ctx.fillStyle = "#ffffff";
    ctx.fill();
    // Curvas de nível: mais juntas nas rampas.
    ctx.strokeStyle = "#000000";
    for (const [f, w] of [[0.18, 0.6], [0.34, 0.6], [0.5, 0.9], [0.66, 0.6], [0.82, 0.6]] as [number, number][]) {
      ctx.lineWidth = w * px;
      poly(ctx, centerline(g, (f - 0.5) * C), false);
      ctx.stroke();
    }
  } else {
    // Sombra projetada além da rampa de trás.
    const sh = ctx.createLinearGradient(e1.x, e1.y, e1.x + a.nx * 0.45, e1.y + a.ny * 0.45);
    sh.addColorStop(0, "rgba(0,0,0,0.16)");
    sh.addColorStop(1, "rgba(0,0,0,0)");
    poly(ctx, ribbon({ ...g, pts: g.pts.map((p) => ({ ...p, x: p.x + p.nx * (C / 2 + 0.22), y: p.y + p.ny * (C / 2 + 0.22) })) }, 0.22));
    ctx.fillStyle = sh;
    ctx.fill();
    const gr = ctx.createLinearGradient(e0.x, e0.y, e1.x, e1.y);
    gr.addColorStop(0, A);
    gr.addColorStop(0.1, shade(A, -0.05));
    gr.addColorStop(0.38, shade(A, 0.22));
    gr.addColorStop(0.52, shade(A, 0.12));
    gr.addColorStop(0.8, shade(A, -0.14));
    gr.addColorStop(1, shade(A, -0.06));
    poly(ctx, rect);
    ctx.fillStyle = gr;
    ctx.fill();
  }
  if (fp(o, "pintura")) {
    ctx.save();
    poly(ctx, rect);
    ctx.clip();
    ctx.strokeStyle = pal.pb ? "#000000" : rgba(env.st.amarela.startsWith("#") ? env.st.amarela : "#d6a200", 0.85);
    ctx.lineWidth = pal.pb ? 0.12 : 0.3;
    for (let s = -C; s <= g.len + C; s += 0.9) {
      const p = v(a.x + a.tx * s, a.y + a.ty * s);
      ctx.beginPath();
      ctx.moveTo(p.x - (a.tx + a.nx) * C, p.y - (a.ty + a.ny) * C);
      ctx.lineTo(p.x + (a.tx + a.nx) * C, p.y + (a.ty + a.ny) * C);
      ctx.stroke();
    }
    ctx.restore();
  }
  // Pé das rampas.
  ctx.strokeStyle = pal.pb ? "#000000" : shade(A, -0.32);
  ctx.lineWidth = pal.pb ? 1.2 * px : 0.8 * px;
  for (const off of [-C / 2, C / 2]) {
    poly(ctx, centerline(g, off), false);
    ctx.stroke();
  }
}

/** Marcação de área de conflito (CONTRAN): contorno e diagonais cruzadas amarelas. */
function drawAreaConflito(env: Env, o: SicroFixtureObject): void {
  const { ctx, pal } = env;
  const g = fixtureGeomM({ ...o, bend: 0 }, env.ppm);
  const W = fn(o, "largura");
  const passo = fn(o, "passo");
  const rect = ribbon(g, W / 2);
  const cor = pal.pb ? "#000000" : env.st.amarela;
  const lw = 0.15;
  ctx.save();
  poly(ctx, rect);
  ctx.clip();
  ctx.strokeStyle = cor;
  ctx.lineWidth = lw;
  const a = traceAt(g, 0);
  const ext = g.len + W;
  for (const sgn of [1, -1]) {
    for (let s = -W; s <= ext; s += passo) {
      const p = v(a.x + a.tx * s, a.y + a.ty * s);
      const dx = (a.tx * sgn + a.nx) * W;
      const dy = (a.ty * sgn + a.ny) * W;
      ctx.beginPath();
      ctx.moveTo(p.x - dx, p.y - dy);
      ctx.lineTo(p.x + dx, p.y + dy);
      ctx.stroke();
    }
  }
  ctx.restore();
  poly(ctx, rect);
  ctx.strokeStyle = cor;
  ctx.lineWidth = lw * 1.4;
  ctx.stroke();
}

function drawSeta(env: Env, o: SicroFixtureObject): void {
  const { ctx } = env;
  const F = frame(o, env.ppm);
  const Lc = fn(o, "comp");
  const k = Lc / 5;
  const w = 0.3 * k;
  const hl = 1.4 * k;
  const hwid = 1.0 * k;
  const tipo = String(fp(o, "tipo"));
  type Path = { pts: [number, number][]; dir: [number, number] };
  const paths: Path[] = [];
  const quarter = (x0: number, side: 1 | -1, R: number): [number, number][] =>
    Array.from({ length: 13 }, (_, i) => {
      const t = (i / 12) * (Math.PI / 2);
      return [x0 + R * Math.sin(t), side * (R - R * Math.cos(t))];
    });
  const frente = (): Path => ({ pts: [[-Lc / 2, 0], [Lc / 2 - hl, 0]], dir: [1, 0] });
  const vira = (side: 1 | -1, x0: number, R: number): Path => ({ pts: [[-Lc / 2, 0], ...quarter(x0, side, R)], dir: [0, side] });
  if (tipo === "frente") paths.push(frente());
  else if (tipo === "dir" || tipo === "esq") paths.push(vira(tipo === "dir" ? 1 : -1, Lc * 0.12, Lc * 0.28));
  else if (tipo === "frente_dir" || tipo === "frente_esq") {
    paths.push(frente());
    paths.push(vira(tipo === "frente_dir" ? 1 : -1, -Lc * 0.05, Lc * 0.22));
  } else {
    // Retorno pela esquerda.
    const R = Lc * 0.2;
    const xr = Lc * 0.15;
    const semi: [number, number][] = Array.from({ length: 19 }, (_, i) => {
      const t = (i / 18) * Math.PI;
      return [xr + R * Math.sin(t), -R + R * Math.cos(t)];
    });
    paths.push({ pts: [[-Lc / 2, 0], ...semi, [xr - Lc * 0.12, -2 * R]], dir: [-1, 0] });
  }
  ctx.strokeStyle = env.st.branca;
  ctx.fillStyle = env.st.branca;
  ctx.lineWidth = w;
  ctx.lineCap = "butt";
  ctx.lineJoin = "round";
  for (const p of paths) {
    poly(ctx, p.pts.map(([x, y]) => F.L(x, y)), false);
    ctx.stroke();
    const [ex, ey] = p.pts[p.pts.length - 1]!;
    const [dx, dy] = p.dir;
    poly(ctx, [F.L(ex + dx * hl, ey + dy * hl), F.L(ex - dy * (hwid / 2), ey + dx * (hwid / 2)), F.L(ex + dy * (hwid / 2), ey - dx * (hwid / 2))]);
    ctx.fill();
  }
}

// ---------------------------------------------------------------------------
// Entorno

function drawPoste(env: Env, o: SicroFixtureObject): void {
  const { ctx, pal, px } = env;
  const F = frame(o, env.ppm);
  const R = Math.max(fn(o, "diam") / 2, 2.5 * px);
  if (fp(o, "luminaria") && F.dist > 0.2) {
    const end = v(o.p1.x / env.ppm, o.p1.y / env.ppm);
    ctx.strokeStyle = pal.linha;
    ctx.lineWidth = Math.max(0.06, 1.5 * px);
    ctx.lineCap = "round";
    poly(ctx, [F.c, end], false);
    ctx.stroke();
    ctx.beginPath();
    ctx.ellipse(end.x, end.y, 0.4, 0.2, F.ang, 0, Math.PI * 2);
    ctx.fillStyle = pal.lampada;
    ctx.fill();
    ctx.lineWidth = px;
    ctx.stroke();
  }
  circle(ctx, F.c, R);
  ctx.fillStyle = pal.fundo;
  ctx.fill();
  ctx.strokeStyle = pal.linha;
  ctx.lineWidth = 1.4 * px;
  ctx.stroke();
  circle(ctx, F.c, R * 0.4);
  ctx.fillStyle = pal.linha;
  ctx.fill();
}

function drawArvore(env: Env, o: SicroFixtureObject): void {
  const { ctx, pal, px } = env;
  const c = v(o.p0.x / env.ppm, o.p0.y / env.ppm);
  const R = fn(o, "raio");
  const N = Math.max(7, Math.min(22, Math.round(R * 2.6)));
  const ph = (o.seed % 360) * (Math.PI / 180);
  const pts: SicroPoint[] = [];
  for (let i = 0; i < 144; i++) {
    const t = (i / 144) * Math.PI * 2;
    const r = R * (0.9 + 0.1 * Math.abs(Math.sin((N * t) / 2)));
    pts.push(v(c.x + Math.cos(t + ph) * r, c.y + Math.sin(t + ph) * r));
  }
  poly(ctx, pts);
  ctx.fillStyle = rgba(pal.copa, pal.pb ? 1 : 0.9);
  ctx.fill();
  ctx.strokeStyle = pal.copaLinha;
  ctx.lineWidth = 1.2 * px;
  ctx.stroke();
  if (fp(o, "ramos")) {
    ctx.lineWidth = 0.8 * px;
    ctx.lineCap = "round";
    const m = Math.max(4, Math.round(N / 2));
    for (let i = 0; i < m; i++) {
      const a = ph + (i / m) * Math.PI * 2;
      const r1 = R * 0.62;
      const tip = v(c.x + Math.cos(a) * r1, c.y + Math.sin(a) * r1);
      poly(ctx, [c, tip], false);
      ctx.stroke();
      for (const s of [-1, 1]) {
        const b0 = v(c.x + Math.cos(a) * r1 * 0.55, c.y + Math.sin(a) * r1 * 0.55);
        const ab = a + s * 0.5;
        poly(ctx, [b0, v(b0.x + Math.cos(ab) * R * 0.22, b0.y + Math.sin(ab) * R * 0.22)], false);
        ctx.stroke();
      }
    }
  }
  circle(ctx, c, Math.max(fn(o, "tronco") / 2, 2 * px));
  ctx.fillStyle = pal.tronco;
  ctx.fill();
  ctx.strokeStyle = pal.linha;
  ctx.lineWidth = px;
  ctx.stroke();
}

function drawHidrante(env: Env, o: SicroFixtureObject): void {
  const { ctx, pal, px } = env;
  const F = frame(o, env.ppm);
  const S = fn(o, "tam");
  ctx.strokeStyle = pal.linha;
  ctx.lineWidth = px;
  ctx.fillStyle = pal.vermelho;
  // Bocais: dois laterais e um voltado para a alça.
  for (const [x, y, a] of [[0, S * 0.56, 0], [0, -S * 0.56, 0], [S * 0.56, 0, 1]] as [number, number, number][]) {
    const w = a ? S * 0.22 : S * 0.26;
    const h = a ? S * 0.26 : S * 0.22;
    poly(ctx, [F.L(x - w / 2, y - h / 2), F.L(x + w / 2, y - h / 2), F.L(x + w / 2, y + h / 2), F.L(x - w / 2, y + h / 2)]);
    ctx.fill();
    ctx.stroke();
  }
  circle(ctx, F.c, S * 0.45);
  ctx.fillStyle = pal.vermelho;
  ctx.fill();
  ctx.stroke();
  circle(ctx, F.c, S * 0.2);
  ctx.fillStyle = pal.fundo;
  ctx.fill();
  ctx.stroke();
}

function drawAbrigo(env: Env, o: SicroFixtureObject): void {
  const { ctx, pal, px } = env;
  const F = frame(o, env.ppm);
  const C = fn(o, "comp");
  const P = fn(o, "prof");
  const rect = [F.L(-P / 2, -C / 2), F.L(P / 2, -C / 2), F.L(P / 2, C / 2), F.L(-P / 2, C / 2)];
  poly(ctx, rect);
  ctx.fillStyle = pal.fundo;
  ctx.fill();
  // Cobertura: hachura leve.
  const cores = parityTemaColors(env.st);
  ctx.save();
  poly(ctx, rect);
  ctx.clip();
  ctx.strokeStyle = rgba(cores.hachuraLinha, 0.55);
  ctx.lineWidth = 0.6 * px;
  for (let s = -C; s <= C; s += 0.5) {
    poly(ctx, [F.L(-P, s - P), F.L(P, s + P)], false);
    ctx.stroke();
  }
  ctx.restore();
  poly(ctx, rect);
  ctx.strokeStyle = pal.linha;
  ctx.lineWidth = 1.4 * px;
  ctx.stroke();
  // Parede de fundo e banco.
  ctx.lineWidth = Math.max(0.1, 2.5 * px);
  ctx.lineCap = "butt";
  poly(ctx, [F.L(-P / 2, -C / 2), F.L(-P / 2, C / 2)], false);
  ctx.stroke();
  ctx.lineWidth = Math.max(0.3, 2 * px);
  ctx.strokeStyle = rgba(pal.linha, 0.45);
  poly(ctx, [F.L(-P / 2 + 0.3, -C * 0.36), F.L(-P / 2 + 0.3, C * 0.36)], false);
  ctx.stroke();
}

function drawBarreira(env: Env, o: SicroFixtureObject): void {
  const { ctx, pal, px } = env;
  const g = fixtureGeomM(o, env.ppm);
  const tipo = String(fp(o, "tipo"));
  ctx.lineJoin = "round";
  if (tipo === "defensa") {
    // Lâmina com postes a cada 2 m, do lado de trás.
    for (let s = 0.3; s < g.len; s += 2) {
      const p = traceAt(g, s);
      const c = v(p.x + p.nx * 0.14, p.y + p.ny * 0.14);
      ctx.fillStyle = pal.linha;
      ctx.fillRect(c.x - 0.075, c.y - 0.075, 0.15, 0.15);
    }
    poly(ctx, ribbon(g, Math.max(0.04, 1.2 * px)));
    ctx.fillStyle = pal.defensa;
    ctx.fill();
    return;
  }
  if (tipo === "new_jersey") {
    poly(ctx, ribbon(g, 0.3));
    ctx.fillStyle = pal.concreto;
    ctx.fill();
    ctx.strokeStyle = pal.linha;
    ctx.lineWidth = 1.2 * px;
    ctx.stroke();
    poly(ctx, centerline(g), false);
    ctx.lineWidth = 0.6 * px;
    ctx.strokeStyle = rgba(pal.linha, 0.6);
    ctx.stroke();
    return;
  }
  if (tipo === "muro") {
    const cores = parityTemaColors(env.st);
    poly(ctx, ribbon(g, fn(o, "esp") / 2));
    ctx.fillStyle = hatch(ctx, cores.hachuraLinha, cores.hachuraFundo, px);
    ctx.fill();
    ctx.strokeStyle = pal.linha;
    ctx.lineWidth = 1.4 * px;
    ctx.stroke();
    return;
  }
  // Gradil / cerca: linha com cruzetas a cada metro.
  ctx.strokeStyle = pal.linha;
  ctx.lineWidth = px;
  poly(ctx, centerline(g), false);
  ctx.stroke();
  ctx.lineWidth = 0.8 * px;
  const k = 0.12;
  for (let s = 0.5; s < g.len; s += 1) {
    const p = traceAt(g, s);
    ctx.beginPath();
    ctx.moveTo(p.x - (p.tx + p.nx) * k, p.y - (p.ty + p.ny) * k);
    ctx.lineTo(p.x + (p.tx + p.nx) * k, p.y + (p.ty + p.ny) * k);
    ctx.moveTo(p.x - (p.tx - p.nx) * k, p.y - (p.ty - p.ny) * k);
    ctx.lineTo(p.x + (p.tx - p.nx) * k, p.y + (p.ty - p.ny) * k);
    ctx.stroke();
  }
}

function drawObstaculo(env: Env, o: SicroFixtureObject): void {
  const { ctx, pal, px } = env;
  const F = frame(o, env.ppm);
  const Lg = fn(o, "larg");
  const P = fn(o, "prof");
  if (fp(o, "forma") === "circ") circle(ctx, F.c, Lg / 2);
  else poly(ctx, [F.L(-Lg / 2, -P / 2), F.L(Lg / 2, -P / 2), F.L(Lg / 2, P / 2), F.L(-Lg / 2, P / 2)]);
  const cores = parityTemaColors(env.st);
  ctx.fillStyle = fp(o, "hachura") ? hatch(ctx, cores.hachuraLinha, cores.hachuraFundo, px) : pal.fundo;
  ctx.fill();
  ctx.strokeStyle = pal.linha;
  ctx.lineWidth = 1.4 * px;
  ctx.stroke();
}

function drawCamera(env: Env, o: SicroFixtureObject): void {
  const { ctx, pal, px } = env;
  const F = frame(o, env.ppm);
  const half = (fn(o, "abert") * Math.PI) / 360;
  if (fp(o, "campo") && F.dist > 0.5) {
    ctx.beginPath();
    ctx.moveTo(F.c.x, F.c.y);
    ctx.arc(F.c.x, F.c.y, F.dist, F.ang - half, F.ang + half);
    ctx.closePath();
    ctx.fillStyle = rgba(pal.camera, pal.pb ? 0.04 : 0.08);
    ctx.fill();
    ctx.strokeStyle = rgba(pal.camera, 0.65);
    ctx.lineWidth = px;
    ctx.setLineDash([0.5, 0.35]);
    ctx.stroke();
    ctx.setLineDash([]);
  }
  const s = Math.max(0.5, 9 * px);
  poly(ctx, [F.L(-s * 0.5, -s * 0.26), F.L(s * 0.25, -s * 0.26), F.L(s * 0.25, s * 0.26), F.L(-s * 0.5, s * 0.26)]);
  ctx.fillStyle = pal.linha;
  ctx.fill();
  poly(ctx, [F.L(s * 0.25, -s * 0.12), F.L(s * 0.55, -s * 0.24), F.L(s * 0.55, s * 0.24), F.L(s * 0.25, s * 0.12)]);
  ctx.fill();
}

const DRAW: Record<SicroFixtureObject["subtype"], (env: Env, o: SicroFixtureObject) => void> = {
  placa: drawPlaca,
  semaforo: drawSemaforo,
  faixa_pedestre: drawFaixa,
  retencao: drawRetencao,
  lombada: drawLombada,
  area_conflito: drawAreaConflito,
  seta: drawSeta,
  poste: drawPoste,
  arvore: drawArvore,
  hidrante: drawHidrante,
  abrigo: drawAbrigo,
  barreira: drawBarreira,
  obstaculo: drawObstaculo,
  camera: drawCamera,
};

/** Desenha o elemento; `ctx` em px de mundo. */
export function drawFixture(ctx: Ctx, o: SicroFixtureObject, opts: FixtureDrawOpts): void {
  const ppm = Math.max(opts.ppm, 1e-6);
  const env: Env = { ctx, pal: PALS[opts.style.tema] ?? PALS.tecnico, st: opts.style, ppm, px: 1 / ppm };
  ctx.save();
  ctx.scale(ppm, ppm);
  DRAW[o.subtype]?.(env, o);
  ctx.restore();
}

/** Elementos pintados no chão ficam sob os vestígios; os de pé, por cima. */
export function fixtureOnGround(o: SicroFixtureObject): boolean {
  return (
    o.subtype === "faixa_pedestre" ||
    o.subtype === "retencao" ||
    o.subtype === "lombada" ||
    o.subtype === "area_conflito" ||
    o.subtype === "seta"
  );
}

/** Área clicável em px de mundo. */
export function fixtureHitShape(o: SicroFixtureObject, ppm: number): { line: SicroPoint[]; width: number; polys: SicroPoint[][] } {
  const spec = FIXTURE_SPECS[o.subtype];
  const toPx = (p: SicroPoint) => v(p.x * ppm, p.y * ppm);
  const disc = (c: SicroPoint, rM: number) =>
    Array.from({ length: 20 }, (_, i) => {
      const a = (i / 20) * Math.PI * 2;
      return v(c.x + Math.cos(a) * rM * ppm, c.y + Math.sin(a) * rM * ppm);
    });
  if (spec.forma === "linha") {
    const g = fixtureGeomM(o.subtype === "barreira" ? o : { ...o, bend: 0 }, ppm);
    return { line: centerline(g).map(toPx), width: Math.max(10, (fixtureExtentM(o) + 0.3) * 2 * ppm), polys: [] };
  }
  const F = frame(o, ppm);
  const cPx = toPx(F.c);
  switch (o.subtype) {
    case "placa": {
      const S = fn(o, "tam");
      return { line: [], width: 0, polys: [disc(cPx, 0.5), disc(toPx(F.L(-(S * 0.5 + 0.35), 0)), S * 0.6)] };
    }
    case "semaforo":
    case "poste": {
      const ext = o.subtype === "poste" ? Math.max(0.5, fn(o, "diam")) : 0.5;
      const reach = o.subtype === "semaforo" ? !!fp(o, "braco") : !!fp(o, "luminaria");
      return {
        line: reach ? [cPx, { ...o.p1 }] : [],
        width: Math.max(10, 0.6 * ppm),
        polys: [disc(cPx, ext), ...(o.subtype === "semaforo" ? [disc(reach ? { ...o.p1 } : toPx(F.L(0.5 + fn(o, "tam") * 0.2, 0)), fn(o, "tam") * 0.6)] : [])],
      };
    }
    case "abrigo":
    case "obstaculo":
    case "seta": {
      if (o.subtype === "obstaculo" && fp(o, "forma") === "circ") return { line: [], width: 0, polys: [disc(cPx, fn(o, "larg") / 2)] };
      const a = o.subtype === "seta" ? fn(o, "comp") / 2 + 1.4 * (fn(o, "comp") / 5) : o.subtype === "abrigo" ? fn(o, "prof") / 2 : fn(o, "larg") / 2;
      const b = o.subtype === "seta" ? fn(o, "comp") * 0.6 : o.subtype === "abrigo" ? fn(o, "comp") / 2 : fn(o, "prof") / 2;
      return { line: [], width: 0, polys: [[F.L(-a, -b), F.L(a, -b), F.L(a, b), F.L(-a, b)].map(toPx)] };
    }
    case "camera":
      return { line: [], width: 0, polys: [disc(cPx, 0.6)] };
    default:
      return { line: [], width: 0, polys: [disc(cPx, Math.max(0.5, fixtureExtentM(o)))] };
  }
}
