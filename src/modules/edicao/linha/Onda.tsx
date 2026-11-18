/** Ondas do som (picos de `edicao_onda`, P1.9): UM canvas por trilha, na parte desenhada da pista, com a onda de cada
 *  item no lugar dele (um canvas por item eram centenas com 200 clipes). Dono: P4.1. */

import { memo, useEffect, useRef } from "react";
import type { Onda as Picos, Racional, Seg } from "@domain/edicao";
import type { ItemNaLinha } from "./modelo";
import type { FonteDoItem } from "./ClipeNaLinha";
import { ondas, picoDe, useOndasVersao } from "./ondas";
import styles from "./Linha.module.css";

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

const COR = "rgba(190, 240, 220, 0.78)";

function desenhar(g: CanvasRenderingContext2D, col: Float32Array, x0: number, h: number) {
  const meio = h / 2;
  for (let i = 0; i < col.length / 2; i++) {
    const lo = col[2 * i] as number;
    const hi = col[2 * i + 1] as number;
    const y0 = meio - hi * meio * 0.95;
    const y1 = meio - lo * meio * 0.95;
    g.fillRect(x0 + i, y0, 1, Math.max(1, y1 - y0));
  }
}

/** A onda de um trecho de fonte num canvas do tamanho dado (peça avulsa; a linha usa `OndasDaTrilha`). */
export const Onda = memo(function Onda({ picos, de_s, ate_s, largura, altura, x = 0 }: OndaProps) {
  const ref = useRef<HTMLCanvasElement>(null);
  const w = Math.max(1, Math.floor(largura));
  const h = Math.max(1, Math.floor(altura));
  useEffect(() => {
    const g = ref.current?.getContext?.("2d");
    if (!g) return;
    g.clearRect(0, 0, w, h);
    if (!picos) return;
    g.fillStyle = COR;
    desenhar(g, colunasDaOnda(picos, de_s, ate_s, w), 0, h);
  }, [picos, de_s, ate_s, w, h]);
  return <canvas ref={ref} width={w} height={h} style={{ left: x, width: w }} data-onda="" aria-hidden />;
});

export interface OndasDaTrilhaProps {
  itens: readonly ItemNaLinha[];
  fonteDe(it: ItemNaLinha): FonteDoItem | null;
  ws: string;
  /** Zoom desenhado (px/s) e a taxa: px por slot. */
  pxPorS: number;
  fps: Racional;
  /** Parte desenhada da pista (px no zoom desenhado). */
  janela: readonly [number, number];
  /** Largura da pista (px no zoom desenhado): a janela não passa dela. */
  largura: number;
  altura: number;
}

/** Topo da onda dentro da trilha: abaixo do nome do item (maquete: 16 px dentro do clipe). */
const TOPO = 19;

export const OndasDaTrilha = memo(function OndasDaTrilha(p: OndasDaTrilhaProps) {
  const versao = useOndasVersao((s) => s.versao);
  const ref = useRef<HTMLCanvasElement>(null);
  const px = (p.pxPorS * p.fps.den) / p.fps.num;
  const x0 = Math.max(0, Math.floor(Number.isFinite(p.janela[0]) ? p.janela[0] : 0));
  const x1 = Math.min(Math.ceil(Number.isFinite(p.janela[1]) ? p.janela[1] : p.largura), Math.ceil(p.largura), x0 + 16000);
  const w = Math.max(1, x1 - x0);
  const h = Math.max(1, p.altura - TOPO - 3);
  useEffect(() => {
    const g = ref.current?.getContext?.("2d");
    if (!g) return;
    g.clearRect(0, 0, w, h);
    g.fillStyle = COR;
    for (const it of p.itens) {
      const a = it.de_q * px;
      const b = it.ate_q * px;
      if (b <= x0 || a >= x1 || b - a < 2) continue;
      const f = p.fonteDe(it);
      const o = f ? ondas.onda(p.ws, f.sha256) : null;
      if (!f || !o) continue;
      // a parte do item que cai no canvas, 1 px para dentro de cada borda
      const xs = Math.max(a + 1, x0);
      const xe = Math.min(b - 1, x1);
      if (xe - xs < 1) continue;
      const k = (f.ate_s - f.de_s) / (b - a);
      desenhar(g, colunasDaOnda(o, f.de_s + (xs - a) * k, f.de_s + (xe - a) * k, xe - xs), Math.round(xs - x0), h);
    }
  }, [p.itens, p.fonteDe, p.ws, px, x0, x1, w, h, versao]); // eslint-disable-line react-hooks/exhaustive-deps
  return (
    <canvas
      ref={ref}
      className={styles.ondas}
      width={w}
      height={h}
      // posição pelo zoom assentado; até a árvore assentar no novo, estica com ele
      style={{ left: `calc(var(--px) * ${x0 / px}px)`, width: w, top: TOPO, height: h, transform: `scaleX(calc(var(--px) / ${px}))` }}
      data-onda=""
      aria-hidden
    />
  );
});
