/**
 * Python Parity Engine — OSM Adapter (Fase H.5).
 *
 * Pipeline OSM → Python Parity Engine. Gera diretamente
 * `SicroRoadObject_parity` + `SicroRoundaboutObject_parity` (NÃO
 * `SicroRoadObject` v2). O resultado precisa parecer o motor aprovado
 * em H.3 — não Road v2, não flares, não junction patches, não
 * smoothing modes, não lane_count, não width em pixels.
 *
 * Pipeline:
 *
 *   OpenStreetMap / Overpass
 *     → nodes/ways/tags
 *     → projeção métrica local (cos-corrected longitude)
 *     → classificação simples por highway (largura_m + marcacao)
 *     → fit uniforme ao canvas (px_per_m)
 *     → Hermite/Bezier 4 pontos
 *     → SicroRoadObject_parity (mundo, metros)
 *     → SicroRoundaboutObject_parity (mundo, metros)
 *     → RoadParityRenderer
 *
 * Princípios:
 *
 *   - OSM é só fonte de geometria.
 *   - Larguras em METROS (largura_m), não pixels.
 *   - Tudo em coordenadas de mundo (metros) — o renderer aplica
 *     pxPerM no momento de desenhar.
 *   - Não destrói topologia: não clipa por raio agressivamente, não
 *     fragmenta endpoints (causa direta da regressão G.3).
 *   - Não rouba responsabilidades do renderer — calçada, eixo,
 *     borda externa são desenhados pelo `RoadParityRenderer`.
 *
 * Restrições verbatim do briefing H.5:
 *
 *   - NÃO gerar SicroRoadObject (Road v2).
 *   - NÃO usar RoadNetworkLayerV2.
 *   - NÃO usar flares.
 *   - NÃO usar junction patches.
 *   - NÃO usar smoothing modes.
 *   - NÃO usar lane_count.
 *   - NÃO usar width em pixels.
 *
 * Pure functions — sem React, sem Konva, sem fetch.
 */

import {
  osmLanesHint,
  osmOnewayToDirection,
  type OsmNode,
  type OsmWay,
} from "../osm";
import { simplifyPolylineDP } from "../osm";
import {
  makeParityRoadBezier,
  makeParityRoundabout,
} from "./factories";
import type {
  ParityMarcacao,
  SicroRoadObject_parity,
  SicroRoundaboutObject_parity,
} from "./types";

// ---------------------------------------------------------------------------
// Tipos Vec2 locais (paridade com `geometry.ts` — coords mundo).

interface Vec2M {
  x: number;
  y: number;
}

// ---------------------------------------------------------------------------
// Tabelas de paridade com SICRO 1.0 Python — `_LARG_CLASSE` + `_marcacao`.

/**
 * Largura física em METROS por classe OSM `highway=*`. Tabela direta
 * do briefing H.5 (paridade com `desenho/osm_via.py:38-53` do SICRO 1.0
 * Python — _LARG_CLASSE).
 *
 *   - primary / trunk:        10.5 m
 *   - secondary:               8.5 m
 *   - tertiary:                7.5 m
 *   - residential / unclassified: 6.0 m
 *   - service:                 4.5 m
 *   - footway / path / pedestrian / cycleway: IGNORAR (não-veiculares)
 *
 * Vias `*_link` herdam a classe principal (ex: primary_link → 10.5 m).
 */
export function parityRoadWidthMetersByHighway(
  highway: string | undefined,
): number {
  if (!highway) return 7.0;
  const h = highway.toLowerCase();
  if (
    h === "motorway" ||
    h === "trunk" ||
    h === "primary" ||
    h === "motorway_link" ||
    h === "trunk_link" ||
    h === "primary_link"
  ) {
    return 10.5;
  }
  if (h === "secondary" || h === "secondary_link") return 8.5;
  if (h === "tertiary" || h === "tertiary_link") return 7.5;
  if (
    h === "residential" ||
    h === "unclassified" ||
    h === "living_street"
  ) {
    return 6.0;
  }
  if (h === "service" || h === "parking_aisle") return 4.5;
  // Default conservador.
  return 6.5;
}

