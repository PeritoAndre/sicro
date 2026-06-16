/**
 * laudoStore — holds the list of laudos of the active workspace plus the
 * currently-open laudo. Kept lean — heavy editor state lives in the TipTap
 * editor instance itself.
 */

import { create } from "zustand";
import type { JSONContent } from "@tiptap/core";
import { commands } from "@core/commands";
import { pushToast } from "@/components/toast/toastStore";
import { toSicroError, type SicroError } from "@core/errors";
import { clearAutoBackups } from "../services/autoBackup";
import {
  coerceSicroDoc,
  findInstitutionalTemplate,
  normalizeEvidenceSrcsForSave,
  resolveEvidenceSrcsForEditor,
  seedHeaderContentFromInstitutionalTemplate,
  type SicroDoc,
  type SicroDocComment,
  type SicroDocFinalization,
  type SicroDocFooter,
  type SicroDocHeader,
  type SicroDocLayout,
  type SicroDocSnapshot,
  type SicroDocStatus,
} from "../document-engine";
import type { Laudo, NewLaudoInput } from "@domain/laudo";

/** N — Região do editor atualmente ativa (modelo Word-style).
 *  `body` (default): typing edita o corpo do laudo, header é só visual
 *  com placeholder. `header`: typing edita `doc.header.content`, body
 *  fica esmaecido e não editável. Transição via double-click no header
 *  ou botão na toolbar. */
export type EditingRegion = "body" | "header" | "footer";

interface LaudoState {
  list: Laudo[];
  isLoadingList: boolean;
  isMutating: boolean;

  currentLaudo: Laudo | null;
  currentDoc: SicroDoc | null;

  /** N — Região ativa de edição. UI-only (não persiste). */
  editingRegion: EditingRegion;

  lastError: SicroError | null;

  loadList: (workspacePath: string) => Promise<void>;
  /**
   * Create a fresh laudo. When `initialContent` is provided (typically the
   * result of `findTemplate(id).build(...)`), it is written to the new
   * `.sicrodoc` immediately after creation — so the file on disk matches
   * what the editor will show. `initialMetadata` is merged into the envelope
   * metadata and is the place where MVP 2 stores `numero_laudo` / `setor`
   * from the NewLaudoDialog.
   *
   * `initialEnvelope` é a "porta dos fundos" pra setar layout/header
   * iniciais. Usado pelo NewLaudoDialog pra já ligar o
   * `institutional_template = pca_padrao_v1` e semear o
   * `header.content` com o cabeçalho oficial. Faz isso aqui (em vez
   * de chamadas extras) pra economizar 2 saves redundantes na criação.
   */
  createLaudo: (
    workspacePath: string,
    input: NewLaudoInput,
    initialContent?: JSONContent,
    initialMetadata?: Record<string, unknown>,
    initialEnvelope?: {
      layout?: Partial<SicroDocLayout>;
      header?: SicroDocHeader;
    },
  ) => Promise<Laudo>;
  /**
   * POC — Importa um `.docx` (Word) como um novo laudo e o deixa aberto
   * (como `createLaudo`, mas o conteúdo vem da conversão do documento).
   */
  importDocx: (
    workspacePath: string,
    sourcePath: string,
    title?: string,
  ) => Promise<Laudo>;
  openLaudo: (workspacePath: string, laudoId: string) => Promise<SicroDoc>;
  saveCurrent: (
    workspacePath: string,
    content: SicroDoc["content"],
  ) => Promise<Laudo>;
  /**
   * Remove o laudo do workspace (linha + `.sicrodoc`). Remove o item
   * da `list` em memória; se for o laudo aberto no editor, limpa
   * `currentLaudo`/`currentDoc` para forçar o caller a voltar pra
   * lista.
   */
  deleteLaudo: (workspacePath: string, laudoId: string) => Promise<void>;
  /**
   * Patch `currentDoc.metadata` and persist via `save_laudo`. Used by the
   * Inspector "Cabeçalho" tab so the institutional header (numero_laudo,
   * setor, etc.) can be edited without touching the editable content.
   */
  updateMetadata: (
    workspacePath: string,
    patch: Record<string, unknown>,
  ) => Promise<Laudo>;
  /**
   * Deep-patch `currentDoc.layout` and persist. Used by the Inspector
   * "Página" tab to write page margins. The patch is shallow-merged at the
   * top level and recursively at `layout.page`.
   */
  updateLayout: (
    workspacePath: string,
    patch: Partial<SicroDocLayout>,
  ) => Promise<Laudo>;
  /**
   * F8 — Replace `currentDoc.comments` and persist. UI normalmente passa
   * a lista inteira já mutada via helpers em `comments/service`.
   */
  setComments: (
    workspacePath: string,
    next: SicroDocComment[],
  ) => Promise<Laudo>;
  /**
   * F8 — Replace `currentDoc.snapshots` e persist (push + cap).
   */
  setSnapshots: (
    workspacePath: string,
    next: SicroDocSnapshot[],
  ) => Promise<Laudo>;
  /**
   * F8/F9 — Define o status do laudo (rascunho/em_revisao/final).
   * Quando `final`, `finalization` deve ser passado em paralelo.
   */
  setStatus: (
    workspacePath: string,
    next: SicroDocStatus,
    finalization?: SicroDocFinalization | null,
  ) => Promise<Laudo>;
  /** N — Replace `currentDoc.header` e persistir. Chamado pela camada do
   *  editor (debounced) quando o usuário edita o cabeçalho, e pelo botão
   *  "Cabeçalho" da toolbar (toggle enabled). */
  setHeader: (
    workspacePath: string,
    next: SicroDocHeader,
  ) => Promise<Laudo>;
  /** W — Replace `currentDoc.footer` e persistir (simétrico ao setHeader). */
  setFooter: (
    workspacePath: string,
    next: SicroDocFooter,
  ) => Promise<Laudo>;
  /** N — Switch UI-only entre regiões body/header/footer. Não persiste. */
  setEditingRegion: (region: EditingRegion) => void;
  clearCurrent: () => void;
  clearError: () => void;
}

