/**
 * Pessoa em planta técnica: silhueta única com contorno fino, membros com perfil de
 * músculo, cabelo em traço, sem rosto. O chamador está em px de mundo.
 */

import { PERSON_COMP, personRig, vadd, vmul, vrot, type PersonRig, type SicroPersonObject, type Vec } from "../engine";
import type { ParityTema } from "../engine/road-parity";

const PI = Math.PI;

interface PPal {
  linha: string;
  det: string;
  giz: string;
  fills: Record<"branco" | "cinza", string>;
}

const PALS: Record<ParityTema, PPal> = {
  tecnico: { linha: "#111111", det: "rgba(17,17,17,0.45)", giz: "#ffffff", fills: { branco: "#ffffff", cinza: "#d5d9de" } },
  pb: { linha: "#000000", det: "rgba(0,0,0,0.65)", giz: "#000000", fills: { branco: "#ffffff", cinza: "#ffffff" } },
  escuro: { linha: "#f2f2f2", det: "rgba(242,242,242,0.6)", giz: "#f2f2f2", fills: { branco: "#3a414a", cinza: "#4f5863" } },
};

export interface PersonDrawOpts {
  ppm: number;
  tema: ParityTema;
  asfalto: string;
  /** Escala da tela; os traços têm espessura física com um mínimo em px de tela. */
  zoom: number;
}

/** Durante a exportação o mínimo em px de tela não vale (o PNG sai em alta resolução). */
let exporting = false;
export function setPersonExporting(on: boolean): void {
  exporting = on;
}

function smooth(pts: Vec[]): Path2D {
  const p = new Path2D();
  const n = pts.length;
  const mid = (a: Vec, b: Vec) => ({ x: (a.x + b.x) / 2, y: (a.y + b.y) / 2 });
  const s0 = mid(pts[n - 1]!, pts[0]!);
  p.moveTo(s0.x, s0.y);
  for (let i = 0; i < n; i++) {
    const c = pts[i]!;
    const e = mid(c, pts[(i + 1) % n]!);
    p.quadraticCurveTo(c.x, c.y, e.x, e.y);
  }
  p.closePath();
  return p;
}

/** Segmento com perfil de músculo: `prof` = [t, meia largura em m]; pontas arredondadas. */
function shaped(a: Vec, b: Vec, prof: [number, number][], wk = 1): Path2D {
  const dx = b.x - a.x;
  const dy = b.y - a.y;
  const L = Math.hypot(dx, dy) || 1e-6;
  const d = { x: dx / L, y: dy / L };
  const n = { x: -d.y, y: d.x };
  const at = (t: number, w: number) => ({ x: a.x + dx * t + n.x * w, y: a.y + dy * t + n.y * w });
  const left = prof.map(([t, w]) => at(t, w * wk));
  const right = prof.slice().reverse().map(([t, w]) => at(t, -w * wk));
  const we = prof[prof.length - 1]![1] * wk;
  const w0 = prof[0]![1] * wk;
  const cap = (p: Vec, dir: number, w: number) => [
    { x: p.x + d.x * w * 0.9 * dir + n.x * w * 0.6 * dir, y: p.y + d.y * w * 0.9 * dir + n.y * w * 0.6 * dir },
    { x: p.x + d.x * w * 0.9 * dir - n.x * w * 0.6 * dir, y: p.y + d.y * w * 0.9 * dir - n.y * w * 0.6 * dir },
  ];
  return smooth([...left, ...cap(b, 1, we), ...right, ...cap(a, -1, w0)]);
}

function ellipsePath(c: Vec, rx: number, ry: number, ang: number): Path2D {
  const p = new Path2D();
  p.ellipse(c.x, c.y, rx, ry, ang, 0, 2 * PI);
  return p;
}

