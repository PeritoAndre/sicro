/**
 * OSM → objetos parity. Projeta nodes/ways para metros locais, classifica por
 * `highway`, ajusta Bezier de 4 pontos e gera via/rotatória em coordenadas de
 * mundo (metros). Funções puras — sem React, Konva ou fetch.
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

interface Vec2M {
  x: number;
  y: number;
}

// ---- Tabelas por classe OSM ----

/** Largura em metros por `highway=*` (tabela `_LARG_CLASSE` do SICRO 1.0). `*_link` herda a classe. */
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
  return 6.5;
}

// `track` entra porque "estrada de chão" no OSM costuma ser trilha rural, fora do croqui urbano.
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

/** Arteriais mão dupla: amarela; residential/service: branca (convenção urbana brasileira); mão única: nenhuma. */
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

// ---- Tipos de entrada/saída ----

interface OsmParityImportInput {
  ways: OsmWay[];
  nodes: OsmNode[];
  center: { lat: number; lon: number };
  radius_m: number;
  canvas: { width: number; height: number };
  options?: OsmParityImportOptions;
}

interface OsmParityImportOptions {
  /** Fração do canvas reservada como margem em cada lado. Default 0.1. */
  margin?: number;
  /** Tolerância Douglas-Peucker em metros. Default 0.6. */
  simplify_tolerance_m?: number;
  /** Comprimento mínimo (m) para importar a way. Default 4. */
  min_way_length_m?: number;
  /** Detectar rotatória por tag ou geometria circular. Default true. */
  preserve_roundabouts?: boolean;
  /** Ignorar footway/path/cycleway etc. Default true. */
  ignore_non_vehicle?: boolean;
  /**
   * Clipa cada way ao círculo de `radius_m`; way que entra e sai várias vezes
   * vira várias sub-vias. Rotatórias não são clipadas (precisam do ring inteiro);
   * se o centro cai fora do raio, a rotatória é descartada. Default true.
   */
  clip_to_radius?: boolean;
}

interface OsmParityImportStats {
  node_count: number;
  way_count: number;
  imported_road_count: number;
  imported_roundabout_count: number;
  skipped_count: number;
  /** Escala em px/m sugerida pelo fit. */
  px_per_m: number;
  /** Bbox em metros locais, origem no centro do sinistro. */
  metric_bbox: { min_x: number; max_x: number; min_y: number; max_y: number };
}

interface OsmParityAdapterResult {
  roads: SicroRoadObject_parity[];
  roundabouts: SicroRoundaboutObject_parity[];
  warnings: string[];
  stats: OsmParityImportStats;
}

// ---- Projeção lat/lon → metros locais ----

const EARTH_R = 6_371_000; // m
const DEG2RAD = Math.PI / 180;

/** X = leste, Y = sul (canvas Y-down). Sem Mercator: erro < 0,1 % em raios urbanos. */
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

// ---- Polilinha → Bezier de 4 pontos ----

interface ParityBezierFit {
  start: Vec2M;
  end: Vec2M;
  c1: Vec2M;
  c2: Vec2M;
  /** Comprimento linear da polilinha em metros. */
  arcLengthM: number;
}

/**
 * Tangentes de Hermite: c1 = start + tangente_inicial·(arc/3), c2 = end − tangente_final·(arc/3).
 * `null` se < 2 pontos ou arco < 5 cm.
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

// ---- Detecção de rotatória ----

/**
 * Rotatória se `junction=roundabout|circular` OU ring fechado quase circular
 * (desvio-padrão do raio < 30 % da média — pega rotatórias sem tag).
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

// ---- Label / metadata ----

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

// ---- Options ----

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

// ---- Clip por círculo ----

/** Parâmetros `t ∈ (0, 1)` onde o segmento `a→b` cruza o círculo de raio `R` na origem (0, 1 ou 2). */
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

/** Corta a polilinha no círculo de raio `R` (origem); devolve 0..N sub-polilinhas com ≥ 2 pontos. */
function clipPolylineToCircle(
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
      // Ambos fora, mas o segmento pode atravessar o círculo (corda).
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

/** Converte o dataset OSM em vias e rotatórias parity (mundo, metros) + stats e warnings. */
export function convertOsmDatasetToParityObjects(
  input: OsmParityImportInput,
): OsmParityAdapterResult {
  const options = resolveOptions(input.options);
  const warnings: string[] = [];

  const nodeIndex = new Map<number, OsmNode>();
  for (const n of input.nodes) nodeIndex.set(n.id, n);

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

    // Via regular clipada vira 0..N sub-vias; rotatória não é clipada.
    let segmentsToProcess: Vec2M[][];
    if (options.clip_to_radius && !isRoundabout) {
      segmentsToProcess = clipPolylineToCircle(raw, input.radius_m);
      if (segmentsToProcess.length === 0) {
        skipped++;
        // Sem warning — way fora do raio é o caso esperado.
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

  // Escala FIXA por raio de referência (25 m), não pelo raio escolhido: assim uma
  // rua de 7 m tem a mesma largura visual em qualquer importação; área maior
  // só extrapola o canvas e o perito navega com zoom. Sem clip, fit do bbox.
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

  // Centraliza os objetos (em metros) no centro do canvas.
  const targetCxM = (input.canvas.width / 2) / Math.max(scale, 0.0001);
  const targetCyM = (input.canvas.height / 2) / Math.max(scale, 0.0001);
  const recentre = (p: Vec2M): Vec2M => ({
    x: p.x - bboxCxM + targetCxM,
    y: p.y - bboxCyM + targetCyM,
  });

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

// ---- Builders ----

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

  const start = recentre(fit.start);
  const c1 = recentre(fit.c1);
  const c2 = recentre(fit.c2);
  const end = recentre(fit.end);

  // Mão única no OSM costuma ser um par de ways (pista dupla): metade da largura
  // em cada uma reconstrói a arterial original (mesma regra do SICRO 1.0).
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

  const center = recentre({ x: cx, y: cy });

  // Anel ≈ 40 % do raio, entre 4 e 9 m (SICRO 1.0) — garante ilha visível.
  const largura_m = Math.min(9, Math.max(4, meanR * 0.4));

  return makeParityRoundabout(center.x, center.y, meanR, {
    largura_m,
    superficie: "asfalto",
    label: pickLabel(m.way.tags) ?? `OSM rotatória ${m.way.id}`,
    metadata_json: buildMetadataJson(m.way, {
      r_m: meanR,
      node_refs: m.nodeRefs,
    }),
  });
}
