/** Onda do som de um item (picos de `edicao_onda`, P1.9) num canvas do tamanho da parte visível do item. Dono: P4.1. */

import { memo, useEffect, useRef } from "react";
import type { Onda as Picos, Seg } from "@domain/edicao";
import { picoDe } from "./ondas";

export interface OndaProps {
  picos: Picos | null;
  /** Faixa de fonte mostrada no canvas. */
  de_s: Seg;
  ate_s: Seg;
  largura: number;
  altura: number;
  // CONTRATO: acréscimo do P4.1: onde o canvas começa dentro do item (só a parte visível é desenhada).
  x?: number;
}

/** Colunas (uma por px) com o menor e o maior pico na fatia de tempo dela, na escala do maior pico da mídia. */
export function colunasDaOnda(o: Picos, de: Seg, ate: Seg, largura: number): Float32Array {
  const n = Math.max(0, Math.floor(largura));
  const r = new Float32Array(n * 2);
  if (n === 0 || !(ate > de) || o.min.length === 0) return r;
  const pico = picoDe(o);
  const por = (ate - de) / n;
  for (let x = 0; x < n; x++) {
    const a = Math.floor((de + x * por - o.inicio_s) * o.por_s);
    const b = Math.max(a + 1, Math.ceil((de + (x + 1) * por - o.inicio_s) * o.por_s));
    let lo = 0;
    let hi = 0;
    for (let i = Math.max(0, a); i < Math.min(b, o.min.length); i++) {
      const mi = o.min[i] as number;
      const ma = o.max[i] as number;
      if (mi < lo) lo = mi;
      if (ma > hi) hi = ma;
    }
    r[2 * x] = lo / pico;
    r[2 * x + 1] = hi / pico;
  }
  return r;
}

export const Onda = memo(function Onda({ picos, de_s, ate_s, largura, altura, x = 0 }: OndaProps) {
  const ref = useRef<HTMLCanvasElement>(null);
  const w = Math.max(1, Math.floor(largura));
  const h = Math.max(1, Math.floor(altura));
  useEffect(() => {
    const cv = ref.current;
    const g = cv?.getContext?.("2d");
    if (!cv || !g) return;
    g.clearRect(0, 0, cv.width, cv.height);
    if (!picos) return;
    const col = colunasDaOnda(picos, de_s, ate_s, w);
    const meio = h / 2;
    g.fillStyle = "rgba(200, 245, 225, 0.8)";
    for (let i = 0; i < w; i++) {
      const lo = col[2 * i] as number;
      const hi = col[2 * i + 1] as number;
      const y0 = meio - hi * meio * 0.95;
      const y1 = meio - lo * meio * 0.95;
      g.fillRect(i, y0, 1, Math.max(1, y1 - y0));
    }
  }, [picos, de_s, ate_s, w, h]);
  return <canvas ref={ref} width={w} height={h} style={{ left: x, width: w }} data-onda="" aria-hidden />;
});
