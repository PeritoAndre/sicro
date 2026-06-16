/**
 * Python Parity Engine — clipping de marcações.
 *
 * Emulação geométrica do `_em_outra` + `_segs` do SICRO 1.0 Python
 * (`editor_croqui.py:2851-2911`). Para cada ponto de uma marcação,
 * checa se está dentro do polígono de outra via / rotatória; se sim,
 * pula. Recorta o segmento exato na fronteira.
 *
 * Por que NÃO usamos `polygon-clipping` (Vatti) aqui:
 *   - polygon-clipping retorna multipolygon, não polilinha;
 *   - precisamos manter ORDEM dos segmentos resultantes para
 *     desenhar dash contínuo;
 *   - per-point ray casting é simples, debugável, com fallback fácil.
 *
 * **Regra fundamental do perito:** se o clipping gerar resultado vazio
 * ou degenerado, o renderer ainda deve mostrar ALGO (a marcação não
 * clipada). NUNCA deixar o croqui virar aberração por causa de
 * boolean op.
 */

import type { Vec2World } from "./geometry";

// ---------------------------------------------------------------------------
// Resultado.

/**
 * Lista de sub-polilinhas (trechos preservados após clipping).
 * Cada sub-polilinha tem >= 2 pontos.
 */
export type ClippedSegments = Vec2World[][];

/**
 * Diagnóstico do clipping — útil para warnings e debug overlay.
 */
export interface ClipReport {
  /** Quantidade de trechos retornados após clipping. */
  segments_count: number;
  /** Quantidade de obstáculos considerados. */
  obstacles_count: number;
  /** True quando algum erro foi capturado pelo try-catch e o fallback foi acionado. */
  fallback_used: boolean;
  /** Mensagem do erro, se houve. */
  fallback_reason?: string;
}

// ---------------------------------------------------------------------------
// Algoritmo principal.

/**
 * Clipa uma polilinha `line` contra `obstacles` (lista de polígonos
 * fechados). Retorna sub-polilinhas dos trechos que NÃO estão dentro
 * de qualquer obstáculo.
 *
 * Comportamento defensivo:
 *   - se `obstacles.length === 0` → retorna `[line.slice()]`.
 *   - se `line.length < 2` → retorna `[]`.
 *   - se algum cálculo lança (geometria degenerada) → captura o erro,
 *     marca `fallback_used = true`, retorna a polilinha original
 *     intacta. **Nunca propaga exceção.**
 */
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
    // Densifica a linha antes de clipar. Garante que segmentos
    // longos que atravessam um obstáculo (ambos endpoints fora,
    // mas trecho intermediário dentro) sejam corretamente cortados.
    // Sem isso, o algoritmo per-ponto perderia a interseção.
    const densified = densifyPolyline(line, 1.0);
    const segments = doClip(densified, obstacles);
    report.segments_count = segments.length;
    return { segments, report };
  } catch (err) {
    // FALLBACK — qualquer erro de geometria devolve a marcação inteira.
    // Garante que o croqui não vira aberração.
    report.fallback_used = true;
    report.fallback_reason =
      err instanceof Error ? err.message : String(err);
    report.segments_count = 1;
    return { segments: [line.slice()], report };
  }
}

// ---------------------------------------------------------------------------
// Densificação — garante que segmentos não passem por dentro de um
// obstáculo sem nenhum vértice intermediário ser detectado.

/**
 * Subdivide cada segmento longo da polilinha, inserindo pontos
 * intermediários, até que nenhum segmento exceda `maxSegLen` metros.
 *
 * Sem isso, o algoritmo per-ponto perderia interseções quando ambos
 * os endpoints de um segmento estão fora do obstáculo mas o trecho
 * intermediário passa por dentro.
 *
 * Aceita `maxSegLen <= 0` como "não densificar" (passa direto).
 */
export function densifyPolyline(
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

// ---------------------------------------------------------------------------
// Implementação.

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

/**
 * Ray casting clássico (ponto-em-polígono).
 */
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

/**
 * Localiza o ponto exato em que o segmento `a→b` cruza a fronteira
 * de algum obstáculo. Busca binária no parâmetro `t ∈ [0, 1]` — 24
 * iterações ≈ 1e-7 de precisão.
 *
 * Retorna `null` se `a` e `b` estão do mesmo lado (não há cruzamento).
 */
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
