/**
 * Espelho de `src-tauri/src/edicao/geometria.rs` (D14, §3.2): `A(τ)` origem → canvas e caixas.
 * Só `+ − × ÷`, na ordem do Rust; toda saída de `Afim` passa por `+ 0` (−0 vira 0).
 * Fixture: `geometria-a.json`.
 */

import type { Caixa, Desconhecido, EfEnquadramento, EfGirar, Efeito, Ponto } from "@domain/edicao";
import { avaliar } from "./curva";
import { efeitoConhecido } from "./projeto";

/** [a, b, c, d, e, f] na ordem do canvas 2D: x' = a·x + c·y + e; y' = b·x + d·y + f (0..1). */
export type Afim = [number, number, number, number, number, number];
export interface Tamanho {
  w: number;
  h: number;
}

export const IDENTIDADE: Afim = [1, 0, 0, 1, 0, 0];
const INTEIRA: Caixa = { x: 0, y: 0, w: 1, h: 1 };

const z = (v: number) => v + 0;

export function ponto(m: Afim, p: Ponto): Ponto {
  const [a, b, c, d, e, f] = m;
  return { x: z(a * p.x + c * p.y + e), y: z(b * p.x + d * p.y + f) };
}

/** Caixa envolvente da caixa transformada. */
export function caixa(m: Afim, cx: Caixa): Caixa {
  const [a, b, c, d, e, f] = m;
  const neg = (v: number) => (v < 0 ? v : 0);
  const px = a * cx.x + c * cx.y + e;
  const py = b * cx.x + d * cx.y + f;
  const wx = a * cx.w;
  const wy = b * cx.w;
  const hx = c * cx.h;
  const hy = d * cx.h;
  return { x: z(px + neg(wx) + neg(hx)), y: z(py + neg(wy) + neg(hy)), w: z(Math.abs(wx) + Math.abs(hx)), h: z(Math.abs(wy) + Math.abs(hy)) };
}

/** Aplica `m` e depois `depois`. */
export function seguidaDe(m: Afim, depois: Afim): Afim {
  const [a, b, c, d, e, f] = m;
  const [a2, b2, c2, d2, e2, f2] = depois;
  return [z(a2 * a + c2 * b), z(b2 * a + d2 * b), z(a2 * c + c2 * d), z(b2 * c + d2 * d), z(a2 * e + c2 * f + e2), z(b2 * e + d2 * f + f2)];
}

/** Canvas → origem; null se degenerada. */
export function inversa(m: Afim): Afim | null {
  const [a, b, c, d, e, f] = m;
  const det = a * d - b * c;
  if (det === 0 || !Number.isFinite(det)) return null;
  const ai = d / det;
  const bi = -b / det;
  const ci = -c / det;
  const di = a / det;
  const r: Afim = [z(ai), z(bi), z(ci), z(di), z(-(ai * e + ci * f)), z(-(bi * e + di * f))];
  return r.every(Number.isFinite) ? r : null;
}

/** Graus (qualquer sinal) → quartos de volta horários, 0..3. */
function quartos(graus: number): number {
  const g = ((Math.trunc(graus) % 360) + 360) % 360;
  return Math.floor((g + 45) / 90) % 4;
}

/** Giro horário (múltiplo de 90) e, depois, os espelhos: transpose → hflip → vflip. */
export function afimGiro(graus: number, espelharH: boolean, espelharV: boolean): Afim {
  const q = quartos(graus);
  let g: Afim = q === 0 ? IDENTIDADE : q === 1 ? [0, 1, -1, 0, 1, 0] : q === 2 ? [-1, 0, 0, -1, 1, 1] : [0, -1, 1, 0, 0, 1];
  if (espelharH) g = seguidaDe(g, [-1, 0, 0, 1, 1, 0]);
  if (espelharV) g = seguidaDe(g, [1, 0, 0, -1, 0, 1]);
  return g;
}