const PROF: Record<string, [number, number][]> = {
  braco: [[0, 0.036], [0.22, 0.036], [0.55, 0.03], [1, 0.022]],
  antebraco: [[0, 0.023], [0.22, 0.026], [0.65, 0.019], [1, 0.015]],
  coxa: [[0, 0.06], [0.3, 0.053], [0.75, 0.041], [1, 0.033]],
  perna: [[0, 0.033], [0.28, 0.037], [0.7, 0.025], [1, 0.019]],
  mao: [[0, 0.016], [0.35, 0.024], [0.78, 0.022], [1, 0.012]],
  pe: [[0, 0.019], [0.25, 0.024], [0.65, 0.029], [0.9, 0.024], [1, 0.012]],
};
const along = (p: Vec, d: Vec, s: number) => ({ x: p.x + d.x * s, y: p.y + d.y * s });
const unit = (a: Vec, b: Vec) => {
  const L = Math.hypot(b.x - a.x, b.y - a.y) || 1e-6;
  return { x: (b.x - a.x) / L, y: (b.y - a.y) / L };
};

interface Parts {
  back: Path2D[];
  torso: Path2D[];
  front: Path2D[];
  head: Path2D[];
  /** Último da cabeça = crânio (o cabelo é recortado nele). */
  hair: Path2D | null;
  hairAxis: { c: Vec; hd: Vec } | null;
  empe: boolean;
}

/** Cabeça de cima: u = para o alto da cabeça, v = lateral. */
function head(R: PersonRig, posicao: SicroPersonObject["posicao"]): { outline: Path2D; hair: Path2D; ears: Path2D[] } {
  const H = R.H;
  const c = R.headC;
  const hd = R.hdir;
  const hn = { x: -hd.y, y: hd.x };
  const P = (u: number, v: number) => ({ x: c.x + hd.x * u * H + hn.x * v * H, y: c.y + hd.y * u * H + hn.y * v * H });
  const earAng = Math.atan2(hd.y, hd.x);
  if (R.lat && R.front) {
    const s = Math.sign(R.front.x * hn.x + R.front.y * hn.y) || 1;
    const Q = (u: number, w: number) => P(u, w * s);
    const outline = smooth([
      Q(0.068, -0.012), Q(0.058, -0.042), Q(0.03, -0.058), Q(-0.005, -0.06), Q(-0.036, -0.048), Q(-0.056, -0.025),
      Q(-0.066, 0.005), Q(-0.07, 0.03), Q(-0.06, 0.045), Q(-0.046, 0.05), Q(-0.034, 0.056), Q(-0.024, 0.056), Q(-0.015, 0.064),
      Q(-0.006, 0.062), Q(0.004, 0.056), Q(0.018, 0.059), Q(0.04, 0.053), Q(0.06, 0.032),
    ]);
    const hair = smooth([Q(0.074, -0.014), Q(0.062, -0.05), Q(0.03, -0.066), Q(-0.01, -0.066), Q(-0.042, -0.052), Q(-0.03, -0.02), Q(0, 0.005), Q(0.03, 0.03), Q(0.06, 0.04), Q(0.072, 0.018)]);
    return { outline, hair, ears: [ellipsePath(Q(-0.004, -0.012), 0.019 * H, 0.011 * H, earAng)] };
  }
  const half: [number, number][] = [[0.068, 0], [0.062, 0.028], [0.04, 0.048], [0.01, 0.052], [-0.02, 0.048], [-0.042, 0.038], [-0.058, 0.022], [-0.067, 0]];
  const outline = smooth(half.map(([u, v]) => P(u, v)).concat(half.slice(1, -1).reverse().map(([u, v]) => P(u, -v))));
  const ears = [-1, 1].map((sg) => ellipsePath(P(-0.004, sg * 0.054), 0.02 * H, 0.01 * H, earAng));
  const hair =
    posicao === "ventral"
      ? smooth([P(0.074, 0), P(0.066, 0.034), P(0.042, 0.056), P(0, 0.058), P(-0.042, 0.046), P(-0.052, 0), P(-0.042, -0.046), P(0, -0.058), P(0.042, -0.056), P(0.066, -0.034)])
      : smooth([P(0.074, 0), P(0.068, 0.034), P(0.048, 0.056), P(0.026, 0.05), P(0.03, 0.02), P(0.034, 0), P(0.03, -0.02), P(0.026, -0.05), P(0.048, -0.056), P(0.068, -0.034)]);
  return { outline, hair, ears };
}

