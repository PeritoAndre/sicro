/**
 * A Edição na Central de Vídeo e Áudio (§8.1, "uma lista só"): criar, abrir, duplicar e excluir projetos, todos
 * pela store (a fila de `emSerie` cuida da ordem com o projeto aberto).
 */

import { commands } from "@core/commands";
import type { Id, ResumoProjeto } from "@domain/edicao";
import { textoClipes, textoDuracao } from "./sequencia/contas";
import { emSerie, useEdicaoStore } from "./store/edicaoStore";

/** Frases do catálogo (`edicao/mensagens.rs`) para os códigos que a lista de projetos pode trazer (contratos (h) 19). */
export const FRASE_DO_PROBLEMA: Readonly<Record<string, string>> = {
  esquema: "Arquivo de projeto inválido",
  item_invalido: "Item inválido",
};
export const fraseDoProblema = (codigo: string): string => FRASE_DO_PROBLEMA[codigo] ?? codigo;

/** "5 clipes · 0:47" (§8.6, Central). */
export const resumoDaEdicao = (p: Pick<ResumoProjeto, "n_clipes" | "duracao_s">): string => `${textoClipes(p.n_clipes)} · ${textoDuracao(p.duracao_s)}`;

export function listarEdicoes(ws: string): Promise<ResumoProjeto[]> {
  return commands.edicaoListar(ws);
}

/** "Edição" (lâmina): projeto novo, que vira a edição corrente do caso. */
export function novaEdicao(ws: string): Promise<Id> {
  return useEdicaoStore.getState().novo(ws);
}

export function abrirEdicao(ws: string, id: Id): Promise<void> {
  return useEdicaoStore.getState().abrir(ws, id);
}

/** Cópia no disco; o projeto aberto grava o pendente antes, para a cópia sair igual à tela. Na fila da store
 *  (contratos (g) 15): não corre no meio de um abrir, fechar ou excluir. */
export function duplicarEdicao(ws: string, id: Id): Promise<ResumoProjeto> {
  return emSerie(async () => {
    const s = useEdicaoStore.getState();
    if (s.ws === ws && s.aberta?.projetoId === id) await s.descarregar();
    return commands.edicaoDuplicar(ws, id);
  });
}

export function excluirEdicao(ws: string, id: Id): Promise<void> {
  return useEdicaoStore.getState().excluir(ws, id);
}
