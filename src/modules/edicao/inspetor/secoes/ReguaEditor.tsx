/** Régua (P1.3): "Calibração" · "Comprimento (m)" · "Divisões" · "Direção" · "Cor" · "Espessura" · "Rótulo".
 *  Fase 0: casca; só leitura até `REGUA_PRONTA`. */

import type { ContextoClipe, Id, EfRegua } from "@domain/edicao";
import type { EditorEfeitoProps } from "../registro";

export const REGUA_PRONTA = false;

export function ReguaEditor(props: EditorEfeitoProps<EfRegua>) {
  void props;
  return null;
}

/** Régua nova: o retrato da calibração vem de `edicaoRetratoCalibracao`. */
export function novaRegua(ctx: ContextoClipe): EfRegua {
  void ctx;
  throw new Error("nao_suportado: regua");
}

/** "Adicionar → …" na lista de marcações: cria o efeito (e o rastreio ou o retrato que ele pede) no clipe; devolve o id. */
export function adicionarRegua(clipeId: Id, rotulo: string): Promise<Id | null> {
  void clipeId;
  void rotulo;
  return Promise.reject(new Error("nao_suportado: regua"));
}