function partsDeitada(o: SicroPersonObject): Parts {
  const R = personRig(o);
  const H = R.H;
  const g = 1.1 * R.k; // um pouco mais cheio que o real, para ler na escala do croqui
  const back: Path2D[] = [];
  const front: Path2D[] = [];
  const sc = (prof: [number, number][]) => prof.map(([t, w]) => [t, w * H] as [number, number]);
  const put = (i: number, upper: Path2D[], lower: Path2D[]) => {
    if (R.lat && i === 0) back.push(...upper, ...lower);
    else if (R.lat) front.push(...upper, ...lower);
    else {
      back.push(...upper);
      front.push(...lower);
    }
  };
  R.arms.forEach((a, i) => {
    const d = unit(a.E, a.T);
    const n = { x: -d.y, y: d.x };
    const side = (i === 0 ? 1 : -1) * (o.posicao === "ventral" ? -1 : 1);
    const hand = shaped(a.T, along(a.T, d, 0.1 * H), sc(PROF.mao!), g * 0.95);
    const thumb = shaped(vadd(along(a.T, d, 0.02 * H), vmul(n, side * 0.017 * H)), vadd(along(a.T, d, 0.055 * H), vmul(n, side * 0.034 * H)), [[0, 0.009 * H], [1, 0.007 * H]]);
    put(i, [shaped(a.S, a.E, sc(PROF.braco!), g)], [shaped(a.E, a.T, sc(PROF.antebraco!), g), hand, thumb]);
  });
  R.legs.forEach((l, i) => {
    const d = unit(l.E, l.T);
    let foot: Path2D;
    if (o.posicao === "dorsal") {
      const fd = vrot(d, (i === 0 ? -1 : 1) * 0.5);
      foot = shaped(along(l.T, fd, -0.012 * H), along(l.T, fd, 0.06 * H), sc(PROF.pe!), g);
    } else if (o.posicao === "ventral") {
      foot = shaped(along(l.T, d, -0.02 * H), along(l.T, d, 0.14 * H), sc(PROF.pe!), g * 0.95);
    } else {
      const fw = vrot(d, (-R.m * PI) / 2);
      foot = shaped(along(l.T, fw, -0.035 * H), along(l.T, fw, 0.12 * H), sc(PROF.pe!), g * 0.95);
    }
    put(i, [shaped(l.S, l.E, sc(PROF.coxa!), g)], [shaped(l.E, l.T, sc(PROF.perna!), g), foot]);
  });
  const hg = head(R, o.posicao);
  const neck = shaped(R.neck, vadd(R.neck, vmul(R.hdir, 0.055 * H)), [[0, 0.032 * H], [1, 0.028 * H]], R.k);
  return {
    back,
    torso: [smooth(R.torso)],
    front,
    head: [neck, ...hg.ears, hg.outline],
    hair: hg.hair,
    hairAxis: { c: R.headC, hd: R.hdir },
    empe: false,
  };
}

/** Em pé, vista de cima: ombros, braços e cabeça com cabelo; sem pés (ficam sob o corpo). */
function partsEmPe(o: SicroPersonObject): Parts {
  const H = o.altura_m;
  const k = PERSON_COMP[o.comp] ?? 1;
  const sx = (o.perfil === "F" ? 0.92 : 1) * k;
  const P = (x: number, y: number) => ({ x: x * H, y: y * H });
  const torso = smooth(
    ([[-0.128, 0.004], [-0.118, -0.034], [-0.07, -0.058], [0, -0.064], [0.07, -0.058], [0.118, -0.034], [0.128, 0.004], [0.118, 0.036], [0.07, 0.052], [0, 0.056], [-0.07, 0.052], [-0.118, 0.036]] as [number, number][]).map(([x, y]) => P(x * sx, y * k)),
  );
  const arms = [-1, 1].map((sg) => shaped(P(sg * 0.126 * sx, -0.012), P(sg * 0.132 * sx, 0.03), [[0, 0.03 * H * k], [1, 0.026 * H * k]]));
  const hc = P(0, 0.004);
  const skull = smooth(
    ([[0, -0.066], [0.036, -0.058], [0.052, -0.026], [0.054, 0.012], [0.044, 0.046], [0.022, 0.064], [0, 0.068], [-0.022, 0.064], [-0.044, 0.046], [-0.054, 0.012], [-0.052, -0.026], [-0.036, -0.058]] as [number, number][]).map(([x, y]) => vadd(hc, P(x, y))),
  );
  const ears = [-1, 1].map((sg) => ellipsePath(vadd(hc, P(sg * 0.055, 0.002)), 0.009 * H, 0.017 * H, 0));
  const hair = smooth(
    ([[0, -0.05], [0.03, -0.046], [0.05, -0.018], [0.054, 0.014], [0.044, 0.046], [0.022, 0.064], [0, 0.068], [-0.022, 0.064], [-0.044, 0.046], [-0.054, 0.014], [-0.05, -0.018], [-0.03, -0.046]] as [number, number][]).map(([x, y]) => vadd(hc, P(x, y))),
  );
  return { back: [], torso: [torso], front: arms, head: [...ears, skull], hair, hairAxis: { c: hc, hd: { x: 0, y: -1 } }, empe: true };
}

