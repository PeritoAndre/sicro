/**
 * imagemStore — lista de análises da ocorrência ativa + análise atualmente
 * em edição. Heavy editor state (seleção, drag) vive em estado local;
 * o que persiste vive aqui (mirror do croquiStore).
 */

import { create } from "zustand";
import { commands } from "@core/commands";
import { toSicroError, type SicroError } from "@core/errors";
import type {
  ImageAnalysis,
  ImportLocalImageInput,
  CreateImageAnalysisInput,
  SaveImageAnalysisInput,
} from "@domain/image_analysis";
import {
  coerceSicroImage,
  serializeSicroImage,
  type SicroImageDoc,
} from "../engine";

interface ImagemState {
  list: ImageAnalysis[];
  isLoadingList: boolean;
  isMutating: boolean;

  activeAnalysisId: string | null;
  activeAnalysis: ImageAnalysis | null;
  activeDoc: SicroImageDoc | null;

  lastError: SicroError | null;

  loadList: (workspacePath: string) => Promise<void>;
  createFromEvidence: (
    workspacePath: string,
    input: CreateImageAnalysisInput,
  ) => Promise<ImageAnalysis>;
  createFromFile: (
    workspacePath: string,
    input: ImportLocalImageInput,
  ) => Promise<ImageAnalysis>;
  openAnalysis: (
    workspacePath: string,
    analysisId: string,
  ) => Promise<SicroImageDoc>;
  saveActive: (
    workspacePath: string,
    doc: SicroImageDoc,
    metadataJson?: string,
  ) => Promise<ImageAnalysis>;
  clearActive: () => void;
  clearError: () => void;
}

export const useImagemStore = create<ImagemState>((set, get) => ({
  list: [],
  isLoadingList: false,
  isMutating: false,
  activeAnalysisId: null,
  activeAnalysis: null,
  activeDoc: null,
  lastError: null,

  async loadList(workspacePath) {
    set({ isLoadingList: true, lastError: null });
    try {
      const list = await commands.listImageAnalyses(workspacePath);
      set({ list, isLoadingList: false });
    } catch (err) {
      set({ isLoadingList: false, lastError: toSicroError(err) });
    }
  },

  async createFromEvidence(workspacePath, input) {
    set({ isMutating: true, lastError: null });
    try {
      const row = await commands.createImageAnalysisFromEvidence(
        workspacePath,
        input,
      );
      set((s) => ({
        list: [row, ...s.list.filter((a) => a.id !== row.id)],
        isMutating: false,
      }));
      return row;
    } catch (err) {
      const e = toSicroError(err);
      set({ isMutating: false, lastError: e });
      throw e;
    }
  },

  async createFromFile(workspacePath, input) {
    set({ isMutating: true, lastError: null });
    try {
      const row = await commands.createImageAnalysisFromFile(
        workspacePath,
        input,
      );
      set((s) => ({
        list: [row, ...s.list.filter((a) => a.id !== row.id)],
        isMutating: false,
      }));
      return row;
    } catch (err) {
      const e = toSicroError(err);
      set({ isMutating: false, lastError: e });
      throw e;
    }
  },

  async openAnalysis(workspacePath, analysisId) {
    set({ isMutating: true, lastError: null });
    try {
      const payload = await commands.readImageAnalysis(
        workspacePath,
        analysisId,
      );
      const doc = coerceSicroImage(payload.doc);
      set({
        activeAnalysisId: payload.analysis.id,
        activeAnalysis: payload.analysis,
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

  async saveActive(workspacePath, doc, metadataJson) {
    const current = get().activeAnalysis;
    if (!current) throw new Error("nenhuma análise aberta");
    set({ isMutating: true, lastError: null });
    try {
      const stamped = serializeSicroImage(doc);
      const input: SaveImageAnalysisInput = {
        doc: stamped,
        title: doc.title,
        metadata_json: metadataJson,
      };
      const updated = await commands.saveImageAnalysis(
        workspacePath,
        current.id,
        input,
      );
      set((s) => ({
        list: s.list.map((a) => (a.id === updated.id ? updated : a)),
        activeAnalysis: updated,
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

  clearActive() {
    set({ activeAnalysisId: null, activeAnalysis: null, activeDoc: null });
  },

  clearError() {
    set({ lastError: null });
  },
}));
