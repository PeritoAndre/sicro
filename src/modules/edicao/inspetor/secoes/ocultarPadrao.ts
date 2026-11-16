/**
 * Padrão da ocultação por conteúdo (§6.6 (1)): placa, documento e tela → sólido; rosto, corpo e outro → desfoque 1,0.
 * Nenhum padrão pede ciência: com os limiares medidos (`regiao::{DESFOQUE_MIN, MOSAICO_MAX_CELULAS}`) o mosaico 0,75 de
 * antes era `ocultacao_fraca`. O "Conteúdo" do `OcultarEditor` e o `novos.ts::novoOcultar` perguntam aqui.
 * Espelho de `regiao::padrao` (Rust), conferido pela fixture `conferencia-padrao.json`.
 */

import type { EfOcultar } from "@domain/edicao";

const TEXTO: ReadonlySet<EfOcultar["conteudo"]> = new Set(["placa", "documento", "tela"]);

export function padraoDoConteudo(conteudo: EfOcultar["conteudo"]): Pick<EfOcultar, "modo" | "intensidade"> {
  return TEXTO.has(conteudo) ? { modo: "solido", intensidade: 1 } : { modo: "desfoque", intensidade: 1 };
}

/** Modo e intensidade ainda no padrão do conteúdo (no sólido a intensidade não conta). */
export function noPadrao(o: Pick<EfOcultar, "conteudo" | "modo" | "intensidade">): boolean {
  const p = padraoDoConteudo(o.conteudo);
  return o.modo === p.modo && (o.modo === "solido" || o.intensidade === p.intensidade);
}

/** "Conteúdo": o padrão do novo conteúdo entra só se o modo ainda era o do anterior (a escolha do perito fica). */
export function comConteudo(x: Pick<EfOcultar, "conteudo" | "modo" | "intensidade">, conteudo: EfOcultar["conteudo"]): void {
  const segue = noPadrao(x);
  x.conteudo = conteudo;
  if (segue) Object.assign(x, padraoDoConteudo(conteudo));
}
