/**
 * Contador de marcações à mão no painel do efeito (contratos-fase4.md D12): âncoras do perito, fora as que o
 * "Simplificar" escolheu e ele não mexeu (`Rastreio.ancoras_do_programa`). Mesma conta do `ProvRastreio.ancoras` do
 * motor (`motor/mod.rs::do_programa`, folga de 1e-6 s no `t`), que o PDF escreve como "marcações do perito: n".
 * Puro. Dono: pacote "Pessoas na Edição e pequenos".
 */

import type { Rastreio } from "@domain/edicao";

/** Folga do `t` da âncora do programa (a do Rust). */
const FOLGA_T = 1e-6;

export function marcadasAMao(r: Pick<Rastreio, "ancoras" | "ancoras_do_programa">): number {
  const programa = r.ancoras_do_programa ?? [];
  if (programa.length === 0) return r.ancoras.length;
  // ordenada: a lista do Simplificar de um efeito longo tem milhares
  const ts = [...programa].sort((a, b) => a - b);
  // a busca acha a vizinhança; a comparação é a do Rust (`(x − t).abs() <= 1e-6`), igual até na borda
  const doPrograma = (t: number) => {
    let lo = 0;
    let hi = ts.length;
    while (lo < hi) {
      const m = (lo + hi) >> 1;
      if ((ts[m] as number) < t - 2 * FOLGA_T) lo = m + 1;
      else hi = m;
    }
    for (let i = lo; i < ts.length && (ts[i] as number) <= t + 2 * FOLGA_T; i++) if (Math.abs((ts[i] as number) - t) <= FOLGA_T) return true;
    return false;
  };
  let n = 0;
  for (const k of r.ancoras) if (!doPrograma(k.t)) n++;
  return n;
}
