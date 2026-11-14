/** Colunas da Edição (§8.2): recolhíveis, com a largura guardada no navegador (conveniência de quem usa). */

import { useCallback, useState } from "react";

export interface Colunas {
  seq: number;
  insp: number;
  seqFechada: boolean;
  inspFechada: boolean;
}
const CHAVE_COLUNAS = "sicro.edicao.colunas.v1";
export const COLUNAS_PADRAO: Colunas = { seq: 300, insp: 340, seqFechada: false, inspFechada: false };
export const LIMITES = { seq: [220, 480], insp: [280, 520] } as const;

export function lerColunas(texto: string | null): Colunas {
  try {
    const o = JSON.parse(texto ?? "{}") as Partial<Colunas>;
    const n = (v: unknown, [a, b]: readonly [number, number], p: number) => (typeof v === "number" && Number.isFinite(v) ? Math.min(Math.max(v, a), b) : p);
    return {
      seq: n(o.seq, LIMITES.seq, COLUNAS_PADRAO.seq),
      insp: n(o.insp, LIMITES.insp, COLUNAS_PADRAO.insp),
      seqFechada: o.seqFechada === true,
      inspFechada: o.inspFechada === true,
    };
  } catch {
    return { ...COLUNAS_PADRAO };
  }
}

export function useColunas(): [Colunas, (f: (c: Colunas) => Colunas) => void] {
  const [c, setC] = useState<Colunas>(() => {
    try {
      return lerColunas(localStorage.getItem(CHAVE_COLUNAS));
    } catch {
      return { ...COLUNAS_PADRAO };
    }
  });
  const mudar = useCallback((f: (c: Colunas) => Colunas) => {
    setC((v) => {
      const n = f(v);
      try {
        localStorage.setItem(CHAVE_COLUNAS, JSON.stringify(n));
      } catch {
        /* sem armazenamento: vale nesta sessão */
      }
      return n;
    });
  }, []);
  return [c, mudar];
}

/** Fração máxima da largura disponível que cada lateral ocupa quando falta espaço (o palco fica com o resto). */
export const FRACAO = { seq: 0.25, insp: 0.3 } as const;

/** Largura de fato: a guardada, cedendo até o mínimo quando o palco ficaria estreito (janela mínima de 1100 px). */
export function efetivas(c: Colunas, disponivel: number): { seq: number; insp: number } {
  if (!(disponivel > 0)) return { seq: c.seq, insp: c.insp };
  return {
    seq: Math.min(c.seq, Math.max(LIMITES.seq[0], Math.round(disponivel * FRACAO.seq))),
    insp: Math.min(c.insp, Math.max(LIMITES.insp[0], Math.round(disponivel * FRACAO.insp))),
  };
}

/** Grade das colunas; `expandido` esconde as duas laterais (atalho de video.fullscreen). */
export function gradeDasColunas(c: Colunas, expandido: boolean, disponivel = 0): string {
  const e = efetivas(c, disponivel);
  const seq = expandido || c.seqFechada ? 0 : e.seq;
  const insp = expandido || c.inspFechada ? 0 : e.insp;
  return `${seq}px ${seq ? 4 : 0}px minmax(0, 1fr) ${insp ? 4 : 0}px ${insp}px`;
}