/**
 * Set de classes OSM que **NÃO devem virar via parity** (briefing H.5):
 *
 *   - footway, path, pedestrian, cycleway, steps, bridleway, track.
 *
 * Track foi incluído como ignorável porque "estrada de chão" em OSM
 * tipicamente é trilha rural — não cabe num croqui pericial urbano.
 * Se o perito precisar, pode ser adicionado depois via Inspector.
 */
const NON_VEHICLE_HIGHWAYS = new Set([
  "footway",
  "path",
  "pedestrian",
  "cycleway",
  "steps",
  "bridleway",
  "track",
  "corridor",
  "elevator",
  "platform",
  "via_ferrata",
]);

export function isNonVehicleHighway(
  highway: string | undefined,
): boolean {
  if (!highway) return true;
  return NON_VEHICLE_HIGHWAYS.has(highway.toLowerCase());
}

/**
 * Marcação central por classe OSM. Paridade com `_marcacao_para_highway`
 * do SICRO 1.0 Python + ajuste do briefing H.5:
 *
 *   - primary/secondary/tertiary mão dupla: amarela
 *   - residential mão dupla: branca (convenção brasileira urbana)
 *   - service mão dupla: branca
 *   - oneway: nenhuma (eixo central não faz sentido em mão única)
 *   - non-vehicle: nenhuma
 */
export function parityRoadMarkingByHighway(
  highway: string | undefined,
  isOneWay: boolean,
): ParityMarcacao {
  if (isOneWay) return "nenhuma";
  if (!highway) return "amarela";
  const h = highway.toLowerCase();
  if (
    h === "motorway" ||
    h === "trunk" ||
    h === "primary" ||
    h === "motorway_link" ||
    h === "trunk_link" ||
    h === "primary_link" ||
    h === "secondary" ||
    h === "secondary_link" ||
    h === "tertiary" ||
    h === "tertiary_link"
  ) {
    return "amarela";
  }
  if (
    h === "residential" ||
    h === "unclassified" ||
    h === "living_street" ||
    h === "service"
  ) {
    return "branca";
  }
  return "amarela";
}

// ---------------------------------------------------------------------------
// Tipos públicos.

export interface OsmParityImportInput {
  ways: OsmWay[];
  nodes: OsmNode[];
  center: { lat: number; lon: number };
  radius_m: number;
  canvas: { width: number; height: number };
  options?: OsmParityImportOptions;
}

export interface OsmParityImportOptions {
  /** Fração do canvas reservada como margem em cada lado. Default 0.1. */
  margin?: number;
  /** Tolerância Douglas-Peucker em METROS. Default 0.6 m. */
  simplify_tolerance_m?: number;
  /** Comprimento mínimo (m) para uma way ser importada. Default 4 m. */
  min_way_length_m?: number;
  /**
   * Detectar rotatória via `junction=roundabout` ou geometria circular.
   * Default true.
   */
  preserve_roundabouts?: boolean;
  /**
   * Ignorar vias `footway`, `path`, `cycleway`, `pedestrian`, etc.
   * Default true (briefing H.5).
   */
  ignore_non_vehicle?: boolean;
  /**
   * Hard cap por raio — quando true, cada way é clipada ao círculo de
   * `input.radius_m` centrado no sinistro. Pedaços fora do círculo são
   * descartados. Quando uma way entra/sai múltiplas vezes, vira várias
   * sub-vias. Default true (Fase S round 2).
   *
   * Rotatórias NÃO são clipadas (ring fechado precisa de geometria
   * completa para virar centro + raio; se o centro está fora do raio
   * a rotatória inteira é descartada).
   */
  clip_to_radius?: boolean;
}

export interface OsmParityImportStats {
  node_count: number;
  way_count: number;
  imported_road_count: number;
  imported_roundabout_count: number;
  skipped_count: number;
  /** Escala em px/m sugerida pelo fit. */
  px_per_m: number;
  /**
   * Bounding box métrico (m) — origem = centro do sinistro, eixos
   * em metros locais.
   */
  metric_bbox: { min_x: number; max_x: number; min_y: number; max_y: number };
}

export interface OsmParityAdapterResult {
  roads: SicroRoadObject_parity[];
  roundabouts: SicroRoundaboutObject_parity[];
  warnings: string[];
  stats: OsmParityImportStats;
}

// ---------------------------------------------------------------------------
// Projeção lat/lon → metros locais.

const EARTH_R = 6_371_000; // m
const DEG2RAD = Math.PI / 180;

