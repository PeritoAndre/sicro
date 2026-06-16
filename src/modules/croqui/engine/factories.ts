/**
 * Factory helpers — create a new `SicroObject` with sensible defaults so
 * the UI doesn't sprinkle UUIDs and constants everywhere.
 *
 * `crypto.randomUUID()` is available in modern browsers and in the Tauri
 * WebView (Chromium-based), so no extra dep needed.
 *
 * Fase S clean cut — Road v1 e Road v2 foram removidos. Vias e
 * rotatórias são criadas via `road-parity/factories` (`makeParityRoad`,
 * `makeParityRoundabout`). Este arquivo cobre apenas as primitivas não-via
 * (veículo, linha, marcador, texto, medição).
 */

import type {
  LineSubtype,
  MarkerSubtype,
  SicroLineObject,
  SicroMarkerObject,
  SicroMeasurementObject,
  SicroPoint,
  SicroTextObject,
  SicroVehicleObject,
  VehicleBodyType,
} from "./schema";
import type { SicroParityObject } from "./road-parity/types";
import { getPessoaArt, getVehicleRealDims } from "./vehicleArt";

const OBJECT_LAYER = "layer_objects";

/**
 * Vehicle silhouette presets — width × height in canvas pixels at zoom 1.
 * Numbers chosen from doc 03 §6.4 references (sedan ~4.5×1.8 m, SUV ~4.7×1.9
 * m, caminhão leve ~6.5×2.2 m, moto ~2.0×0.7 m, bike ~1.6×0.5 m). The pixel
 * mapping uses ~18 px/m as a sensible starting size — the user can
 * re-scale anything anyway. Keeping aspect ratios honest matters more
 * than absolute size for the spike.
 */
const VEHICLE_DIMENSIONS: Record<
  VehicleBodyType,
  { width: number; height: number; color: string }
> = {
  car: { width: 80, height: 40, color: "#3b82f6" },
  sedan: { width: 80, height: 35, color: "#3b82f6" },
  suv: { width: 84, height: 42, color: "#475569" },
  hatch: { width: 70, height: 36, color: "#0ea5e9" },
  truck: { width: 120, height: 50, color: "#7c2d12" },
  caminhao: { width: 120, height: 50, color: "#7c2d12" },
  moto: { width: 36, height: 16, color: "#facc15" },
  bike: { width: 28, height: 12, color: "#22c55e" },
  other: { width: 80, height: 40, color: "#6b7280" },
  // MVP 9 — frota expandida
  pickup: { width: 96, height: 42, color: "#0f766e" }, // caminhonete
  van: { width: 96, height: 46, color: "#854d0e" },
  onibus: { width: 220, height: 60, color: "#b45309" },
  moto_esportiva: { width: 38, height: 14, color: "#dc2626" },
  moto_carga: { width: 50, height: 28, color: "#a16207" }, // moto com bagageiro
  caminhao_pesado: { width: 160, height: 60, color: "#7c2d12" },
  carreta: { width: 280, height: 60, color: "#451a03" }, // cavalo + semi-reboque
  // Frota SVG do designer — presets em px usados só quando o doc NÃO tem
  // escala definida (com escala, makeVehicle usa as dimensões REAIS em m).
  van_furgao: { width: 96, height: 46, color: "#854d0e" },
  micro_onibus: { width: 140, height: 44, color: "#b45309" },
  onibus_leito: { width: 240, height: 78, color: "#b45309" },
  reboque_guincho: { width: 120, height: 54, color: "#0f766e" },
  trator: { width: 78, height: 38, color: "#15803d" },
  bike_estrada: { width: 28, height: 12, color: "#22c55e" },
  bike_cargueira: { width: 30, height: 14, color: "#22c55e" },
  // Pintura oficial fixa — a cor aqui é só rótulo/fallback (a arte não repinta).
  ambulancia: { width: 96, height: 42, color: "#dc2626" },
  taxi: { width: 80, height: 34, color: "#f59e0b" },
  vtr_pm: { width: 82, height: 36, color: "#1e3a8a" },
  vtr_pc: { width: 82, height: 35, color: "#111827" },
  vtr_pci: { width: 82, height: 33, color: "#111827" },
  vtr_bm: { width: 94, height: 40, color: "#b91c1c" },
  vtr_pp: { width: 82, height: 34, color: "#374151" },
};

