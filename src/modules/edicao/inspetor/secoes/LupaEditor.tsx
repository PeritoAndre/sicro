/** Lupa (P1.2): "Forma" · "Ampliação" · "Janela" · "Posição" (Fixa, Acompanha) · "Ligação" · "Interpolação" · "Cor" · "Espessura".
 *  Fase 0: casca; o registro mostra a lupa só para leitura até `LUPA_PRONTA`. */

import type { ContextoClipe, Id, EfLupa } from "@domain/edicao";
import type { EditorEfeitoProps } from "../registro";

/** O P1.2 liga ao entregar: "Adicionar → Lupa" aparece e o editor abre. */
export const LUPA_PRONTA = false;

export function LupaEditor(props: EditorEfeitoProps<EfLupa>) {
  void props;
  return null;
}

/** Lupa nova com os padrões (rastreio criado junto por `adicionarEfeito`). */
export function novaLupa(ctx: ContextoClipe): EfLupa {
  void ctx;
  throw new Error("nao_suportado: lupa");
}

/** "Adicionar → …" na lista de marcações: cria o efeito (e o rastreio ou o retrato que ele pede) no clipe; devolve o id. */
export function adicionarLupa(clipeId: Id, rotulo: string): Promise<Id | null> {
  void clipeId;
  void rotulo;
  return Promise.reject(new Error("nao_suportado: lupa"));
}