/**
 * Projeta lat/lon → metros locais relativos ao centro fornecido.
 *
 * Convenções (mesmo road-v2/osmAdapter):
 *   - X = leste positivo.
 *   - Y = sul positivo (eixo canvas Y-down).
 *   - Correção cos(lat) para distância em X.
 *   - Sem Mercator — erro < 0.1% em raios urbanos (< 2 km).
 */
export function projectLatLonToLocalMeters(
  lat: number,
  lon: number,
  centerLat: number,
  centerLon: number,
): Vec2M {
  const cosLat = Math.cos(centerLat * DEG2RAD);
  return {
    x: (lon - centerLon) * cosLat * EARTH_R * DEG2RAD,
    y: -(lat - centerLat) * EARTH_R * DEG2RAD,
  };
}

// ---------------------------------------------------------------------------
// Hermite → Bezier 4-point sobre polyline métrica.

export interface ParityBezierFit {
  /** Âncora inicial (mundo, metros). */
  start: Vec2M;
  /** Âncora final (mundo, metros). */
  end: Vec2M;
  /** Controles Bezier (mundo, metros). */
  c1: Vec2M;
  c2: Vec2M;
  /** Comprimento do arco linear em metros (sum of segments). */
  arcLengthM: number;
}

/**
 * Reduz uma polilinha métrica a **4 pontos Bezier cúbico** via tangentes
 * Hermite — paridade direta com `_pontos_para_spline` do SICRO 1.0 Python.
 *
 * Estratégia:
 *   - `start` = primeiro ponto.
 *   - `end` = último ponto.
 *   - Tangente inicial = direção do primeiro segmento (normalizada).
 *   - Tangente final = direção do último segmento (normalizada).
 *   - `arc` = comprimento total da polilinha.
 *   - c1 = start + tangente_inicial × (arc / 3)
 *   - c2 = end   − tangente_final   × (arc / 3)
 *
 * Equivalente ao Bezier "natural" — trechos retos viram retas, curvas
 * suaves preservam direção. Para vias OSM urbanas (típico 50–200 m), a
 * aproximação é excelente.
 *
 * Retorna `null` se a polilinha for degenerada (< 2 pontos OU
 * arc < 5 cm — clamp defensivo).
 */
export function polylineToParityBezier(
  pts: ReadonlyArray<Vec2M>,
): ParityBezierFit | null {
  if (pts.length < 2) return null;
  const a = pts[0] as Vec2M;
  const b = pts[pts.length - 1] as Vec2M;

  // Tangente inicial — primeiro segmento não-zero.
  let txStart = 0;
  let tyStart = 0;
  for (let i = 1; i < pts.length; i++) {
    const p = pts[i] as Vec2M;
    const dx = p.x - a.x;
    const dy = p.y - a.y;
    const len = Math.hypot(dx, dy);
    if (len > 1e-6) {
      txStart = dx / len;
      tyStart = dy / len;
      break;
    }
  }
  // Tangente final — último segmento não-zero.
  let txEnd = 0;
  let tyEnd = 0;
  for (let i = pts.length - 2; i >= 0; i--) {
    const p = pts[i] as Vec2M;
    const dx = b.x - p.x;
    const dy = b.y - p.y;
    const len = Math.hypot(dx, dy);
    if (len > 1e-6) {
      txEnd = dx / len;
      tyEnd = dy / len;
      break;
    }
  }
  // Arc length linear.
  let arc = 0;
  for (let i = 1; i < pts.length; i++) {
    const p = pts[i] as Vec2M;
    const prev = pts[i - 1] as Vec2M;
    arc += Math.hypot(p.x - prev.x, p.y - prev.y);
  }
  if (arc < 0.05) return null;
  const sc = arc / 3;
  return {
    start: a,
    end: b,
    c1: { x: a.x + txStart * sc, y: a.y + tyStart * sc },
    c2: { x: b.x - txEnd * sc, y: b.y - tyEnd * sc },
    arcLengthM: arc,
  };
}

// ---------------------------------------------------------------------------
// Detecção de rotatória OSM.