function uid(prefix: string): string {
  // crypto.randomUUID is the canonical path in modern Chromium.
  if (typeof crypto !== "undefined" && "randomUUID" in crypto) {
    return `${prefix}_${crypto.randomUUID()}`;
  }
  // Defensive fallback for non-browser test environments (vitest under jsdom
  // sometimes lacks the API). Not cryptographically strong — only used to
  // disambiguate within a session.
  return `${prefix}_${Date.now().toString(36)}_${Math.random()
    .toString(36)
    .slice(2, 8)}`;
}

export function makeVehicle(
  p: SicroPoint,
  label = "V1",
  body_type: VehicleBodyType = "car",
  /**
   * Escala do doc (px por metro). Quando definida e o tipo tem arte do
   * designer (desenhada em escala real), o veículo é inserido já no TAMANHO
   * REAL — ex.: sedan 4,60×2,21 m. Sem escala, caem os presets em px.
   */
  pxPerM?: number | null,
): SicroVehicleObject {
  const preset = VEHICLE_DIMENSIONS[body_type] ?? VEHICLE_DIMENSIONS.car;
  const real = getVehicleRealDims(body_type);
  const useReal = real && typeof pxPerM === "number" && pxPerM > 0;
  return {
    id: uid("vehicle"),
    layer_id: OBJECT_LAYER,
    kind: "vehicle",
    x: p.x,
    y: p.y,
    // No croqui o eixo x do veículo é a FRENTE → width = comprimento.
    width: useReal ? real.lengthM * pxPerM : preset.width,
    height: useReal ? real.widthM * pxPerM : preset.height,
    rotation: 0,
    label,
    color: preset.color,
    body_type,
    visible: true,
    locked: false,
    category: "veiculos",
  };
}

/** Central palette for line subtypes (color + width + dashing). MVP 6+9. */
export const LINE_STYLES: Record<
  LineSubtype,
  { color: string; width: number; dashed: boolean }
> = {
  road: { color: "#1f2937", width: 6, dashed: false },
  r1: { color: "#d97706", width: 4, dashed: true },
  r2: { color: "#0ea5e9", width: 4, dashed: true },
  lane: { color: "#9ca3af", width: 2, dashed: false },
  lane_separator: { color: "#9ca3af", width: 2, dashed: true },
  sidewalk: { color: "#52525b", width: 3, dashed: false },
  arrow: { color: "#111827", width: 3, dashed: false },
  freehand: { color: "#111827", width: 2, dashed: false },
  // MVP 9
  canteiro: { color: "#22c55e", width: 8, dashed: false }, // verde grosso
  acostamento: { color: "#a8a29e", width: 4, dashed: false },
  trajetoria: { color: "#2563eb", width: 3, dashed: true },
  callout: { color: "#0ea5e9", width: 1.5, dashed: true },
};

export function makeLine(
  p1: SicroPoint,
  p2: SicroPoint,
  subtype: LineSubtype = "road",
): SicroLineObject {
  const style = LINE_STYLES[subtype];
  return {
    id: uid("line"),
    layer_id: OBJECT_LAYER,
    kind: "line",
    subtype,
    points: [p1.x, p1.y, p2.x, p2.y],
    stroke_width: style.width,
    dashed: style.dashed,
    color: style.color,
    label:
      subtype === "r1" ? "R1" : subtype === "r2" ? "R2" : null,
    visible: true,
    locked: false,
    category:
      subtype === "r1" || subtype === "r2"
        ? "referenciais"
        : subtype === "freehand" || subtype === "callout"
          ? "anotacoes"
          : "vias",
  };
}

