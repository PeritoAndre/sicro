/** Estado global da ocorrência ativa e da lista de recentes. */

import { create } from "zustand";
import { commands } from "@core/commands";
import { toSicroError, type SicroError } from "@core/errors";
import type {
  LoadedOccurrence,
  NewOccurrenceInput,
  Occurrence,
  OccurrenceEdit,
  OccurrenceStatus,
  RecentOccurrence,
} from "@domain/occurrence";
import {
  caseCountsFromCounters,
  caseEntryFromOccurrence,
} from "@domain/case_index";

/** Alimenta o índice global de casos (fire-and-forget); sem contagens, o backend preserva as anteriores. */
function indexCase(loaded: LoadedOccurrence, onIndexed?: () => void): void {
  const entry = caseEntryFromOccurrence(
    loaded.occurrence,
    loaded.workspace_path,
  );
  void commands
    .getOccurrenceCounts(loaded.workspace_path)
    .then((c) => {
      entry.counts = caseCountsFromCounters(c);
    })
    .catch(() => {
      /* contagens são best-effort */
    })
    .finally(() => {
      void commands
        .upsertCaseIndex(entry)
        .catch(() => {
          /* índice é best-effort */
        })
        // Avisa a Home só DEPOIS de gravado, senão ela recarrega antes do caso estar lá.
        .finally(() => {
          onIndexed?.();
        });
    });
}

interface WorkspaceState {
  activeOccurrence: Occurrence | null;
  activeWorkspacePath: string | null;
  recents: RecentOccurrence[];
  isLoadingRecents: boolean;
  isMutating: boolean;
  lastError: SicroError | null;

  loadRecents: () => Promise<void>;
  createOccurrence: (input: NewOccurrenceInput) => Promise<LoadedOccurrence>;
  openOccurrence: (workspacePath: string) => Promise<LoadedOccurrence>;
  updateActiveOccurrence: (edit: OccurrenceEdit) => Promise<Occurrence>;
  /** Comando dedicado: não corre o risco de zerar campos do cabeçalho. */
  setActiveStatus: (status: OccurrenceStatus) => Promise<Occurrence>;
  closeOccurrence: () => void;
  forgetRecent: (workspaceId: string) => Promise<void>;
  /** Só tira das listas (recentes + índice); não toca no disco. */
  forgetCase: (workspacePath: string, occurrenceId: string) => Promise<void>;
  /** Apaga a pasta `.sicro` do disco (irreversível) e depois limpa as listas. */
  deleteCaseFromDisk: (
    workspacePath: string,
    occurrenceId: string,
  ) => Promise<void>;
  clearError: () => void;
  /** Incrementa sempre que o índice é regravado; a Home usa como gatilho de reload. */
  caseIndexVersion: number;
  bumpCaseIndex: () => void;
}

