/** Geometria 2D em metros (y para baixo, como na tela). */

export interface Pt {
  x: number;
  y: number;
}

export const EPS = 1e-9;

export const add = (a: Pt, b: Pt): Pt => ({ x: a.x + b.x, y: a.y + b.y });
export const sub = (a: Pt, b: Pt): Pt => ({ x: a.x - b.x, y: a.y - b.y });
export const mul = (a: Pt, k: number): Pt => ({ x: a.x * k, y: a.y * k });
export const dot = (a: Pt, b: Pt) => a.x * b.x + a.y * b.y;
export const cross = (a: Pt, b: Pt) => a.x * b.y - a.y * b.x;
export const len = (a: Pt) => Math.hypot(a.x, a.y);
export const dist = (a: Pt, b: Pt) => Math.hypot(a.x - b.x, a.y - b.y);
export const norm = (a: Pt): Pt => {
  const l = len(a);
  return l < EPS ? { x: 0, y: 0 } : { x: a.x / l, y: a.y / l };
};
/** Normal à esquerda de uma direção (gira +90°; com y para baixo fica "à direita" na tela). */
export const perp = (a: Pt): Pt => ({ x: -a.y, y: a.x });
export const lerp = (a: Pt, b: Pt, t: number): Pt => ({ x: a.x + (b.x - a.x) * t, y: a.y + (b.y - a.y) * t });
export const angle = (a: Pt) => Math.atan2(a.y, a.x);

/** Projeção de p no segmento ab: parâmetro t (0..1, não limitado) e distância até a reta. */
export function projectOnSegment(p: Pt, a: Pt, b: Pt): { t: number; point: Pt; distance: number } {
  const ab = sub(b, a);
  const l2 = dot(ab, ab);
  const t = l2 < EPS ? 0 : dot(sub(p, a), ab) / l2;
  const tc = Math.max(0, Math.min(1, t));
  const point = lerp(a, b, tc);
  return { t, point, distance: dist(p, point) };
}

/** Interseção de duas retas (ponto + direção); null se paralelas. */
export function intersectLines(p: Pt, d: Pt, q: Pt, e: Pt): Pt | null {
  const den = cross(d, e);
  if (Math.abs(den) < 1e-9) return null;
  const t = cross(sub(q, p), e) / den;
  return add(p, mul(d, t));
}

/** Interseção própria de dois segmentos (exclui pontas encostadas); parâmetros em cada um. */
export function intersectSegments(a: Pt, b: Pt, c: Pt, d: Pt): { t: number; u: number; point: Pt } | null {
  const r = sub(b, a);
  const s = sub(d, c);
  const den = cross(r, s);
  if (Math.abs(den) < 1e-12) return null;
  const t = cross(sub(c, a), s) / den;
  const u = cross(sub(c, a), r) / den;
  if (t <= 1e-6 || t >= 1 - 1e-6 || u <= 1e-6 || u >= 1 - 1e-6) return null;
  return { t, u, point: add(a, mul(r, t)) };
}

/** Área com sinal (positiva no sentido horário da tela, y para baixo). */
export function signedArea(poly: Pt[]): number {
  let s = 0;
  for (let i = 0; i < poly.length; i++) {
    const p = poly[i]!;
    const q = poly[(i + 1) % poly.length]!;
    s += p.x * q.y - q.x * p.y;
  }
  return s / 2;
}

export const polygonArea = (poly: Pt[]) => Math.abs(signedArea(poly));

export function pointInPolygon(p: Pt, poly: Pt[]): boolean {
  let inside = false;
  for (let i = 0, j = poly.length - 1; i < poly.length; j = i++) {
    const a = poly[i]!;
    const b = poly[j]!;
    if (a.y > p.y !== b.y > p.y && p.x < ((b.x - a.x) * (p.y - a.y)) / (b.y - a.y) + a.x) inside = !inside;
  }
  return inside;
}

export function centroid(poly: Pt[]): Pt {
  const a = signedArea(poly);
  if (Math.abs(a) < EPS) {
    const s = poly.reduce((acc, p) => add(acc, p), { x: 0, y: 0 });
    return mul(s, 1 / Math.max(1, poly.length));
  }
  let cx = 0;
  let cy = 0;
  for (let i = 0; i < poly.length; i++) {
    const p = poly[i]!;
    const q = poly[(i + 1) % poly.length]!;
    const f = p.x * q.y - q.x * p.y;
    cx += (p.x + q.x) * f;
    cy += (p.y + q.y) * f;
  }
  return { x: cx / (6 * a), y: cy / (6 * a) };
}

/** Ponto bem dentro do polígono (para rótulos): o centro se cair dentro, senão o melhor de uma grade. */
export function interiorPoint(poly: Pt[]): Pt {
  const c = centroid(poly);
  if (pointInPolygon(c, poly)) return c;
  const xs = poly.map((p) => p.x);
  const ys = poly.map((p) => p.y);
  const [x0, x1, y0, y1] = [Math.min(...xs), Math.max(...xs), Math.min(...ys), Math.max(...ys)];
  let best = c;
  let bestD = -1;
  for (let i = 1; i < 12; i++)
    for (let j = 1; j < 12; j++) {
      const p = { x: x0 + ((x1 - x0) * i) / 12, y: y0 + ((y1 - y0) * j) / 12 };
      if (!pointInPolygon(p, poly)) continue;
      let d = Infinity;
      for (let k = 0; k < poly.length; k++) d = Math.min(d, projectOnSegment(p, poly[k]!, poly[(k + 1) % poly.length]!).distance);
      if (d > bestD) {
        bestD = d;
        best = p;
      }
    }
  return best;
}

export function bbox(points: Pt[]): { x0: number; y0: number; x1: number; y1: number } | null {
  if (points.length === 0) return null;
  let x0 = Infinity;
  let y0 = Infinity;
  let x1 = -Infinity;
  let y1 = -Infinity;
  for (const p of points) {
    x0 = Math.min(x0, p.x);
    y0 = Math.min(y0, p.y);
    x1 = Math.max(x1, p.x);
    y1 = Math.max(y1, p.y);
  }
  return { x0, y0, x1, y1 };
}

/** Rotaciona p em torno de c (graus, horário na tela). */
export function rotate(p: Pt, c: Pt, deg: number): Pt {
  const r = (deg * Math.PI) / 180;
  const cs = Math.cos(r);
  const sn = Math.sin(r);
  const d = sub(p, c);
  return { x: c.x + d.x * cs - d.y * sn, y: c.y + d.x * sn + d.y * cs };
}

/** Número com vírgula, em metros. */
export function fmtM(v: number, digits = 2): string {
  return v.toFixed(digits).replace(".", ",");
}

/** "4,00" ou "4" etc. → metros; aceita vírgula ou ponto. null se inválido. */
export function parseM(text: string): number | null {
  const t = text.trim().replace(/\s*m$/i, "").replace(",", ".");
  if (!/^-?\d+(\.\d+)?$/.test(t)) return null;
  const v = Number(t);
  return Number.isFinite(v) ? v : null;
}