/** Tamanho exibido (px): largura × sar arredondada, troca em 90°/270°, lados pares (mínimo 2). */
export function exibido(codificado: Tamanho, sar: { num: number; den: number }, giro: number): Tamanho {
  const [num, den] = sar.num > 0 && sar.den > 0 ? [sar.num, sar.den] : [1, 1];
  const w0 = Math.min(Math.floor((2 * codificado.w * num + den) / (2 * den)), 0xffffffff);
  const [w, h] = quartos(giro) % 2 === 1 ? [codificado.h, w0] : [w0, codificado.h];
  const par = (v: number) => Math.max(Math.floor(v / 2) * 2, 2);
  return { w: par(w), h: par(h) };
}

function giroDosEfeitos(efeitos: readonly (Efeito | Desconhecido)[]): [Afim, number] {
  let g = IDENTIDADE;
  let q = 0;
  for (const e of efeitos) {
    if (efeitoConhecido(e) && e.tipo === "girar" && e.ligado) {
      const gi = e as EfGirar;
      g = seguidaDe(g, afimGiro(gi.graus, gi.espelhar_h, gi.espelhar_v));
      q = (q + quartos(gi.graus)) % 4;
    }
  }
  return [g, q];
}

/** Primeiro `enquadramento` ligado (vale no clipe inteiro). */
function enquadramento(efeitos: readonly (Efeito | Desconhecido)[]): EfEnquadramento | null {
  for (const e of efeitos) if (efeitoConhecido(e) && e.tipo === "enquadramento" && e.ligado) return e;
  return null;
}

/** Região do enquadramento em `tau` (0..1 da origem girada); sem o efeito ou sem chaves, a imagem inteira. */
export function regiaoEm(efeitos: readonly (Efeito | Desconhecido)[], tau: number): Caixa {
  const q = enquadramento(efeitos);
  if (!q) return INTEIRA;
  if (Array.isArray(q.regiao) && q.regiao.length === 0) return INTEIRA;
  return avaliar(q.regiao, tau);
}

/** `A(τ)`: `girar` + região do `enquadramento` em `tau` + ajuste. `origem` e `canvas` em px. */
export function aDe(efeitos: readonly (Efeito | Desconhecido)[], tau: number, origem: Tamanho, canvas: Tamanho): Afim {
  const [g, q] = giroDosEfeitos(efeitos);
  const [wo, ho] = q % 2 === 1 ? [origem.h, origem.w] : [origem.w, origem.h];
  const ajuste = enquadramento(efeitos)?.ajuste ?? "conter";
  let r = regiaoEm(efeitos, tau);
  const valida = r.w > 0 && r.h > 0 && Number.isFinite(r.x) && Number.isFinite(r.y) && Number.isFinite(r.w) && Number.isFinite(r.h);
  if (!valida) r = INTEIRA;
  const p = r.w * wo * canvas.h;
  const qq = r.h * ho * canvas.w;
  if (!(p > 0 && qq > 0 && Number.isFinite(p) && Number.isFinite(qq))) return g;
  let sx: number;
  let sy: number;
  if (ajuste === "conter") [sx, sy] = p <= qq ? [p / qq, 1] : [1, qq / p];
  else [sx, sy] = p >= qq ? [p / qq, 1] : [1, qq / p];
  const kx = sx / r.w;
  const ky = sy / r.h;
  const ex = (1 - sx) * 0.5 - r.x * kx;
  const ey = (1 - sy) * 0.5 - r.y * ky;
  return seguidaDe(g, [kx, 0, 0, ky, ex, ey]);
}

/** Nome do §8.3 para `aDe`. */
export const A = aDe;

/** Caixa da origem levada ao canvas (0..1). */
export function caixaNoCanvas(m: Afim, c: Caixa): Caixa {
  return caixa(m, c);
}

/** Caixa com a margem (fração do lado), limitada a 0..1; margem negativa encolhe até o centro. */
export function comMargem(c: Caixa, margem: number): Caixa {
  const lado = (ini: number, tam: number): [number, number] => {
    const d = tam * margem;
    let a = ini - d;
    let b = ini + tam + d;
    if (b < a) {
      const meio = ini + tam * 0.5;
      a = meio;
      b = meio;
    }
    a = Math.min(Math.max(a, 0), 1);
    b = Math.min(Math.max(b, 0), 1);
    return [a, b - a];
  };
  const [x, w] = lado(c.x, c.w);
  const [y, h] = lado(c.y, c.h);
  return { x, y, w, h };
}