/**
 * Uma way OSM é rotatória se:
 *   - tag `junction=roundabout` OU `junction=circular`;
 *   - OU ring fechado (primeiro node_ref == último) com geometria
 *     aproximadamente circular (desvio padrão do raio < 30% da média).
 *
 * O segundo critério captura rotatórias mal-taggeadas. Critério 30%
 * é conservador — pouco falso positivo.
 *
 * Idêntico ao da road-v2/osmAdapter — mantém comportamento já validado.
 */
export function isOsmRoundaboutForParity(
  way: OsmWay,
  metricPoints: ReadonlyArray<Vec2M>,
): boolean {
  if (way.tags.junction === "roundabout") return true;
  if (way.tags.junction === "circular") return true;
  const refs = way.node_refs;
  if (refs.length < 5) return false;
  if (refs[0] !== refs[refs.length - 1]) return false;
  if (metricPoints.length < 5) return false;
  let cx = 0;
  let cy = 0;
  const unique = metricPoints.slice(0, -1);
  for (const p of unique) {
    cx += p.x;
    cy += p.y;
  }
  cx /= unique.length;
  cy /= unique.length;
  const radii = unique.map((p) => Math.hypot(p.x - cx, p.y - cy));
  const meanR = radii.reduce((acc, r) => acc + r, 0) / radii.length;
  if (meanR < 3) return false; // rotatória < 6 m de diâmetro é improvável
  const variance =
    radii.reduce((acc, r) => acc + (r - meanR) ** 2, 0) / radii.length;
  const stdDev = Math.sqrt(variance);
  return stdDev / meanR < 0.3;
}

// ---------------------------------------------------------------------------
// Helpers de label / metadata.

function pickLabel(tags: Record<string, string>): string | null {
  if (tags.name && tags.name.trim().length > 0) return tags.name.trim();
  if (tags.ref && tags.ref.trim().length > 0) return tags.ref.trim();
  return null;
}

function buildMetadataJson(
  way: OsmWay,
  extras: Record<string, unknown> = {},
): string {
  return JSON.stringify({
    source: "osm",
    osm_id: way.id,
    name: way.tags.name,
    highway: way.tags.highway,
    oneway: way.tags.oneway,
    lanes: way.tags.lanes,
    ref: way.tags.ref,
    junction: way.tags.junction,
    raw_tags: way.tags,
    ...extras,
  });
}

// ---------------------------------------------------------------------------
// Adapter principal.

interface ResolvedOptions {
  margin: number;
  simplify_tolerance_m: number;
  min_way_length_m: number;
  preserve_roundabouts: boolean;
  ignore_non_vehicle: boolean;
  clip_to_radius: boolean;
}

function resolveOptions(opts?: OsmParityImportOptions): ResolvedOptions {
  return {
    margin: clamp(opts?.margin ?? 0.1, 0, 0.45),
    simplify_tolerance_m: Math.max(opts?.simplify_tolerance_m ?? 0.6, 0),
    min_way_length_m: Math.max(opts?.min_way_length_m ?? 4, 0),
    preserve_roundabouts: opts?.preserve_roundabouts ?? true,
    ignore_non_vehicle: opts?.ignore_non_vehicle ?? true,
    clip_to_radius: opts?.clip_to_radius ?? true,
  };
}

// ---------------------------------------------------------------------------
// Clip por raio — corta polilinha pelo círculo de raio R centrado na origem.
//
// Algoritmo Cohen-Sutherland-like para círculo:
//   - Para cada segmento p→q:
//     * Se ambos dentro do círculo: emite o segmento.
//     * Se ambos fora: descarta.
//     * Caso misto: calcula interseção(ões) com o círculo e emite só
//       o trecho dentro.
//   - Quando o trecho dentro termina (saída do círculo), a sub-polilinha
//     atual fecha e uma nova começa quando reentra.
//
// Resultado: `Vec2M[][]` — array de sub-polilinhas, cada uma totalmente
// dentro (ou tocando) o círculo.

/**
 * Resolve interseção(ões) do segmento `a→b` com o círculo de raio `R`
 * centrado na origem. Retorna parâmetros `t ∈ [0, 1]` (posição
 * normalizada no segmento). Pode retornar 0, 1 ou 2 valores.
 */
