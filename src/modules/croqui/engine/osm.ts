/**
 * OpenStreetMap: tipos compartilhados, hints de tags, Douglas-Peucker e o
 * consulta ao Overpass (via Rust) com cache em memória.
 */

import { commands } from "@core/commands";

type OsmDirection = "one_way" | "two_way" | "unknown";

export interface OsmNode {
  id: number;
  lat: number;
  lon: number;
}

export interface OsmWay {
  id: number;
  node_refs: number[];
  tags: Record<string, string>;
}

export interface OsmDataset {
  nodes: OsmNode[];
  ways: OsmWay[];
  /** true quando veio do cache em memória (a UI mostra "carregado do cache"). */
  from_cache?: boolean;
}

/** Bbox geográfica + tamanho do canvas-alvo; mapeamento linear, bom o bastante em escala de quarteirão. */
export interface OsmViewport {
  min_lat: number;
  max_lat: number;
  min_lon: number;
  max_lon: number;
  width_px: number;
  height_px: number;
}

/** Valor da tag `lanes=*` quando presente e parseável; senão null. */
export function osmLanesHint(
  tags: Record<string, string>,
): number | null {
  const raw = tags.lanes;
  if (!raw) return null;
  const n = Number.parseInt(raw, 10);
  return Number.isFinite(n) && n > 0 ? n : null;
}

/**
 * Tag `oneway=*`: yes/true/1/-1/reverse → one_way; no/false/0 → two_way; ausente → unknown.
 * O sentido reverso (`-1`) ainda conta como mão única — o sinal é ignorado aqui.
 */
export function osmOnewayToDirection(
  tags: Record<string, string>,
): OsmDirection {
  const raw = tags.oneway;
  if (raw == null) return "unknown";
  const v = String(raw).trim().toLowerCase();
  if (v === "yes" || v === "true" || v === "1" || v === "-1" || v === "reverse") {
    return "one_way";
  }
  if (v === "no" || v === "false" || v === "0") return "two_way";
  return "unknown";
}

// ---- Douglas-Peucker ----
// Iterativo (stack) para não estourar a pilha em ways gigantes. Preserva os
// endpoints exatos — essencial para o ring fechado das rotatórias.

interface Vec2Like {
  x: number;
  y: number;
}

/** Distância perpendicular à reta `a→b` (não ao segmento); se `a == b`, distância ao ponto. */
function perpendicularDistance(p: Vec2Like, a: Vec2Like, b: Vec2Like): number {
  const dx = b.x - a.x;
  const dy = b.y - a.y;
  const lenSq = dx * dx + dy * dy;
  if (lenSq < 1e-12) {
    const px = p.x - a.x;
    const py = p.y - a.y;
    return Math.hypot(px, py);
  }
  const num = Math.abs(dy * p.x - dx * p.y + b.x * a.y - b.y * a.x);
  return num / Math.sqrt(lenSq);
}

/** Simplifica a polilinha; `epsilon <= 0` ou < 3 pontos devolve cópia rasa intacta. */
export function simplifyPolylineDP<T extends Vec2Like>(
  points: ReadonlyArray<T>,
  epsilon: number,
): T[] {
  if (points.length < 3 || epsilon <= 0) {
    return points.slice();
  }

  const keep = new Uint8Array(points.length);
  keep[0] = 1;
  keep[points.length - 1] = 1;

  const stack: Array<[number, number]> = [[0, points.length - 1]];

  while (stack.length > 0) {
    const top = stack.pop();
    if (!top) break;
    const [lo, hi] = top;
    if (hi - lo < 2) continue;
    const a = points[lo] as T;
    const b = points[hi] as T;
    let maxDist = -1;
    let maxIdx = -1;
    for (let i = lo + 1; i < hi; i++) {
      const d = perpendicularDistance(points[i] as T, a, b);
      if (d > maxDist) {
        maxDist = d;
        maxIdx = i;
      }
    }
    if (maxIdx >= 0 && maxDist > epsilon) {
      keep[maxIdx] = 1;
      stack.push([lo, maxIdx]);
      stack.push([maxIdx, hi]);
    }
  }

  const out: T[] = [];
  for (let i = 0; i < points.length; i++) {
    if (keep[i]) out.push(points[i] as T);
  }
  return out;
}

// ---- Overpass ----
// Só a bbox geográfica sai do app — nenhum dado pericial é enviado.

const overpassCache = new Map<string, OsmDataset>();

function bboxCacheKey(bbox: {
  min_lat: number;
  max_lat: number;
  min_lon: number;
  max_lon: number;
}): string {
  // 5 casas decimais (~1 m) para micro-variações da UI não perderem o cache.
  const r = (v: number) => v.toFixed(5);
  return `${r(bbox.min_lat)},${r(bbox.min_lon)},${r(bbox.max_lat)},${r(bbox.max_lon)}`;
}

/** Overpass QL: todas as ways `highway=*` na bbox + nodes referenciados, em JSON. */
function buildOverpassQuery(bbox: {
  min_lat: number;
  max_lat: number;
  min_lon: number;
  max_lon: number;
}): string {
  const south = bbox.min_lat;
  const west = bbox.min_lon;
  const north = bbox.max_lat;
  const east = bbox.max_lon;
  return `[out:json][timeout:25];
(
  way["highway"](${south},${west},${north},${east});
);
out body;
>;
out skel qt;`;
}

/**
 * Busca nodes + ways na bbox via Overpass, com cache em memória por bbox.
 * Erros de rede/HTTP/parse viram `Error` com mensagem para o usuário.
 */
export async function fetchOverpassBBox(bbox: {
  min_lat: number;
  max_lat: number;
  min_lon: number;
  max_lon: number;
}): Promise<OsmDataset> {
  const key = bboxCacheKey(bbox);
  const cached = overpassCache.get(key);
  if (cached) {
    return { ...cached, from_cache: true };
  }

  const query = buildOverpassQuery(bbox);
  let text: string;
  try {
    text = await commands.fetchOverpass(query);
  } catch (e) {
    throw new Error((e as Error).message);
  }

  let json: {
    elements?: Array<{
      type?: string;
      id?: number;
      lat?: number;
      lon?: number;
      nodes?: number[];
      tags?: Record<string, string>;
    }>;
  };
  try {
    json = JSON.parse(text);
  } catch (e) {
    throw new Error(`Resposta inválida do Overpass: ${(e as Error).message}`);
  }

  const elements = Array.isArray(json.elements) ? json.elements : [];
  const nodes: OsmNode[] = [];
  const ways: OsmWay[] = [];
  for (const el of elements) {
    if (!el || typeof el.id !== "number") continue;
    if (el.type === "node" && typeof el.lat === "number" && typeof el.lon === "number") {
      nodes.push({ id: el.id, lat: el.lat, lon: el.lon });
    } else if (
      el.type === "way" &&
      Array.isArray(el.nodes) &&
      el.nodes.length >= 2
    ) {
      ways.push({
        id: el.id,
        node_refs: el.nodes.filter((n): n is number => typeof n === "number"),
        tags: el.tags ?? {},
      });
    }
  }

  const dataset: OsmDataset = { nodes, ways, from_cache: false };
  overpassCache.set(key, dataset);
  return dataset;
}

/** Força nova consulta mesmo com a mesma bbox (botão "Recarregar"). */
export function clearOverpassCache(): void {
  overpassCache.clear();
}
