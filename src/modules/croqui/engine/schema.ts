/**
 * Schema do `.sicrocroqui` — fonte da verdade do croqui (PNG é derivado).
 * Sem React/Konva. Compatibilidade: só ADICIONAR campos; renomear ou mudar
 * tipo exige bump de `schema_version`. Campos desconhecidos são preservados.
 */

export const CURRENT_SCHEMA_VERSION = "0.3";

/**
 * A folha do croqui: o retângulo que vai para o PNG, em px de mundo. Com
 * escala definida, a UI mostra e edita em metros. `origin_*` permite ajustar a
 * folha à cena sem mover os objetos (ausente ⇒ 0).
 */
export interface SicroCroquiCanvas {
  width_px: number;
  height_px: number;
  origin_x?: number;
  origin_y?: number;
  background_color: string;
  grid?: {
    enabled: boolean;
    size_px: number;
    /** Grade em metros (vale quando há escala); `size_px` fica como fallback. */
    size_m?: number;
  };
}

/** Escala; `null` ⇒ distâncias em pixels. `definition` guarda os dois pontos e a distância declarada para auditoria/recalibração. */
export interface SicroCroquiScale {
  px_per_m: number;
  definition?: {
    p1: SicroPoint;
    p2: SicroPoint;
    real_distance_m: number;
  };
}

export interface SicroPoint {
  x: number;
  y: number;
}

export interface SicroCroquiBackgroundImage {
  /** Caminho relativo ao workspace ou absoluto; o frontend resolve via convertFileSrc. */
  source_path: string;
  x: number;
  y: number;
  width: number;
  height: number;
  opacity: number;
  locked: boolean;
  /** Graus, em torno do centro da imagem. Opcional (default 0) para envelopes antigos. */
  rotation?: number;
  /** Sidecar JSON da importação de drone (proveniência: hash, k1/k2/k3, crop). Só auditoria. */
  sidecar_path?: string;
  /** Arquivo original de onde veio a derivada; o renderer sempre carrega `source_path`. */
  original_path?: string;
}

export type LayerKind =
  | "background"
  | "objects"
  | "annotations"
  | "vias"
  | "veiculos"
  | "vestigios"
  | "medidas"
  | "referenciais";

export interface SicroCroquiLayer {
  id: string;
  name: string;
  visible: boolean;
  locked: boolean;
  kind: LayerKind;
}

// ---- Objetos (union discriminada por `kind`) ----

type SicroObjectKind =
  | "vehicle"
  | "line"
  | "marker"
  | "text"
  | "measurement"
  | "road"
  | "roundabout"
  | "trace"
  | "fixture"
  | "person";

export type LineSubtype =
  | "road"
  | "r1"
  | "r2"
  | "lane"
  | "freehand"
  | "arrow"
  | "sidewalk"
  | "lane_separator"
  | "canteiro" // canteiro central (avenida)
  | "acostamento" // acostamento lateral
  | "trajetoria" // seta de trajetória do veículo
  | "callout"; // chamada explicativa (callout line)

export type MarkerSubtype =
  | "collision_x"
  | "victim_point"
  | "trace_point"
  | "brake_mark"
  | "drag_mark"
  | "fluid"
  | "blood"
  | "debris"
  // Pessoas renderizadas como marker para reaproveitar drag/select.
  | "pedestrian"
  | "body"
  | "skid_curve" // derrapagem em curva
  | "sulcagem" // sulcagem profunda
  | "ranhura" // ranhura no pavimento
  | "impact_area" // área de concentração de impacto
  | "rest_position" // ponto de repouso final do veículo
  | "semaforo"
  | "placa_pare"
  | "placa_preferencia"
  | "poste"
  | "arvore"
  | "guia" // guia / meio-fio (ponto)
  | "faixa_pedestre" // marcação puntual de faixa de pedestres
  // Frota SVG do designer — pedestres em decúbito (escala real ~1,6–1,75 m)
  | "pedestre_m_dorsal"
  | "pedestre_m_lateral"
  | "pedestre_m_ventral"
  | "pedestre_f_dorsal"
  | "pedestre_f_lateral"
  | "pedestre_f_ventral";

