/**
 * OpenStreetMap utilities — types + projeções compartilhadas pela
 * importação OSM.
 *
 * Fase S — Removidas as funções v1 `osmWayToRoad` e `osmDatasetToRoads`
 * (geravam `SicroRoadObject` do engine v1). A conversão OSM → road
 * agora vive 100% em `road-parity/osmAdapter.ts` (Python Parity Engine).
 *
 * Também removido: `osmTagToRoadStyle` (dependia do `RoadStyle` v1,
 * substituído por `parityRoadWidthMetersByHighway` no parity adapter).
 *
 * Este módulo retém apenas o que é **fonte-agnóstica de motor**:
 *   - Tipos compartilhados (OsmNode, OsmWay, OsmDataset, OsmViewport)
 *   - `osmLanesHint` — hint cru de lanes
 *   - `osmOnewayToDirection` — classificação one_way/two_way/unknown
 *     (literal type, sem depender do schema)
 *   - Simplificação Douglas-Peucker (simplifyPolylineDP) — usada pelo
 *     adapter parity para reduzir nodes redundantes
 *   - Projeção lon/lat → canvas (projectLonLat, projectWay)
 *   - Stubs de fetchOverpassBBox / clearOverpassCache (a implementação
 *     real vive no `OsmImportModal`)
 *
 * Tudo aqui é puro — fácil de testar, sem DOM, sem Tauri, sem fetch real.
 */

import type { SicroPoint } from "./schema";

/**
 * Classificação de direção lida da tag `oneway=*` do OSM. Literal type
 * local — antes esse era o `RoadDirection` do schema v1, mas o parity
 * adapter só precisa distinguir mão única de mão dupla.
 */
export type OsmDirection = "one_way" | "two_way" | "unknown";

/** A single OSM node — geographic point with stable id. */
export interface OsmNode {
  id: number;
  lat: number;
  lon: number;
}

/** A single OSM way — ordered list of node ids + tag bag. */
export interface OsmWay {
  id: number;
  node_refs: number[];
  tags: Record<string, string>;
}

/**
 * OSM dataset retornado pelo Overpass — bolha de nodes + ways.
 *
 * `from_cache` indica se o dataset veio do cache em memória do modal
 * (sem hit no Overpass). UI usa esse flag para mostrar "carregado do cache"
 * em vez de "baixado".
 */
export interface OsmDataset {
  nodes: OsmNode[];
  ways: OsmWay[];
  from_cache?: boolean;
}

/**
 * Bounding box used to project lon/lat onto canvas coordinates. The box
 * is *inclusive* and assumes a square mercator-ish mapping that's good
 * enough for forensic croquis at city scale (sub-km error tolerated).
 */
export interface OsmViewport {
  min_lat: number;
  max_lat: number;
  min_lon: number;
  max_lon: number;
  /** Canvas width/height in pixels we're projecting into. */
  width_px: number;
  height_px: number;
}

/**
 * Number of lanes hinted by the OSM `lanes=*` tag (when present and
 * parseable). Falls back to the road style's default by returning null.
 */
export function osmLanesHint(
  tags: Record<string, string>,
): number | null {
  const raw = tags.lanes;
  if (!raw) return null;
  const n = Number.parseInt(raw, 10);
  return Number.isFinite(n) && n > 0 ? n : null;
}

/**
 * Mapeia a tag `oneway=*` para um valor `OsmDirection`.
 *
 * Tabela OSM (https://wiki.openstreetmap.org/wiki/Key:oneway):
 *   - `yes`, `true`, `1`     → one_way
 *   - `-1`, `reverse`        → one_way (reverso — direction ignorada aqui)
 *   - `no`, `false`, `0`     → two_way
 *   - ausente                → unknown (assume bidirecional por padrão na UI)
 *
 * O sinal de reverso (`-1`) ainda conta como mão única — quem decidir
 * inverter o trace é o consumer (o parity adapter ignora o sinal).
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

/**
 * Project a lon/lat point onto canvas pixel coordinates inside the
 * given viewport. Pure linear interpolation — no projection distortion
 * correction, which is fine at city-block scale.
 */
export function projectLonLat(
  lat: number,
  lon: number,
  view: OsmViewport,
): SicroPoint {
  const dx = (lon - view.min_lon) / (view.max_lon - view.min_lon);
  // y inverted: top of canvas = max_lat (north).
  const dy = (view.max_lat - lat) / (view.max_lat - view.min_lat);
  return {
    x: dx * view.width_px,
    y: dy * view.height_px,
  };
}

/**
 * Project an entire OSM way's geometry onto canvas coordinates, given
 * the lookup table of nodes. Skips any node_ref that isn't present in
 * the index (defensive — OSM dumps are sometimes truncated).
 */
export function projectWay(
  way: OsmWay,
  nodes: Map<number, OsmNode>,
  view: OsmViewport,
): number[] {
  const out: number[] = [];
  for (const ref of way.node_refs) {
    const n = nodes.get(ref);
    if (!n) continue;
    const p = projectLonLat(n.lat, n.lon, view);
    out.push(p.x, p.y);
  }
  return out;
}

// ---------------------------------------------------------------------------
// Douglas-Peucker — simplificação de polyline genérica.
//
// O algoritmo recursivo encontra o ponto mais distante do segmento entre
// os endpoints; se a distância > epsilon, divide e recurse; senão, descarta
// todos os intermediários. Preserva endpoints exatos — essencial para que
// o parity adapter mantenha ring fechado de rotatórias.
//
// Implementação iterativa via stack para evitar stack-overflow em ways
// gigantes (raro em OSM mas defensivo).