/** Marker palette and default labels. MVP 6 + MVP 9. */
export const MARKER_STYLES: Record<
  MarkerSubtype,
  { color: string; defaultLabel: string; defaultSize: number }
> = {
  collision_x: { color: "#dc2626", defaultLabel: "X", defaultSize: 24 },
  victim_point: { color: "#7c3aed", defaultLabel: "V", defaultSize: 22 },
  trace_point: { color: "#059669", defaultLabel: "T", defaultSize: 22 },
  brake_mark: { color: "#1f2937", defaultLabel: "Frenagem", defaultSize: 60 },
  drag_mark: { color: "#52525b", defaultLabel: "Arrasto", defaultSize: 60 },
  fluid: { color: "#0e7490", defaultLabel: "Fluido", defaultSize: 28 },
  blood: { color: "#991b1b", defaultLabel: "Sangue", defaultSize: 28 },
  debris: { color: "#a16207", defaultLabel: "Destroços", defaultSize: 30 },
  pedestrian: { color: "#0f172a", defaultLabel: "Pedestre", defaultSize: 22 },
  body: { color: "#0f172a", defaultLabel: "Vítima", defaultSize: 32 },
  // Frota SVG do designer — pedestres em decúbito (size = comprimento; com
  // escala definida o makeMarker usa a altura humana real ~1,6–1,75 m).
  pedestre_m_dorsal: { color: "#0f172a", defaultLabel: "Vítima M (dorsal)", defaultSize: 60 },
  pedestre_m_lateral: { color: "#0f172a", defaultLabel: "Vítima M (lateral)", defaultSize: 60 },
  pedestre_m_ventral: { color: "#0f172a", defaultLabel: "Vítima M (ventral)", defaultSize: 60 },
  pedestre_f_dorsal: { color: "#0f172a", defaultLabel: "Vítima F (dorsal)", defaultSize: 56 },
  pedestre_f_lateral: { color: "#0f172a", defaultLabel: "Vítima F (lateral)", defaultSize: 56 },
  pedestre_f_ventral: { color: "#0f172a", defaultLabel: "Vítima F (ventral)", defaultSize: 56 },
  // MVP 9 — vestígios adicionais
  skid_curve: { color: "#1f2937", defaultLabel: "Derrapagem", defaultSize: 70 },
  sulcagem: { color: "#451a03", defaultLabel: "Sulcagem", defaultSize: 60 },
  ranhura: { color: "#78350f", defaultLabel: "Ranhura", defaultSize: 50 },
  impact_area: { color: "#b91c1c", defaultLabel: "Área de impacto", defaultSize: 80 },
  rest_position: { color: "#0e7490", defaultLabel: "Repouso final", defaultSize: 28 },
  // MVP 9 — mobiliário urbano
  semaforo: { color: "#f97316", defaultLabel: "Semáforo", defaultSize: 22 },
  placa_pare: { color: "#dc2626", defaultLabel: "PARE", defaultSize: 26 },
  placa_preferencia: { color: "#f59e0b", defaultLabel: "Preferência", defaultSize: 26 },
  poste: { color: "#52525b", defaultLabel: "Poste", defaultSize: 14 },
  arvore: { color: "#15803d", defaultLabel: "Árvore", defaultSize: 26 },
  guia: { color: "#a8a29e", defaultLabel: "Guia", defaultSize: 18 },
  faixa_pedestre: { color: "#1e293b", defaultLabel: "Faixa pedestre", defaultSize: 40 },
};

