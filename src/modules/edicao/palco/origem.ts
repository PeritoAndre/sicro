/**
 * Intermediário de origem no palco (§7.5, fecho da Fase 1). Clipe com realce que muda a imagem marcada (estabilizar,
 * lente, desentrelace) só aceita marcação de anotação sobre o intermediário: enquanto ele é feito, "Marcar" e
 * "Seguir" ficam desligados naquele clipe; a ocultação segue no original (D17). Aqui ficam as regras puras, o relógio
 * do arquivo intermediário e a consulta que não põe nada na fila (`edicao_origem_estado`, para os cartões).
 * CONTRATO: módulo novo (contratos-fase1 F4).
 */

import { commands } from "@core/commands";
import type { ClipePrincipal, ClipeSobreposto, Desconhecido, Efeito, Id, PedidoMarcacao, PreviaMidia, ProjetoEdicao, Seg, TabelaQuadros } from "@domain/edicao";
import { acharClipe, acharEfeito, efeitoConhecido } from "../modelo/projeto";
import { campoCabe, porCampo } from "../modelo/realce";
import { passoDe, ptsDaTabela, type MapaQuadros, slotsDoQuadro } from "../modelo/tempo";
import { separarErro } from "../store/edicaoStore";
import { relogioMidia, type RelogioMidia } from "./motor/relogioMidia";
import { useOrigemStore, type OrigemDoClipe } from "../store/origemStore";

type Clipe = ClipePrincipal | ClipeSobreposto;

/** Efeitos de origem que mudam o espaço (ou o quadro) em que a anotação é marcada num trecho. */
export const MUDA_A_IMAGEM: ReadonlySet<string> = new Set(["estabilizar", "lente", "desentrelacar"]);
/** Os que mudam a GEOMETRIA (§3.2). O quadro congelado não tem intermediário: nele só estes param a marcação
 *  (o desentrelace troca a imagem do quadro, não o lugar das coisas). */
export const MUDA_O_ESPACO: ReadonlySet<string> = new Set(["estabilizar", "lente"]);

type ComEfeitos = Pick<Clipe, "tipo" | "efeitos">;

/** O clipe tem, ligado, um efeito que muda a imagem em que a anotação é marcada? */
export function mudaAImagem(c: ComEfeitos): boolean {
  const tipos = c.tipo === "trecho" ? MUDA_A_IMAGEM : MUDA_O_ESPACO;
  return c.efeitos.some((e) => efeitoConhecido(e) && e.ligado && tipos.has(e.tipo));
}

/** Marcação de anotação parada: o clipe muda a imagem e o intermediário não está pronto (o quadro congelado não tem
 *  intermediário: com Lente, a anotação não se marca nele). */
export function origemPendente(c: ComEfeitos | null | undefined, origem: OrigemDoClipe | undefined): boolean {
  return !!c && mudaAImagem(c) && !(origem?.tipo === "pronto" && origem.previa.pronta);
}

/** "Marcar" e "Seguir" das anotações desligados nesse clipe (§7.5). */
export function useOrigemPendente(c: (ComEfeitos & { id: Id }) | null | undefined): boolean {
  const o = useOrigemStore((s) => (c ? s.porClipe[c.id] : undefined));
  return origemPendente(c, o);
}

/** A marca mora no espaço ORIGINAL (ocultação) ou no canvas (`lugar`)? Essas não dependem do intermediário. */
export function marcaForaDaOrigem(doc: ProjetoEdicao, p: Pick<PedidoMarcacao, "tipo" | "efeito">): boolean {
  if (p.tipo === "lugar") return true;
  return !!p.efeito && acharEfeito(doc, p.efeito)?.efeito.tipo === "ocultar";
}

/** O pedido de marcação está parado pelo intermediário do clipe do efeito? (`origens` = `useOrigemStore.porClipe`) */
export function marcacaoParada(doc: ProjetoEdicao, p: Pick<PedidoMarcacao, "tipo" | "efeito">, origens: Record<Id, OrigemDoClipe | undefined>): boolean {
  if (marcaForaDaOrigem(doc, p) || !p.efeito) return false;
  const dono = acharEfeito(doc, p.efeito)?.clipe;
  const c = dono ? acharClipe(doc, dono)?.clipe : null;
  return origemPendente(c, dono ? origens[dono] : undefined);
}

/** O que o palco toca: o intermediário pronto, com o quadro mostrado dentro dele e sem marcação no original;
 *  senão a prévia da mídia. */
export function previaDoPalco(x: { origem: OrigemDoClipe | undefined; daMidia: PreviaMidia | null; t: Seg; original: boolean }): { previa: PreviaMidia | null; daOrigem: boolean } {
  const o = x.origem?.tipo === "pronto" && x.origem.previa.pronta ? x.origem.previa : null;
  if (o && !x.original && x.t >= o.de_s && x.t < o.ate_s) return { previa: o, daOrigem: true };
  return { previa: x.daMidia, daOrigem: false };
}

/** Quadros do arquivo por quadro da fonte: 2 no desentrelace por campo (quando o campo cabe na base), senão 1. */
export function camposPorQuadro(tab: TabelaQuadros, efeitos: readonly (Efeito | Desconhecido)[]): 1 | 2 {
  return porCampo(efeitos) && campoCabe(tab) ? 2 : 1;
}

/** Relógio do intermediário com os índices da tabela da FONTE (o transporte não muda). Por campo, o arquivo tem dois
 *  quadros por quadro da fonte: a busca vai ao meio do PRIMEIRO campo (o meio do quadro cairia na divisa dos dois). */
export function relogioDaOrigem(previa: PreviaMidia, tab: TabelaQuadros, campos: 1 | 2): RelogioMidia {
  const r = relogioMidia(previa, tab);
  if (campos === 1) return r;
  const pts = ptsDaTabela(tab);
  const limitar = (j: number) => Math.min(Math.max(Math.trunc(j), 0), r.n - 1);
  return {
    ...r,
    alvo: (j) => {
      const k = limitar(j);
      // primeiro campo = [pts, pts + teto(passo/2)): o meio dele, em oitavos de tique para não perder a metade
      const meio = Math.floor((passoDe(tab, k) + 1) / 2);
      return Number((8n * BigInt(pts[k] as number) + BigInt(4 * meio)) * BigInt(tab.tb.num)) / (8 * tab.tb.den) - previa.desloc_s;
    },
  };
}

/** Slots do programa (locais ao clipe) do quadro `j` da fonte, num mapa feito na tabela do clipe: por campo, o quadro
 *  `j` são as entradas `2j` e `2j + 1` (os slots dos dois campos, seguidos). */
export function slotsDoQuadroDaFonte(mapa: MapaQuadros, j: number, campos: 1 | 2): [number, number] | null {
  if (campos === 1) return slotsDoQuadro(mapa, j);
  const a = slotsDoQuadro(mapa, 2 * j);
  const b = slotsDoQuadro(mapa, 2 * j + 1);
  return a && b ? [a[0], a[1] + b[1]] : (a ?? b);
}

/** `edicao_origem_estado`: a situação do intermediário do clipe SEM pedir (não põe render na fila). undefined = nem
 *  pronto nem na fila; o que falhou há pouco volta como `falhou`. Chamada isolada: só aqui. */
export async function estadoDaOrigem(ws: string, doc: ProjetoEdicao, clipe: Id): Promise<OrigemDoClipe | undefined> {
  try {
    return (await commands.edicaoOrigemEstado(ws, doc, clipe)) ?? undefined;
  } catch (e) {
    return { tipo: "falhou", erro: separarErro(e) };
  }
}
