/**
 * Serializer — coerce arbitrary JSON into a `SicroCroquiDoc`.
 *
 * The Rust backend treats `.sicrocroqui` as opaque JSON, so we have no
 * type guarantees on the wire. The serializer:
 *   - fills missing fields with safe defaults (so older envelopes keep working);
 *   - drops fields it doesn't recognise (still preserves them in `raw_unknown`
 *     once we need that — out of scope for the spike);
 *   - throws on shape that can't possibly be a croqui (no `croqui_id`, etc.).
 *
 * Inverse direction is trivial: `JSON.stringify(doc)` works because the
 * shape is plain data — no class instances.
 *
 * Fase S clean cut — vias/rotatórias legacy (`kind: "road"`, `kind:
 * "roundabout"`) e os campos legacy `road_engine_version` / `parity_objects`
 * são silenciosamente DESCARTADOS pelo coercer. Croquis antigos perdem
 * essas primitivas (não eram distribuídos pra users) e voltam a abrir;
 * o único motor de via passa a ser Python Parity Engine (kind:
 * "road_parity" / "roundabout_parity").
 */

import {
  CURRENT_SCHEMA_VERSION,
  type ObjectCategory,
  type SicroCroquiCanvas,
  type SicroCroquiDoc,
  type SicroCroquiExportSettings,
  type SicroCroquiLayer,
  type SicroCroquiScale,
  type SicroCroquiBackgroundImage,
  type SicroCroquiStampMetadata,
  type SicroCroquiViewSettings,
  type SicroObject,
} from "./schema";

// ---------------------------------------------------------------------------
// MVP 9 — defaults for the new optional sections

const DEFAULT_VIEW_SETTINGS: SicroCroquiViewSettings = {
  show_grid: true,
  grid_size: 50,
  snap_to_grid: false,
  show_rulers: true,
  show_labels: true,
  show_measurements: true,
};

const DEFAULT_EXPORT_SETTINGS: SicroCroquiExportSettings = {
  with_stamp: true,
  with_background: true,
  with_legend: false,
  default_kind: "tecnico",
};

const DEFAULT_CANVAS: SicroCroquiCanvas = {
  width_px: 1600,
  height_px: 1000,
  background_color: "#ffffff",
  grid: { enabled: true, size_px: 50 },
};

const DEFAULT_LAYERS: SicroCroquiLayer[] = [
  {
    id: "layer_background",
    name: "Imagem de fundo",
    visible: true,
    locked: true,
    kind: "background",
  },
  {
    id: "layer_objects",
    name: "Objetos",
    visible: true,
    locked: false,
    kind: "objects",
  },
];

/**
 * Conjunto de `kind`s suportados pelo Fase S. Qualquer objeto com `kind`
 * fora desta lista é descartado pelo coercer (caso típico: croquis
 * pré-S com `kind: "road"` / `kind: "roundabout"` do engine v1/v2).
 */
const SUPPORTED_KINDS = new Set<string>([
  "vehicle",
  "line",
  "marker",
  "text",
  "measurement",
  "road_parity",
  "roundabout_parity",
]);

export function coerceCroquiDoc(raw: unknown): SicroCroquiDoc {
  if (!raw || typeof raw !== "object") {
    throw new Error("invalid .sicrocroqui: not an object");
  }
  const o = raw as Record<string, unknown>;
  const croqui_id = stringField(o, "croqui_id");
  const occurrence_id = stringField(o, "occurrence_id");
  if (!croqui_id || !occurrence_id) {
    throw new Error("invalid .sicrocroqui: missing croqui_id or occurrence_id");
  }

  const layers = Array.isArray(o.layers)
    ? (o.layers as SicroCroquiLayer[])
    : DEFAULT_LAYERS;

  // Fase S — descarta silenciosamente objetos legacy (`kind: "road"`,
  // `kind: "roundabout"`, ou qualquer `kind` fora do conjunto suportado).
  // Os demais ganham `category` default se ausente.
  //
  // Não há migração para parity — quem precisar de via abre o croqui
  // e recria. Decisão consciente do usuário: croquis antigos não eram
  // distribuídos pra outros peritos.
  const objects: SicroObject[] = [];
  if (Array.isArray(o.objects)) {
    for (const rawObj of o.objects as unknown[]) {
      if (!rawObj || typeof rawObj !== "object") continue;
      const probe = rawObj as { kind?: unknown };
      if (typeof probe.kind !== "string" || !SUPPORTED_KINDS.has(probe.kind)) {
        continue;
      }
      const typed = rawObj as SicroObject;
      objects.push({ ...typed, category: typed.category ?? inferCategory(typed) } as SicroObject);
    }
  }

  // Fase S — Se o envelope antigo carrega `parity_objects` (Fase H.1),
  // são absorvidos pelo array `objects` principal. O coercer só aceita
  // formas válidas; outras passam sem validação fina (qualquer regressão
  // seria detectada como `kind` desconhecido).
  if (Array.isArray(o.parity_objects)) {
    for (const rawObj of o.parity_objects as unknown[]) {
      if (!rawObj || typeof rawObj !== "object") continue;
      const probe = rawObj as { kind?: unknown };
      if (typeof probe.kind !== "string" || !SUPPORTED_KINDS.has(probe.kind)) {
        continue;
      }
      const typed = rawObj as SicroObject;
      objects.push({ ...typed, category: typed.category ?? inferCategory(typed) } as SicroObject);
    }
  }

  return {
    schema_version:
      stringField(o, "schema_version") ?? CURRENT_SCHEMA_VERSION,
    croqui_id,
    occurrence_id,
    title: stringField(o, "title") ?? "Croqui sem título",
    created_at: stringField(o, "created_at") ?? new Date().toISOString(),
    updated_at: stringField(o, "updated_at") ?? new Date().toISOString(),
    canvas: coerceCanvas(o.canvas),
    scale: coerceScale(o.scale),
    background_image: coerceBackgroundImage(o.background_image),
    layers,
    objects,
    // MVP 9 — opcionais aditivos
    view_settings: coerceViewSettings(o.view_settings),
    export_settings: coerceExportSettings(o.export_settings),
    stamp_metadata: coerceStampMetadata(o.stamp_metadata),
    // MVP 10 — array opcional. Passa pelo coercer só pra filtrar
    // entries claramente malformadas; documentos sem o campo carregam
    // intactos.
    ...(Array.isArray(o.osm_imports)
      ? { osm_imports: coerceOsmImports(o.osm_imports) }
      : {}),
  };
}

