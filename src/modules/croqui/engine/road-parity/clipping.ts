/**
 * Clipping de marcações: corta polilinhas onde entram em polígonos de outras
 * vias/rotatórias (ponto-em-polígono por ray casting). Não usa `polygon-clipping`
 * porque precisamos da ORDEM dos trechos para o dash contínuo.
 * Regra do perito: em qualquer erro, devolve a marcação inteira — nunca lança.
 */

import type { Vec2World } from "./geometry";

/** Sub-polilinhas preservadas; cada uma com ≥ 2 pontos. */
type ClippedSegments = Vec2World[][];

interface ClipReport {
  segments_count: number;
  obstacles_count: number;
  /** true quando um erro foi capturado e a polilinha original foi devolvida. */
  fallback_used: boolean;
  fallback_reason?: string;
}

/** Trechos de `line` fora de todos os `obstacles`. Sem obstáculos devolve a linha intacta. */
export function clipPolylineAgainstPolygons(
  line: ReadonlyArray<Vec2World>,
  obstacles: ReadonlyArray<ReadonlyArray<Vec2World>>,
): { segments: ClippedSegments; report: ClipReport } {
  const report: ClipReport = {
    segments_count: 0,
    obstacles_count: obstacles.length,
    fallback_used: false,
  };

  if (line.length < 2) {
    return { segments: [], report };
  }
  if (obstacles.length === 0) {
    const segments = [line.slice()];
    report.segments_count = 1;
    return { segments, report };
  }

  try {
    // Densifica antes: um segmento longo com os dois extremos fora mas o meio
    // dentro do obstáculo passaria despercebido pelo teste por ponto.
    const densified = densifyPolyline(line, 1.0);
    const segments = doClip(densified, obstacles);
    report.segments_count = segments.length;
    return { segments, report };
  } catch (err) {
    report.fallback_used = true;
    report.fallback_reason =
      err instanceof Error ? err.message : String(err);
    report.segments_count = 1;
    return { segments: [line.slice()], report };
  }
}

/** Insere pontos até nenhum segmento exceder `maxSegLen` metros; `maxSegLen <= 0` não densifica. */
function densifyPolyline(
  line: ReadonlyArray<Vec2World>,
  maxSegLen: number,
): Vec2World[] {
  if (line.length < 2 || maxSegLen <= 0) return line.slice();
  const out: Vec2World[] = [line[0] as Vec2World];
  for (let i = 1; i < line.length; i++) {
    const prev = line[i - 1] as Vec2World;
    const cur = line[i] as Vec2World;
    const dx = cur.x - prev.x;
    const dy = cur.y - prev.y;
    const len = Math.hypot(dx, dy);
    if (len > maxSegLen) {
      const n = Math.ceil(len / maxSegLen);
      for (let k = 1; k < n; k++) {
        const t = k / n;
        out.push({ x: prev.x + dx * t, y: prev.y + dy * t });
      }
    }
    out.push(cur);
  }
  return out;
}

function doClip(
  line: ReadonlyArray<Vec2World>,
  obstacles: ReadonlyArray<ReadonlyArray<Vec2World>>,
): ClippedSegments {
  const inside = (p: Vec2World): boolean => {
    for (const poly of obstacles) {
      if (pointInPolygon(p, poly)) return true;
    }
    return false;
  };

  const result: ClippedSegments = [];
  let current: Vec2World[] = [];
  const n = line.length;

  for (let i = 0; i < n; i++) {
    const p = line[i] as Vec2World;
    const pIn = inside(p);

    if (i === 0) {
      if (!pIn) current.push(p);
      continue;
    }

    const prev = line[i - 1] as Vec2World;
    const prevIn = inside(prev);

    if (!prevIn && !pIn) {
      current.push(p);
    } else if (prevIn && pIn) {
      if (current.length >= 2) result.push(current);
      current = [];
    } else if (!prevIn && pIn) {
      // Saindo do espaço livre — adiciona o ponto exato de entrada.
      const cross = findBoundaryCrossing(prev, p, obstacles);
      if (cross) current.push(cross);
      if (current.length >= 2) result.push(current);
      current = [];
    } else {
      // !pIn && prevIn — entrando no espaço livre.
      const cross = findBoundaryCrossing(prev, p, obstacles);
      if (cross) current = [cross, p];
      else current = [p];
    }
  }

  if (current.length >= 2) result.push(current);
  return result;
}

function pointInPolygon(
  p: Vec2World,
  polygon: ReadonlyArray<Vec2World>,
): boolean {
  let inside = false;
  const n = polygon.length;
  for (let i = 0, j = n - 1; i < n; j = i++) {
    const pi = polygon[i] as Vec2World;
    const pj = polygon[j] as Vec2World;
    const intersect =
      pi.y > p.y !== pj.y > p.y &&
      p.x < ((pj.x - pi.x) * (p.y - pi.y)) / (pj.y - pi.y) + pi.x;
    if (intersect) inside = !inside;
  }
  return inside;
}

/** Ponto onde `a→b` cruza a fronteira de um obstáculo (busca binária em t, 24 iterações ≈ 1e-7). */
function findBoundaryCrossing(
  a: Vec2World,
  b: Vec2World,
  obstacles: ReadonlyArray<ReadonlyArray<Vec2World>>,
): Vec2World | null {
  const sampleAt = (t: number): Vec2World => ({
    x: a.x + (b.x - a.x) * t,
    y: a.y + (b.y - a.y) * t,
  });
  const isInside = (p: Vec2World): boolean =>
    obstacles.some((poly) => pointInPolygon(p, poly));

  const aIn = isInside(a);
  const bIn = isInside(b);
  if (aIn === bIn) return null;

  let lo = 0;
  let hi = 1;
  for (let i = 0; i < 24; i++) {
    const mid = (lo + hi) / 2;
    const midIn = isInside(sampleAt(mid));
    if (midIn === aIn) lo = mid;
    else hi = mid;
  }
  return sampleAt((lo + hi) / 2);
}
