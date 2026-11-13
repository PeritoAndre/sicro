/** Montagem aberta: documento em memória, salvo sozinho 3 s depois da última mudança. */

import { create } from "zustand";
import { commands } from "@core/commands";
import { toSicroError } from "@core/errors";
import type { Montagem, MontagemItem } from "@domain/montagem";
import { mover, novaMontagem } from "./montagemModel";

interface State {
  lista: Montagem[];
  doc: Montagem | null;
  dirty: boolean;
  erro: string | null;
  carregar: (ws: string) => Promise<void>;
  nova: () => void;
  abrir: (id: string) => void;
  fechar: () => void;
  excluir: (ws: string, id: string) => Promise<void>;
  salvar: (ws: string) => Promise<boolean>;
  titulo: (t: string) => void;
  saida: (p: Partial<Montagem["saida"]>) => void;
  adicionar: (it: MontagemItem) => void;
  alterar: (id: string, p: Partial<MontagemItem>) => void;
  remover: (id: string) => void;
  moverItem: (de: number, para: number) => void;
}

export const useMontagemStore = create<State>((set, get) => ({
  lista: [],
  doc: null,
  dirty: false,
  erro: null,

  async carregar(ws) {
    try {
      set({ lista: await commands.listMontagens(ws), erro: null });
    } catch (e) {
      set({ erro: toSicroError(e).message });
    }
  },
  nova: () => set({ doc: novaMontagem(), dirty: true }),
  abrir(id) {
    const m = get().lista.find((x) => x.id === id);
    if (m) set({ doc: m, dirty: false });
  },
  fechar: () => set({ doc: null, dirty: false }),

  async excluir(ws, id) {
    await commands.deleteMontagem(ws, id);
    set((s) => ({
      lista: s.lista.filter((m) => m.id !== id),
      doc: s.doc?.id === id ? null : s.doc,
      dirty: s.doc?.id === id ? false : s.dirty,
    }));
  },

  async salvar(ws) {
    const doc = get().doc;
    if (!doc) return true;
    try {
      const salvo = await commands.saveMontagem(ws, doc);
      set((s) => ({
        lista: [salvo, ...s.lista.filter((m) => m.id !== salvo.id)],
        // Mudou durante o salvamento: continua sujo.
        dirty: s.doc === doc ? false : s.dirty,
        erro: null,
      }));
      return true;
    } catch (e) {
      set({ erro: toSicroError(e).message });
      return false;
    }
  },

  titulo: (titulo) => set((s) => (s.doc ? { doc: { ...s.doc, titulo }, dirty: true } : s)),
  saida: (p) => set((s) => (s.doc ? { doc: { ...s.doc, saida: { ...s.doc.saida, ...p } }, dirty: true } : s)),
  adicionar: (it) => set((s) => (s.doc ? { doc: { ...s.doc, itens: [...s.doc.itens, it] }, dirty: true } : s)),
  alterar: (id, p) =>
    set((s) =>
      s.doc
        ? {
            doc: { ...s.doc, itens: s.doc.itens.map((i) => (i.id === id ? ({ ...i, ...p } as MontagemItem) : i)) },
            dirty: true,
          }
        : s,
    ),
  remover: (id) =>
    set((s) => (s.doc ? { doc: { ...s.doc, itens: s.doc.itens.filter((i) => i.id !== id) }, dirty: true } : s)),
  moverItem: (de, para) =>
    set((s) => (s.doc ? { doc: { ...s.doc, itens: mover(s.doc.itens, de, para) }, dirty: true } : s)),
}));
