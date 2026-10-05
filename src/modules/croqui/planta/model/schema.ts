/**
 * Formato da planta 5.0 (.sicroplanta, schema 2). Tudo em metros, y para baixo.
 * Paredes, portas e janelas guardam altura e peitoril para um 3D futuro.
 */
import type { Pt } from "./geom";

export const PLANTA_SCHEMA = "2.0.0";

export type WallKind = "parede" | "muro" | "grade";
export interface PNode {
  id: string;
  x: number;
  y: number;
}
export interface PWall {
  id: string;
  a: string;
  b: string;
  thickness: number;
  height: number;
  kind: WallKind;
}

export type OpeningKind = "porta" | "porta_dupla" | "correr" | "janela" | "basculante" | "vao";
export interface POpening {
  id: string;
  wall: string;
  kind: OpeningKind;
  /** Distância do nó `a` da parede até o começo do vão, pelo eixo. */
  t0: number;
  width: number;
  /** Peitoril e altura do vão (para o 3D). */
  sill: number;
  height: number;
  /** Lado da dobradiça: começo ou fim do vão (no sentido a→b). */
  hinge: "start" | "end";
  /** Para qual lado abre: 1 = normal à esquerda de a→b (perp), -1 = o outro. */
  side: 1 | -1;
}

export interface PRoom {
  id: string;
  name: string;
  /** Ponto dentro do cômodo: o contorno é achado pelas paredes em volta dele. */
  seed: Pt;
  /** Posição do rótulo; null = centro do cômodo. */
  label: Pt | null;
  show_area: boolean;
}

/** Mobília e peças de estrutura (escada, pilar). (x, y) = centro; d = profundidade (fundo encosta na parede). */
export interface PItem {
  id: string;
  symbol: string;
  x: number;
  y: number;
  rot: number;
  w: number;
  d: number;
  label: string | null;
}

export type EvidenceTipo =
  | "estojo"
  | "projetil"
  | "sangue"
  | "arma_fogo"
  | "arma_branca"
  | "celular"
  | "documento"
  | "pegada"
  | "outro";
export interface PEvidence {
  id: string;
  label: string;
  tipo: EvidenceTipo;
  descricao: string;
  x: number;
  y: number;
  /** Como medir: até duas paredes (coordenadas retangulares) ou dois cantos (triangulação). */
  measure: "paredes" | "pontos";
  show_measure: boolean;
  /** Triangulação: ids de dois nós; vazio = os dois cantos mais próximos. */
  points: string[];
}

export interface PPerson {
  id: string;
  pose: "em_pe" | "caido";
  x: number;
  y: number;
  rot: number;
  label: string;
  descricao: string;
}

export interface PTrajectory {
  id: string;
  a: Pt;
  b: Pt;
  label: string;
  descricao: string;
  color: string;
}

export interface PText {
  id: string;
  x: number;
  y: number;
  text: string;
  size: number;
  color: string;
  bold: boolean;
  rot: number;
}

/** Cota livre: dois pontos e o afastamento da linha de cota (m, para o lado perp de a→b). */
export interface PDimension {
  id: string;
  a: Pt;
  b: Pt;
  offset: number;
}

export interface PBackground {
  asset: string;
  x: number;
  y: number;
  /** Largura no mundo (m); a altura segue a proporção da imagem. */
  width: number;
  rot: number;
  opacity: number;
  locked: boolean;
}

export interface PSheet {
  paper: "A4" | "A3" | "custom";
  orientation: "paisagem" | "retrato";
  /** Escala de referência (50 = 1:50) para o tamanho da folha. */
  scale: number;
  x: number;
  y: number;
  w: number;
  h: number;
  grid: number;
  show_grid: boolean;
  png_width: number;
}

export interface POptions {
  /** Cotas externas automáticas no PNG técnico. */
  auto_dims: boolean;
  /** Medida interna escrita em cada parede, no PNG. */
  wall_measures: boolean;
  room_areas: boolean;
  label_kind: "letra" | "numero";
  compass: { show: boolean; x: number; y: number; deg: number };
}

export interface SicroPlantaDoc {
  schema_version: string;
  planta_id: string;
  occurrence_id: string;
  title: string;
  created_at: string;
  updated_at: string;
  sheet: PSheet;
  nodes: PNode[];
  walls: PWall[];
  openings: POpening[];
  rooms: PRoom[];
  items: PItem[];
  evidences: PEvidence[];
  people: PPerson[];
  trajectories: PTrajectory[];
  texts: PText[];
  dims: PDimension[];
  background: PBackground | null;
  options: POptions;
}

export const WALL_DEFAULT = { thickness: 0.15, height: 2.8 } as const;
export const WALL_THICKNESSES = [0.1, 0.15, 0.2, 0.25] as const;