function segmentCircleIntersections(
  a: Vec2M,
  b: Vec2M,
  R: number,
): number[] {
  const dx = b.x - a.x;
  const dy = b.y - a.y;
  const A = dx * dx + dy * dy;
  if (A < 1e-12) return []; // a == b
  const B = 2 * (a.x * dx + a.y * dy);
  const C = a.x * a.x + a.y * a.y - R * R;
  const disc = B * B - 4 * A * C;
  if (disc < 0) return [];
  const sqrtD = Math.sqrt(disc);
  const t1 = (-B - sqrtD) / (2 * A);
  const t2 = (-B + sqrtD) / (2 * A);
  const out: number[] = [];
  if (t1 > 0 && t1 < 1) out.push(t1);
  if (t2 > 0 && t2 < 1 && t2 !== t1) out.push(t2);
  return out;
}

function lerpVec(a: Vec2M, b: Vec2M, t: number): Vec2M {
  return { x: a.x + (b.x - a.x) * t, y: a.y + (b.y - a.y) * t };
}

function isInsideCircle(p: Vec2M, R: number): boolean {
  return p.x * p.x + p.y * p.y <= R * R;
}

/**
 * Clipa uma polilinha contra o círculo de raio `R` centrado na origem.
 * Retorna 0..N sub-polilinhas (cada uma com >= 2 pontos), todas dentro
 * do círculo.
 *
 * Sem suavização de bordas — onde a polilinha sai do círculo, ela é
 * cortada exatamente na interseção. O perito pode arrastar os endpoints
 * depois para refinar.
 */
export function clipPolylineToCircle(
  points: ReadonlyArray<Vec2M>,
  R: number,
): Vec2M[][] {
  if (points.length < 2 || R <= 0) return [];

  const result: Vec2M[][] = [];
  let current: Vec2M[] = [];

  const flush = () => {
    if (current.length >= 2) result.push(current);
    current = [];
  };

  for (let i = 0; i < points.length - 1; i++) {
    const a = points[i] as Vec2M;
    const b = points[i + 1] as Vec2M;
    const aIn = isInsideCircle(a, R);
    const bIn = isInsideCircle(b, R);

    if (aIn && bIn) {
      // Ambos dentro — emite p1, e p2 vai como p1 da próxima iter.
      if (current.length === 0) current.push(a);
      current.push(b);
    } else if (aIn && !bIn) {
      // Sai do círculo no meio do segmento.
      const ts = segmentCircleIntersections(a, b, R);
      if (current.length === 0) current.push(a);
      if (ts.length >= 1) {
        current.push(lerpVec(a, b, ts[0]!));
      }
      flush();
    } else if (!aIn && bIn) {
      // Entra no círculo no meio.
      flush();
      const ts = segmentCircleIntersections(a, b, R);
      if (ts.length >= 1) {
        current.push(lerpVec(a, b, ts[0]!));
      }
      current.push(b);
    } else {
      // Ambos fora — pode haver chord (2 interseções) atravessando o
      // círculo. Raro pra um único segmento mas existe.
      const ts = segmentCircleIntersections(a, b, R);
      if (ts.length === 2) {
        flush();
        current.push(lerpVec(a, b, ts[0]!));
        current.push(lerpVec(a, b, ts[1]!));
        flush();
      }
    }
  }
  flush();
  return result;
}

function clamp(v: number, lo: number, hi: number): number {
  if (v < lo) return lo;
  if (v > hi) return hi;
  return v;
}

/**
 * Converte um OsmDataset em `SicroRoadObject_parity` + `SicroRoundaboutObject_parity`.
 *
 * Etapas:
 *   1. Index de nodes por id.
 *   2. Filtra ways com tag `highway` veicular (skip footway/path/etc).
 *   3. Projeta cada way para metros locais centrados no sinistro.
 *   4. Detecta rotatória (tag OR geometria circular).
 *   5. Simplifica polyline com Douglas-Peucker (preserva rings).
 *   6. Calcula bbox métrico + escala uniforme para o canvas (px/m).
 *   7. Para cada way regular: Hermite→Bezier 4-point.
 *      Para cada rotatória: centro + raio + largura_m.
 *   8. Constrói SicroRoadObject_parity / SicroRoundaboutObject_parity.
 *   9. Acumula warnings + stats.
 *
 * NÃO faz clip por raio (causou a regressão G.3 — endpoints clipados
 * não casam com node_ids, fragmentando topologia).
 */
