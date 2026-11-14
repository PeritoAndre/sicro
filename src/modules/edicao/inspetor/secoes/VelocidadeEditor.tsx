/** Velocidade exibida (P1.3): "Cálculo" · "Posição". Fase 0: casca; só leitura até `VELOCIDADE_PRONTA`. */

import type { ContextoClipe, Id, EfVelocidadeExibida } from "@domain/edicao";
import type { EditorEfeitoProps } from "../registro";

export const VELOCIDADE_PRONTA = false;

export function VelocidadeEditor(props: EditorEfeitoProps<EfVelocidadeExibida>) {
  void props;
  return null;
}

/** Velocidade nova: o retrato do cálculo vem de `edicaoRetratoCalculo`. */
export function novaVelocidade(ctx: ContextoClipe): EfVelocidadeExibida {
  void ctx;
  throw new Error("nao_suportado: velocidade_exibida");
}

/** "Adicionar → …" na lista de marcações: cria o efeito (e o rastreio ou o retrato que ele pede) no clipe; devolve o id. */
export function adicionarVelocidade(clipeId: Id, rotulo: string): Promise<Id | null> {
  void clipeId;
  void rotulo;
  return Promise.reject(new Error("nao_suportado: velocidade_exibida"));
}