export const OPENING_DEFAULTS: Record<OpeningKind, { label: string; width: number; sill: number; height: number }> = {
  porta: { label: "Porta", width: 0.8, sill: 0, height: 2.1 },
  porta_dupla: { label: "Porta dupla", width: 1.4, sill: 0, height: 2.1 },
  correr: { label: "Porta de correr", width: 1.6, sill: 0, height: 2.1 },
  janela: { label: "Janela", width: 1.2, sill: 1.0, height: 1.2 },
  basculante: { label: "Basculante", width: 0.6, sill: 1.6, height: 0.6 },
  vao: { label: "Vão livre", width: 1.0, sill: 0, height: 2.1 },
};

export const EVIDENCE_TIPOS: Record<EvidenceTipo, string> = {
  estojo: "Estojo",
  projetil: "Projétil",
  sangue: "Mancha de sangue",
  arma_fogo: "Arma de fogo",
  arma_branca: "Arma branca",
  celular: "Celular",
  documento: "Documento",
  pegada: "Pegada",
  outro: "Outro vestígio",
};

/** Folhas prontas: papel × escala → tamanho em metros. */
export const PAPERS = { A4: [297, 210], A3: [420, 297] } as const;
export const SCALES = [50, 75, 100, 125, 150, 200] as const;

export function sheetSize(paper: "A4" | "A3", orientation: "paisagem" | "retrato", scale: number) {
  const [a, b] = PAPERS[paper];
  const [w, h] = orientation === "paisagem" ? [a, b] : [b, a];
  return { w: (w * scale) / 1000, h: (h * scale) / 1000 };
}

export function uid(prefix: string): string {
  const r = typeof crypto !== "undefined" && "randomUUID" in crypto ? crypto.randomUUID().replace(/-/g, "") : Math.random().toString(36).slice(2) + Date.now().toString(36);
  return `${prefix}_${r.slice(0, 12)}`;
}

export function defaultSheet(): PSheet {
  const { w, h } = sheetSize("A4", "paisagem", 50);
  return { paper: "A4", orientation: "paisagem", scale: 50, x: 0, y: 0, w, h, grid: 1, show_grid: true, png_width: 3508 };
}

export function emptyPlanta(meta: { planta_id: string; occurrence_id: string; title: string; created_at?: string; updated_at?: string }): SicroPlantaDoc {
  const sheet = defaultSheet();
  const now = new Date().toISOString();
  return {
    schema_version: PLANTA_SCHEMA,
    planta_id: meta.planta_id,
    occurrence_id: meta.occurrence_id,
    title: meta.title,
    created_at: meta.created_at ?? now,
    updated_at: meta.updated_at ?? now,
    sheet,
    nodes: [],
    walls: [],
    openings: [],
    rooms: [],
    items: [],
    evidences: [],
    people: [],
    trajectories: [],
    texts: [],
    dims: [],
    background: null,
    options: {
      auto_dims: true,
      wall_measures: false,
      room_areas: true,
      label_kind: "letra",
      compass: { show: true, x: sheet.x + sheet.w - 1.2, y: sheet.y + 1.4, deg: 0 },
    },
  };
}

/** Planta do editor antigo (arcada/Pixi): a 5.0 não abre. */
export function isLegacyPlanta(raw: unknown): boolean {
  if (!raw || typeof raw !== "object") return false;
  const r = raw as Record<string, unknown>;
  const v = String(r.schema_version ?? "");
  return "floorplan" in r || !v.startsWith("2");
}

const arr = <T,>(v: unknown): T[] => (Array.isArray(v) ? (v as T[]) : []);

/** Completa campos faltando de um schema 2 (arquivos salvos por versões mais novas/antigas do 2.x). */
export function coercePlanta(raw: unknown, meta: { planta_id: string; occurrence_id: string; title: string }): SicroPlantaDoc {
  const base = emptyPlanta(meta);
  if (!raw || typeof raw !== "object") return base;
  const r = raw as Partial<SicroPlantaDoc>;
  return {
    ...base,
    ...r,
    schema_version: PLANTA_SCHEMA,
    sheet: { ...base.sheet, ...(r.sheet ?? {}) },
    nodes: arr(r.nodes),
    walls: arr<Partial<PWall>>(r.walls).map((w) => ({ ...WALL_DEFAULT, kind: "parede" as WallKind, ...w }) as PWall),
    openings: arr(r.openings),
    rooms: arr<Partial<PRoom>>(r.rooms).map((x) => ({ label: null, show_area: true, ...x }) as PRoom),
    items: arr(r.items),
    evidences: arr<Partial<PEvidence>>(r.evidences).map((e) => ({ measure: "paredes" as const, show_measure: false, points: [], descricao: "", ...e }) as PEvidence),
    people: arr(r.people),
    trajectories: arr(r.trajectories),
    texts: arr(r.texts),
    dims: arr(r.dims),
    background: r.background ?? null,
    options: {
      ...base.options,
      ...(r.options ?? {}),
      compass: { ...base.options.compass, ...(r.options?.compass ?? {}) },
    },
  };
}
