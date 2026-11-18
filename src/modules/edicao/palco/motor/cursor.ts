/**
 * O cursor do programa na base de um clipe (P3.3): o plano do documento com as tabelas que a tela já tem (o mesmo
 * `resolver` do palco e da linha) e o tempo do clipe num slot. O Inspetor ("Começar no cursor", "Marcar") lê o
 * tempo do clipe com o palco no modo "Programa" por aqui.
 */

import type { Id, ProjetoEdicao, Seg } from "@domain/edicao";
import { acharMidia } from "../../modelo/projeto";
import { resolver } from "../../modelo/tempo";
import { recursoDe, useRecursos } from "../recursos";
import { planoDe, tempoNoSegmento, type PlanoPrevia } from "./plano";

/** O último plano (documento e tabelas iguais = o mesmo plano): no projeto real do dono custa 8 a 19 ms, e a
 *  Sequência o pede a cada cursor publicado tocando ("Congelar aqui" no modo Programa). */
let ultimo: { doc: ProjetoEdicao; midias: unknown; versao: number; plano: PlanoPrevia } | null = null;

export function planoDoDoc(doc: ProjetoEdicao): PlanoPrevia {
  const { midias, versao } = useRecursos.getState();
  if (ultimo && ultimo.doc === doc && ultimo.midias === midias && ultimo.versao === versao) return ultimo.plano;
  const ctx = { tabela: (id: Id) => recursoDe(acharMidia(doc, id)?.sha256).tabela };
  const plano = planoDe(doc, resolver(doc, ctx), ctx);
  ultimo = { doc, midias, versao, plano };
  return plano;
}

/** Tempo na base do clipe (fonte no trecho; local nos demais) no slot `q`; null = o clipe não está no slot. */
export function tempoNoPrograma(doc: ProjetoEdicao, clipe: Id, q: number): Seg | null {
  const p = planoDoDoc(doc);
  const s = p.segmentos.find((x) => x.clipe === clipe);
  if (!s || q < s.de_q || q >= s.ate_q) return null;
  return tempoNoSegmento(s, q - s.de_q, p.fps).t;
}