export function convertOsmDatasetToParityObjects(
  input: OsmParityImportInput,
): OsmParityAdapterResult {
  const options = resolveOptions(input.options);
  const warnings: string[] = [];

  // 1. Index.
  const nodeIndex = new Map<number, OsmNode>();
  for (const n of input.nodes) nodeIndex.set(n.id, n);

  // 2-5. Filtra + projeta + simplifica + detecta rotatória.
  type WayMetric = {
    way: OsmWay;
    nodeRefs: number[];
    metricPoints: Vec2M[];
    isRoundabout: boolean;
  };
  const wayMetrics: WayMetric[] = [];
  let skipped = 0;

  for (const w of input.ways) {
    if (!w.tags || !w.tags.highway) {
      skipped++;
      continue;
    }
    if (options.ignore_non_vehicle && isNonVehicleHighway(w.tags.highway)) {
      skipped++;
      continue;
    }

    // Projeta nodes para metros locais.
    const raw: Vec2M[] = [];
    const validRefs: number[] = [];
    for (const ref of w.node_refs) {
      const n = nodeIndex.get(ref);
      if (!n) continue;
      raw.push(
        projectLatLonToLocalMeters(
          n.lat,
          n.lon,
          input.center.lat,
          input.center.lon,
        ),
      );
      validRefs.push(ref);
    }
    if (raw.length < 2) {
      skipped++;
      warnings.push(
        `Way ${w.id} ignorada: menos de 2 nodes válidos após filtragem.`,
      );
      continue;
    }

    // Detecta rotatória ANTES de clipar/simplificar (depende do ring completo).
    const isRoundabout =
      options.preserve_roundabouts && isOsmRoundaboutForParity(w, raw);
    const isRing =
      validRefs.length >= 5 &&
      validRefs[0] === validRefs[validRefs.length - 1];

    // Fase S round 2 — Hard cap por raio. Rotatórias NÃO são clipadas
    // (precisa do ring inteiro pra virar centro+raio). Vias regulares
    // viram 0..N sub-vias após o clip — cada pedaço dentro do círculo
    // vira sua própria way métrica.
    let segmentsToProcess: Vec2M[][];
    if (options.clip_to_radius && !isRoundabout) {
      segmentsToProcess = clipPolylineToCircle(raw, input.radius_m);
      if (segmentsToProcess.length === 0) {
        skipped++;
        // Não emite warning — way fora do raio é o caso esperado.
        continue;
      }
    } else if (isRoundabout) {
      // Rotatória: descarta se o centro está fora do raio.
      let sumX = 0;
      let sumY = 0;
      const head = isRing ? raw.slice(0, -1) : raw;
      for (const p of head) {
        sumX += p.x;
        sumY += p.y;
      }
      const cx = sumX / head.length;
      const cy = sumY / head.length;
      if (cx * cx + cy * cy > input.radius_m * input.radius_m) {
        skipped++;
        continue;
      }
      segmentsToProcess = [raw];
    } else {
      segmentsToProcess = [raw];
    }

    for (const seg of segmentsToProcess) {
      // Comprimento total (m).
      let totalLen = 0;
      for (let i = 1; i < seg.length; i++) {
        const a = seg[i - 1] as Vec2M;
        const b = seg[i] as Vec2M;
        totalLen += Math.hypot(b.x - a.x, b.y - a.y);
      }
      if (totalLen < options.min_way_length_m) {
        skipped++;
        warnings.push(
          `Way ${w.id} ignorada: comprimento ${totalLen.toFixed(1)} m < mínimo ${options.min_way_length_m} m.`,
        );
        continue;
      }

      // Simplifica — preserva ring intacto para rotatórias.
      let simplified: Vec2M[];
      if (isRoundabout && isRing) {
        const head = seg.slice(0, -1);
        const simp = simplifyPolylineDP(head, options.simplify_tolerance_m);
        simplified = [...simp, simp[0] as Vec2M];
      } else {
        simplified = simplifyPolylineDP(seg, options.simplify_tolerance_m);
      }

      if (simplified.length < 2) {
        skipped++;
        warnings.push(
          `Way ${w.id} ignorada: geometria insuficiente após simplificação.`,
        );
        continue;
      }

      wayMetrics.push({
        way: w,
        nodeRefs: validRefs,
        metricPoints: simplified,
        isRoundabout,
      });
    }
  }

  if (wayMetrics.length === 0) {
    return {
      roads: [],
      roundabouts: [],
      warnings,
      stats: {
        node_count: input.nodes.length,
        way_count: input.ways.length,
        imported_road_count: 0,
        imported_roundabout_count: 0,
        skipped_count: skipped,
        px_per_m: 1,
        metric_bbox: { min_x: 0, max_x: 0, min_y: 0, max_y: 0 },
      },
    };
  }

  // 6. Bbox métrico + escala uniforme.
  //
  // ESCALA FIXA — independente do raio escolhido. Antes a escala era
  // calculada como `canvas / (radius_m * 2)`, então raios grandes
  // (200 m) faziam ruas finíssimas e raios pequenos (25 m) faziam
  // ruas grossas. Visualmente a "qualidade" das vias dependia da
  // área importada — não fazia sentido.
  //
  // Agora usamos um RAIO DE REFERÊNCIA constante (25 m) pra calcular
  // o `px_per_m`. Resultado: largura de rua igual em qualquer
  // importação. Se o usuário pediu 200 m, o conteúdo simplesmente
  // ocupa um quadrado lógico de ~5760 px × 5760 px e ele navega
  // dando zoom out (faixa atual: 5 %–10000 %, dá conta de qualquer
  // tamanho de bairro).
  //
  // Quando não há clip, mantém o comportamento antigo (fit do bbox
  // das vias) — usado quando a importação não vem com raio definido.
  const REFERENCE_RADIUS_M = 25;
  let minX = Infinity;
  let minY = Infinity;
  let maxX = -Infinity;
  let maxY = -Infinity;
  for (const m of wayMetrics) {
    for (const p of m.metricPoints) {
      if (p.x < minX) minX = p.x;
      if (p.x > maxX) maxX = p.x;
      if (p.y < minY) minY = p.y;
      if (p.y > maxY) maxY = p.y;
    }
  }
  const usableW = input.canvas.width * (1 - 2 * options.margin);
  const usableH = input.canvas.height * (1 - 2 * options.margin);
  let metricW: number;
  let metricH: number;
  let bboxCxM: number;
  let bboxCyM: number;
  if (options.clip_to_radius && input.radius_m > 0) {
    // Usa o RAIO DE REFERÊNCIA FIXO (25 m) pra escala, NÃO o raio
    // escolhido pelo perito. Isso garante px_per_m idêntico em todas
    // as importações — uma rua de 7 m sempre tem a mesma largura
    // visual, seja em raio 25, 100 ou 200 m. Áreas maiores que o
    // raio de referência simplesmente extrapolam o canvas (overflow
    // gerenciado via pan/zoom).
    metricW = REFERENCE_RADIUS_M * 2;
    metricH = REFERENCE_RADIUS_M * 2;
    bboxCxM = 0;
    bboxCyM = 0;
  } else {
    metricW = Math.max(maxX - minX, 1);
    metricH = Math.max(maxY - minY, 1);
    bboxCxM = (minX + maxX) / 2;
    bboxCyM = (minY + maxY) / 2;
  }
  const scale = Math.min(usableW / metricW, usableH / metricH);

  // Em coords parity (mundo, metros), os objetos ficam centralizados
  // no centro do canvas; o renderer translada conforme pxPerM. Mesma
  // lógica que estava antes — só muda o quê estamos centralizando
  // (círculo do raio vs bbox das vias).
  const targetCxM = (input.canvas.width / 2) / Math.max(scale, 0.0001);
  const targetCyM = (input.canvas.height / 2) / Math.max(scale, 0.0001);
  const recentre = (p: Vec2M): Vec2M => ({
    x: p.x - bboxCxM + targetCxM,
    y: p.y - bboxCyM + targetCyM,
  });

  // 7-8. Constrói objetos parity.
  const roads: SicroRoadObject_parity[] = [];
  const roundabouts: SicroRoundaboutObject_parity[] = [];

  for (const m of wayMetrics) {
    if (m.isRoundabout) {
      const rb = buildParityRoundaboutFromOsm(m, recentre);
      if (rb) {
        roundabouts.push(rb);
      } else {
        skipped++;
        warnings.push(
          `Way ${m.way.id} (junction=roundabout) ignorada: geometria irregular demais.`,
        );
      }
      continue;
    }

    const road = buildParityRoadFromOsm(m, recentre);
    if (road) {
      roads.push(road);
    } else {
      skipped++;
      warnings.push(
        `Way ${m.way.id} ignorada: fit Bezier degenerado.`,
      );
    }
  }

  return {
    roads,
    roundabouts,
    warnings,
    stats: {
      node_count: input.nodes.length,
      way_count: input.ways.length,
      imported_road_count: roads.length,
      imported_roundabout_count: roundabouts.length,
      skipped_count: skipped,
      px_per_m: scale,
      metric_bbox: { min_x: minX, max_x: maxX, min_y: minY, max_y: maxY },
    },
  };
}

