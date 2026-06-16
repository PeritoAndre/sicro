/**
 * Schema do `.sicroplanta` — croqui de planta baixa (patrimonial / cena).
 *
 * O backend trata o arquivo como JSON OPACO; este módulo é a fonte de verdade.
 * `coercePlantaDoc` carrega qualquer JSON, preenche defaults e nunca quebra ao
 * abrir. O `floorplan` é o modelo do motor de planta (fork arcada) e é mantido
 * INTACTO (opaco aqui — o Serializer do motor cuida dele). A camada PERICIAL
 * (vestígios + bússola + escala) é nossa.
 */

import {
  isEvidenceTipo,
  type EvidenceTipo,
  type EvidenceLabelKind,
} from "./evidence";

export const PLANTA_SCHEMA_VERSION = "0.1";

/** Marcador de vestígio posicionado sobre a planta (coords do mundo Pixi). */
export interface PlantaEvidenceMarker {
  id: string;
  x: number;
  y: number;
  tipo: EvidenceTipo;
  descricao?: string | null;
  /** Override de cor; se ausente, usa a cor do tipo. */
  cor?: string | null;
}

/** Trajetória balística (linha de tiro) — origem → impacto, coords do mundo Pixi. */
export interface PlantaTrajectory {
  id: string;
  x1: number;
  y1: number;
  x2: number;
  y2: number;
  /** Rótulo curto (ex.: "T1"); se ausente, derivado da ordem. */
  label?: string | null;
  cor?: string | null;
}

/** Rótulo de texto livre posicionado sobre a planta (coords do mundo Pixi). */
export interface PlantaText {
  id: string;
  x: number;
  y: number;
  text: string;
  /** Tamanho da fonte em px de mundo (METER=100 ⇒ 28 ≈ 0,28 m de altura). */
  size?: number;
  cor?: string | null;
}

/** Tipos de estrutura linear desenhada à parte das paredes do motor. */
export type PlantaStructureKind =
  | "muro"
  | "cerca_madeira"
  | "cerca_arame"
  | "calcada";

/** Estrutura linear (muro/cerca/calçada) — segmento origem→fim, coords do mundo. */
export interface PlantaStructure {
  id: string;
  kind: PlantaStructureKind;
  x1: number;
  y1: number;
  x2: number;
  y2: number;
  /** Espessura/largura em px de mundo; default por tipo se ausente. */
  espessura?: number;
}

export interface SicroPlantaDoc {
  schema_version: string;
  planta_id: string;
  occurrence_id: string;
  title: string;
  created_at: string;
  updated_at: string;
  /** Pixels por metro do motor (arcada METER=100). */
  px_per_m: number;
  /** Modelo do motor de planta (FloorPlanSerializable). Opaco aqui. */
  floorplan: unknown;
  /** Esquema de rótulo dos vestígios: letra (A,B,C) ou número (1,2,3). */
  label_kind: EvidenceLabelKind;
  evidences: PlantaEvidenceMarker[];
  /** Trajetórias balísticas (linhas de tiro). */
  trajectories: PlantaTrajectory[];
  /** Rótulos de texto livre. */
  texts: PlantaText[];
  /** Estruturas lineares (LEGADO — substituídas por skins de parede). */
  structures: PlantaStructure[];
  /** Estilo ("skin") por parede: chave = par de nós ordenado, valor = tipo. */
  wallStyles: Record<string, PlantaStructureKind>;
  /** Offset (x,y locais) do rótulo de cota arrastado, por parede (par de nós). */
  labelOffsets: Record<string, { x: number; y: number }>;
  /** Rotação da rosa dos ventos, em graus (0 = Norte pra cima). */
  compass_deg: number;
}

// ---------------------------------------------------------------------------
// Coerção

const VALID_LABEL_KINDS: EvidenceLabelKind[] = ["letra", "numero"];

function str(o: Record<string, unknown>, k: string): string | undefined {
  const v = o[k];
  return typeof v === "string" ? v : undefined;
}
function num(v: unknown, fallback: number): number {
  return typeof v === "number" && Number.isFinite(v) ? v : fallback;
}
function strOrNull(v: unknown): string | null {
  return typeof v === "string" && v.length > 0 ? v : null;
}

function coerceEvidence(raw: unknown, index: number): PlantaEvidenceMarker | null {
  if (!raw || typeof raw !== "object") return null;
  const o = raw as Record<string, unknown>;
  const tipo: EvidenceTipo = isEvidenceTipo(o["tipo"]) ? o["tipo"] : "outro";
  return {
    id: str(o, "id") ?? `ev_${Date.now().toString(36)}_${index}`,
    x: num(o["x"], 0),
    y: num(o["y"], 0),
    tipo,
    descricao: strOrNull(o["descricao"]),
    cor: strOrNull(o["cor"]),
  };
}

function coerceTrajectory(raw: unknown, index: number): PlantaTrajectory | null {
  if (!raw || typeof raw !== "object") return null;
  const o = raw as Record<string, unknown>;
  return {
    id: str(o, "id") ?? `tr_${Date.now().toString(36)}_${index}`,
    x1: num(o["x1"], 0),
    y1: num(o["y1"], 0),
    x2: num(o["x2"], 0),
    y2: num(o["y2"], 0),
    label: strOrNull(o["label"]),
    cor: strOrNull(o["cor"]),
  };
}

