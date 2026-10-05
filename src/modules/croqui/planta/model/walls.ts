/**
 * Paredes como grafo: nós ligados por paredes (eixo + espessura). Toda parede nova
 * reaproveita nós próximos, corta paredes em T e em cruzamentos e não duplica trechos,
 * para que os cômodos possam ser achados pelo contorno.
 */
import {
  add,
  angle,
  dist,
  dot,
  intersectLines,
  intersectSegments,
  lerp,
  mul,
  norm,
  perp,
  projectOnSegment,
  sub,
  type Pt,
} from "./geom";
import { uid, WALL_DEFAULT, type PNode, type POpening, type PWall, type SicroPlantaDoc } from "./schema";

type Doc = SicroPlantaDoc;

export function nodeMap(doc: Doc): Map<string, PNode> {
  return new Map(doc.nodes.map((n) => [n.id, n]));
}

export interface WallFrame {
  A: Pt;
  B: Pt;
  dir: Pt;
  nrm: Pt;
  L: number;
}

export function wallFrame(w: PWall, nodes: Map<string, PNode>): WallFrame | null {
  const a = nodes.get(w.a);
  const b = nodes.get(w.b);
  if (!a || !b) return null;
  const A = { x: a.x, y: a.y };
  const B = { x: b.x, y: b.y };
  const dir = norm(sub(B, A));
  return { A, B, dir, nrm: perp(dir), L: dist(A, B) };
}

export function findNode(doc: Doc, p: Pt, tol: number, exclude?: string): PNode | null {
  let best: PNode | null = null;
  let bd = tol;
  for (const n of doc.nodes) {
    if (n.id === exclude) continue;
    const d = Math.hypot(n.x - p.x, n.y - p.y);
    if (d <= bd) {
      bd = d;
      best = n;
    }
  }
  return best;
}

export interface WallHit {
  wall: PWall;
  t: number;
  point: Pt;
  distance: number;
}

/** Parede mais próxima de p (pelo eixo), só no miolo (longe das pontas). */
export function findWall(doc: Doc, p: Pt, tol: number, excludeNode?: string): WallHit | null {
  const nodes = nodeMap(doc);
  let best: WallHit | null = null;
  for (const w of doc.walls) {
    if (excludeNode && (w.a === excludeNode || w.b === excludeNode)) continue;
    const f = wallFrame(w, nodes);
    if (!f || f.L < 1e-6) continue;
    const pr = projectOnSegment(p, f.A, f.B);
    const lim = Math.max(tol, w.thickness / 2);
    if (pr.t <= 0 || pr.t >= 1 || pr.distance > lim) continue;
    if (!best || pr.distance < best.distance) best = { wall: w, t: pr.t, point: pr.point, distance: pr.distance };
  }
  return best;
}

export function wallBetween(doc: Doc, u: string, v: string): PWall | undefined {
  return doc.walls.find((w) => (w.a === u && w.b === v) || (w.a === v && w.b === u));
}

export function degree(doc: Doc, nodeId: string): number {
  return doc.walls.reduce((k, w) => k + (w.a === nodeId ? 1 : 0) + (w.b === nodeId ? 1 : 0), 0);
}

/** Divide a parede no nó dado (que fica sobre ela); as aberturas vão para a parte onde está o centro delas. */
export function splitWallWithNode(doc: Doc, wallId: string, nodeId: string): Doc {
  const w = doc.walls.find((x) => x.id === wallId);
  const nodes = nodeMap(doc);
  const n = nodes.get(nodeId);
  if (!w || !n || w.a === nodeId || w.b === nodeId) return doc;
  const f = wallFrame(w, nodes);
  if (!f) return doc;
  const L1 = Math.max(0, Math.min(f.L, dot(sub(n, f.A), f.dir)));
  const w1: PWall = { ...w, b: nodeId };
  const w2: PWall = { ...w, id: uid("w"), a: nodeId };
  const openings = doc.openings.map((o): POpening => {
    if (o.wall !== w.id) return o;
    return o.t0 + o.width / 2 <= L1 ? o : { ...o, wall: w2.id, t0: o.t0 - L1 };
  });
  return { ...doc, walls: doc.walls.flatMap((x) => (x.id === w.id ? [w1, w2] : [x])), openings };
}