function personParts(o: SicroPersonObject): Parts {
  return o.posicao === "empe" ? partsEmPe(o) : partsDeitada(o);
}

const shadeCache = new Map<string, string>();
function shade(hex: string, k: number): string {
  const key = hex + k;
  let out = shadeCache.get(key);
  if (!out) {
    const n = parseInt(hex.slice(1, 7), 16);
    const c = [n >> 16, (n >> 8) & 255, n & 255].map((v) => Math.round(k >= 0 ? v + (255 - v) * k : v * (1 + k)));
    out = `#${c.map((v) => v.toString(16).padStart(2, "0")).join("")}`;
    shadeCache.set(key, out);
  }
  return out;
}

/** Desenha a pessoa; `ctx` em px de mundo. */
export function drawPerson(ctx: CanvasRenderingContext2D, o: SicroPersonObject, opts: PersonDrawOpts): void {
  const ppm = Math.max(opts.ppm, 1e-6);
  // Traço em metros, com mínimo de `n` px de tela (fora da exportação).
  const scr = 1 / (ppm * Math.max(opts.zoom, 1e-6));
  const lw = (n: number, m: number) => (exporting ? m : Math.max(m, n * scr));
  const pal = PALS[opts.tema] ?? PALS.tecnico;
  const P = personParts(o);
  const all = [...P.back, ...P.torso, ...P.front, ...P.head];
  const giz = o.acab === "giz";
  const transparente = o.cor === "transparente";
  const fill = o.cor === "cinza" ? pal.fills.cinza : pal.fills.branco;
  const k = o.traco ?? 1;
  const H = o.altura_m;
  ctx.save();
  ctx.translate(o.x, o.y);
  ctx.rotate((o.rotation * PI) / 180);
  ctx.scale(ppm, ppm);
  ctx.lineJoin = "round";
  ctx.lineCap = "round";
  const borda = (giz ? lw(1.2, 0.022) : lw(0.6, 0.008)) * k;
  const cor = giz ? pal.giz : pal.linha;
  if (transparente) {
    // Só o contorno: traça a silhueta num canvas auxiliar e recorta o miolo (sem apagar a pista embaixo).
    outlineOnly(ctx, all, cor, borda * 2, H);
    ctx.restore();
    return;
  }
  // Silhueta única: traço gordo por baixo e preenchimento por cima deixam um só contorno.
  ctx.strokeStyle = cor;
  ctx.lineWidth = borda * 2;
  for (const p of all) ctx.stroke(p);
  const lat = !P.empe && personRig(o).lat;
  for (const layer of [P.back, P.torso, P.front, P.head]) {
    ctx.fillStyle = layer === P.back && lat ? shade(fill, -0.04) : fill;
    for (const p of layer) ctx.fill(p);
  }
  if (!giz) {
    const skull = P.head[P.head.length - 1]!;
    if (P.hair && P.hairAxis) {
      // Cabelo em traço: contorno e fios curtos.
      ctx.save();
      ctx.clip(skull);
      ctx.save();
      ctx.clip(P.hair);
      ctx.strokeStyle = pal.det;
      ctx.lineWidth = lw(0.6, 0.004);
      const { c, hd } = P.hairAxis;
      const hn = { x: -hd.y, y: hd.x };
      for (let i = -4; i <= 4; i++) {
        const q = vadd(c, vmul(hn, i * 0.012 * H));
        ctx.beginPath();
        ctx.moveTo(q.x - hd.x * 0.09 * H, q.y - hd.y * 0.09 * H);
        ctx.quadraticCurveTo(q.x + hn.x * 0.01 * H, q.y + hn.y * 0.01 * H, q.x + hd.x * 0.09 * H, q.y + hd.y * 0.09 * H);
        ctx.stroke();
      }
      ctx.restore();
      ctx.strokeStyle = pal.det;
      ctx.lineWidth = lw(0.8, 0.006);
      ctx.stroke(P.hair);
      ctx.restore();
    }
    // Linha interna onde braço ou perna passa por cima do tronco ou do outro membro.
    const under = new Path2D();
    for (const p of [...P.back, ...P.torso]) under.addPath(p);
    ctx.save();
    ctx.clip(under);
    ctx.strokeStyle = pal.linha;
    ctx.lineWidth = lw(0.8, 0.008);
    for (const p of P.front) ctx.stroke(p);
    ctx.restore();
    if (P.empe) {
      // Seta curta à frente: para onde a pessoa está virada.
      const w = 0.025 * H;
      const y0 = -0.1 * H;
      const y1 = -0.16 * H;
      ctx.strokeStyle = pal.linha;
      ctx.lineWidth = lw(1, 0.012);
      ctx.beginPath();
      ctx.moveTo(-w, y1 + w);
      ctx.lineTo(0, y1);
      ctx.lineTo(w, y1 + w);
      ctx.moveTo(0, y1);
      ctx.lineTo(0, y0);
      ctx.stroke();
    }
  }
  ctx.restore();
}

