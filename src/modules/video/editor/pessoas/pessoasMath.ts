/** Tempo e caixas das pessoas — a mesma regra do Rust (`video/pessoas/mod.rs`). */

import type { PessoaView } from "@domain/video_pessoas";

export interface Cadencia {
  /** Intervalo típico entre quadros do vídeo (s). */
  frameDur: number;
  /** Um quadro analisado a cada `passo`. */
  passo: number;
}

export interface Caixa {
  x: number;
  y: number;
  w: number;
  h: number;
}

const folga = (c: Cadencia) => c.frameDur * c.passo * 1.5;

/** Índice do último quadro com t <= alvo (busca binária), ou -1. */
export function ultimoAte(caixas: PessoaView["caixas"], t: number): number {
  let lo = 0;
  let hi = caixas.length;
  while (lo < hi) {
    const mid = (lo + hi) >> 1;
    if (caixas[mid]![0] <= t + 1e-6) lo = mid + 1;
    else hi = mid;
  }
  return lo - 1;
}

/** Caixa mostrada no instante `t`: a do quadro exibido; com passo > 1 caminha entre analisados. */
export function caixaEm(p: PessoaView, t: number, c: Cadencia): Caixa | null {
  const k = ultimoAte(p.caixas, t);
  if (k < 0) return null;
  const a = p.caixas[k]!;
  if (t - a[0] > folga(c)) return null;
  if (c.passo > 1) {
    const n = p.caixas[k + 1];
    if (n && n[0] > a[0] && n[0] - a[0] <= folga(c)) {
      const f = Math.min(1, Math.max(0, (t - a[0]) / (n[0] - a[0])));
      return {
        x: a[1] + (n[1] - a[1]) * f,
        y: a[2] + (n[2] - a[2]) * f,
        w: a[3] + (n[3] - a[3]) * f,
        h: a[4] + (n[4] - a[4]) * f,
      };
    }
  }
  return { x: a[1], y: a[2], w: a[3], h: a[4] };
}

/** Instante do quadro analisado mais próximo de `t` (onde a correção fica guardada). */
export function quadroMaisProximo(p: PessoaView, t: number): number | null {
  if (p.caixas.length === 0) return null;
  const k = ultimoAte(p.caixas, t);
  const a = p.caixas[Math.max(0, k)]!;
  const b = p.caixas[Math.min(p.caixas.length - 1, k + 1)]!;
  return Math.abs(b[0] - t) < Math.abs(a[0] - t) ? b[0] : a[0];
}

/** Trechos contínuos em que a pessoa aparece (para a faixa da linha do tempo). */
export function segmentos(p: PessoaView, c: Cadencia): [number, number][] {
  const out: [number, number][] = [];
  const g = folga(c);
  let ini: number | null = null;
  let prev = 0;
  for (const cx of p.caixas) {
    const t = cx[0];
    if (ini === null) ini = t;
    else if (t - prev > g) {
      out.push([ini, prev + c.frameDur]);
      ini = t;
    }
    prev = t;
  }
  if (ini !== null) out.push([ini, prev + c.frameDur]);
  return out;
}

/** Quem aparece no instante `t`. */
export function visiveisEm(ps: PessoaView[], t: number, c: Cadencia): PessoaView[] {
  return ps.filter((p) => caixaEm(p, t, c) !== null);
}