export function makeMarker(
  p: SicroPoint,
  subtype: MarkerSubtype = "collision_x",
  labelOverride?: string,
  /** Escala do doc — pedestres com arte entram no tamanho humano real. */
  pxPerM?: number | null,
): SicroMarkerObject {
  const style = MARKER_STYLES[subtype];
  const pessoa = getPessoaArt(subtype);
  const size =
    pessoa && typeof pxPerM === "number" && pxPerM > 0
      ? pessoa.lengthM * pxPerM
      : style.defaultSize;
  // Mobiliário urbano vai para a categoria dedicada (MVP 9).
  const isMobiliario =
    subtype === "semaforo" ||
    subtype === "placa_pare" ||
    subtype === "placa_preferencia" ||
    subtype === "poste" ||
    subtype === "arvore" ||
    subtype === "guia" ||
    subtype === "faixa_pedestre";
  return {
    id: uid("marker"),
    layer_id: OBJECT_LAYER,
    kind: "marker",
    subtype,
    x: p.x,
    y: p.y,
    size,
    color: style.color,
    label: labelOverride ?? style.defaultLabel,
    visible: true,
    locked: false,
    category: isMobiliario ? "mobiliario_urbano" : "vestigios",
  };
}

export function makeText(p: SicroPoint, text = "Anotação"): SicroTextObject {
  return {
    id: uid("text"),
    layer_id: OBJECT_LAYER,
    kind: "text",
    x: p.x,
    y: p.y,
    text,
    font_size: 16,
    color: "#111827",
    visible: true,
    locked: false,
    category: "anotacoes",
  };
}

export function makeMeasurement(
  p1: SicroPoint,
  p2: SicroPoint,
): SicroMeasurementObject {
  return {
    id: uid("measurement"),
    layer_id: OBJECT_LAYER,
    kind: "measurement",
    p1,
    p2,
    color: "#dc2626",
    visible: true,
    locked: false,
    category: "medidas",
  };
}

/**
 * Clone an object with a new id (Ctrl+D / duplicate).
 *
 * Fase S — agora aceita também os tipos parity (`road_parity`,
 * `roundabout_parity`); o offset de 16 px aplica em coordenadas
 * de mundo (metros) — efetivamente um deslocamento sub-pixel quando
 * `px_per_m` está em torno de 10. Para um clone que se distinga
 * visualmente, o caller deve adicionar offset adicional. (TODO Fase S+1)
 */
export function cloneObject<T extends SicroVehicleObject
  | SicroLineObject
  | SicroMarkerObject
  | SicroTextObject
  | SicroMeasurementObject
  | SicroParityObject>(source: T): T {
  const cloned = { ...source } as T;
  cloned.id = uid(source.kind);
  // Nudge so the duplicate doesn't overlap the source visually.
  if ("x" in cloned && typeof cloned.x === "number") {
    cloned.x += 16;
  }
  if ("y" in cloned && typeof cloned.y === "number") {
    cloned.y += 16;
  }
  if (cloned.kind === "line") {
    cloned.points = source.kind === "line"
      ? source.points.map((v, i) => v + (i % 2 === 0 ? 16 : 16))
      : cloned.points;
  }
  if (cloned.kind === "measurement") {
    if (source.kind === "measurement") {
      cloned.p1 = { x: source.p1.x + 16, y: source.p1.y + 16 };
      cloned.p2 = { x: source.p2.x + 16, y: source.p2.y + 16 };
    }
  }
  // Parity road — desloca todos os pontos de controle.
  if (cloned.kind === "road_parity" && source.kind === "road_parity") {
    cloned.ax = source.ax + 1;
    cloned.ay = source.ay + 1;
    cloned.bx = source.bx + 1;
    cloned.by = source.by + 1;
    cloned.cx1 = source.cx1 + 1;
    cloned.cy1 = source.cy1 + 1;
    cloned.cx2 = source.cx2 + 1;
    cloned.cy2 = source.cy2 + 1;
  }
  // Parity roundabout — desloca o centro.
  if (cloned.kind === "roundabout_parity" && source.kind === "roundabout_parity") {
    cloned.cx = source.cx + 1;
    cloned.cy = source.cy + 1;
  }
  return cloned;
}