export const useWorkspaceStore = create<WorkspaceState>((set, get) => ({
  activeOccurrence: null,
  activeWorkspacePath: null,
  recents: [],
  isLoadingRecents: false,
  isMutating: false,
  lastError: null,
  caseIndexVersion: 0,

  bumpCaseIndex() {
    set((s) => ({ caseIndexVersion: s.caseIndexVersion + 1 }));
  },

  async loadRecents() {
    set({ isLoadingRecents: true, lastError: null });
    try {
      const recents = await commands.listRecentOccurrences();
      set({ recents, isLoadingRecents: false });
    } catch (err) {
      set({
        isLoadingRecents: false,
        lastError: toSicroError(err),
      });
    }
  },

  async createOccurrence(input) {
    set({ isMutating: true, lastError: null });
    try {
      const loaded = await commands.createOccurrence(input);
      set({
        activeOccurrence: loaded.occurrence,
        activeWorkspacePath: loaded.workspace_path,
        isMutating: false,
      });
      void get().loadRecents();
      indexCase(loaded, () => get().bumpCaseIndex());
      return loaded;
    } catch (err) {
      const e = toSicroError(err);
      set({ isMutating: false, lastError: e });
      throw e;
    }
  },

  async openOccurrence(workspacePath) {
    set({ isMutating: true, lastError: null });
    try {
      const loaded = await commands.openOccurrence(workspacePath);
      set({
        activeOccurrence: loaded.occurrence,
        activeWorkspacePath: loaded.workspace_path,
        isMutating: false,
      });
      void get().loadRecents();
      indexCase(loaded, () => get().bumpCaseIndex());
      return loaded;
    } catch (err) {
      const e = toSicroError(err);
      set({ isMutating: false, lastError: e });
      throw e;
    }
  },

  async updateActiveOccurrence(edit) {
    const workspacePath = get().activeWorkspacePath;
    if (!workspacePath) {
      const e = toSicroError(new Error("nenhuma ocorrência ativa para editar"));
      set({ lastError: e });
      throw e;
    }
    set({ isMutating: true, lastError: null });
    try {
      const occurrence = await commands.updateOccurrence(workspacePath, edit);
      set({ activeOccurrence: occurrence, isMutating: false });
      void get().loadRecents();
      indexCase(
        { occurrence, workspace_path: workspacePath },
        () => get().bumpCaseIndex(),
      );
      return occurrence;
    } catch (err) {
      const e = toSicroError(err);
      set({ isMutating: false, lastError: e });
      throw e;
    }
  },

  async setActiveStatus(status) {
    const workspacePath = get().activeWorkspacePath;
    if (!workspacePath) {
      const e = toSicroError(new Error("nenhuma ocorrência ativa"));
      set({ lastError: e });
      throw e;
    }
    set({ isMutating: true, lastError: null });
    try {
      const occurrence = await commands.setOccurrenceStatus(
        workspacePath,
        status,
      );
      set({ activeOccurrence: occurrence, isMutating: false });
      void get().loadRecents();
      indexCase(
        { occurrence, workspace_path: workspacePath },
        () => get().bumpCaseIndex(),
      );
      return occurrence;
    } catch (err) {
      const e = toSicroError(err);
      set({ isMutating: false, lastError: e });
      throw e;
    }
  },

  closeOccurrence() {
    set({ activeOccurrence: null, activeWorkspacePath: null });
  },

  async forgetRecent(workspaceId) {
    try {
      await commands.forgetRecentOccurrence(workspaceId);
      await get().loadRecents();
    } catch (err) {
      set({ lastError: toSicroError(err) });
    }
  },

  async forgetCase(workspacePath, occurrenceId) {
    set({ isMutating: true, lastError: null });
    try {
      // Recentes são chaveados pelo `workspace_id` do manifesto (≠ occurrence.id).
      const recent = get().recents.find(
        (r) => r.workspace_path === workspacePath,
      );
      if (recent) await commands.forgetRecentOccurrence(recent.workspace_id);
      await commands.removeCaseIndex(occurrenceId);
      await get().loadRecents();
      get().bumpCaseIndex();
      set({ isMutating: false });
    } catch (err) {
      const e = toSicroError(err);
      set({ isMutating: false, lastError: e });
      throw e;
    }
  },

  async deleteCaseFromDisk(workspacePath, occurrenceId) {
    set({ isMutating: true, lastError: null });
    try {
      // Disco primeiro: se falhar, as listas ficam intactas e dá para tentar de novo.
      await commands.deleteOccurrence(workspacePath);
      const recent = get().recents.find(
        (r) => r.workspace_path === workspacePath,
      );
      if (recent) await commands.forgetRecentOccurrence(recent.workspace_id);
      await commands.removeCaseIndex(occurrenceId);
      if (get().activeWorkspacePath === workspacePath) {
        set({ activeOccurrence: null, activeWorkspacePath: null });
      }
      await get().loadRecents();
      set({ isMutating: false });
    } catch (err) {
      const e = toSicroError(err);
      set({ isMutating: false, lastError: e });
      throw e;
    }
  },

  clearError() {
    set({ lastError: null });
  },
}));

export const selectActiveOccurrence = (s: WorkspaceState) => s.activeOccurrence;
export const selectActiveWorkspacePath = (s: WorkspaceState) =>
  s.activeWorkspacePath;
export const selectRecents = (s: WorkspaceState) => s.recents;
