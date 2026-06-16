/**
 * laudoStore — holds the list of laudos of the active workspace plus the
 * currently-selected laudo (SICRO 3.0).
 *
 * SICRO 3.0 pivot: o laudo É um `.docx` editado no Word/LibreOffice — não há
 * mais editor de texto in-app. O store é fino: lista + laudo selecionado +
 * ações de criação/registro/abertura externa/cópia de artefato. As antigas
 * mutações do editor (save/header/footer/comentários/snapshots/status) foram
 * removidas junto com o editor.
 */

import { create } from "zustand";
import { commands } from "@core/commands";
import { pushToast } from "@/components/toast/toastStore";
import { toSicroError, type SicroError } from "@core/errors";
import { useWorkspaceStore } from "@stores/workspaceStore";
import { clearAutoBackups } from "../services/autoBackup";
import { buildStarterEnvelope } from "../services/starterDocx";
import type { Laudo } from "@domain/laudo";

interface LaudoState {
  list: Laudo[];
  isLoadingList: boolean;
  isMutating: boolean;

  currentLaudo: Laudo | null;

  lastError: SicroError | null;

  loadList: (workspacePath: string) => Promise<void>;

  // ----- SICRO 3.0 — laudo como `.docx` (registro + ponte com o Word) -----

  /**
   * SICRO 3.0 — Cria um laudo novo já materializado como `.docx`. Lê o
   * workspace + ocorrência ativos do `workspaceStore`, monta o envelope inicial
   * (esqueleto + cabeçalho institucional + pílulas de campo) via
   * `buildStarterEnvelope`, chama `create_laudo_docx`, recarrega a `list` e
   * deixa o laudo como `currentLaudo` (a UI mostra a BridgeView).
   */
  createLaudoDocx: (
    title: string,
    templateId: string,
    opts?: { numeroLaudo?: string },
  ) => Promise<Laudo>;
  /** SICRO 3.0 — Abre o `.docx` do laudo no Word/LibreOffice (app padrão do SO). */
  openExternal: (laudoId: string) => Promise<void>;
  /**
   * SICRO 3.0 — Registra um `.docx` já escrito por fora: copia pro workspace +
   * cria a linha. Recarrega a `list` e deixa o laudo como `currentLaudo`.
   * `sourceAbsolutePath` é absoluto (vindo do file picker).
   */
  registerExistingDocx: (
    title: string,
    sourceAbsolutePath: string,
  ) => Promise<Laudo>;
  /**
   * SICRO 3.0 — Copia uma imagem do workspace (caminho relativo) pra área de
   * transferência, pro perito colar no `.docx` (Ctrl+V). Emite toast.
   */
  copyArtifact: (relativePath: string) => Promise<void>;
  /**
   * SICRO 3.0 — Seleciona um laudo SEM carregar o `.sicrodoc` para edição:
   * apenas seta `currentLaudo` (a linha de metadados), o que faz o módulo
   * mostrar a BridgeView. (O editor in-app foi aposentado.)
   */
  selectLaudo: (laudo: Laudo) => void;
  /**
   * SICRO 3.0 — Seleciona um laudo a partir do seu `id` (sem carregar o
   * `.docx`): garante que a `list` do workspace esteja carregada, encontra a
   * linha e a deixa como `currentLaudo` (a UI mostra a BridgeView). Usado pela
   * Central de Provas para abrir um laudo a partir do registro de evidências.
   */
  selectLaudoById: (workspacePath: string, laudoId: string) => Promise<void>;
  /**
   * Remove o laudo do workspace (linha + arquivo). Remove o item da `list` em
   * memória; se for o laudo selecionado, limpa `currentLaudo`.
   */
  deleteLaudo: (workspacePath: string, laudoId: string) => Promise<void>;
  clearCurrent: () => void;
  clearError: () => void;
}

