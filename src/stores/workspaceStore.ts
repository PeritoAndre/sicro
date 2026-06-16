/**
 * workspaceStore — global state for the active occurrence and the recents list.
 *
 * This store is intentionally thin: it owns the *what* (data + loading flags)
 * but delegates the *how* to the command wrappers. UI components subscribe to
 * slices via the selector API to avoid unnecessary re-renders.
 */

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

/**
 * Alimenta o índice global de casos (base das estatísticas gerais) sempre que
 * um caso vira ativo. Fire-and-forget: nunca bloqueia nem quebra o fluxo.
 *
 * Captura também as contagens por módulo (laudos/croquis/mídias…) — best-effort:
 * se falhar, grava só o cabeçalho (o backend preserva as contagens anteriores).
 */
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
        // Avisa a UI (Home) que o índice mudou — só DEPOIS de gravado, pra não
        // recarregar antes do caso novo estar lá. Antes, criar uma ocorrência
        // não aparecia no histórico até navegar pra outro módulo e voltar.
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
  /**
   * Edita a identificação do caso ativo (cabeçalho do Dossiê) e reflete na UI.
   * Palavra final do perito; também atualiza recentes + índice global de casos.
   */
  updateActiveOccurrence: (edit: OccurrenceEdit) => Promise<Occurrence>;
  /**
   * Muda SÓ o status do caso ativo (concluir/reabrir) via comando dedicado —
   * sem o risco de zerar campos do cabeçalho. Atualiza UI + recentes + índice.
   */
  setActiveStatus: (status: OccurrenceStatus) => Promise<Occurrence>;
  closeOccurrence: () => void;
  forgetRecent: (workspaceId: string) => Promise<void>;
  /**
   * Remove um caso das LISTAS (recentes + índice global de casos). NÃO toca no
   * disco — os arquivos permanecem e o caso reaparece se reaberto.
   */
  forgetCase: (workspacePath: string, occurrenceId: string) => Promise<void>;
  /**
   * EXCLUI PERMANENTEMENTE a pasta `.sicro` do disco e, em seguida, limpa as
   * listas. Irreversível — só chamar após confirmação explícita do usuário.
   */
  deleteCaseFromDisk: (
    workspacePath: string,
    occurrenceId: string,
  ) => Promise<void>;
  clearError: () => void;
  /**
   * Versão do índice de casos. Incrementa SEMPRE que o índice é (re)gravado
   * (criar/abrir/editar/excluir ocorrência). A Home subscreve como gatilho de
   * reload do histórico — antes, criar uma ocorrência não aparecia até navegar
   * pra outro módulo e voltar (o useEffect de carga só rodava no mount).
   */
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
      // Refresh recents in background; don't block the caller.
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
      // Rótulo/tipo/município podem ter mudado → atualiza recentes + índice.
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
      // Status mudou → reflete em recentes + índice global (badge na Home/Histórico).
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
      // Recentes são chaveados pelo `workspace_id` do MANIFESTO (≠ occurrence.id),
      // então localizamos a entrada pelo caminho para achar a chave certa.
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
      // Disco PRIMEIRO: se falhar (arquivo travado/permissão), nada mais muda —
      // as listas continuam intactas e o usuário pode tentar de novo.
      await commands.deleteOccurrence(workspacePath);
      const recent = get().recents.find(
        (r) => r.workspace_path === workspacePath,
      );
      if (recent) await commands.forgetRecentOccurrence(recent.workspace_id);
      await commands.removeCaseIndex(occurrenceId);
      // Se o caso excluído era o ativo, fecha (o caminho não existe mais).
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

/** Convenience selectors — use these to avoid subscribing to the whole store. */
export const selectActiveOccurrence = (s: WorkspaceState) => s.activeOccurrence;
export const selectActiveWorkspacePath = (s: WorkspaceState) =>
  s.activeWorkspacePath;
export const selectRecents = (s: WorkspaceState) => s.recents;