export function splitWallAt(doc: Doc, wallId: string, p: Pt): { doc: Doc; node: string } {
  const w = doc.walls.find((x) => x.id === wallId);
  const nodes = nodeMap(doc);
  const f = w ? wallFrame(w, nodes) : null;
  if (!w || !f) return { doc, node: "" };
  const pr = projectOnSegment(p, f.A, f.B);
  const node: PNode = { id: uid("n"), ...pr.point };
  const withNode = { ...doc, nodes: [...doc.nodes, node] };
  return { doc: splitWallWithNode(withNode, wallId, node.id), node: node.id };
}

/** Nó em p: reaproveita um nó perto, corta uma parede perto, ou cria um novo. */
export function ensureNode(doc: Doc, p: Pt, tol: number): { doc: Doc; node: string } {
  const n = findNode(doc, p, tol);
  if (n) return { doc, node: n.id };
  const hit = findWall(doc, p, tol);
  if (hit) return splitWallAt(doc, hit.wall.id, hit.point);
  const node: PNode = { id: uid("n"), x: p.x, y: p.y };
  return { doc: { ...doc, nodes: [...doc.nodes, node] }, node: node.id };
}

/** Corta as paredes que cruzam o segmento uv (cruzamento próprio) e devolve o documento. */
function splitCrossings(doc: Doc, u: string, v: string): Doc {
  for (let guard = 0; guard < 200; guard++) {
    const nodes = nodeMap(doc);
    const P = nodes.get(u)!;
    const Q = nodes.get(v)!;
    let hit: { wall: PWall; point: Pt } | null = null;
    for (const w of doc.walls) {
      if (w.a === u || w.b === u || w.a === v || w.b === v) continue;
      const f = wallFrame(w, nodes);
      if (!f) continue;
      const x = intersectSegments(P, Q, f.A, f.B);
      if (x) {
        hit = { wall: w, point: x.point };
        break;
      }
    }
    if (!hit) return doc;
    doc = splitWallAt(doc, hit.wall.id, hit.point).doc;
  }
  return doc;
}

/** Parede nova de p a q. Devolve as paredes criadas (trechos que já existiam são reaproveitados). */
export function addWall(
  doc: Doc,
  p: Pt,
  q: Pt,
  props: Partial<Pick<PWall, "thickness" | "height" | "kind">>,
  tol: number,
): { doc: Doc; walls: string[]; start: string; end: string } {
  const r1 = ensureNode(doc, p, tol);
  const r2 = ensureNode(r1.doc, q, tol);
  doc = r2.doc;
  const u = r1.node;
  const v = r2.node;
  if (!u || !v || u === v) return { doc, walls: [], start: u, end: v };
  doc = splitCrossings(doc, u, v);
  const nodes = nodeMap(doc);
  const P = nodes.get(u)!;
  const Q = nodes.get(v)!;
  const on = doc.nodes
    .map((n) => ({ n, pr: projectOnSegment(n, P, Q) }))
    .filter(({ n, pr }) => n.id === u || n.id === v || (pr.t > 1e-6 && pr.t < 1 - 1e-6 && pr.distance < Math.max(1e-4, tol * 0.35)))
    .sort((x, y) => x.pr.t - y.pr.t)
    .map((x) => x.n.id);
  const created: string[] = [];
  const walls = [...doc.walls];
  for (let i = 0; i + 1 < on.length; i++) {
    const a = on[i]!;
    const b = on[i + 1]!;
    if (a === b || walls.some((w) => (w.a === a && w.b === b) || (w.a === b && w.b === a))) continue;
    const w: PWall = {
      id: uid("w"),
      a,
      b,
      thickness: props.thickness ?? WALL_DEFAULT.thickness,
      height: props.height ?? WALL_DEFAULT.height,
      kind: props.kind ?? "parede",
    };
    walls.push(w);
    created.push(w.id);
  }
  return { doc: { ...doc, walls }, walls: created, start: u, end: v };
}

