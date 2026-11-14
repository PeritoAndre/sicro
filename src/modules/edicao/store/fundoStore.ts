/**
 * Fila de fundo (§5.10): tabelas de quadros, cópias de prévia, proxies… O estado vem do evento
 * `edicao:fundo` (lista inteira); o cartão da mídia acha a sua pela chave "<tipo>:<sha256>".
 */

import { create } from "zustand";
import { listen } from "@tauri-apps/api/event";
import { commands } from "@core/commands";
import { EVENTO_FUNDO } from "@domain/edicao";
import type { TarefaFundo } from "@domain/edicao";
import { separarErro, type ErroComando } from "./edicaoStore";

interface FundoState {
  tarefas: TarefaFundo[];
  erro: ErroComando | null;
  /** Assina o evento (uma vez) e lê a lista. */
  ouvir(): Promise<void>;
  receber(t: TarefaFundo[]): void;
}

let ouvindo: Promise<void> | null = null;
/** Já chegou lista por evento: a leitura inicial, se atrasar, é mais velha que ela. */
let chegou = false;

export const useFundoStore = create<FundoState>((set, get) => ({
  tarefas: [],
  erro: null,

  ouvir() {
    ouvindo ??= (async () => {
      await listen<TarefaFundo[]>(EVENTO_FUNDO, (e) => {
        chegou = true;
        get().receber(e.payload);
      });
      try {
        const lida = await commands.edicaoFundo();
        if (!chegou) get().receber(lida);
      } catch (e) {
        set({ erro: separarErro(e) });
      }
    })().catch((e: unknown) => {
      ouvindo = null; // sem eventos: a próxima chamada tenta de novo
      throw e;
    });
    return ouvindo;
  },

  receber: (tarefas) => set({ tarefas }),
}));

export const chaveFundo = (tipo: TarefaFundo["tipo"], hash: string) => `${tipo}:${hash}`;

/** Trabalho de fundo com essa chave (o mais recente, se houver vários). */
export function fundoPorChave(tarefas: TarefaFundo[], chave: string): TarefaFundo | null {
  for (let i = tarefas.length - 1; i >= 0; i--) if (tarefas[i]?.chave === chave) return tarefas[i] as TarefaFundo;
  return null;
}

/** Trabalhos de fundo de uma mídia (qualquer tipo). */
export const fundoDaMidia = (tarefas: TarefaFundo[], sha256: string) => tarefas.filter((t) => t.chave.endsWith(`:${sha256}`));