export const useLaudoStore = create<LaudoState>((set, get) => ({
  list: [],
  isLoadingList: false,
  isMutating: false,
  currentLaudo: null,
  currentDoc: null,
  editingRegion: "body",
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

  async createLaudo(
    workspacePath,
    input,
    initialContent,
    initialMetadata,
    initialEnvelope,
  ) {
    set({ isMutating: true, lastError: null });
    try {
      const payload = await commands.createLaudo(workspacePath, input);
      let doc = coerceSicroDoc(payload.doc);

      // If a template seeded the content OR initial metadata was supplied
      // OR an initial layout/header was passed (e.g. para semear o
      // cabeçalho oficial), persist RIGHT AWAY so the disk file equals
      // what the editor renders. Sem isso, o primeiro open do laudo
      // mostraria o parágrafo vazio do `create_laudo` puro.
      let laudoRow = payload.laudo;
      const hasInitialContent = !!initialContent;
      const hasInitialMetadata =
        !!initialMetadata && Object.keys(initialMetadata).length > 0;
      const hasInitialEnvelope =
        !!initialEnvelope &&
        (!!initialEnvelope.layout || !!initialEnvelope.header);

      if (hasInitialContent || hasInitialMetadata || hasInitialEnvelope) {
        const nextLayout: SicroDocLayout = initialEnvelope?.layout
          ? { ...doc.layout, ...initialEnvelope.layout }
          : doc.layout;
        const nextDoc: SicroDoc = {
          ...doc,
          content: initialContent ?? doc.content,
          metadata: hasInitialMetadata
            ? { ...(doc.metadata ?? {}), ...initialMetadata }
            : doc.metadata,
          layout: nextLayout,
          header: initialEnvelope?.header ?? doc.header,
          updated_at: new Date().toISOString(),
        };
        try {
          laudoRow = await commands.saveLaudo(
            workspacePath,
            payload.laudo.id,
            nextDoc,
          );
          doc = nextDoc;
        } catch (err) {
          // Best-effort: keep the laudo open with the in-memory template even
          // if save fails (next user-driven save will retry).
          // eslint-disable-next-line no-console
          console.warn("failed to persist initial template content", err);
        }
      }

      set((s) => ({
        list: [laudoRow, ...s.list.filter((l) => l.id !== laudoRow.id)],
        currentLaudo: laudoRow,
        currentDoc: doc,
        isMutating: false,
      }));
      return laudoRow;
    } catch (err) {
      const e = toSicroError(err);
      set({ isMutating: false, lastError: e });
      throw e;
    }
  },

  async importDocx(workspacePath, sourcePath, title) {
    set({ isMutating: true, lastError: null });
    try {
      const payload = await commands.importDocxAsLaudo(
        workspacePath,
        sourcePath,
        title,
      );
      const doc = coerceSicroDoc(payload.doc);
      // Passo 2 — resolve srcs do CORPO e do CABEÇALHO (brasão importado).
      const docForState = resolveDocForEditor(doc, workspacePath);
      set((s) => ({
        list: [
          payload.laudo,
          ...s.list.filter((l) => l.id !== payload.laudo.id),
        ],
        currentLaudo: payload.laudo,
        currentDoc: docForState,
        isMutating: false,
      }));
      return payload.laudo;
    } catch (err) {
      const e = toSicroError(err);
      set({ isMutating: false, lastError: e });
      throw e;
    }
  },

  async openLaudo(workspacePath, laudoId) {
    set({ isMutating: true, lastError: null });
    try {
      const payload = await commands.readLaudo(workspacePath, laudoId);
      // Assinatura de versão: laudo salvo numa versão mais nova do SICRO.
      if (payload.opened_with_newer_version) {
        pushToast(
          "warn",
          `Este laudo foi salvo numa versão mais nova do SICRO (${payload.opened_with_newer_version}) que a sua. Pode não abrir corretamente — recomendamos atualizar o software.`,
          { title: "Versão mais nova", durationMs: 9000 },
        );
      }
      const raw = coerceSicroDoc(payload.doc);

      // N12 — Migração suave de docs legados que tinham
      // `institutional_template` setado mas não tinham o header
      // Word-style ainda. Detecção: header existe (via coerceSicroDoc)
      // mas content é o stub vazio E há template institucional. Se
      // sim, semeia o header com brand_lines/subtitle/metadata e
      // persiste. Próximas aberturas pulam essa branch.
      let migratedDoc = raw;
      const needsHeaderSeed =
        !!raw.layout?.institutional_template &&
        !!raw.header &&
        isHeaderContentEmpty(raw.header.content);

      if (needsHeaderSeed) {
        const template = findInstitutionalTemplate(
          raw.layout?.institutional_template,
        );
        const seeded = seedHeaderContentFromInstitutionalTemplate(
          template,
          raw.metadata ?? {},
          // Occurrence não está disponível aqui (vive em workspaceStore);
          // o seeder degrada graciosamente — fields que dependem dela
          // simplesmente não aparecem no header migrado.
          null,
        );
        const newHeader: SicroDocHeader = {
          enabled: true,
          content: seeded as JSONContent,
        };
        migratedDoc = {
          ...raw,
          header: newHeader,
          updated_at: new Date().toISOString(),
        };
        // Persiste de forma síncrona para que próximas aberturas vejam o
        // doc migrado. Se falhar, mantém o doc em memória mesmo assim.
        try {
          await commands.saveLaudo(workspacePath, laudoId, migratedDoc);
        } catch (err) {
          // eslint-disable-next-line no-console
          console.warn("[N12] failed to persist seeded header", err);
        }
      }

      // Resolve any relative_path → convertFileSrc so figures/storyboard
      // frames AND the header brasão display in the editor. The on-disk doc
      // stays untouched (see normalizeDocForSave on the save path).
      const doc = resolveDocForEditor(migratedDoc, workspacePath);
      set({
        currentLaudo: payload.laudo,
        currentDoc: doc,
        isMutating: false,
      });
      return doc;
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
      // Limpa os auto-backups locais (IndexedDB) do laudo excluído — senão
      // ficam órfãos para sempre (bug encontrado na auditoria de código morto:
      // clearAutoBackups existia mas nunca era chamado). Best-effort.
      void clearAutoBackups(laudoId).catch(() => {});
      set((s) => {
        const wasCurrent = s.currentLaudo?.id === laudoId;
        return {
          list: s.list.filter((l) => l.id !== laudoId),
          // Se o laudo deletado estava aberto no editor, limpa a
          // referência para forçar o caller a voltar pra lista
          // (caso contrário o editor mostraria um doc órfão).
          currentLaudo: wasCurrent ? null : s.currentLaudo,
          currentDoc: wasCurrent ? null : s.currentDoc,
          editingRegion: wasCurrent ? "body" : s.editingRegion,
          isMutating: false,
        };
      });
    } catch (err) {
      const e = toSicroError(err);
      set({ isMutating: false, lastError: e });
      throw e;
    }
  },

  async saveCurrent(workspacePath, content) {
    const current = get().currentLaudo;
    const currentDoc = get().currentDoc;
    if (!current || !currentDoc) {
      throw new Error("no laudo currently open");
    }
    set({ isMutating: true, lastError: null });
    try {
      // Strip absolute / convertFileSrc URLs back to relative paths so
      // the `.sicrodoc` stays portable across workspaces — CORPO E CABEÇALHO
      // (brasão importado também).
      const docToPersist = normalizeDocForSave({
        ...currentDoc,
        content,
        updated_at: new Date().toISOString(),
      });
      const updatedRow = await commands.saveLaudo(
        workspacePath,
        current.id,
        docToPersist,
      );
      // In-memory state keeps the editor-friendly (resolved) version so
      // images keep rendering after a save (corpo + cabeçalho).
      const docForState = resolveDocForEditor(docToPersist, workspacePath);
      set((s) => ({
        list: s.list.map((l) => (l.id === updatedRow.id ? updatedRow : l)),
        currentLaudo: updatedRow,
        currentDoc: docForState,
        isMutating: false,
      }));
      return updatedRow;
    } catch (err) {
      const e = toSicroError(err);
      set({ isMutating: false, lastError: e });
      throw e;
    }
  },

  async updateLayout(workspacePath, patch) {
    const current = get().currentLaudo;
    const currentDoc = get().currentDoc;
    if (!current || !currentDoc) {
      throw new Error("no laudo currently open");
    }
    set({ isMutating: true, lastError: null });
    try {
      const prevLayout = currentDoc.layout;
      const nextLayout: SicroDocLayout = {
        ...prevLayout,
        ...patch,
        // Page sub-object merges recursively so margins/page_size coexist.
        page: patch.page
          ? { ...(prevLayout.page ?? {}), ...patch.page }
          : prevLayout.page,
      };
      const nextDoc: SicroDoc = {
        ...currentDoc,
        layout: nextLayout,
        updated_at: new Date().toISOString(),
      };
      const updatedRow = await commands.saveLaudo(
        workspacePath,
        current.id,
        normalizeDocForSave(nextDoc),
      );
      set((s) => ({
        list: s.list.map((l) => (l.id === updatedRow.id ? updatedRow : l)),
        currentLaudo: updatedRow,
        currentDoc: nextDoc,
        isMutating: false,
      }));
      return updatedRow;
    } catch (err) {
      const e = toSicroError(err);
      set({ isMutating: false, lastError: e });
      throw e;
    }
  },

  async updateMetadata(workspacePath, patch) {
    const current = get().currentLaudo;
    const currentDoc = get().currentDoc;
    if (!current || !currentDoc) {
      throw new Error("no laudo currently open");
    }
    set({ isMutating: true, lastError: null });
    try {
      const nextDoc: SicroDoc = {
        ...currentDoc,
        metadata: { ...(currentDoc.metadata ?? {}), ...patch },
        updated_at: new Date().toISOString(),
      };
      const updatedRow = await commands.saveLaudo(
        workspacePath,
        current.id,
        normalizeDocForSave(nextDoc),
      );
      set((s) => ({
        list: s.list.map((l) => (l.id === updatedRow.id ? updatedRow : l)),
        currentLaudo: updatedRow,
        currentDoc: nextDoc,
        isMutating: false,
      }));
      return updatedRow;
    } catch (err) {
      const e = toSicroError(err);
      set({ isMutating: false, lastError: e });
      throw e;
    }
  },

  async setComments(workspacePath, next) {
    return patchAndSave(workspacePath, get, set, (doc) => ({
      ...doc,
      comments: next,
    }));
  },

  async setSnapshots(workspacePath, next) {
    return patchAndSave(workspacePath, get, set, (doc) => ({
      ...doc,
      snapshots: next,
    }));
  },

  async setStatus(workspacePath, next, finalization) {
    return patchAndSave(workspacePath, get, set, (doc) => ({
      ...doc,
      status: next,
      finalization:
        next === "final"
          ? finalization ?? doc.finalization ?? undefined
          : undefined,
    }));
  },

  async setHeader(workspacePath, next) {
    return patchAndSave(workspacePath, get, set, (doc) => ({
      ...doc,
      header: next,
    }));
  },

  async setFooter(workspacePath, next) {
    return patchAndSave(workspacePath, get, set, (doc) => ({
      ...doc,
      footer: next,
    }));
  },

  setEditingRegion(region) {
    set({ editingRegion: region });
  },

  clearCurrent() {
    set({ currentLaudo: null, currentDoc: null, editingRegion: "body" });
  },

  clearError() {
    set({ lastError: null });
  },
}));