function coerceText(raw: unknown, index: number): PlantaText | null {
  if (!raw || typeof raw !== "object") return null;
  const o = raw as Record<string, unknown>;
  const text = str(o, "text");
  if (!text) return null; // texto vazio não persiste
  return {
    id: str(o, "id") ?? `tx_${Date.now().toString(36)}_${index}`,
    x: num(o["x"], 0),
    y: num(o["y"], 0),
    text,
    size: num(o["size"], 28),
    cor: strOrNull(o["cor"]),
  };
}

const VALID_STRUCTURE_KINDS: PlantaStructureKind[] = [
  "muro",
  "cerca_madeira",
  "cerca_arame",
  "calcada",
];

function coerceStructure(raw: unknown, index: number): PlantaStructure | null {
  if (!raw || typeof raw !== "object") return null;
  const o = raw as Record<string, unknown>;
  const kRaw = str(o, "kind");
  const kind: PlantaStructureKind = VALID_STRUCTURE_KINDS.includes(
    kRaw as PlantaStructureKind,
  )
    ? (kRaw as PlantaStructureKind)
    : "muro";
  return {
    id: str(o, "id") ?? `st_${Date.now().toString(36)}_${index}`,
    kind,
    x1: num(o["x1"], 0),
    y1: num(o["y1"], 0),
    x2: num(o["x2"], 0),
    y2: num(o["y2"], 0),
    espessura:
      typeof o["espessura"] === "number" && Number.isFinite(o["espessura"])
        ? (o["espessura"] as number)
        : undefined,
  };
}

const DEFAULT_FLOORPLAN = { floors: [], furnitureId: 0, wallNodeId: 0 };

export function coercePlantaDoc(raw: unknown): SicroPlantaDoc {
  if (!raw || typeof raw !== "object") {
    throw new Error("invalid .sicroplanta: not an object");
  }
  const o = raw as Record<string, unknown>;
  const planta_id = str(o, "planta_id");
  const occurrence_id = str(o, "occurrence_id");
  if (!planta_id || !occurrence_id) {
    throw new Error("invalid .sicroplanta: missing planta_id or occurrence_id");
  }

  const labelRaw = str(o, "label_kind");
  const label_kind: EvidenceLabelKind = VALID_LABEL_KINDS.includes(
    labelRaw as EvidenceLabelKind,
  )
    ? (labelRaw as EvidenceLabelKind)
    : "letra";

  const floorplan =
    o["floorplan"] && typeof o["floorplan"] === "object"
      ? o["floorplan"]
      : { ...DEFAULT_FLOORPLAN };

  const evRaw = Array.isArray(o["evidences"]) ? o["evidences"] : [];
  const evidences = evRaw
    .map((e, i) => coerceEvidence(e, i))
    .filter((e): e is PlantaEvidenceMarker => e !== null);

  const trRaw = Array.isArray(o["trajectories"]) ? o["trajectories"] : [];
  const trajectories = trRaw
    .map((t, i) => coerceTrajectory(t, i))
    .filter((t): t is PlantaTrajectory => t !== null);

  const txRaw = Array.isArray(o["texts"]) ? o["texts"] : [];
  const texts = txRaw
    .map((t, i) => coerceText(t, i))
    .filter((t): t is PlantaText => t !== null);

  const stRaw = Array.isArray(o["structures"]) ? o["structures"] : [];
  const structures = stRaw
    .map((s, i) => coerceStructure(s, i))
    .filter((s): s is PlantaStructure => s !== null);

  const wsRaw =
    o["wallStyles"] && typeof o["wallStyles"] === "object"
      ? (o["wallStyles"] as Record<string, unknown>)
      : {};
  const wallStyles: Record<string, PlantaStructureKind> = {};
  for (const k of Object.keys(wsRaw)) {
    const v = wsRaw[k];
    if (VALID_STRUCTURE_KINDS.includes(v as PlantaStructureKind)) {
      wallStyles[k] = v as PlantaStructureKind;
    }
  }

  const loRaw =
    o["labelOffsets"] && typeof o["labelOffsets"] === "object"
      ? (o["labelOffsets"] as Record<string, unknown>)
      : {};
  const labelOffsets: Record<string, { x: number; y: number }> = {};
  for (const k of Object.keys(loRaw)) {
    const v = loRaw[k] as { x?: unknown; y?: unknown } | null;
    if (v && typeof v.x === "number" && typeof v.y === "number") {
      labelOffsets[k] = { x: v.x, y: v.y };
    }
  }

  const now = str(o, "updated_at") ?? str(o, "created_at") ?? "";
  return {
    schema_version: str(o, "schema_version") ?? PLANTA_SCHEMA_VERSION,
    planta_id,
    occurrence_id,
    title: str(o, "title") ?? "Croqui de planta",
    created_at: str(o, "created_at") ?? now,
    updated_at: str(o, "updated_at") ?? now,
    px_per_m: num(o["px_per_m"], 100),
    floorplan,
    label_kind,
    evidences,
    trajectories,
    texts,
    structures,
    wallStyles,
    labelOffsets,
    compass_deg: num(o["compass_deg"], 0),
  };
}