export function moveNodes(doc: Doc, moves: Map<string, Pt>): Doc {
  if (moves.size === 0) return doc;
  return { ...doc, nodes: doc.nodes.map((n) => (moves.has(n.id) ? { ...n, ...moves.get(n.id)! } : n)) };
}

/** Funde o nó `from` em `into`: paredes redirecionadas, sem parede de comprimento zero nem duplicada. */
export function mergeNode(doc: Doc, from: string, into: string): Doc {
  if (from === into) return doc;
  const walls: PWall[] = [];
  const drop = new Set<string>();
  for (const w0 of doc.walls) {
    const w = { ...w0, a: w0.a === from ? into : w0.a, b: w0.b === from ? into : w0.b };
    if (w.a === w.b || walls.some((x) => (x.a === w.a && x.b === w.b) || (x.a === w.b && x.b === w.a))) {
      drop.add(w.id);
      continue;
    }
    walls.push(w);
  }
  return {
    ...doc,
    walls,
    nodes: doc.nodes.filter((n) => n.id !== from),
    openings: doc.openings.filter((o) => !drop.has(o.wall)),
  };
}

/** Depois de soltar um nó: funde com nó vizinho, vira T numa parede que encostou e corta cruzamentos. */
export function settleNode(doc: Doc, nodeId: string, tol: number): Doc {
  const n = doc.nodes.find((x) => x.id === nodeId);
  if (!n) return doc;
  const other = findNode(doc, n, tol, nodeId);
  if (other) return removeOrphans(mergeNode(doc, nodeId, other.id));
  const hit = findWall(doc, n, tol, nodeId);
  if (hit) doc = splitWallWithNode(moveNodes(doc, new Map([[nodeId, hit.point]])), hit.wall.id, nodeId);
  for (const w of doc.walls.filter((x) => x.a === nodeId || x.b === nodeId)) doc = splitCrossings(doc, w.a, w.b);
  return doc;
}

export function removeOrphans(doc: Doc): Doc {
  const used = new Set(doc.walls.flatMap((w) => [w.a, w.b]));
  return { ...doc, nodes: doc.nodes.filter((n) => used.has(n.id)) };
}

/** Inverte o sentido da parede mantendo portas e janelas no mesmo lugar. */
function flipped(w: PWall, ops: POpening[], L: number): { wall: PWall; ops: POpening[] } {
  return {
    wall: { ...w, a: w.b, b: w.a },
    ops: ops.map((o) => ({ ...o, t0: L - o.t0 - o.width, hinge: o.hinge === "start" ? "end" : "start", side: (o.side === 1 ? -1 : 1) as 1 | -1 })),
  };
}

/** Nó com duas paredes alinhadas e iguais vira uma parede só. */
export function healNode(doc: Doc, nodeId: string): Doc {
  const inc = doc.walls.filter((w) => w.a === nodeId || w.b === nodeId);
  if (inc.length !== 2) return doc;
  const [w1, w2] = inc as [PWall, PWall];
  if (w1.kind !== w2.kind || Math.abs(w1.thickness - w2.thickness) > 1e-6) return doc;
  const nodes = nodeMap(doc);
  const f1 = wallFrame(w1, nodes);
  const f2 = wallFrame(w2, nodes);
  if (!f1 || !f2) return doc;
  const n = nodes.get(nodeId)!;
  const d1 = norm(sub(w1.a === nodeId ? f1.B : f1.A, n));
  const d2 = norm(sub(w2.a === nodeId ? f2.B : f2.A, n));
  if (dot(d1, d2) > -0.9995) return doc;
  // w1 orientado x→n, w2 orientado n→y.
  let A = { wall: w1, ops: doc.openings.filter((o) => o.wall === w1.id) };
  if (w1.a === nodeId) A = flipped(w1, A.ops, f1.L);
  let B = { wall: w2, ops: doc.openings.filter((o) => o.wall === w2.id) };
  if (w2.b === nodeId) B = flipped(w2, B.ops, f2.L);
  const merged: PWall = { ...A.wall, b: B.wall.b };
  const ops = [...A.ops, ...B.ops.map((o) => ({ ...o, wall: merged.id, t0: o.t0 + f1.L }))];
  return {
    ...doc,
    walls: doc.walls.filter((w) => w.id !== w2.id).map((w) => (w.id === w1.id ? merged : w)),
    nodes: doc.nodes.filter((x) => x.id !== nodeId),
    openings: [...doc.openings.filter((o) => o.wall !== w1.id && o.wall !== w2.id), ...ops],
  };
}