/** Contorno da união das partes, sem preenchimento (o miolo fica transparente). */
function outlineOnly(ctx: CanvasRenderingContext2D, paths: Path2D[], color: string, width: number, H: number): void {
  const m = ctx.getTransform();
  // Caixa do corpo (m, local) levada ao canvas e cortada pela área visível.
  const r = 0.85 * H;
  const pts = [[-r, -r], [r, -r], [r, r], [-r, r]].map(([x, y]) => ({ x: m.a * x! + m.c * y! + m.e, y: m.b * x! + m.d * y! + m.f }));
  const x0 = Math.max(0, Math.floor(Math.min(...pts.map((p) => p.x))));
  const y0 = Math.max(0, Math.floor(Math.min(...pts.map((p) => p.y))));
  const x1 = Math.min(ctx.canvas.width, Math.ceil(Math.max(...pts.map((p) => p.x))));
  const y1 = Math.min(ctx.canvas.height, Math.ceil(Math.max(...pts.map((p) => p.y))));
  if (x1 <= x0 || y1 <= y0) return;
  const off = document.createElement("canvas");
  off.width = x1 - x0;
  off.height = y1 - y0;
  const o = off.getContext("2d");
  if (!o) return;
  o.setTransform(m.a, m.b, m.c, m.d, m.e - x0, m.f - y0);
  o.lineJoin = "round";
  o.lineCap = "round";
  o.strokeStyle = color;
  o.lineWidth = width;
  for (const p of paths) o.stroke(p);
  o.globalCompositeOperation = "destination-out";
  o.fillStyle = "#000";
  for (const p of paths) o.fill(p);
  ctx.save();
  ctx.setTransform(1, 0, 0, 1, 0, 0);
  ctx.drawImage(off, x0, y0);
  ctx.restore();
}

/** Área clicável: a própria silhueta (no canvas de hit do Konva, já em px de mundo). */
export function hitPerson(ctx: CanvasRenderingContext2D, o: SicroPersonObject, ppm: number, color: string): void {
  const P = personParts(o);
  ctx.save();
  ctx.translate(o.x, o.y);
  ctx.rotate((o.rotation * PI) / 180);
  ctx.scale(ppm, ppm);
  ctx.fillStyle = color;
  ctx.strokeStyle = color;
  ctx.lineWidth = 6 / ppm;
  for (const p of [...P.back, ...P.torso, ...P.front, ...P.head]) {
    ctx.fill(p);
    ctx.stroke(p);
  }
  ctx.restore();
}