// ---------------------------------------------------------------------------
// Builders por tipo.

function buildParityRoadFromOsm(
  m: { way: OsmWay; nodeRefs: number[]; metricPoints: Vec2M[] },
  recentre: (p: Vec2M) => Vec2M,
): SicroRoadObject_parity | null {
  const fit = polylineToParityBezier(m.metricPoints);
  if (!fit) return null;
  const direction = osmOnewayToDirection(m.way.tags);
  const is_one_way = direction === "one_way";
  const highway = m.way.tags.highway;
  const largura_m = parityRoadWidthMetersByHighway(highway);
  const marcacao = parityRoadMarkingByHighway(highway, is_one_way);
  const label = pickLabel(m.way.tags);

  // Aplica recentre para que os objetos fiquem no centro do canvas.
  const start = recentre(fit.start);
  const c1 = recentre(fit.c1);
  const c2 = recentre(fit.c2);
  const end = recentre(fit.end);

  // Largura para divided carriageway (`oneway=yes` em par de ways):
  // SICRO 1.0 Python divide largura ao meio para que ambos os lados
  // somados reconstruam a arterial original. Mantemos paridade.
  const largura_final = is_one_way ? largura_m / 2 : largura_m;

  return makeParityRoadBezier(
    start.x,
    start.y,
    c1.x,
    c1.y,
    c2.x,
    c2.y,
    end.x,
    end.y,
    {
      largura_m: largura_final,
      superficie: "asfalto",
      mao_dupla: !is_one_way,
      marcacao,
      label,
      metadata_json: buildMetadataJson(m.way, {
        arc_length_m: fit.arcLengthM,
        lanes_hint: osmLanesHint(m.way.tags),
      }),
    },
  );
}