/**
 * N12 — True quando o conteúdo do header é o stub default (doc com 1
 * único parágrafo vazio). Usado pra decidir se vale rodar a migração
 * a partir do `institutional_template`.
 */
function isHeaderContentEmpty(content: JSONContent | undefined): boolean {
  if (!content || content.type !== "doc") return true;
  const blocks = content.content ?? [];
  if (blocks.length === 0) return true;
  if (blocks.length === 1) {
    const first = blocks[0]!;
    if (first.type === "paragraph") {
      const inner = first.content ?? [];
      if (inner.length === 0) return true;
      // Parágrafo só com texto vazio também conta como vazio.
      const allEmpty = inner.every(
        (n) =>
          n.type === "text" && (n.text === undefined || n.text.trim() === ""),
      );
      if (allEmpty) return true;
    }
  }
  return false;
}

/**
 * F8 — Helper genérico: aplica um patcher no `currentDoc` corrente e
 * persiste o resultado via `save_laudo`. Compartilhado por setComments,
 * setSnapshots e setStatus.
 *
 * Aceita o `set` da própria zustand (tipo bem permissivo) para evitar
 * gambiarras de cast.
 */
async function patchAndSave(
  workspacePath: string,
  get: () => LaudoState,
  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  set: any,
  patcher: (doc: SicroDoc) => SicroDoc,
): Promise<Laudo> {
  const current = get().currentLaudo;
  const currentDoc = get().currentDoc;
  if (!current || !currentDoc) {
    throw new Error("no laudo currently open");
  }
  set({ isMutating: true, lastError: null });
  try {
    const nextDoc: SicroDoc = {
      ...patcher(currentDoc),
      updated_at: new Date().toISOString(),
    };
    const updatedRow = await commands.saveLaudo(
      workspacePath,
      current.id,
      normalizeDocForSave(nextDoc),
    );
    set((s: LaudoState) => ({
      list: s.list.map((l) => (l.id === updatedRow.id ? updatedRow : l)),
      currentLaudo: updatedRow,
      currentDoc: nextDoc,
      isMutating: false,
    }));
    return updatedRow;
  } catch (err) {
    const e = toSicroError(err);
    set({ isMutating: false, lastError: e });
    throw e;
  }
}

