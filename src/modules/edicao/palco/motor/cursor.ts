/**
 * O cursor do programa na base de um clipe (P3.3): o plano do documento com as tabelas que a tela já tem (o mesmo
 * `resolver` do palco e da linha) e o tempo do clipe num slot. O Inspetor ("Começar no cursor", "Marcar") lê o
 * tempo do clipe com o palco no modo "Programa" por aqui.
 */

import type { Id, ProjetoEdicao, Seg } from "@domain/edicao";
import { acharMidia } from "../../modelo/projeto";
import { resolver } from "../../modelo/tempo";
import { recursoDe } from "../recursos";
import { planoDe, tempoNoSegmento, type PlanoPrevia } from "./plano";

export function planoDoDoc(doc: ProjetoEdicao): PlanoPrevia {
  const ctx = { tabela: (id: Id) => recursoDe(acharMidia(doc, id)?.sha256).tabela };
  return planoDe(doc, resolver(doc, ctx), ctx);
}

/** Tempo na base do clipe (fonte no trecho; local nos demais) no slot `q`; null = o clipe não está no slot. */
export function tempoNoPrograma(doc: ProjetoEdicao, clipe: Id, q: number): Seg | null {
  const p = planoDoDoc(doc);
  const s = p.segmentos.find((x) => x.clipe === clipe);
  if (!s || q < s.de_q || q >= s.ate_q) return null;
  return tempoNoSegmento(s, q - s.de_q, p.fps).t;
}
