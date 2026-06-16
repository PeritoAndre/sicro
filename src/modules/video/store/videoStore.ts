/**
 * videoStore — owns the list of videos of the active occurrence plus the
 * currently-open `VideoBundle`. Heavy editor state (player position,
 * selected event) is component-local; this store only keeps what
 * survives navigation.
 */

import { create } from "zustand";
import { commands } from "@core/commands";
import { toSicroError, type SicroError } from "@core/errors";
import type {
  CollectFrameInput,
  CreateVideoEventInput,
  UpdateStoryboardFrameInput,
  UpdateVideoEventInput,
  VideoBundle,
  VideoEvent,
  VideoMedia,
  VideoStoryboardFrame,
} from "@domain/video";
import type {
  ComputeSpeedInput,
  CreateSpeedCalibrationInput,
  VideoSpeedCalculation,
  VideoSpeedCalibration,
} from "@domain/video_speed";
import type {
  CreateDistanceMeasurementInput,
  VideoDistanceMeasurement,
} from "@domain/video_distance";

interface VideoState {
  list: VideoMedia[];
  isLoadingList: boolean;
  isMutating: boolean;

  activeMediaId: string | null;
  bundle: VideoBundle | null;

  lastError: SicroError | null;
  warningsFromLastAction: string[];

  loadList: (workspacePath: string) => Promise<void>;
  registerMedia: (workspacePath: string, sourcePath: string) => Promise<VideoMedia>;
  openMedia: (workspacePath: string, mediaId: string) => Promise<VideoBundle>;
  closeMedia: () => void;

  createEvent: (
    workspacePath: string,
    input: CreateVideoEventInput,
  ) => Promise<VideoEvent>;
  updateEvent: (
    workspacePath: string,
    eventId: string,
    input: UpdateVideoEventInput,
  ) => Promise<VideoEvent>;
  deleteEvent: (workspacePath: string, eventId: string) => Promise<void>;

  collectFrame: (
    workspacePath: string,
    input: CollectFrameInput,
  ) => Promise<VideoStoryboardFrame>;
  updateStoryboardFrame: (
    workspacePath: string,
    frameId: string,
    input: UpdateStoryboardFrameInput,
  ) => Promise<VideoStoryboardFrame>;
  deleteStoryboardFrame: (
    workspacePath: string,
    frameId: string,
    deletePng?: boolean,
  ) => Promise<void>;

  // ----- Calculador de Velocidade -----
  speedCalibrations: VideoSpeedCalibration[];
  speedCalculations: VideoSpeedCalculation[];
  loadSpeedData: (workspacePath: string, mediaHash: string) => Promise<void>;
  createCalibration: (
    workspacePath: string,
    input: CreateSpeedCalibrationInput,
  ) => Promise<VideoSpeedCalibration>;
  computeSpeed: (
    workspacePath: string,
    input: ComputeSpeedInput,
  ) => Promise<VideoSpeedCalculation>;

  // ----- Medição de Distância (compartilha as calibrações da velocidade) -----
  distanceMeasurements: VideoDistanceMeasurement[];
  loadDistanceData: (workspacePath: string, mediaHash: string) => Promise<void>;
  createDistanceMeasurement: (
    workspacePath: string,
    input: CreateDistanceMeasurementInput,
  ) => Promise<VideoDistanceMeasurement>;

  clearError: () => void;
  clearWarnings: () => void;
}