/**
 * Passo 2 (imagens) — resolve `relative_path → src` (convertFileSrc) tanto no
 * CORPO quanto no `header.content`, pra que figuras do corpo E o brasão do
 * cabeçalho renderizem no editor. Espelha `resolveEvidenceSrcsForEditor`,
 * que sozinho só cobria o corpo.
 */
function resolveDocForEditor(doc: SicroDoc, workspacePath: string): SicroDoc {
  return {
    ...doc,
    content: resolveEvidenceSrcsForEditor(doc.content, workspacePath),
    header: doc.header
      ? {
          ...doc.header,
          content: resolveEvidenceSrcsForEditor(doc.header.content, workspacePath),
        }
      : doc.header,
    footer: doc.footer
      ? {
          ...doc.footer,
          content: resolveEvidenceSrcsForEditor(doc.footer.content, workspacePath),
        }
      : doc.footer,
  };
}

/**
 * Inverso de `resolveDocForEditor`: troca `src` pelo `relative_path` (corpo +
 * `header.content`) ANTES de salvar, mantendo o `.sicrodoc` portátil (sem
 * URLs `asset://` específicas da máquina). Idempotente.
 */
function normalizeDocForSave(doc: SicroDoc): SicroDoc {
  return {
    ...doc,
    content: normalizeEvidenceSrcsForSave(doc.content),
    header: doc.header
      ? {
          ...doc.header,
          content: normalizeEvidenceSrcsForSave(doc.header.content),
        }
      : doc.header,
    footer: doc.footer
      ? {
          ...doc.footer,
          content: normalizeEvidenceSrcsForSave(doc.footer.content),
        }
      : doc.footer,
  };
}
