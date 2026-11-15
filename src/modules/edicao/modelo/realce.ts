/**
 * Espelho de `motor::efeitos::realce::tabela` (P1.4): a tabela que o mapa de quadros do clipe usa (a da fonte; no
 * desentrelace por campo, uma entrada por campo). `modelo/tempo.ts` (resolver) pergunta aqui.
 * A tabela de campos fica na base da fonte: o primeiro campo guarda o pts do quadro e o segundo cai no meio do
 * passo até o seguinte, arredondado para cima. Passo de um tique não tem meio: fica a tabela da fonte.
 */

import type { Desconhecido, Efeito, TabelaQuadros } from "@domain/edicao";
import { efeitoConhecido } from "./projeto";

/** Metade do passo, arredondada para cima. */
const meio = (passo: number) => Math.floor((passo + 1) / 2);

/** O segundo campo cabe entre dois quadros: todo passo (e o mediano, que fecha o último) tem pelo menos 2 tiques. */
export const campoCabe = (tab: TabelaQuadros) => tab.passo_mediano >= 2 && tab.passos.every((p) => p >= 2);

/** O clipe pede um quadro por campo? (o primeiro `desentrelacar` ligado, em modo `campo`) */
export function porCampo(efeitos: readonly (Efeito | Desconhecido)[]): boolean {
  const d = efeitos.find((e) => efeitoConhecido(e) && e.tipo === "desentrelacar" && e.ligado);
  return !!d && efeitoConhecido(d) && d.tipo === "desentrelacar" && d.modo === "campo";
}

export function tabelaDoClipe(tab: TabelaQuadros, efeitos: readonly (Efeito | Desconhecido)[]): TabelaQuadros {
  if (!porCampo(efeitos) || !campoCabe(tab)) return tab;
  const passos: number[] = [];
  for (const p of tab.passos) passos.push(meio(p), p - meio(p));
  passos.push(meio(tab.passo_mediano));
  return { ...tab, passos, passo_mediano: tab.passo_mediano - meio(tab.passo_mediano), chaves: tab.chaves.map((c) => 2 * c) };
}
