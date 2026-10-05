/** Cômodos achados pelo contorno das paredes (faces do grafo), áreas e contorno externo. */
import { add, angle, intersectLines, mul, norm, perp, pointInPolygon, polygonArea, signedArea, sub, dot, type Pt } from "./geom";
import { nodeMap } from "./walls";
import { uid, type PRoom, type PWall, type SicroPlantaDoc } from "./schema";

type Doc = SicroPlantaDoc;

export interface Face {
  /** Nós na ordem do contorno. */
  nodes: string[];
  /** Parede de cada trecho (nodes[i] → nodes[i+1]). */
  walls: PWall[];
  poly: Pt[];
  /** > 0: cômodo; ≤ 0: contorno externo de um bloco de paredes. */
  area: number;
}

/** Faces do grafo de paredes: a partir de cada meia-aresta, vira sempre para o vizinho anterior em ângulo. */
export function faces(doc: Doc): Face[] {
  const nodes = nodeMap(doc);
  const adj = new Map<string, { to: string; wall: PWall; ang: number }[]>();
  for (const w of doc.walls) {
    const a = nodes.get(w.a);
    const b = nodes.get(w.b);
    if (!a || !b || w.a === w.b) continue;
    (adj.get(w.a) ?? adj.set(w.a, []).get(w.a)!).push({ to: w.b, wall: w, ang: angle(sub(b, a)) });
    (adj.get(w.b) ?? adj.set(w.b, []).get(w.b)!).push({ to: w.a, wall: w, ang: angle(sub(a, b)) });
  }
  for (const list of adj.values()) list.sort((x, y) => x.ang - y.ang);
  const seen = new Set<string>();
  const out: Face[] = [];
  for (const [u, list] of adj) {
    for (const e of list) {
      const key0 = `${u}>${e.to}>${e.wall.id}`;
      if (seen.has(key0)) continue;
      const fn: string[] = [];
      const fw: PWall[] = [];
      let cu = u;
      let ce = e;
      for (let guard = 0; guard < 10000; guard++) {
        const key = `${cu}>${ce.to}>${ce.wall.id}`;
        if (seen.has(key)) break;
        seen.add(key);
        fn.push(cu);
        fw.push(ce.wall);
        const v = ce.to;
        const out2 = adj.get(v)!;
        const back = out2.findIndex((x) => x.to === cu && x.wall.id === ce.wall.id);
        const next = out2[(back - 1 + out2.length) % out2.length]!;
        cu = v;
        ce = next;
      }
      if (fn.length < 2) continue;
      const poly = fn.map((id) => {
        const n = nodes.get(id)!;
        return { x: n.x, y: n.y };
      });
      out.push({ nodes: fn, walls: fw, poly, area: signedArea(poly) });
    }
  }
  return out;
}

/**
 * Desloca cada trecho do contorno pela meia espessura da sua parede, para o lado de dentro
 * da face: num cômodo dá o piso útil; no contorno externo dá a borda de fora das paredes.
 */
export function offsetFace(doc: Doc, f: Face): Pt[] {
  const nodes = nodeMap(doc);
  const k = f.nodes.length;
  const lines: { p: Pt; d: Pt }[] = [];
  for (let i = 0; i < k; i++) {
    const a = nodes.get(f.nodes[i]!)!;
    const b = nodes.get(f.nodes[(i + 1) % k]!)!;
    const d = norm(sub(b, a));
    lines.push({ p: add(a, mul(perp(d), f.walls[i]!.thickness / 2)), d });
  }
  const out: Pt[] = [];
  for (let i = 0; i < k; i++) {
    const l1 = lines[i]!;
    const l2 = lines[(i + 1) % k]!;
    const x = intersectLines(l1.p, l1.d, l2.p, l2.d);
    if (x) {
      out.push(x);
      continue;
    }
    // Paralelas: trecho reto (nó no meio) ou ponta de parede solta (ida e volta).
    const n = nodes.get(f.nodes[(i + 1) % k]!)!;
    if (dot(l1.d, l2.d) > 0) out.push(add(n, mul(perp(l1.d), f.walls[i]!.thickness / 2)));
    else {
      out.push(add(n, mul(perp(l1.d), f.walls[i]!.thickness / 2)));
      out.push(add(n, mul(perp(l2.d), f.walls[(i + 1) % k]!.thickness / 2)));
    }
  }
  return out;
}

export function roomFaces(doc: Doc): Face[] {
  return faces(doc).filter((f) => f.area > 1e-6);
}

/** Face (cômodo) que contém o ponto: a menor que o contém. */
export function faceAt(doc: Doc, p: Pt, all?: Face[]): Face | null {
  let best: Face | null = null;
  for (const f of all ?? roomFaces(doc)) {
    if (f.area <= 1e-6 || !pointInPolygon(p, f.poly)) continue;
    if (!best || f.area < best.area) best = f;
  }
  return best;
}

export interface RoomGeom {
  room: PRoom;
  face: Face;
  inner: Pt[];
  area: number;
}

export function roomGeometry(doc: Doc): RoomGeom[] {
  const all = roomFaces(doc);
  const out: RoomGeom[] = [];
  for (const room of doc.rooms) {
    const face = faceAt(doc, room.seed, all);
    if (!face) continue;
    const inner = offsetFace(doc, face);
    out.push({ room, face, inner, area: polygonArea(inner) });
  }
  return out;
}

/** Área construída: soma dos contornos externos (borda de fora das paredes). */
export function builtArea(doc: Doc): number {
  return faces(doc)
    .filter((f) => f.area < -1e-6)
    .reduce((s, f) => s + polygonArea(offsetFace(doc, f)), 0);
}

export function outerOutlines(doc: Doc): Pt[][] {
  return faces(doc)
    .filter((f) => f.area < -1e-6)
    .map((f) => offsetFace(doc, f));
}

/** Cria o cômodo do ponto clicado, se ele estiver cercado de paredes e ainda não tiver nome. */
export function createRoomAt(doc: Doc, p: Pt, name: string): { doc: Doc; room: string | null } {
  const all = roomFaces(doc);
  const face = faceAt(doc, p, all);
  if (!face) return { doc, room: null };
  const taken = doc.rooms.find((r) => faceAt(doc, r.seed, all) === face);
  if (taken) return { doc, room: taken.id };
  const room: PRoom = { id: uid("r"), name, seed: p, label: null, show_area: true };
  return { doc: { ...doc, rooms: [...doc.rooms, room] }, room: room.id };
}

/** Próximo nome livre "Cômodo N". */
export function nextRoomName(doc: Doc): string {
  let i = doc.rooms.length + 1;
  while (doc.rooms.some((r) => r.name === `Cômodo ${i}`)) i++;
  return `Cômodo ${i}`;
}
