/**
 * Espelho de `camada/lupa.rs` (P1.2): primitivas da lupa (`lupa` + moldura + ligação), pela fixture
 * `primitivas-lupa.json`, com as mesmas contas na mesma ordem: só `+ − × ÷`, `floor`, `abs` e comparações
 * explícitas (nada de `Math.min`/`Math.max`, que diferem no −0 do Rust). Ampliação em relação ao tamanho da janela
 * no canvas, por `A` do quadro. E a ampliação aproximada da prévia viva (§7.4).
 */

import type { Caixa, EfLupa, Primitiva, QuadroPalco, Traco } from "@domain/edicao";
import { caixa, ponto, type Afim } from "../../modelo/geometria";
import type { Pintura } from "./pintar";
import { COR_CONTORNO } from "./primitivas";

/** Ampliação aceita (§2.1). */
export const FATOR_MIN = 1.5;
export const FATOR_MAX = 8;

type Seg = Extract<Primitiva, { k: "caminho" }>["d"][number];
type Traco2 = NonNullable<Extract<Primitiva, { k: "caminho" }>["traco"]>;
type Caixa4 = [number, number, number, number];

const z = (v: number) => v + 0;
const erro = (campo: string) => new Error(`item_invalido: ${campo}`);
/** Par mais perto (meio sobe). */
const par = (v: number) => 2 * Math.floor(v * 0.5 + 0.5);
/** Maior par ≤ v. */
const parAbaixo = (v: number) => 2 * Math.floor(v * 0.5);
/** `v` em [lo, hi]; hi < lo vale lo. */
function limitar(v: number, lo: number, hi: number): number {
  const x = v > hi ? hi : v;
  return x < lo ? lo : x;
}

const traco2 = (t: Traco): Traco2 => (t.contorno ? { cor: t.cor, largura: t.espessura, contorno: COR_CONTORNO } : { cor: t.cor, largura: t.espessura });

function conferir(e: EfLupa): void {
  if (!(Number.isFinite(e.janela) && e.janela > 0 && e.janela <= 1)) throw erro("janela");
  if (!(Number.isFinite(e.fator) && e.fator >= FATOR_MIN && e.fator <= FATOR_MAX)) throw erro("fator");
  const finitos = e.onde.modo === "fixa" ? Number.isFinite(e.onde.x) && Number.isFinite(e.onde.y) : Number.isFinite(e.onde.dx) && Number.isFinite(e.onde.dy);
  if (!finitos) throw erro("onde");
}

/** Elipse inscrita ou retângulo na caixa (px do canvas). */
function forma(f: EfLupa["forma"], [x0, y0, x1, y1]: Caixa4, traco: Traco2, wc: number, hc: number): Primitiva {
  if (f === "circulo") {
    const rx = (x1 - x0) * 0.5;
    const ry = (y1 - y0) * 0.5;
    return { k: "elipse", cx: z((x0 + rx) / wc), cy: z((y0 + ry) / hc), rx: z(rx / wc), ry: z(ry / hc), traco };
  }
  const d: Seg[] = [["M", z(x0 / wc), z(y0 / hc)], ["L", z(x1 / wc), z(y0 / hc)], ["L", z(x1 / wc), z(y1 / hc)], ["L", z(x0 / wc), z(y1 / hc)], ["Z"]];
  return { k: "caminho", d, traco };
}

/** Reta da ligação: do meio do lado da janela ao meio do lado da lupa que se encaram; null = uma toca a outra. */
function linha(o: Caixa4, d: Caixa4): Caixa4 | null {
  let a = d[0] - o[2];
  let b = o[0] - d[2];
  const gx = a > b ? a : b;
  a = d[1] - o[3];
  b = o[1] - d[3];
  const gy = a > b ? a : b;
  if (gx <= 0 && gy <= 0) return null;
  const omx = (o[0] + o[2]) * 0.5;
  const omy = (o[1] + o[3]) * 0.5;
  const dmx = (d[0] + d[2]) * 0.5;
  const dmy = (d[1] + d[3]) * 0.5;
  if (gx >= gy) return d[0] >= o[2] ? [o[2], omy, d[0], dmy] : [o[0], omy, d[2], dmy];
  return d[1] >= o[3] ? [omx, o[3], dmx, d[1]] : [omx, o[1], dmx, d[3]];
}