export function deleteWalls(doc: Doc, ids: string[]): Doc {
  const set = new Set(ids);
  const ends = new Set(doc.walls.filter((w) => set.has(w.id)).flatMap((w) => [w.a, w.b]));
  doc = {
    ...doc,
    walls: doc.walls.filter((w) => !set.has(w.id)),
    openings: doc.openings.filter((o) => !set.has(o.wall)),
  };
  doc = removeOrphans(doc);
  for (const n of ends) doc = healNode(doc, n);
  return doc;
}

/** Medida de cada face e margens livres nas pontas (para portas e janelas não invadirem a quina). */
export interface WallGeom {
  poly: Pt[];
  frame: WallFrame;
  faceLeft: number;
  faceRight: number;
  inner: number;
  marginA: number;
  marginB: number;
}

/**
 * Polígono de cada parede com as quinas resolvidas em cada nó (cantos em L, T e cruz):
 * entre duas paredes vizinhas em ângulo, a quina é o encontro das faces voltadas uma para a outra.
 */
export function wallGeometry(doc: Doc): Map<string, WallGeom> {
  return wallGeometryFull(doc).walls;
}

/** Paredes e "miolos" dos encontros de 3 ou mais paredes (o centro do T ou da cruz). */
export function wallGeometryFull(doc: Doc): { walls: Map<string, WallGeom>; hubs: { node: string; poly: Pt[]; wall: PWall }[] } {
  const nodes = nodeMap(doc);
  const frames = new Map<string, WallFrame>();
  for (const w of doc.walls) {
    const f = wallFrame(w, nodes);
    if (f && f.L > 1e-6) frames.set(w.id, f);
  }
  type End = { w: PWall; dir: Pt; t: number; ang: number; end: "a" | "b" };
  const byNode = new Map<string, End[]>();
  for (const w of doc.walls) {
    const f = frames.get(w.id);
    if (!f) continue;
    const ea: End = { w, dir: f.dir, t: w.thickness, ang: angle(f.dir), end: "a" };
    const back = mul(f.dir, -1);
    const eb: End = { w, dir: back, t: w.thickness, ang: angle(back), end: "b" };
    (byNode.get(w.a) ?? byNode.set(w.a, []).get(w.a)!).push(ea);
    (byNode.get(w.b) ?? byNode.set(w.b, []).get(w.b)!).push(eb);
  }
  // Quinas por (nó, parede): lado +perp e lado −perp da direção que sai do nó.
  const corners = new Map<string, { plus: Pt; minus: Pt }>();
  const hubs: { node: string; poly: Pt[]; wall: PWall }[] = [];
  for (const [nid, ends] of byNode) {
    const n = nodes.get(nid)!;
    const c = { x: n.x, y: n.y };
    ends.sort((x, y) => x.ang - y.ang);
    const k = ends.length;
    const sector: Pt[] = [];
    for (let i = 0; i < k; i++) {
      const e = ends[i]!;
      const f = ends[(i + 1) % k]!;
      const pe = add(c, mul(perp(e.dir), e.t / 2));
      const pf = add(c, mul(perp(f.dir), -f.t / 2));
      let x = k > 1 ? intersectLines(pe, e.dir, pf, f.dir) : null;
      if (!x || dist(x, c) > 2.5 * Math.max(e.t, f.t) + 1e-9) x = pe;
      sector.push(x);
    }
    if (k >= 3) hubs.push({ node: nid, poly: [c, ...sector].slice(1), wall: ends[0]!.w });
    for (let i = 0; i < k; i++) {
      const e = ends[i]!;
      const plus = k > 1 ? sector[i]! : add(c, mul(perp(e.dir), e.t / 2));
      const minus = k > 1 ? sector[(i - 1 + k) % k]! : add(c, mul(perp(e.dir), -e.t / 2));
      corners.set(`${nid}|${e.w.id}|${e.end}`, { plus, minus });
    }
  }
  const out = new Map<string, WallGeom>();
  for (const w of doc.walls) {
    const f = frames.get(w.id);
    if (!f) continue;
    const ca = corners.get(`${w.a}|${w.id}|a`);
    const cb = corners.get(`${w.b}|${w.id}|b`);
    if (!ca || !cb) continue;
    // Em a, +perp = esquerda de a→b; em b, +perp da direção b→a = direita de a→b.
    const poly = [ca.plus, cb.minus, cb.plus, ca.minus];
    const along = (p: Pt) => dot(sub(p, f.A), f.dir);
    const faceLeft = Math.max(0, along(cb.minus) - along(ca.plus));
    const faceRight = Math.max(0, along(cb.plus) - along(ca.minus));
    out.set(w.id, {
      poly,
      frame: f,
      faceLeft,
      faceRight,
      inner: Math.min(faceLeft, faceRight),
      marginA: Math.max(0, along(ca.plus), along(ca.minus)),
      marginB: Math.max(0, f.L - along(cb.minus), f.L - along(cb.plus)),
    });
  }
  return { walls: out, hubs };
}