function coerceOsmImports(raw: unknown[]): SicroCroquiDoc["osm_imports"] {
  const out: NonNullable<SicroCroquiDoc["osm_imports"]> = [];
  for (const entry of raw) {
    if (!entry || typeof entry !== "object") continue;
    const e = entry as Record<string, unknown>;
    const lat = numberField(e, "center_lat");
    const lon = numberField(e, "center_lon");
    const radius = numberField(e, "radius_m");
    if (lat == null || lon == null || radius == null) continue;
    const bboxRaw =
      e.query_bbox && typeof e.query_bbox === "object"
        ? (e.query_bbox as Record<string, unknown>)
        : {};
    out.push({
      imported_at: stringField(e, "imported_at") ?? new Date().toISOString(),
      source: stringField(e, "source") ?? "osm",
      center_lat: lat,
      center_lon: lon,
      radius_m: radius,
      query_bbox: {
        min_lat: numberField(bboxRaw, "min_lat") ?? lat,
        max_lat: numberField(bboxRaw, "max_lat") ?? lat,
        min_lon: numberField(bboxRaw, "min_lon") ?? lon,
        max_lon: numberField(bboxRaw, "max_lon") ?? lon,
      },
      selected_way_ids: Array.isArray(e.selected_way_ids)
        ? (e.selected_way_ids.filter(
            (n) => typeof n === "number",
          ) as number[])
        : [],
      suggested_px_per_m: numberField(e, "suggested_px_per_m") ?? null,
    });
  }
  return out;
}

function coerceViewSettings(raw: unknown): SicroCroquiViewSettings {
  if (!raw || typeof raw !== "object") return { ...DEFAULT_VIEW_SETTINGS };
  const o = raw as Record<string, unknown>;
  return {
    show_grid: o.show_grid !== false,
    grid_size: numberField(o, "grid_size") ?? DEFAULT_VIEW_SETTINGS.grid_size,
    snap_to_grid: o.snap_to_grid === true,
    show_rulers: o.show_rulers !== false,
    show_labels: o.show_labels !== false,
    show_measurements: o.show_measurements !== false,
  };
}

function coerceExportSettings(raw: unknown): SicroCroquiExportSettings {
  if (!raw || typeof raw !== "object") return { ...DEFAULT_EXPORT_SETTINGS };
  const o = raw as Record<string, unknown>;
  return {
    with_stamp: o.with_stamp !== false,
    with_background: o.with_background !== false,
    with_legend: o.with_legend === true,
    default_kind:
      stringField(o, "default_kind") ?? DEFAULT_EXPORT_SETTINGS.default_kind,
  };
}

function coerceStampMetadata(raw: unknown): SicroCroquiStampMetadata {
  const base: SicroCroquiStampMetadata = {
    bo: null,
    protocolo: null,
    tipo_pericia: null,
    municipio: null,
    perito: null,
    custom_note: null,
  };
  if (!raw || typeof raw !== "object") return base;
  const o = raw as Record<string, unknown>;
  return {
    bo: stringField(o, "bo"),
    protocolo: stringField(o, "protocolo"),
    tipo_pericia: stringField(o, "tipo_pericia"),
    municipio: stringField(o, "municipio"),
    perito: stringField(o, "perito"),
    custom_note: stringField(o, "custom_note"),
  };
}

