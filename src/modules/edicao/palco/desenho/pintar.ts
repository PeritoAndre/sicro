/**
 * Pintor do palco (§7.2 `pintor`): fundo, UM `drawImage(video)` com A, depois as `Primitiva`s (§8.3). Implementa
 * todos os `k` uma vez; efeito novo só devolve primitivas. Canvas 2D sem `ctx.filter` (não existe no WebKitGTK).
 * Mosaico e desfoque por reduzir e ampliar a região (a região e o instante são os reais; a qualidade é a do render).
 * Borda suave e lupa: aproximadas/ausentes na prévia viva (§7.4).
 */

import type { Lugar, Primitiva } from "@domain/edicao";
import type { Afim } from "../../modelo/geometria";
import { CONTORNO } from "./primitivas";

export interface Pintura {
  ctx: CanvasRenderingContext2D;
  /** Canvas em px. */
  w: number;
  h: number;
  fundo: string;
  /** Quadro de vídeo e o A dele (fonte 0..1 → canvas 0..1); null = sem vídeo (título, imagem). */
  video: CanvasImageSource | null;
  A: Afim;
  primitivas: readonly Primitiva[];
  /** Imagem de um `src` de primitiva (sprite, atlas, "midia:<id>"); null = ainda carregando. */
  imagem(src: string): CanvasImageSource | null;
  /** Canvas de rascunho para mosaico e desfoque (um por palco). */
  rascunho?: () => HTMLCanvasElement | OffscreenCanvas | null;
}

type Caminho = Extract<Primitiva, { k: "caminho" }>;
type Elipse = Extract<Primitiva, { k: "elipse" }>;
type Regiao = Extract<Primitiva, { k: "regiao" }>;

const fatorH = (h: Lugar["h"]) => (h === "esq" ? 0 : h === "centro" ? 0.5 : 1);
const fatorV = (v: Lugar["v"]) => (v === "topo" ? 0 : v === "meio" ? 0.5 : 1);

/** "#rrggbb[aa]" → cor do canvas (o 2D aceita as duas formas). */
const cor = (c: string) => c;

/** Transforma 0..1 (do espaço `A` ou do canvas) em px. */
function emPx(p: Pintura, A: Afim | null) {
  const { w: W, h: H } = p;
  if (!A) return (x: number, y: number): [number, number] => [x * W, y * H];
  const [a, b, c, d, e, f] = A;
  return (x: number, y: number): [number, number] => [(a * x + c * y + e) * W, (b * x + d * y + f) * H];
}

function tracarForma(ctx: CanvasRenderingContext2D, f: Caminho | Elipse, px: (x: number, y: number) => [number, number]) {
  if (f.k === "elipse") {
    // elipse num espaço com A: pela caixa de 4 pontos (giro de 90° troca os raios)
    const [cx, cy] = px(f.cx, f.cy);
    const [ax, ay] = px(f.cx + f.rx, f.cy);
    const [bx, by] = px(f.cx, f.cy + f.ry);
    const rx = Math.hypot(ax - cx, ay - cy);
    const ry = Math.hypot(bx - cx, by - cy);
    const rot = Math.atan2(ay - cy, ax - cx);
    ctx.ellipse(cx, cy, Math.max(rx, 0), Math.max(ry, 0), rot, 0, Math.PI * 2);
    return;
  }
  for (const s of f.d) {
    if (s[0] === "M") ctx.moveTo(...px(s[1], s[2]));
    else if (s[0] === "L") ctx.lineTo(...px(s[1], s[2]));
    else if (s[0] === "C") {
      const [x1, y1] = px(s[1], s[2]);
      const [x2, y2] = px(s[3], s[4]);
      const [x3, y3] = px(s[5], s[6]);
      ctx.bezierCurveTo(x1, y1, x2, y2, x3, y3);
    } else ctx.closePath();
  }
}

function formaSimples(p: Pintura, f: Caminho | Elipse) {
  const { ctx } = p;
  const px = emPx(p, null);
  ctx.beginPath();
  tracarForma(ctx, f, px);
  if (f.preenche) {
    ctx.fillStyle = cor(f.preenche);
    ctx.fill(f.k === "caminho" && f.regra === "evenodd" ? "evenodd" : "nonzero");
  }
  const t = f.traco;
  if (t) {
    ctx.lineJoin = "round";
    ctx.lineCap = "round";
    if (t.contorno) {
      ctx.strokeStyle = cor(t.contorno);
      ctx.lineWidth = t.largura * p.h * CONTORNO;
      ctx.stroke();
    }
    ctx.strokeStyle = cor(t.cor);
    ctx.lineWidth = t.largura * p.h;
    ctx.stroke();
  }
}

/** Caixa envolvente (px) da forma de uma região. */
function envolvente(f: Caminho | Elipse, px: (x: number, y: number) => [number, number]): [number, number, number, number] {
  const pts: [number, number][] =
    f.k === "elipse"
      ? [px(f.cx - f.rx, f.cy - f.ry), px(f.cx + f.rx, f.cy - f.ry), px(f.cx + f.rx, f.cy + f.ry), px(f.cx - f.rx, f.cy + f.ry)]
      : f.d.flatMap((s) => (s[0] === "M" || s[0] === "L" ? [px(s[1], s[2])] : s[0] === "C" ? [px(s[1], s[2]), px(s[3], s[4]), px(s[5], s[6])] : []));
  let [x0, y0, x1, y1] = [Infinity, Infinity, -Infinity, -Infinity];
  for (const [x, y] of pts) {
    x0 = Math.min(x0, x);
    y0 = Math.min(y0, y);
    x1 = Math.max(x1, x);
    y1 = Math.max(y1, y);
  }
  return [x0, y0, x1, y1];
}

