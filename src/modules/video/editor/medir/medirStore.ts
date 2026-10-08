/**
 * Estado dos modos Velocidade e Distância (o que o perito está montando antes de
 * gravar). O que vai para o banco (calibração, cálculo, medição) continua no videoStore.
 * Momentos, marcas e nitidez ficam guardados por vídeo (localStorage).
 */

import { create } from "zustand";

export type Modo = "assistir" | "velocidade" | "distancia";
export type RefKind = "retangulo" | "fila" | "medida" | "veiculo";
export type Nitidez = "nitido" | "normal" | "borrado";

export interface Pt {
  x: number;
  y: number;
}

/** Rascunho da referência no chão (vira calibração ao salvar). */
export interface RefDraft {
  kind: RefKind;
  frameId: string | null;
  points: Pt[];
  /** Retângulo: comprimento (ao longo da via) e largura (m), texto do campo. */
  comprimento: string;
  largura: string;
  /** Uma distância medida (m). */
  distancia: string;
  /** Marcas em fila: preset de cadência ou posições livres. */
  fila: "urbano" | "rodovia" | "livre";
  filaPos: string[];
  /** O próprio veículo: preset e entre-eixos (m). */
  veiculo: string;
  entreEixos: string;
  fonte: "campo" | "norma_viaria";
  /** Refazendo a referência mesmo havendo uma salva. */
  editando: boolean;
}

export interface SpeedDraft {
  step: 0 | 1 | 2 | 3;
  /** Quadros coletados que entram no cálculo (ids do storyboard). */
  moments: string[];
  marks: Record<string, Pt>;
  nitidez: Nitidez;
  /** Quadro mostrado no passo do pneu (null = o primeiro sem marca). */
  focus: string | null;
  /** σ editáveis no modo perito (texto do campo; vazio = padrão). */
  sigmaCal: string;
  sigmaTempo: string;
  sigmaMundo: string;
  /** Assinatura das entradas do último cálculo (detecta resultado desatualizado). */
  calcSig: string | null;
}

export interface DistDraft {
  step: 0 | 1 | 2;
  frameId: string | null;
  points: Pt[];
  nitidez: Nitidez;
}

/** Precisão do clique por nitidez (σ marcação em px). */
export const SIGMA_NITIDEZ: Record<Nitidez, number> = { nitido: 1, normal: 2, borrado: 4 };

export const emptyRef = (): RefDraft => ({
  kind: "retangulo",
  frameId: null,
  points: [],
  comprimento: "",
  largura: "",
  distancia: "",
  fila: "urbano",
  filaPos: [],
  veiculo: "sedan",
  entreEixos: "2,65",
  fonte: "campo",
  editando: false,
});

const emptySpeed = (): SpeedDraft => ({
  step: 0,
  moments: [],
  marks: {},
  nitidez: "normal",
  focus: null,
  sigmaCal: "",
  sigmaTempo: "",
  sigmaMundo: "",
  calcSig: null,
});

const emptyDist = (): DistDraft => ({ step: 0, frameId: null, points: [], nitidez: "normal" });

interface MedirState {
  mediaHash: string | null;
  modo: Modo;
  perito: boolean;
  speed: SpeedDraft;
  dist: DistDraft;
  ref: RefDraft;
  /** Troca de vídeo: carrega o que estava guardado para ele. */
  bindMedia: (hash: string) => void;
  setModo: (m: Modo) => void;
  setPerito: (on: boolean) => void;
  patchSpeed: (p: Partial<SpeedDraft>) => void;
  patchDist: (p: Partial<DistDraft>) => void;
  patchRef: (p: Partial<RefDraft>) => void;
  resetRef: () => void;
}

const KEY = (hash: string) => `sicro.medir.v1.${hash}`;
const PERITO_KEY = "sicro.medir.perito.v1";

function load(hash: string): { speed: SpeedDraft; dist: DistDraft } {
  try {
    const raw = localStorage.getItem(KEY(hash));
    if (raw) {
      const j = JSON.parse(raw) as Partial<{ speed: Partial<SpeedDraft>; dist: Partial<DistDraft> }>;
      return { speed: { ...emptySpeed(), ...(j.speed ?? {}) }, dist: { ...emptyDist(), ...(j.dist ?? {}) } };
    }
  } catch {
    /* sem armazenamento: começa do zero */
  }
  return { speed: emptySpeed(), dist: emptyDist() };
}

function save(hash: string | null, speed: SpeedDraft, dist: DistDraft): void {
  if (!hash) return;
  try {
    localStorage.setItem(KEY(hash), JSON.stringify({ speed, dist }));
  } catch {
    /* ignorado */
  }
}

function loadPerito(): boolean {
  try {
    return localStorage.getItem(PERITO_KEY) === "1";
  } catch {
    return false;
  }
}

export const useMedirStore = create<MedirState>((set, get) => ({
  mediaHash: null,
  modo: "assistir",
  perito: loadPerito(),
  speed: emptySpeed(),
  dist: emptyDist(),
  ref: emptyRef(),
  bindMedia(hash) {
    if (get().mediaHash === hash) return;
    const saved = load(hash);
    set({ mediaHash: hash, speed: saved.speed, dist: saved.dist, ref: emptyRef() });
  },
  setModo(modo) {
    set({ modo });
  },
  setPerito(on) {
    try {
      localStorage.setItem(PERITO_KEY, on ? "1" : "0");
    } catch {
      /* ignorado */
    }
    set({ perito: on });
  },
  patchSpeed(p) {
    const speed = { ...get().speed, ...p };
    set({ speed });
    save(get().mediaHash, speed, get().dist);
  },
  patchDist(p) {
    const dist = { ...get().dist, ...p };
    set({ dist });
    save(get().mediaHash, get().speed, dist);
  },
  patchRef(p) {
    set({ ref: { ...get().ref, ...p } });
  },
  resetRef() {
    set({ ref: emptyRef() });
  },
}));
