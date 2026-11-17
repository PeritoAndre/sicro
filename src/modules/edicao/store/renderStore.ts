/**
 * Fila de render (§5.10): o estado vem dos eventos `edicao:fila` (lista inteira) e `edicao:progresso`
 * (até 5 por segundo). A tela filtra por caso e por projeto.
 */

import { create } from "zustand";
import { listen } from "@tauri-apps/api/event";
import { commands } from "@core/commands";
import { EVENTO_FILA, EVENTO_PROGRESSO } from "@domain/edicao";
import type { AlvoRender, Id, Progresso, ProjetoEdicao, Tarefa } from "@domain/edicao";
import { separarErro, useEdicaoStore, type ErroComando } from "./edicaoStore";

interface RenderState {
  tarefas: Tarefa[];
  /** Último progresso de cada tarefa rodando. */
  progresso: Record<Id, Progresso>;
  erro: ErroComando | null;
  /** Assina os eventos (uma vez) e lê a fila. */
  ouvir(): Promise<void>;
  enfileirar(ws: string, projeto: ProjetoEdicao, alvo: AlvoRender, ciencias: string[]): Promise<Id>;
  /** Enfileira o projeto aberto, depois de gravar o que estiver pendente (§8.4). */
  exportar(alvo: AlvoRender, ciencias: string[]): Promise<Id>;
  cancelar(id: Id): Promise<void>;
  limpar(): Promise<void>;
  receberFila(t: Tarefa[]): void;
  receberProgresso(p: Progresso): void;
}

let ouvindo: Promise<void> | null = null;
/** Pasta do caso de cada tarefa pedida nesta sessão: o aviso de fim abre a pasta da saída mesmo com outro caso aberto. */
export const wsDaTarefa = new Map<Id, string>();
/** Já chegou lista por evento: a leitura inicial, se atrasar, é mais velha que ela. */
let chegouFila = false;

/** Estados em que o progresso ainda vale (um progresso atrasado não desfaz o fim). */
const EM_CURSO = new Set(["na_fila", "preparando", "rodando"]);

export const useRenderStore = create<RenderState>((set, get) => ({
  tarefas: [],
  progresso: {},
  erro: null,

  ouvir() {
    ouvindo ??= (async () => {
      await listen<Tarefa[]>(EVENTO_FILA, (e) => {
        chegouFila = true;
        get().receberFila(e.payload);
      });
      await listen<Progresso>(EVENTO_PROGRESSO, (e) => get().receberProgresso(e.payload));
      try {
        const lida = await commands.edicaoFila();
        if (!chegouFila) get().receberFila(lida);
      } catch (e) {
        set({ erro: separarErro(e) });
      }
    })().catch((e: unknown) => {
      ouvindo = null; // sem eventos: a próxima chamada tenta de novo
      throw e;
    });
    return ouvindo;
  },

  async enfileirar(ws, projeto, alvo, ciencias) {
    try {
      const id = await commands.edicaoEnfileirar(ws, projeto, alvo, ciencias);
      if (alvo.tipo === "final") wsDaTarefa.set(id, ws);
      return id;
    } catch (e) {
      set({ erro: separarErro(e) });
      throw e;
    }
  },

  async exportar(alvo, ciencias) {
    const e = useEdicaoStore.getState();
    if (!e.ws || !e.doc) throw new Error("nenhum projeto aberto");
    await e.descarregar();
    // fechado ou trocado durante a gravação: não exporta outro documento no lugar
    const agora = useEdicaoStore.getState();
    if (!agora.ws || !agora.doc || agora.ws !== e.ws || agora.doc.id !== e.doc.id) throw new Error("projeto fechado");
    return get().enfileirar(agora.ws, agora.doc, alvo, ciencias);
  },

  async cancelar(id) {
    try {
      await commands.edicaoCancelar(id);
    } catch (e) {
      set({ erro: separarErro(e) });
      throw e;
    }
  },

  async limpar() {
    try {
      get().receberFila(await commands.edicaoFilaLimpar());
    } catch (e) {
      set({ erro: separarErro(e) });
      throw e;
    }
  },

  receberFila(tarefas) {
    set((s) => {
      const progresso: Record<Id, Progresso> = {};
      for (const t of tarefas) {
        const p = s.progresso[t.id];
        if (p && t.estado.tipo === "rodando") progresso[t.id] = p;
      }
      return { tarefas, progresso };
    });
  },

  receberProgresso(p) {
    set((s) => {
      const i = s.tarefas.findIndex((t) => t.id === p.tarefa_id);
      const t = s.tarefas[i];
      if (!t || !EM_CURSO.has(t.estado.tipo)) return {};
      const tarefas = s.tarefas.slice();
      tarefas[i] = { ...t, estado: { tipo: "rodando", fase: p.fase, feito_q: p.feito_q, total_q: p.total_q, qps: p.qps, restante_s: p.restante_s } };
      return { tarefas, progresso: { ...s.progresso, [p.tarefa_id]: p } };
    });
  },
}));

export const tarefasDoCaso = (tarefas: Tarefa[], caso: string) => tarefas.filter((t) => t.caso === caso);
export const tarefasDoProjeto = (tarefas: Tarefa[], projeto: Id) => tarefas.filter((t) => t.projeto_id === projeto);
