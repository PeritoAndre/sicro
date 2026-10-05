/** Estado do editor de planta: documento, seleção, ferramenta, vista e histórico (desfazer/refazer). */
import { create } from "zustand";
import type { SicroPlantaDoc } from "../model/schema";

export type SelKind = "wall" | "opening" | "room" | "item" | "evidence" | "person" | "traj" | "text" | "dim" | "compass" | "bg";
export interface Sel {
  kind: SelKind;
  id: string;
}

export type Tool =
  | "select"
  | "pan"
  | "wall"
  | "room"
  | "dim"
  | "evidence"
  | "traj"
  | "text"
  | "bgscale"
  | `opening:${string}`
  | `item:${string}`
  | `person:${string}`;

export interface Viewport {
  x: number;
  y: number;
  /** px de tela por unidade de mundo (1 m = 100 unidades). */
  scale: number;
}

interface PlantaState {
  doc: SicroPlantaDoc | null;
  savedJson: string;
  history: SicroPlantaDoc[];
  future: SicroPlantaDoc[];
  gestureBase: SicroPlantaDoc | null;
  sel: Sel[];
  tool: Tool;
  viewport: Viewport;
  /** Ajusta a vista à folha até o usuário mexer no zoom ou arrastar a vista. */
  autoFit: boolean;
  load: (doc: SicroPlantaDoc) => void;
  /** Alteração com histórico. */
  apply: (fn: (d: SicroPlantaDoc) => SicroPlantaDoc) => void;
  /** Gesto (arrasto): mudanças ao vivo, uma entrada no histórico ao terminar. */
  beginGesture: () => void;
  live: (fn: (d: SicroPlantaDoc) => SicroPlantaDoc) => void;
  endGesture: (fn?: (d: SicroPlantaDoc) => SicroPlantaDoc) => void;
  undo: () => void;
  redo: () => void;
  markSaved: (doc: SicroPlantaDoc) => void;
  setSel: (sel: Sel[]) => void;
  setTool: (t: Tool) => void;
  setViewport: (v: Viewport, user?: boolean) => void;
}

const LIMIT = 120;

export const usePlanta = create<PlantaState>((set, get) => ({
  doc: null,
  savedJson: "",
  history: [],
  future: [],
  gestureBase: null,
  sel: [],
  tool: "select",
  viewport: { x: 40, y: 40, scale: 0.5 },
  autoFit: true,
  load: (doc) => set({ doc, savedJson: JSON.stringify(doc), history: [], future: [], sel: [], tool: "select", autoFit: true, gestureBase: null }),
  apply: (fn) => {
    const { doc, history } = get();
    if (!doc) return;
    const next = fn(doc);
    if (next === doc) return;
    set({ doc: next, history: [...history, doc].slice(-LIMIT), future: [] });
  },
  beginGesture: () => set({ gestureBase: get().doc }),
  live: (fn) => {
    const { doc } = get();
    if (doc) set({ doc: fn(doc) });
  },
  endGesture: (fn) => {
    const { doc, gestureBase, history } = get();
    if (!doc) return;
    const next = fn ? fn(doc) : doc;
    if (!gestureBase || next === gestureBase) {
      set({ doc: next, gestureBase: null });
      return;
    }
    set({ doc: next, gestureBase: null, history: [...history, gestureBase].slice(-LIMIT), future: [] });
  },
  undo: () => {
    const { doc, history, future } = get();
    const prev = history[history.length - 1];
    if (!doc || !prev) return;
    set({ doc: prev, history: history.slice(0, -1), future: [doc, ...future].slice(0, LIMIT), sel: [] });
  },
  redo: () => {
    const { doc, history, future } = get();
    const next = future[0];
    if (!doc || !next) return;
    set({ doc: next, history: [...history, doc].slice(-LIMIT), future: future.slice(1), sel: [] });
  },
  markSaved: (doc) => set({ savedJson: JSON.stringify(doc) }),
  setSel: (sel) => set({ sel }),
  setTool: (tool) => set({ tool }),
  setViewport: (viewport, user = false) => set(user ? { viewport, autoFit: false } : { viewport }),
}));

export const M = 100;
export const m2w = (m: number) => m * M;
export const w2m = (w: number) => w / M;
