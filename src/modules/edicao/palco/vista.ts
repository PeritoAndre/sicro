/**
 * Zoom e deslocamento da VISTA do palco (§7.2): só a tela muda; o documento e o efeito de enquadramento, não.
 * Tudo em px da caixa do palco. O tamanho vem da saída do projeto, nunca do `<video>`.
 */

import type { Ponto } from "@domain/edicao";

export interface Tamanho {
  w: number;
  h: number;
}
export interface Retangulo {
  x: number;
  y: number;
  w: number;
  h: number;
}
/** `s` = zoom; (tx, ty) = deslocamento em px. Retângulo mostrado = (img.x·s + tx, img.y·s + ty, img.w·s, img.h·s). */
export interface Vista {
  s: number;
  tx: number;
  ty: number;
}

export const VISTA_INICIAL: Vista = Object.freeze({ s: 1, tx: 0, ty: 0 });
/** Zoom máximo da vista: 16× ou até cada pixel da saída ocupar 40 px de tela. */
export const ZOOM_MAX = 16;
const PX_POR_PIXEL = 40;
/** Passo dos atalhos de zoom. */
export const PASSO_ZOOM = 1.25;

/** Medida da caixa do palco; 0×0 = palco escondido (`hidden`): vale a anterior. */
export function caixaMedida(anterior: Tamanho, w: number, h: number): Tamanho {
  if (w === 0 && h === 0) return anterior;
  return anterior.w === w && anterior.h === h ? anterior : { w, h };
}

/** Canvas da saída contido na caixa (proporção mantida, centrado). */
export function contido(caixa: Tamanho, saida: Tamanho): Retangulo {
  if (!(caixa.w > 0 && caixa.h > 0 && saida.w > 0 && saida.h > 0)) return { x: 0, y: 0, w: 0, h: 0 };
  const r = saida.w / saida.h;
  const w = caixa.w / caixa.h > r ? caixa.h * r : caixa.w;
  const h = w / r;
  return { x: (caixa.w - w) / 2, y: (caixa.h - h) / 2, w, h };
}

/** Teto do zoom para essa saída mostrada nessa largura. */
export function zoomMaximo(img: Retangulo, saida: Tamanho): number {
  return img.w > 0 ? Math.max(ZOOM_MAX, (PX_POR_PIXEL * saida.w) / img.w) : ZOOM_MAX;
}

/** Imagem maior que a caixa não deixa sobrar borda; menor fica centrada naquele eixo. */
export function limitar(v: Vista, caixa: Tamanho, img: Retangulo, max = ZOOM_MAX): Vista {
  const s = Math.min(Math.max(Number.isFinite(v.s) ? v.s : 1, 1), max);
  const eixo = (t: number, tam: number, c: number, cs: number) =>
    s * cs >= tam ? Math.min(-s * c, Math.max(tam - s * (c + cs), Number.isFinite(t) ? t : 0)) : (tam - s * cs) / 2 - s * c;
  return { s, tx: eixo(v.tx, caixa.w, img.x, img.w), ty: eixo(v.ty, caixa.h, img.y, img.h) };
}

/** Zoom por `fator` com o ponto `p` (px da caixa) parado. */
export function zoomEm(v: Vista, fator: number, p: { x: number; y: number }, caixa: Tamanho, img: Retangulo, max = ZOOM_MAX): Vista {
  const s2 = Math.min(Math.max(v.s * fator, 1), max);
  const k = s2 / v.s;
  return limitar({ s: s2, tx: p.x - (p.x - v.tx) * k, ty: p.y - (p.y - v.ty) * k }, caixa, img, max);
}

export function deslocar(v: Vista, dx: number, dy: number, caixa: Tamanho, img: Retangulo, max = ZOOM_MAX): Vista {
  return limitar({ s: v.s, tx: v.tx + dx, ty: v.ty + dy }, caixa, img, max);
}

/** Retângulo da saída na tela com a vista. */
export function mostrado(v: Vista, img: Retangulo): Retangulo {
  return { x: img.x * v.s + v.tx, y: img.y * v.s + v.ty, w: img.w * v.s, h: img.h * v.s };
}

/** px da caixa → 0..1 do canvas da saída. */
export function daTela(v: Vista, img: Retangulo, p: { x: number; y: number }): Ponto {
  const m = mostrado(v, img);
  return { x: m.w > 0 ? (p.x - m.x) / m.w : 0, y: m.h > 0 ? (p.y - m.y) / m.h : 0 };
}

/** 0..1 do canvas da saída → px da caixa. */
export function paraTela(v: Vista, img: Retangulo, p: Ponto): { x: number; y: number } {
  const m = mostrado(v, img);
  return { x: m.x + p.x * m.w, y: m.y + p.y * m.h };
}

/** Parte do retângulo `r` que cabe na caixa do palco (com zoom o quadro passa das bordas: o que se desenha sobre ele
 *  e precisa ser visto, como a barra da corrida, fica nesta parte). */
export function visivel(r: Retangulo, caixa: Tamanho): Retangulo {
  const x0 = Math.max(r.x, 0);
  const y0 = Math.max(r.y, 0);
  const x1 = Math.min(r.x + r.w, caixa.w);
  const y1 = Math.min(r.y + r.h, caixa.h);
  return { x: x0, y: y0, w: Math.max(0, x1 - x0), h: Math.max(0, y1 - y0) };
}