/** Ponto 2D mínimo — compatível com Vec2M do parity adapter. */
interface Vec2Like {
  x: number;
  y: number;
}

/**
 * Distância perpendicular do ponto `p` ao segmento `a→b`. Se `a == b`,
 * cai pra distância euclidiana ao ponto.
 */
function perpendicularDistance(p: Vec2Like, a: Vec2Like, b: Vec2Like): number {
  const dx = b.x - a.x;
  const dy = b.y - a.y;
  const lenSq = dx * dx + dy * dy;
  if (lenSq < 1e-12) {
    const px = p.x - a.x;
    const py = p.y - a.y;
    return Math.hypot(px, py);
  }
  // Projeção escalar sobre o segmento, clampada a [0, 1] não é necessária
  // aqui — queremos a distância perpendicular à reta, não ao segmento.
  const num = Math.abs(dy * p.x - dx * p.y + b.x * a.y - b.y * a.x);
  return num / Math.sqrt(lenSq);
}

/**
 * Simplifica uma polilinha pelo algoritmo de Douglas-Peucker.
 *
 * - `epsilon` < 0 ou polyline com < 3 pontos → retorna cópia rasa intacta.
 * - Preserva primeiro e último ponto sempre.
 * - Genérico sobre `T extends Vec2Like` para que o adapter possa passar
 *   seus Vec2M sem coerções.
 */
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

  // Stack de pares (lo, hi) — usa Int32Array seria mais rápido, mas
  // number[] é suficiente pra ways OSM (geralmente < 1000 nodes).
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

// ---------------------------------------------------------------------------
// Overpass fetch — implementação real (HTTP + cache em memória + timeout).
//
// A consulta usa o endpoint público Overpass-API (overpass-api.de). O modal
// passa uma bbox geográfica e a função:
//   1. Verifica cache em memória keyed pela bbox arredondada (5 casas
//      decimais — ~1 m de granularidade). Cache hit ⇒ retorna `from_cache: true`.
//   2. Cache miss ⇒ POSTa a query Overpass QL no endpoint, com timeout de 25 s.
//   3. Parseia o JSON e separa nodes (geometria) das ways (com tags).
//   4. Armazena no cache antes de retornar.
//
// Privacidade: apenas a bbox geográfica é enviada — nenhum dado pericial
// vaza do app.

/** Endpoint default Overpass-API público. */
const OVERPASS_ENDPOINT = "https://overpass-api.de/api/interpreter";

/** Timeout do POST (ms). Overpass costuma responder em < 5 s pra bboxes pequenas. */
const OVERPASS_TIMEOUT_MS = 25_000;

/** Cache em memória — key = bbox quantizada; valor = dataset. */
const overpassCache = new Map<string, OsmDataset>();

function bboxCacheKey(bbox: {
  min_lat: number;
  max_lat: number;
  min_lon: number;
  max_lon: number;
}): string {
  // Arredonda a 5 casas decimais (~1 m) para que micro-variações de UI
  // não percam o cache.
  const r = (v: number) => v.toFixed(5);
  return `${r(bbox.min_lat)},${r(bbox.min_lon)},${r(bbox.max_lat)},${r(bbox.max_lon)}`;
}

/**
 * Monta a query Overpass QL para buscar todas as ways `highway=*` dentro
 * da bbox + os nodes referenciados. Saída em JSON.
 *
 * Sintaxe:
 *   - `[out:json][timeout:25];` — output JSON, timeout 25 s.
 *   - `(way["highway"](bbox);)` — todas as ways `highway` dentro da bbox.
 *   - `out body; >; out skel qt;` — emite way + tags, depois os nodes
 *     referenciados, e finalmente as coordenadas dos nodes em formato
 *     compacto.
 */
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
 * Busca o dataset OSM (nodes + ways) dentro da bbox via Overpass-API.
 *
 * Cache em memória (chave = bbox arredondada). Cache hit retorna
 * `from_cache: true`. Limpa via `clearOverpassCache()`.
 *
 * Erros HTTP / parse / timeout viram `Error` com mensagem humana — o
 * modal mostra essa mensagem na faixa de erro.
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
  const ctrl = new AbortController();
  const timer = setTimeout(() => ctrl.abort(), OVERPASS_TIMEOUT_MS);

  let resp: Response;
  try {
    resp = await fetch(OVERPASS_ENDPOINT, {
      method: "POST",
      headers: {
        "Content-Type": "application/x-www-form-urlencoded",
        Accept: "application/json",
      },
      body: `data=${encodeURIComponent(query)}`,
      signal: ctrl.signal,
    });
  } catch (e) {
    clearTimeout(timer);
    const msg = (e as Error).name === "AbortError"
      ? "Tempo esgotado consultando o Overpass (verifique a conexão)."
      : `Falha de rede ao consultar o Overpass: ${(e as Error).message}`;
    throw new Error(msg);
  }
  clearTimeout(timer);

  if (!resp.ok) {
    throw new Error(
      `Overpass respondeu com status ${resp.status}. Tente novamente em alguns segundos.`,
    );
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
    json = await resp.json();
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

/**
 * Limpa o cache em memória do Overpass. Útil pro botão "Recarregar" do
 * modal — força nova consulta mesmo se a bbox bater.
 */
export function clearOverpassCache(): void {
  overpassCache.clear();
}
