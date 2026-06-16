/**
 * Store do módulo Documentoscopia — lista de documentos da ocorrência e seleção
 * ativa. O detalhe (blocos OCR, campos, regiões, histórico) é carregado sob
 * demanda pelo workbench; aqui ficam apenas a lista e as ações de ciclo de vida.
 */

import { create } from "zustand";

import { commands } from "@core/commands";
import { toSicroError, type SicroError } from "@core/errors";
import type { DocumentCaseFile } from "@domain/documentoscopia";

interface DocumentsState {
  documents: DocumentCaseFile[];
  selectedId: string | null;
  loading: boolean;
  error: SicroError | null;

  load: (workspacePath: string) => Promise<void>;
  select: (id: string | null) => void;
  importFile: (
    workspacePath: string,
    filePath: string,
    docType?: string,
  ) => Promise<DocumentCaseFile | null>;
  remove: (workspacePath: string, id: string) => Promise<void>;
  applyUpdated: (doc: DocumentCaseFile) => void;
  clearError: () => void;
}

export const useDocumentsStore = create<DocumentsState>((set, get) => ({
  documents: [],
  selectedId: null,
  loading: false,
  error: null,

  async load(workspacePath) {
    set({ loading: true, error: null });
    try {
      const documents = await commands.listDocuments(workspacePath);
      // Mantém a seleção se o documento ainda existe.
      const selectedId =
        get().selectedId && documents.some((d) => d.id === get().selectedId)
          ? get().selectedId
          : (documents[0]?.id ?? null);
      set({ documents, selectedId, loading: false });
    } catch (err) {
      set({ loading: false, error: toSicroError(err) });
    }
  },

  select(id) {
    set({ selectedId: id });
  },

  async importFile(workspacePath, filePath, docType) {
    set({ error: null });
    try {
      const doc = await commands.importDocument(workspacePath, filePath, docType);
      set((s) => ({
        documents: [doc, ...s.documents.filter((d) => d.id !== doc.id)],
        selectedId: doc.id,
      }));
      return doc;
    } catch (err) {
      set({ error: toSicroError(err) });
      return null;
    }
  },

  async remove(workspacePath, id) {
    set({ error: null });
    try {
      await commands.deleteDocument(workspacePath, id);
      set((s) => {
        const documents = s.documents.filter((d) => d.id !== id);
        const selectedId =
          s.selectedId === id ? (documents[0]?.id ?? null) : s.selectedId;
        return { documents, selectedId };
      });
    } catch (err) {
      set({ error: toSicroError(err) });
    }
  },

  applyUpdated(doc) {
    set((s) => ({
      documents: s.documents.map((d) => (d.id === doc.id ? doc : d)),
    }));
  },

  clearError() {
    set({ error: null });
  },
}));

export const selectSelectedDocument = (
  s: DocumentsState,
): DocumentCaseFile | null =>
  s.documents.find((d) => d.id === s.selectedId) ?? null;