export const useLaudoStore = create<LaudoState>((set) => ({
  list: [],
  isLoadingList: false,
  isMutating: false,
  currentLaudo: null,
  lastError: null,

  async loadList(workspacePath) {
    set({ isLoadingList: true, lastError: null });
    try {
      const list = await commands.listLaudos(workspacePath);
      set({ list, isLoadingList: false });
    } catch (err) {
      set({ isLoadingList: false, lastError: toSicroError(err) });
    }
  },

  // ----- SICRO 3.0 — laudo como `.docx` -----

  async createLaudoDocx(title, templateId, opts) {
    const ws = useWorkspaceStore.getState().activeWorkspacePath;
    if (!ws) {
      const e = toSicroError(new Error("nenhuma ocorrência ativa"));
      set({ lastError: e });
      throw e;
    }
    const occurrence = useWorkspaceStore.getState().activeOccurrence;
    set({ isMutating: true, lastError: null });
    try {
      // Override local opcional no momento da criação: o número do laudo
      // informado no diálogo (campo `numero_laudo` da metadata). Vazio →
      // metadata segue sem o campo e `{numero_laudo}` resolve da ocorrência.
      const numeroLaudo = opts?.numeroLaudo?.trim();
      const metadata: Record<string, unknown> = numeroLaudo
        ? { numero_laudo: numeroLaudo }
        : {};
      const { envelope, fieldValues } = buildStarterEnvelope({
        templateId,
        metadata,
        occurrence: (occurrence ?? null) as Record<string, unknown> | null,
      });
      const laudo = await commands.createLaudoDocx(
        ws,
        title,
        templateId,
        envelope,
        fieldValues,
      );
      // Recarrega a lista do disco (fonte de verdade) e seleciona o novo laudo.
      const list = await commands.listLaudos(ws);
      set({ list, currentLaudo: laudo, isMutating: false });
      return laudo;
    } catch (err) {
      const e = toSicroError(err);
      set({ isMutating: false, lastError: e });
      throw e;
    }
  },

  async openExternal(laudoId) {
    const ws = useWorkspaceStore.getState().activeWorkspacePath;
    if (!ws) return;
    try {
      await commands.openLaudoExternal(ws, laudoId);
    } catch (err) {
      const e = toSicroError(err);
      set({ lastError: e });
      pushToast("error", e.message, { title: "Não foi possível abrir o laudo" });
    }
  },

  async registerExistingDocx(title, sourceAbsolutePath) {
    const ws = useWorkspaceStore.getState().activeWorkspacePath;
    if (!ws) {
      const e = toSicroError(new Error("nenhuma ocorrência ativa"));
      set({ lastError: e });
      throw e;
    }
    set({ isMutating: true, lastError: null });
    try {
      const laudo = await commands.registerExistingDocx(
        ws,
        title,
        sourceAbsolutePath,
      );
      const list = await commands.listLaudos(ws);
      set({ list, currentLaudo: laudo, isMutating: false });
      return laudo;
    } catch (err) {
      const e = toSicroError(err);
      set({ isMutating: false, lastError: e });
      throw e;
    }
  },

  async copyArtifact(relativePath) {
    const ws = useWorkspaceStore.getState().activeWorkspacePath;
    if (!ws) return;
    try {
      await commands.copyImageToClipboard(ws, relativePath);
      pushToast("success", "Copiado. Cole no Word com Ctrl+V.", {
        title: "Artefato copiado",
      });
    } catch (err) {
      const e = toSicroError(err);
      set({ lastError: e });
      pushToast("error", e.message, { title: "Falha ao copiar" });
    }
  },

  selectLaudo(laudo) {
    set({ currentLaudo: laudo });
  },

  async selectLaudoById(workspacePath, laudoId) {
    set({ isMutating: true, lastError: null });
    try {
      // Garante a lista carregada (fonte de verdade das linhas de laudo) e
      // encontra a linha pedida — sem tocar no `.docx`.
      let list = useLaudoStore.getState().list;
      if (!list.some((l) => l.id === laudoId)) {
        list = await commands.listLaudos(workspacePath);
      }
      const laudo = list.find((l) => l.id === laudoId);
      if (!laudo) {
        throw new Error("Laudo não encontrado no workspace.");
      }
      set({ list, currentLaudo: laudo, isMutating: false });
    } catch (err) {
      const e = toSicroError(err);
      set({ isMutating: false, lastError: e });
      throw e;
    }
  },

  async deleteLaudo(workspacePath, laudoId) {
    set({ isMutating: true, lastError: null });
    try {
      await commands.deleteLaudo(workspacePath, laudoId);
      // Limpa os auto-backups locais (IndexedDB) do laudo excluído. Best-effort.
      void clearAutoBackups(laudoId).catch(() => {});
      set((s) => {
        const wasCurrent = s.currentLaudo?.id === laudoId;
        return {
          list: s.list.filter((l) => l.id !== laudoId),
          currentLaudo: wasCurrent ? null : s.currentLaudo,
          isMutating: false,
        };
      });
    } catch (err) {
      const e = toSicroError(err);
      set({ isMutating: false, lastError: e });
      throw e;
    }
  },

  clearCurrent() {
    set({ currentLaudo: null });
  },

  clearError() {
    set({ lastError: null });
  },
}));