function coerceCanvas(raw: unknown): SicroCroquiCanvas {
  if (!raw || typeof raw !== "object") return { ...DEFAULT_CANVAS };
  const o = raw as Record<string, unknown>;
  return {
    width_px: numberField(o, "width_px") ?? DEFAULT_CANVAS.width_px,
    height_px: numberField(o, "height_px") ?? DEFAULT_CANVAS.height_px,
    background_color:
      stringField(o, "background_color") ?? DEFAULT_CANVAS.background_color,
    grid:
      o.grid && typeof o.grid === "object"
        ? {
            enabled:
              (o.grid as Record<string, unknown>).enabled !== false,
            size_px:
              numberField(o.grid as Record<string, unknown>, "size_px") ??
              50,
          }
        : DEFAULT_CANVAS.grid,
  };
}

function coerceScale(raw: unknown): SicroCroquiScale | null {
  if (!raw || typeof raw !== "object") return null;
  const o = raw as Record<string, unknown>;
  const px_per_m = numberField(o, "px_per_m");
  if (!px_per_m || px_per_m <= 0) return null;
  return {
    px_per_m,
    definition: o.definition as SicroCroquiScale["definition"],
  };
}

function coerceBackgroundImage(
  raw: unknown,
): SicroCroquiBackgroundImage | null {
  if (!raw || typeof raw !== "object") return null;
  const o = raw as Record<string, unknown>;
  const source_path = stringField(o, "source_path");
  if (!source_path) return null;
  // MVP 9 Round 5 — `rotation`, `sidecar_path`, `original_path` are all
  // optional and default to safe values, so docs from earlier rounds
  // keep loading without intervention.
  const sidecar = stringField(o, "sidecar_path");
  const original = stringField(o, "original_path");
  return {
    source_path,
    x: numberField(o, "x") ?? 0,
    y: numberField(o, "y") ?? 0,
    width: numberField(o, "width") ?? 0,
    height: numberField(o, "height") ?? 0,
    opacity: numberField(o, "opacity") ?? 1,
    locked: o.locked !== false,
    rotation: numberField(o, "rotation") ?? 0,
    ...(sidecar ? { sidecar_path: sidecar } : {}),
    ...(original ? { original_path: original } : {}),
  };
}

/** Project an object onto the layer-panel category buckets. */
export function inferCategory(obj: SicroObject): ObjectCategory {
  switch (obj.kind) {
    case "vehicle":
      return "veiculos";
    case "measurement":
      return "medidas";
    case "text":
      return "anotacoes";
    case "road_parity":
    case "roundabout_parity":
      // Vias e rotatórias parity vão para a camada "vias".
      return "vias";
    case "line":
      if (obj.subtype === "r1" || obj.subtype === "r2") return "referenciais";
      if (
        obj.subtype === "road" ||
        obj.subtype === "lane" ||
        obj.subtype === "lane_separator" ||
        obj.subtype === "sidewalk" ||
        obj.subtype === "arrow" ||
        obj.subtype === "canteiro" ||
        obj.subtype === "acostamento" ||
        obj.subtype === "trajetoria"
      ) {
        return "vias";
      }
      if (obj.subtype === "callout") {
        return "anotacoes";
      }
      return "outros";
    case "marker":
      // Mobiliário urbano (placas/postes/árvores/semáforos/faixa).
      if (
        obj.subtype === "semaforo" ||
        obj.subtype === "placa_pare" ||
        obj.subtype === "placa_preferencia" ||
        obj.subtype === "poste" ||
        obj.subtype === "arvore" ||
        obj.subtype === "guia" ||
        obj.subtype === "faixa_pedestre"
      ) {
        return "mobiliario_urbano";
      }
      // Vestígios periciais.
      if (
        obj.subtype === "collision_x" ||
        obj.subtype === "brake_mark" ||
        obj.subtype === "drag_mark" ||
        obj.subtype === "fluid" ||
        obj.subtype === "blood" ||
        obj.subtype === "debris" ||
        obj.subtype === "trace_point" ||
        obj.subtype === "skid_curve" ||
        obj.subtype === "sulcagem" ||
        obj.subtype === "ranhura" ||
        obj.subtype === "impact_area" ||
        obj.subtype === "rest_position"
      ) {
        return "vestigios";
      }
      // Pessoas — pericial contexto.
      if (
        obj.subtype === "pedestrian" ||
        obj.subtype === "body" ||
        obj.subtype === "victim_point"
      ) {
        return "vestigios";
      }
      return "outros";
    default:
      return "outros";
  }
}

function stringField(o: Record<string, unknown>, key: string): string | null {
  const v = o[key];
  return typeof v === "string" && v.length > 0 ? v : null;
}
function numberField(o: Record<string, unknown>, key: string): number | null {
  const v = o[key];
  return typeof v === "number" && Number.isFinite(v) ? v : null;
}

/** Stamp `updated_at` and serialize. */
export function serializeCroquiDoc(doc: SicroCroquiDoc): SicroCroquiDoc {
  return {
    ...doc,
    schema_version: CURRENT_SCHEMA_VERSION,
    updated_at: new Date().toISOString(),
  };
}