/** Nós de uma linha reta de paredes, andando a partir de `start` na direção `d` enquanto houver continuação. */
function straightRun(doc: Doc, start: string, d: Pt, nodes: Map<string, PNode>): string[] {
  const out: string[] = [];
  let cur = start;
  for (let guard = 0; guard < 1000; guard++) {
    const c = nodes.get(cur)!;
    const next = doc.walls
      .map((w) => (w.a === cur ? w.b : w.b === cur ? w.a : null))
      .filter((id): id is string => !!id && !out.includes(id) && id !== start)
      .find((id) => dot(norm(sub(nodes.get(id)!, c)), d) > 0.999);
    if (!next) break;
    out.push(next);
    cur = next;
  }
  return out;
}

/**
 * Ajusta o comprimento interno (face menor) empurrando a ponta `b`: a parede perpendicular
 * que sai dela anda inteira, paralela (como num cômodo retangular), e o resto acompanha.
 */
export function setWallInner(doc: Doc, wallId: string, target: number): Doc {
  const g = wallGeometry(doc).get(wallId);
  const w = doc.walls.find((x) => x.id === wallId);
  if (!g || !w || target <= 0) return doc;
  const delta = mul(g.frame.dir, target - g.inner);
  const nodes = nodeMap(doc);
  const B = nodes.get(w.b)!;
  const move = new Set<string>([w.b]);
  for (const x of doc.walls) {
    if (x.id === w.id || (x.a !== w.b && x.b !== w.b)) continue;
    const other = nodes.get(x.a === w.b ? x.b : x.a)!;
    const d = norm(sub(other, B));
    if (Math.abs(dot(d, g.frame.dir)) > 0.3) continue;
    for (const id of straightRun(doc, w.b, d, nodes)) move.add(id);
  }
  const moves = new Map<string, Pt>();
  for (const id of move) moves.set(id, add(nodes.get(id)!, delta));
  return moveNodes(doc, moves);
}

export function moveWallsBy(doc: Doc, wallIds: string[], delta: Pt, base?: Doc): Doc {
  const src = base ?? doc;
  const ids = new Set(src.walls.filter((w) => wallIds.includes(w.id)).flatMap((w) => [w.a, w.b]));
  const moves = new Map<string, Pt>();
  for (const n of src.nodes) if (ids.has(n.id)) moves.set(n.id, add(n, delta));
  return moveNodes(doc, moves);
}

export function wallMidpoint(w: PWall, nodes: Map<string, PNode>): Pt | null {
  const f = wallFrame(w, nodes);
  return f ? lerp(f.A, f.B, 0.5) : null;
}