/** Reduz a área (px) a `fator` e amplia de volta: mosaico sem suavizar, desfoque suavizado em passos. */
function reduzirAmpliar(p: Pintura, x: number, y: number, w: number, h: number, alvoW: number, alvoH: number, suave: boolean) {
  const r = p.rascunho?.();
  if (!r || w < 1 || h < 1) return;
  const ctx = p.ctx;
  const rw = Math.max(1, Math.round(alvoW));
  const rh = Math.max(1, Math.round(alvoH));
  r.width = rw;
  r.height = rh;
  const rc = r.getContext("2d") as CanvasRenderingContext2D | null;
  if (!rc) return;
  rc.imageSmoothingEnabled = true;
  rc.drawImage(ctx.canvas, x, y, w, h, 0, 0, rw, rh);
  ctx.imageSmoothingEnabled = suave;
  ctx.drawImage(r as CanvasImageSource, 0, 0, rw, rh, x, y, w, h);
  ctx.imageSmoothingEnabled = true;
}

function regiao(p: Pintura, r: Regiao) {
  const forma = r.forma;
  if (forma.k !== "caminho" && forma.k !== "elipse") return;
  const { ctx } = p;
  const px = emPx(p, r.espaco === "fonte" ? p.A : null);
  ctx.save();
  ctx.beginPath();
  if (r.inverter) ctx.rect(0, 0, p.w, p.h);
  tracarForma(ctx, forma, px);
  const regra = r.inverter || (forma.k === "caminho" && forma.regra === "evenodd") ? "evenodd" : "nonzero";
  if (r.modo === "solido" || r.modo === "escurecer") {
    ctx.fillStyle = r.modo === "solido" ? cor(r.cor ?? "#000000ff") : `rgba(0,0,0,${Math.min(Math.max(r.intensidade, 0), 1)})`;
    ctx.fill(regra);
    ctx.restore();
    return;
  }
  ctx.clip(regra);
  let [x0, y0, x1, y1] = r.inverter ? [0, 0, p.w, p.h] : envolvente(forma, px);
  x0 = Math.max(0, Math.floor(x0));
  y0 = Math.max(0, Math.floor(y0));
  x1 = Math.min(p.w, Math.ceil(x1));
  y1 = Math.min(p.h, Math.ceil(y1));
  const w = x1 - x0;
  const h = y1 - y0;
  const menor = Math.max(1, Math.min(w, h));
  if (r.modo === "mosaico") {
    // round(12 − 11 × intensidade) células no lado menor (§2.1)
    const celulas = Math.max(1, Math.round(12 - 11 * r.intensidade));
    const lado = menor / celulas;
    reduzirAmpliar(p, x0, y0, w, h, w / lado, h / lado, false);
  } else {
    // raio = intensidade × 0,25 × lado menor; reduzir ao raio e voltar em 3 passos suaves
    const raio = Math.max(1, r.intensidade * 0.25 * menor);
    for (let i = 0; i < 3; i++) reduzirAmpliar(p, x0, y0, w, h, w / raio, h / raio, true);
  }
  ctx.restore();
}

function sprite(p: Pintura, s: Extract<Primitiva, { k: "sprite" }>) {
  const img = p.imagem(s.src);
  if (!img) return;
  const { ctx } = p;
  ctx.save();
  ctx.globalAlpha = Math.min(Math.max(s.opacidade, 0), 1);
  ctx.drawImage(img, s.caixa.x * p.w, s.caixa.y * p.h, s.caixa.w * p.w, s.caixa.h * p.h);
  ctx.restore();
}

function atlas(p: Pintura, a: Extract<Primitiva, { k: "atlas" }>) {
  const img = p.imagem(a.src);
  if (!img || !(a.celula.w > 0 && a.celula.h > 0)) return;
  const gh = a.altura * p.h;
  const gw = (a.celula.w * gh) / a.celula.h;
  const glifos = [...a.glifos];
  const texto = [...a.texto];
  const x0 = a.lugar.x * p.w - texto.length * gw * fatorH(a.lugar.h);
  const y0 = a.lugar.y * p.h - gh * fatorV(a.lugar.v);
  texto.forEach((ch, i) => {
    const k = glifos.indexOf(ch);
    if (k < 0) return;
    p.ctx.drawImage(img, k * a.celula.w, 0, a.celula.w, a.celula.h, x0 + i * gw, y0, gw, gh);
  });
}

export function pintar(p: Pintura): void {
  const { ctx } = p;
  ctx.setTransform(1, 0, 0, 1, 0, 0);
  ctx.globalAlpha = 1;
  ctx.fillStyle = cor(p.fundo);
  ctx.fillRect(0, 0, p.w, p.h);
  if (p.video) {
    // fonte 0..1 → canvas px; drawImage no quadrado unitário
    const [a, b, c, d, e, f] = p.A;
    ctx.save();
    ctx.setTransform(a * p.w, b * p.h, c * p.w, d * p.h, e * p.w, f * p.h);
    ctx.drawImage(p.video, 0, 0, 1, 1);
    ctx.restore();
  }
  for (const pr of p.primitivas) {
    switch (pr.k) {
      case "caminho":
      case "elipse":
        formaSimples(p, pr);
        break;
      case "regiao":
        regiao(p, pr);
        break;
      case "sprite":
        sprite(p, pr);
        break;
      case "atlas":
        atlas(p, pr);
        break;
      case "lupa":
        // Fase 1
        break;
    }
  }
}
