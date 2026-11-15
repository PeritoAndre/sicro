/**
 * Alças do alvo de `ocultar` e `holofote` no palco (§8.3): as da caixa do rastreio (do registro) e, na forma
 * "Polígono", as dos vértices (`alvo.vertices`, 0..1 da caixa com a margem; `Mudanca` `{ efeito }`). Dono: P2.6.
 */

import type { Alca, Caixa, EfHolofote, EfOcultar, Ponto, QuadroPalco } from "@domain/edicao";
import { comMargem, inversa, ponto, type Afim } from "../../modelo/geometria";

const lim = (v: number) => Math.min(1, Math.max(0, v));

/** Vértices iniciais do "Polígono": os cantos da caixa com a margem. */
export const VERTICES_PADRAO: Ponto[] = [
  { x: 0, y: 0 },
  { x: 1, y: 0 },
  { x: 1, y: 1 },
  { x: 0, y: 1 },
];

export function alcasDoAlvo(e: EfOcultar | EfHolofote, q: QuadroPalco, daCaixa: Alca[]): Alca[] {
  const vs = e.alvo.forma === "poligono" ? e.alvo.vertices : null;
  const cx = q.rastreio(e.alvo.rastreio);
  const A = q.A as Afim;
  const inv = inversa(A);
  if (!vs || vs.length < 3 || !cx || !inv) return daCaixa;
  const c: Caixa = comMargem(cx, e.alvo.margem);
  const naCaixa = (p: Ponto): Ponto => ({ x: c.x + p.x * c.w, y: c.y + p.y * c.h });
  const vertices = vs.map(
    (p, i): Alca => ({
      id: `${e.id}:vertice:${i}`,
      tipo: "ponto",
      em: ponto(A, naCaixa(p)),
      cursor: "move",
      arrastar: (de, para) => {
        // o deslocamento volta ao espaço do rastreio e vira fração da caixa
        const a = ponto(inv, de);
        const b = ponto(inv, para);
        const novo: Ponto = { x: c.w > 0 ? lim(p.x + (b.x - a.x) / c.w) : p.x, y: c.h > 0 ? lim(p.y + (b.y - a.y) / c.h) : p.y };
        const alvo = { ...e.alvo, vertices: vs.map((v, k) => (k === i ? novo : v)) };
        return { efeito: { ...e, alvo } as EfOcultar | EfHolofote };
      },
    }),
  );
  return [...daCaixa, ...vertices];
}