export type VehicleBodyType =
  | "car"
  | "sedan"
  | "suv"
  | "hatch"
  | "truck"
  | "caminhao"
  | "moto"
  | "bike"
  | "other"
  | "pickup"
  | "van"
  | "onibus"
  | "moto_esportiva"
  | "moto_carga"
  | "caminhao_pesado"
  | "carreta"
  // Frota SVG do designer (escala real 1mm=1m) — civis recoloríveis…
  | "van_furgao"
  | "micro_onibus"
  | "onibus_leito"
  | "reboque_guincho"
  | "trator"
  | "bike_estrada"
  | "bike_cargueira"
  // …e pintura oficial fixa (não recolorível).
  | "ambulancia"
  | "taxi"
  | "vtr_pm"
  | "vtr_pc"
  | "vtr_pci"
  | "vtr_bm"
  | "vtr_pp";

/** Agrupamento usado pelo painel de camadas. */
export type ObjectCategory =
  | "vias"
  | "veiculos"
  | "vestigios"
  | "anotacoes"
  | "medidas"
  | "referenciais"
  | "mobiliario_urbano"
  | "sinalizacao"
  | "entorno"
  | "pessoas"
  | "outros";

interface SicroObjectBase {
  id: string;
  layer_id: string;
  kind: SicroObjectKind;
  label?: string | null;
  /** Rótulo solto: deslocamento (px de mundo) a partir da âncora do objeto, tamanho e cor. Ausente = padrão. */
  label_dx?: number;
  label_dy?: number;
  label_size?: number;
  label_color?: string | null;
  /** Graus absolutos; ausente = padrão (0°, ou ao longo da linha na cota). */
  label_rotation?: number | null;
  color?: string | null;
  z?: number;
  /** Independente da visibilidade da camada. */
  visible?: boolean;
  locked?: boolean;
  /** Observação pericial livre. */
  notes?: string | null;
  category?: ObjectCategory;
}

export interface SicroVehicleObject extends SicroObjectBase {
  kind: "vehicle";
  x: number;
  y: number;
  width: number;
  height: number;
  rotation: number;
  body_type?: VehicleBodyType;
  /** Contorno tracejado (posição no impacto); só no desenho em traço. */
  tracejado?: boolean;
}

export interface SicroLineObject extends SicroObjectBase {
  kind: "line";
  subtype: LineSubtype;
  /** Lista plana: [x1, y1, x2, y2, ...]. */
  points: number[];
  stroke_width: number;
  dashed?: boolean;
}

export interface SicroMarkerObject extends SicroObjectBase {
  kind: "marker";
  subtype: MarkerSubtype;
  x: number;
  y: number;
  size: number;
  rotation?: number;
}

export interface SicroTextObject extends SicroObjectBase {
  kind: "text";
  x: number;
  y: number;
  text: string;
  font_size: number;
  rotation?: number;
}

export interface SicroMeasurementObject extends SicroObjectBase {
  kind: "measurement";
  p1: SicroPoint;
  p2: SicroPoint;
  /** Quando presente, substitui o rótulo calculado. */
  label_override?: string | null;
}

export type TraceSubtype =
  | "frenagem"
  | "derrapagem"
  | "arrasto"
  | "sulcagem"
  | "ranhura"
  | "fluido"
  | "fragmentos"
  | "colisao";

/** Vestígio paramétrico (ver `traces.ts`): traçado p0 → p1 com flecha, em px de mundo. */
export interface SicroTraceObject extends SicroObjectBase {
  kind: "trace";
  subtype: TraceSubtype;
  p0: SicroPoint;
  /** Fim do traçado; no fluido é o centro da poça, nos fragmentos o alcance do leque. */
  p1: SicroPoint;
  /** Flecha (px de mundo) do meio do traçado em relação à corda; + = normal (−dy, dx). */
  bend: number;
  /** Semente do desenho irregular (bordas, riscos, peças). */
  seed: number;
  /** Larguras/raios em metros, frações e graus; ausente = padrão do tipo. */
  params: Record<string, number | string | boolean>;
  /** Cota do comprimento (ou corda e flecha na derrapagem). */
  show_measure?: boolean;
}

export type FixtureSubtype =
  // Sinalização
  | "placa"
  | "semaforo"
  | "faixa_pedestre"
  | "retencao"
  | "lombada"
  | "area_conflito"
  | "seta"
  // Entorno
  | "poste"
  | "arvore"
  | "hidrante"
  | "abrigo"
  | "barreira"
  | "obstaculo"
  | "camera";

/**
 * Sinalização e entorno paramétricos (ver `fixtures.ts`). Em px de mundo:
 * elemento de ponto = `p0` posição e `p1` direção/alcance; elemento de linha = traçado p0 → p1.
 */
export interface SicroFixtureObject extends SicroObjectBase {
  kind: "fixture";
  subtype: FixtureSubtype;
  p0: SicroPoint;
  p1: SicroPoint;
  /** Flecha (px de mundo), só nos elementos de linha que curvam. */
  bend: number;
  seed: number;
  params: Record<string, number | string | boolean>;
}