export function primitivasLupa(e: EfLupa, q: QuadroPalco): Primitiva[] {
  conferir(e);
  const r = q.rastreio(e.rastreio);
  if (!r) return [];
  const cx = r.x + r.w * 0.5;
  const cy = r.y + r.h * 0.5;
  const A = q.A as Afim;
  const [a, b, c, d] = A;
  const { w: wc, h: hc } = q.canvas;
  // largura e altura inteiras da origem levadas ao canvas (px)
  const sw = Math.abs(a) * wc + Math.abs(b) * hc;
  const sh = Math.abs(c) * wc + Math.abs(d) * hc;
  if (!(sw > 0 && sh > 0 && Number.isFinite(sw) && Number.isFinite(sh) && Number.isFinite(cx) && Number.isFinite(cy))) return [];
  let [j, ow] = e.janela * sh >= sw ? [sw / sh, 1] : [e.janela, (e.janela * sh) / sw];
  // lupa que não cabe no canvas: fica do tamanho dele e a JANELA encolhe; a ampliação é a pedida
  const menor = wc < hc ? wc : hc;
  const teto = parAbaixo(menor);
  const cheio = par(e.fator * (j * sh));
  if (cheio > teto) {
    j = teto / (e.fator * sh);
    ow = (j * sh) / sw;
  }
  const oh = j;
  const ox = limitar(cx - ow * 0.5, 0, 1 - ow);
  const oy = limitar(cy - oh * 0.5, 0, 1 - oh);
  const origem: Caixa = { x: z(ox), y: z(oy), w: z(ow), h: z(oh) };
  const lado = limitar(cheio, 2, teto);
  let px: number;
  let py: number;
  if (e.onde.modo === "fixa") {
    px = e.onde.x * wc;
    py = e.onde.y * hc;
  } else {
    const p = ponto(A, { x: cx, y: cy });
    px = (p.x + e.onde.dx) * wc;
    py = (p.y + e.onde.dy) * hc;
  }
  const x0 = limitar(par(px - lado * 0.5), 0, parAbaixo(wc - lado));
  const y0 = limitar(par(py - lado * 0.5), 0, parAbaixo(hc - lado));
  const destino: Caixa = { x: z(x0 / wc), y: z(y0 / hc), w: z(lado / wc), h: z(lado / hc) };
  const dc: Caixa4 = [x0, y0, x0 + lado, y0 + lado];
  const traco = traco2(e.moldura);
  const v: Primitiva[] = [{ k: "lupa", origem, destino, forma: e.forma }, forma(e.forma, dc, traco, wc, hc)];
  if (e.ligacao) {
    const ob = caixa(A, origem);
    const oc: Caixa4 = [ob.x * wc, ob.y * hc, (ob.x + ob.w) * wc, (ob.y + ob.h) * hc];
    v.push(forma(e.forma, oc, traco, wc, hc));
    const l = linha(oc, dc);
    if (l)
      v.push({
        k: "caminho",
        d: [
          ["M", z(l[0] / wc), z(l[1] / hc)],
          ["L", z(l[2] / wc), z(l[3] / hc)],
        ],
        traco,
      });
  }
  return v;
}

type Forma = Extract<Primitiva, { k: "caminho" | "elipse" }>;

/** Caminho da forma de uma região da fonte (0..1) levado a px por `px`. */
function tracar(ctx: CanvasRenderingContext2D, f: Forma, px: (u: number, v: number) => [number, number]): void {
  if (f.k === "elipse") {
    const [cx, cy] = px(f.cx, f.cy);
    const [ax, ay] = px(f.cx + f.rx, f.cy);
    const [bx, by] = px(f.cx, f.cy + f.ry);
    ctx.ellipse(cx, cy, Math.hypot(ax - cx, ay - cy), Math.hypot(bx - cx, by - cy), Math.atan2(ay - cy, ax - cx), 0, Math.PI * 2);
    return;
  }
  for (const s of f.d) {
    if (s[0] === "M") ctx.moveTo(...px(s[1], s[2]));
    else if (s[0] === "L") ctx.lineTo(...px(s[1], s[2]));
    else if (s[0] === "C") ctx.bezierCurveTo(...px(s[1], s[2]), ...px(s[3], s[4]), ...px(s[5], s[6]));
    else ctx.closePath();
  }
}

/** Pinta a ampliação de uma primitiva `lupa`: o vídeo inteiro posto de modo que a janela de origem (0..1 da fonte)
 *  caia no destino, na orientação do vídeo (giro e espelho de `A`), recortado na forma (aproximada: a do render é a
 *  do ffmpeg). O que o quadro oculta (regiões no espaço da fonte) sai tapado também na ampliação, como no render,
 *  em que a cópia da lupa é tirada depois do `ocultar`. Sem vídeo, nada. */
export function pintarLupa(p: Pintura, l: Extract<Primitiva, { k: "lupa" }>): void {
  const { ctx, video } = p;
  const { origem: o, destino: d } = l;
  if (!video || !(o.w > 0 && o.h > 0 && d.w > 0 && d.h > 0)) return;
  const [x, y, w, h] = [d.x * p.w, d.y * p.h, d.w * p.w, d.h * p.h];
  // a janela como o vídeo a mostra (por A), esticada até o destino
  const [a, b, c, dd, e, f] = p.A;
  const ob = caixa(p.A, o);
  if (!(ob.w > 0 && ob.h > 0)) return;
  const kx = w / ob.w;
  const ky = h / ob.h;
  ctx.save();
  ctx.beginPath();
  if (l.forma === "circulo") ctx.ellipse(x + w / 2, y + h / 2, w / 2, h / 2, 0, 0, Math.PI * 2);
  else ctx.rect(x, y, w, h);
  ctx.clip();
  const t = [a * kx, b * ky, c * kx, dd * ky, x + (e - ob.x) * kx, y + (f - ob.y) * ky] as const;
  ctx.setTransform(...t);
  ctx.drawImage(video, 0, 0, 1, 1);
  // ocultado: tapado na cor do sólido; mosaico e desfoque, em cinza (a prévia não refaz o efeito ampliado)
  ctx.setTransform(1, 0, 0, 1, 0, 0);
  const px = (u: number, v: number): [number, number] => [t[0] * u + t[2] * v + t[4], t[1] * u + t[3] * v + t[5]];
  for (const r of p.primitivas) {
    if (r.k !== "regiao" || r.espaco !== "fonte" || (r.forma.k !== "caminho" && r.forma.k !== "elipse")) continue;
    ctx.beginPath();
    if (r.inverter) ctx.rect(x, y, w, h);
    tracar(ctx, r.forma, px);
    ctx.fillStyle = r.modo === "solido" ? (r.cor ?? "#000000ff") : "#808080";
    ctx.fill(r.inverter || (r.forma.k === "caminho" && r.forma.regra === "evenodd") ? "evenodd" : "nonzero");
  }
  ctx.restore();
}