export const useVideoStore = create<VideoState>((set) => ({
  list: [],
  isLoadingList: false,
  isMutating: false,
  activeMediaId: null,
  bundle: null,
  lastError: null,
  warningsFromLastAction: [],
  speedCalibrations: [],
  speedCalculations: [],
  distanceMeasurements: [],

  async loadList(workspacePath) {
    set({ isLoadingList: true, lastError: null });
    try {
      const list = await commands.listVideoMedia(workspacePath);
      set({ list, isLoadingList: false });
    } catch (err) {
      set({ isLoadingList: false, lastError: toSicroError(err) });
    }
  },

  async registerMedia(workspacePath, sourcePath) {
    set({ isMutating: true, lastError: null });
    try {
      const media = await commands.registerVideoMedia(workspacePath, {
        source_path: sourcePath,
      });
      set((s) => ({
        list: [media, ...s.list.filter((m) => m.id !== media.id)],
        isMutating: false,
      }));
      return media;
    } catch (err) {
      const e = toSicroError(err);
      set({ isMutating: false, lastError: e });
      throw e;
    }
  },

  async openMedia(workspacePath, mediaId) {
    set({ isMutating: true, lastError: null });
    try {
      const bundle = await commands.openVideoMedia(workspacePath, mediaId);
      set({
        activeMediaId: bundle.media.id,
        bundle,
        isMutating: false,
        // Velocidade/medição são por-mídia: limpa o que sobrou da anterior.
        speedCalibrations: [],
        speedCalculations: [],
        distanceMeasurements: [],
      });
      return bundle;
    } catch (err) {
      const e = toSicroError(err);
      set({ isMutating: false, lastError: e });
      throw e;
    }
  },

  closeMedia() {
    set({
      activeMediaId: null,
      bundle: null,
      speedCalibrations: [],
      speedCalculations: [],
      distanceMeasurements: [],
    });
  },

  async createEvent(workspacePath, input) {
    const event = await commands.createVideoEvent(workspacePath, input);
    set((s) => ({
      bundle: s.bundle
        ? {
            ...s.bundle,
            events: [...s.bundle.events, event].sort(
              (a, b) => a.timestamp_s - b.timestamp_s,
            ),
          }
        : s.bundle,
    }));
    return event;
  },

  async updateEvent(workspacePath, eventId, input) {
    const event = await commands.updateVideoEvent(workspacePath, eventId, input);
    set((s) => ({
      bundle: s.bundle
        ? {
            ...s.bundle,
            events: s.bundle.events
              .map((e) => (e.id === event.id ? event : e))
              .sort((a, b) => a.timestamp_s - b.timestamp_s),
          }
        : s.bundle,
    }));
    return event;
  },

  async deleteEvent(workspacePath, eventId) {
    await commands.deleteVideoEvent(workspacePath, eventId);
    set((s) => ({
      bundle: s.bundle
        ? {
            ...s.bundle,
            events: s.bundle.events.filter((e) => e.id !== eventId),
          }
        : s.bundle,
    }));
  },

  async collectFrame(workspacePath, input) {
    set({ isMutating: true, warningsFromLastAction: [] });
    try {
      const result = await commands.collectVideoFrame(workspacePath, input);
      set((s) => ({
        bundle: s.bundle
          ? {
              ...s.bundle,
              exports: [result.export, ...s.bundle.exports],
              storyboard: [...s.bundle.storyboard, result.storyboard_frame].sort(
                (a, b) => a.requested_timestamp_s - b.requested_timestamp_s,
              ),
            }
          : s.bundle,
        isMutating: false,
        warningsFromLastAction: result.warnings,
      }));
      return result.storyboard_frame;
    } catch (err) {
      const e = toSicroError(err);
      set({ isMutating: false, lastError: e });
      throw e;
    }
  },

  async updateStoryboardFrame(workspacePath, frameId, input) {
    const updated = await commands.updateStoryboardFrame(
      workspacePath,
      frameId,
      input,
    );
    set((s) => ({
      bundle: s.bundle
        ? {
            ...s.bundle,
            storyboard: s.bundle.storyboard.map((f) =>
              f.id === updated.id ? updated : f,
            ),
          }
        : s.bundle,
    }));
    return updated;
  },

  async deleteStoryboardFrame(workspacePath, frameId, deletePng) {
    await commands.deleteStoryboardFrame(workspacePath, frameId, deletePng);
    set((s) => ({
      bundle: s.bundle
        ? {
            ...s.bundle,
            storyboard: s.bundle.storyboard.filter((f) => f.id !== frameId),
          }
        : s.bundle,
    }));
  },

  async loadSpeedData(workspacePath, mediaHash) {
    try {
      const [calibrations, calculations] = await Promise.all([
        commands.listSpeedCalibrations(workspacePath, mediaHash),
        commands.listSpeedCalculations(workspacePath, mediaHash),
      ]);
      set({ speedCalibrations: calibrations, speedCalculations: calculations });
    } catch (err) {
      set({ lastError: toSicroError(err) });
    }
  },

  async createCalibration(workspacePath, input) {
    set({ isMutating: true, lastError: null });
    try {
      const cal = await commands.createSpeedCalibration(workspacePath, input);
      set((s) => ({
        speedCalibrations: [cal, ...s.speedCalibrations],
        isMutating: false,
      }));
      return cal;
    } catch (err) {
      const e = toSicroError(err);
      set({ isMutating: false, lastError: e });
      throw e;
    }
  },

  async computeSpeed(workspacePath, input) {
    set({ isMutating: true, lastError: null });
    try {
      const calc = await commands.computeSpeed(workspacePath, input);
      set((s) => ({
        speedCalculations: [calc, ...s.speedCalculations],
        isMutating: false,
      }));
      return calc;
    } catch (err) {
      const e = toSicroError(err);
      set({ isMutating: false, lastError: e });
      throw e;
    }
  },

  async loadDistanceData(workspacePath, mediaHash) {
    // Carrega as medições E reatualiza as calibrações (compartilhadas com a
    // velocidade) — a aba Medições consome a MESMA geometria de cena.
    try {
      const [calibrations, measurements] = await Promise.all([
        commands.listSpeedCalibrations(workspacePath, mediaHash),
        commands.listDistanceMeasurements(workspacePath, mediaHash),
      ]);
      set({ speedCalibrations: calibrations, distanceMeasurements: measurements });
    } catch (err) {
      set({ lastError: toSicroError(err) });
    }
  },

  async createDistanceMeasurement(workspacePath, input) {
    set({ isMutating: true, lastError: null });
    try {
      const m = await commands.createDistanceMeasurement(workspacePath, input);
      set((s) => ({
        distanceMeasurements: [m, ...s.distanceMeasurements],
        isMutating: false,
      }));
      return m;
    } catch (err) {
      const e = toSicroError(err);
      set({ isMutating: false, lastError: e });
      throw e;
    }
  },

  clearError() {
    set({ lastError: null });
  },
  clearWarnings() {
    set({ warningsFromLastAction: [] });
  },
}));