export type PersonPosicao = "dorsal" | "ventral" | "lat_e" | "lat_d" | "empe";

/** Pessoa articulada (ver `people.ts`): posição em px, corpo em metros, pose em fração da altura. */
export interface SicroPersonObject extends SicroObjectBase {
  kind: "person";
  posicao: PersonPosicao;
  x: number;
  y: number;
  /** Graus; 0 = cabeça (ou frente, em pé) para o topo da folha. */
  rotation: number;
  altura_m: number;
  comp: "magro" | "medio" | "robusto";
  perfil: "M" | "F";
  /** Curvatura do tronco e giro da cabeça, em graus. */
  curva: number;
  cabeca: number;
  /** "giz": só o contorno, como a marcação no local. */
  acab: "normal" | "giz";
  /** "transparente": só o contorno (padrão no giz). */
  cor: "branco" | "cinza" | "transparente";
  /** Multiplicador da espessura do contorno; ausente = 1. */
  traco?: number;
  pose: {
    wrist: [SicroPoint, SicroPoint];
    ankle: [SicroPoint, SicroPoint];
    /** Lado da dobra de cotovelos e joelhos (±1). */
    sArm: [number, number];
    sLeg: [number, number];
  };
}

// Vias e rotatórias vivem só em `road-parity/` (`kind: "road_parity"` /
// "roundabout_parity"). Objetos `kind: "road"`/"roundabout" de croquis antigos
// são descartados pelo `coerceCroquiDoc`.

import type { SicroParityObject } from "./road-parity/types";
import type { ParityStyle } from "./road-parity/style";

export type SicroObject =
  | SicroVehicleObject
  | SicroLineObject
  | SicroMarkerObject
  | SicroTextObject
  | SicroMeasurementObject
  | SicroTraceObject
  | SicroFixtureObject
  | SicroPersonObject
  | SicroParityObject;

// ---- View / export / stamp (todos opcionais) ----

export interface SicroCroquiViewSettings {
  show_grid: boolean;
  grid_size: number; // px
  snap_to_grid: boolean;
  show_rulers: boolean;
  show_labels: boolean;
  show_measurements: boolean;
}

/** Preferências lembradas; o usuário ainda pode mudar na hora de exportar. */
export interface SicroCroquiExportSettings {
  with_stamp: boolean;
  with_background: boolean;
  with_legend: boolean;
  /** "tecnico" (default) | "limpo". */
  default_kind: string;
  /** Largura do PNG em px (a folha inteira, independente do zoom). Ausente ⇒ 3508 (A4 a 300 dpi). */
  png_width_px?: number;
}

export const CROQUI_EXPORT_WIDTH_PX_DEFAULT = 3508;

/** Cabeçalho técnico do PNG; o renderer usa o que estiver presente. */
export interface SicroCroquiStampMetadata {
  bo?: string | null;
  protocolo?: string | null;
  tipo_pericia?: string | null;
  municipio?: string | null;
  perito?: string | null;
  custom_note?: string | null;
}

/** Trilha de auditoria de uma importação OSM; o renderer ignora. */
export interface SicroOsmImportSession {
  imported_at: string;
  /** "osm" | "osm:overpass" | futuro. */
  source: string;
  center_lat: number;
  center_lon: number;
  radius_m: number;
  query_bbox: {
    min_lat: number;
    max_lat: number;
    min_lon: number;
    max_lon: number;
  };
  /** Ways que o perito escolheu. */
  selected_way_ids: number[];
  /** Escala sugerida na importação, para o modal reoferecer; nunca aplicada em `scale` automaticamente. */
  suggested_px_per_m?: number | null;
}

// ---- Envelope ----

/** Estilo das vias (parcial; ausente ⇒ planta técnica). Ver `road-parity/style.ts`. */
export type SicroCroquiStyle = Partial<ParityStyle>;

export interface SicroCroquiDoc {
  schema_version: string;
  croqui_id: string;
  occurrence_id: string;
  title: string;
  created_at: string;
  updated_at: string;
  canvas: SicroCroquiCanvas;
  scale: SicroCroquiScale | null;
  background_image: SicroCroquiBackgroundImage | null;
  layers: SicroCroquiLayer[];
  objects: SicroObject[];
  view_settings?: SicroCroquiViewSettings;
  export_settings?: SicroCroquiExportSettings;
  stamp_metadata?: SicroCroquiStampMetadata;
  osm_imports?: SicroOsmImportSession[];
  style?: SicroCroquiStyle;
}