function buildParityRoundaboutFromOsm(
  m: { way: OsmWay; nodeRefs: number[]; metricPoints: Vec2M[] },
  recentre: (p: Vec2M) => Vec2M,
): SicroRoundaboutObject_parity | null {
  // Drop o último ponto se for duplicata do primeiro (ring fechado).
  const pts =
    m.metricPoints.length > 4 &&
    m.metricPoints[0]!.x === m.metricPoints[m.metricPoints.length - 1]!.x &&
    m.metricPoints[0]!.y === m.metricPoints[m.metricPoints.length - 1]!.y
      ? m.metricPoints.slice(0, -1)
      : m.metricPoints;
  if (pts.length < 4) return null;

  // Centroide + raio médio (em metros).
  let cx = 0;
  let cy = 0;
  for (const p of pts) {
    cx += p.x;
    cy += p.y;
  }
  cx /= pts.length;
  cy /= pts.length;
  const radii = pts.map((p) => Math.hypot(p.x - cx, p.y - cy));
  const meanR = radii.reduce((acc, r) => acc + r, 0) / radii.length;
  if (meanR < 2) return null;

  // Aplica recentre ao centro.
  const center = recentre({ x: cx, y: cy });

  // Largura do anel — paridade Python: ~ 40% do raio, mas no mínimo
  // 4 m e no máximo 9 m. Garante ilha visível em todos os tamanhos.
  const largura_m = Math.min(9, Math.max(4, meanR * 0.4));

  return makeParityRoundabout(center.x, center.y, meanR, {
    largura_m,
    superficie: "asfalto",
    // inner_color omitido → renderer aplica `#3A6535` (verde canteiro
    // Python padrão).
    label: pickLabel(m.way.tags) ?? `OSM rotatória ${m.way.id}`,
    metadata_json: buildMetadataJson(m.way, {
      r_m: meanR,
      node_refs: m.nodeRefs,
    }),
  });
}
