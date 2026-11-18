/**
 * Faixas de efeito da linha do tempo (contratos-fase4.md D2, D6): cada efeito de clipe é uma barra no intervalo
 * `ativo`, convertido entre a base do clipe (fonte no trecho, local nos demais) e os slots do programa pelo mapa
 * do clipe; os quadros-chave (região do enquadramento, âncoras do rastreio seguido) viram losangos. "Começar no
 * cursor" e "Terminar no cursor" (Inspetor, `MarcacoesSecao`) usam `ativoNoCursor`. Puro. Dono: P4.1.
 */

import type { Desconhecido, Efeito, Id, Intervalo, LinhaResolvida, ProjetoEdicao, Seg } from "@domain/edicao";
import { efeitoConhecido } from "../modelo/projeto";
import type { ContextoTempo } from "../modelo/tempo";
import { chaveDaFaixaDe, donoNaLinha, instantesDasChaves, slotsNaBase, tempoDoSlot } from "./modelo";

/** Chave da faixa do efeito: tipo + rastreio seguido (`destaque`, `lupa`, `ocultar`, `holofote`, velocidade que
 *  acompanha, enquadramento que segue), ou só o tipo. */
// CONTRATO: aceita também o item de versão futura (`Desconhecido`): a faixa dele é a do tipo cru. `doc` opcional
// (rodada 2): com ele, a chave da linha (a da maquete: pessoa pelo nome, destaque pela forma, lupa e zoom pelo tipo).
export function chaveDaFaixa(e: Efeito | Desconhecido, doc?: ProjetoEdicao): string {
  return chaveDaFaixaDe(e, doc);
}

/** `ativo` (base do clipe) em slots do programa `[de_q, ate_q)`; null = fora do clipe. */
// CONTRATO: `ctx` opcional (tabelas das mídias; sem ele, as da tela).
export function slotsDoAtivo(doc: ProjetoEdicao, linha: LinhaResolvida, clipe: Id, ativo: Intervalo | null, ctx?: ContextoTempo): [number, number] | null {
  const d = donoNaLinha(doc, linha, clipe, ctx);
  return d ? slotsNaBase(d, ativo) : null;
}

/** Barra arrastada até os slots `[de_q, ate_q)` → `ativo` na base do clipe (borda do clipe = campo nulo). Sempre dentro
 *  do clipe e com pelo menos um slot. `daMidia` (ocultação da mídia, tempo de fonte obrigatório): nunca nulo. */
// CONTRATO: `ctx` e `op` opcionais.
export function ativoDoArrasto(doc: ProjetoEdicao, linha: LinhaResolvida, clipe: Id, de_q: number, ate_q: number, ctx?: ContextoTempo, op?: { daMidia?: boolean }): Intervalo {
  const d = donoNaLinha(doc, linha, clipe, ctx);
  if (!d || d.n_q <= 0) return { de: null, ate: null };
  const fim = d.inicio_q + d.n_q;
  const a = Math.min(Math.max(Math.round(de_q), d.inicio_q), fim - 1);
  const b = Math.min(Math.max(Math.round(ate_q), a + 1), fim);
  const nulos = !op?.daMidia;
  return {
    de: a <= d.inicio_q && nulos ? null : tempoDoSlot(d, a - d.inicio_q),
    ate: b >= fim && nulos ? null : tempoDoSlot(d, b - 1 - d.inicio_q),
  };
}

/** "Começar no cursor" / "Terminar no cursor": o `ativo` com a borda `lado` no tempo do efeito no cursor (`t`,
 *  `ContextoClipe.tempo()`); a outra borda fica, e nunca cruza (fica 1 quadro antes ou depois). */
export function ativoNoCursor(ativo: Intervalo | null, lado: "de" | "ate", t: Seg, passo_s: Seg): Intervalo {
  const de = ativo?.de ?? null;
  const ate = ativo?.ate ?? null;
  const p = Number.isFinite(passo_s) && passo_s > 0 ? passo_s : 0;
  if (lado === "de") return { de: ate !== null ? Math.min(t, ate - p) : t, ate };
  return { de, ate: de !== null ? Math.max(t, de + p) : t };
}

/** Instantes (base do clipe) dos quadros-chave do efeito: chaves da região do enquadramento; âncoras do rastreio
 *  que ele segue (destaque, lupa, ocultar, holofote, velocidade que acompanha, enquadramento que segue). */
export function chavesDoEfeito(doc: ProjetoEdicao, e: Efeito | Desconhecido): Seg[] {
  return efeitoConhecido(e) ? instantesDasChaves(doc, e) : [];
}
