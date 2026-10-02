/**
 * Lista de croquis do workspace ativo + croqui aberto no editor.
 * Só o doc persistido vive aqui; seleção/drag ficam no componente.
 */

import { create } from "zustand";
import { commands } from "@core/commands";
import { toSicroError, type SicroError } from "@core/errors";
import type { Croqui, CroquiKind } from "@domain/croqui";
import {
  coerceCroquiDoc,
  serializeCroquiDoc,
  type SicroCroquiDoc,
} from "../engine";

interface CroquiState {
  list: Croqui[];
  isLoadingList: boolean;
  isMutating: boolean;

  /** UUID of the croqui currently open in the editor, or null. */
  activeCroquiId: string | null;
  activeCroqui: Croqui | null;
  activeDoc: SicroCroquiDoc | null;

  /**
   * ISO do último export PNG por croqui id. Só em memória: após reload
   * `isExportStale` volta a dizer "stale" e o PNG é regenerado (seguro).
   */
  lastExportedAt: Record<string, string>;

  lastError: SicroError | null;

  loadList: (workspacePath: string) => Promise<void>;
  createCroqui: (
    workspacePath: string,
    title: string,
    kind?: CroquiKind,
  ) => Promise<Croqui>;
  openCroqui: (workspacePath: string, croquiId: string) => Promise<SicroCroquiDoc>;
  /** Só marca o croqui ativo; o CorpoEditor carrega o `.sicrocorpo` sozinho. */
  openCorpo: (croquiId: string) => void;
  /** Só marca o croqui ativo; o PlantaEditor carrega o `.sicroplanta` sozinho. */
  openPlanta: (croquiId: string) => void;
  saveCurrent: (
    workspacePath: string,
    doc: SicroCroquiDoc,
  ) => Promise<Croqui>;
  /** Remove linha + `.sicrocroqui`; PNGs já exportados ficam. */
  deleteCroqui: (workspacePath: string, croquiId: string) => Promise<void>;
  exportPng: (
    workspacePath: string,
    pngBase64: string,
  ) => Promise<string>;
  clearCurrent: () => void;
  clearError: () => void;

  /** `true` se o save é mais novo que o último export PNG (ou se não houve export). */
  isExportStale: (croquiId: string) => boolean;
}

export const useCroquiStore = create<CroquiState>((set, get) => ({
  list: [],
  isLoadingList: false,
  isMutating: false,
  activeCroquiId: null,
  activeCroqui: null,
  activeDoc: null,
  lastExportedAt: {},
  lastError: null,

  async loadList(workspacePath) {
    set({ isLoadingList: true, lastError: null });
    try {
      const list = await commands.listCroquis(workspacePath);
      set({ list, isLoadingList: false });
    } catch (err) {
      set({ isLoadingList: false, lastError: toSicroError(err) });
    }
  },

  async createCroqui(workspacePath, title, kind = "viario") {
    set({ isMutating: true, lastError: null });
    try {
      const payload = await commands.createCroqui(workspacePath, {
        title,
        kind,
      });
      // Corporal e planta têm engines próprios: não coagir como viário,
      // só atualizar a lista.
      if (kind === "corporal" || kind === "planta") {
        set((s) => ({
          list: [
            payload.croqui,
            ...s.list.filter((c) => c.id !== payload.croqui.id),
          ],
          isMutating: false,
        }));
        return payload.croqui;
      }
      const doc = coerceCroquiDoc(payload.doc);
      set((s) => ({
        list: [payload.croqui, ...s.list.filter((c) => c.id !== payload.croqui.id)],
        activeCroquiId: payload.croqui.id,
        activeCroqui: payload.croqui,
        activeDoc: doc,
        isMutating: false,
      }));
      return payload.croqui;
    } catch (err) {
      const e = toSicroError(err);
      set({ isMutating: false, lastError: e });
      throw e;
    }
  },

  async openCroqui(workspacePath, croquiId) {
    set({ isMutating: true, lastError: null });
    try {
      const payload = await commands.readCroqui(workspacePath, croquiId);
      const doc = coerceCroquiDoc(payload.doc);
      set({
        activeCroquiId: payload.croqui.id,
        activeCroqui: payload.croqui,
        activeDoc: doc,
        isMutating: false,
      });
      return doc;
    } catch (err) {
      const e = toSicroError(err);
      set({ isMutating: false, lastError: e });
      throw e;
    }
  },

  openCorpo(croquiId) {
    const row = get().list.find((c) => c.id === croquiId) ?? null;
    set({ activeCroquiId: croquiId, activeCroqui: row, activeDoc: null });
  },

  openPlanta(croquiId) {
    const row = get().list.find((c) => c.id === croquiId) ?? null;
    set({ activeCroquiId: croquiId, activeCroqui: row, activeDoc: null });
  },

  async saveCurrent(workspacePath, doc) {
    const current = get().activeCroqui;
    if (!current) throw new Error("no croqui currently open");
    set({ isMutating: true, lastError: null });
    try {
      const stamped = serializeCroquiDoc(doc);
      const updated = await commands.saveCroqui(workspacePath, current.id, stamped);
      set((s) => ({
        list: s.list.map((c) => (c.id === updated.id ? updated : c)),
        activeCroqui: updated,
        activeDoc: stamped,
        isMutating: false,
      }));
      return updated;
    } catch (err) {
      const e = toSicroError(err);
      set({ isMutating: false, lastError: e });
      throw e;
    }
  },

  async deleteCroqui(workspacePath, croquiId) {
    set({ isMutating: true, lastError: null });
    try {
      await commands.deleteCroqui(workspacePath, croquiId);
      set((s) => {
        const wasActive = s.activeCroquiId === croquiId;
        const remainingExports = { ...s.lastExportedAt };
        delete remainingExports[croquiId];
        return {
          list: s.list.filter((c) => c.id !== croquiId),
          activeCroquiId: wasActive ? null : s.activeCroquiId,
          activeCroqui: wasActive ? null : s.activeCroqui,
          activeDoc: wasActive ? null : s.activeDoc,
          lastExportedAt: remainingExports,
          isMutating: false,
        };
      });
    } catch (err) {
      const e = toSicroError(err);
      set({ isMutating: false, lastError: e });
      throw e;
    }
  },

  async exportPng(workspacePath, pngBase64) {
    const current = get().activeCroqui;
    if (!current) throw new Error("no croqui currently open");
    set({ isMutating: true, lastError: null });
    try {
      const path = await commands.exportCroquiPng(workspacePath, current.id, {
        png_base64: pngBase64,
      });
      // Recarrega a lista para o last_export_relative_path/status aparecerem.
      const list = await commands.listCroquis(workspacePath);
      const refreshed = list.find((c) => c.id === current.id) ?? current;
      set((s) => ({
        list,
        activeCroqui: refreshed,
        isMutating: false,
        lastExportedAt: {
          ...s.lastExportedAt,
          [current.id]: new Date().toISOString(),
        },
      }));
      return path;
    } catch (err) {
      const e = toSicroError(err);
      set({ isMutating: false, lastError: e });
      throw e;
    }
  },

  clearCurrent() {
    set({ activeCroquiId: null, activeCroqui: null, activeDoc: null });
  },

  clearError() {
    set({ lastError: null });
  },

  isExportStale(croquiId) {
    const s = get();
    const exportedAt = s.lastExportedAt[croquiId];
    if (!exportedAt) return true;
    const croqui = s.list.find((c) => c.id === croquiId);
    if (!croqui) return true;
    return Date.parse(croqui.updated_at) > Date.parse(exportedAt);
  },
}));
