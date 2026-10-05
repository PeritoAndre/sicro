import { describe, expect, it } from "vitest";
import { emptyPlanta, type SicroPlantaDoc } from "../schema";
import { addWall, deleteWalls, setWallInner, splitWallAt, wallGeometry, wallGeometryFull, nodeMap, wallFrame } from "../walls";
import { builtArea, createRoomAt, faces, roomGeometry } from "../rooms";
import { autoDims } from "../dims";
import { clampT0, newOpening, openingCenter } from "../openings";
import { wallMeasures, labelFor, cardinal } from "../evidence";

const T = 0.15;
const tol = 0.05;
const doc0 = () => emptyPlanta({ planta_id: "p", occurrence_id: "o", title: "t" });

function rect(doc: SicroPlantaDoc, x0: number, y0: number, x1: number, y1: number) {
  const pts = [
    { x: x0, y: y0 },
    { x: x1, y: y0 },
    { x: x1, y: y1 },
    { x: x0, y: y1 },
  ];
  for (let i = 0; i < 4; i++) doc = addWall(doc, pts[i]!, pts[(i + 1) % 4]!, { thickness: T }, tol).doc;
  return doc;
}

describe("paredes e cômodos", () => {
  it("quadrado vira um cômodo com área útil e construída", () => {
    let d = rect(doc0(), 0, 0, 4, 3);
    expect(d.walls).toHaveLength(4);
    expect(d.nodes).toHaveLength(4);
    const f = faces(d);
    expect(f.filter((x) => x.area > 0)).toHaveLength(1);
    expect(f.filter((x) => x.area < 0)).toHaveLength(1);
    d = createRoomAt(d, { x: 2, y: 1.5 }, "Sala").doc;
    const rg = roomGeometry(d);
    expect(rg).toHaveLength(1);
    expect(rg[0]!.area).toBeCloseTo((4 - T) * (3 - T), 6);
    expect(builtArea(d)).toBeCloseTo((4 + T) * (3 + T), 6);
  });

  it("não duplica parede já desenhada", () => {
    let d = rect(doc0(), 0, 0, 4, 3);
    const r = addWall(d, { x: 0, y: 0 }, { x: 4, y: 0 }, {}, tol);
    expect(r.walls).toHaveLength(0);
    expect(r.doc.walls).toHaveLength(4);
  });

  it("parede em T divide o cômodo em dois", () => {
    let d = rect(doc0(), 0, 0, 4, 3);
    d = addWall(d, { x: 2, y: 0 }, { x: 2, y: 3 }, { thickness: T }, tol).doc;
    expect(d.walls).toHaveLength(7);
    d = createRoomAt(d, { x: 1, y: 1.5 }, "A").doc;
    d = createRoomAt(d, { x: 3, y: 1.5 }, "B").doc;
    const rg = roomGeometry(d);
    expect(rg).toHaveLength(2);
    for (const r of rg) expect(r.area).toBeCloseTo((2 - T) * (3 - T), 6);
  });

  it("cruzamento corta as duas paredes", () => {
    let d = addWall(doc0(), { x: 0, y: 1 }, { x: 4, y: 1 }, {}, tol).doc;
    d = addWall(d, { x: 2, y: 0 }, { x: 2, y: 2 }, {}, tol).doc;
    expect(d.walls).toHaveLength(4);
    expect(d.nodes).toHaveLength(5);
  });

  it("trecho sobreposto reaproveita a parede e só acrescenta o que falta", () => {
    let d = addWall(doc0(), { x: 0, y: 0 }, { x: 10, y: 0 }, {}, tol).doc;
    const r1 = addWall(d, { x: 2, y: 0 }, { x: 5, y: 0 }, {}, tol);
    expect(r1.walls).toHaveLength(0);
    d = r1.doc;
    const r2 = addWall(d, { x: 8, y: 0 }, { x: 12, y: 0 }, {}, tol);
    expect(r2.walls).toHaveLength(1);
    const total = r2.doc.walls.reduce((s, w) => s + wallFrame(w, nodeMap(r2.doc))!.L, 0);
    expect(total).toBeCloseTo(12, 6);
  });

  it("quinas: parede de cima tem face interna = vão livre", () => {
    const d = rect(doc0(), 0, 0, 4, 3);
    const geo = wallGeometry(d);
    const top = d.walls.find((w) => {
      const f = wallFrame(w, nodeMap(d))!;
      return Math.abs(f.A.y) < 1e-9 && Math.abs(f.B.y) < 1e-9;
    })!;
    const g = geo.get(top.id)!;
    expect(g.inner).toBeCloseTo(4 - T, 6);
    expect(Math.max(g.faceLeft, g.faceRight)).toBeCloseTo(4 + T, 6);
    expect(g.marginA).toBeCloseTo(T / 2, 6);
  });

  it("apagar a parede do T junta de novo os trechos alinhados", () => {
    let d = rect(doc0(), 0, 0, 4, 3);
    d = addWall(d, { x: 2, y: 0 }, { x: 2, y: 3 }, {}, tol).doc;
    const mid = d.walls.find((w) => {
      const f = wallFrame(w, nodeMap(d))!;
      return Math.abs(f.A.x - 2) < 1e-9 && Math.abs(f.B.x - 2) < 1e-9;
    })!;
    d = deleteWalls(d, [mid.id]);
    expect(d.walls).toHaveLength(4);
    expect(d.nodes).toHaveLength(4);
  });

  it("dividir e juntar parede mantém a porta no mesmo lugar", () => {
    let d = addWall(doc0(), { x: 0, y: 0 }, { x: 6, y: 0 }, {}, tol).doc;
    const w = d.walls[0]!;
    d = { ...d, openings: [{ ...newOpening("porta", w.id, 4, 1) }] };
    const before = openingCenter(wallGeometry(d).get(w.id)!, d.openings[0]!);
    const sp = splitWallAt(d, w.id, { x: 2, y: 0 });
    d = sp.doc;
    const o1 = d.openings[0]!;
    expect(o1.wall).not.toBe(w.id);
    expect(openingCenter(wallGeometry(d).get(o1.wall)!, o1).x).toBeCloseTo(before.x, 6);
  });

  it("digitar a medida interna move a ponta da parede", () => {
    let d = rect(doc0(), 0, 0, 4, 3);
    const top = d.walls[0]!;
    d = setWallInner(d, top.id, 5 - T);
    expect(wallGeometry(d).get(top.id)!.inner).toBeCloseTo(5 - T, 6);
  });

  it("porta não invade a quina", () => {
    const d = rect(doc0(), 0, 0, 4, 3);
    const g = wallGeometry(d).get(d.walls[0]!.id)!;
    expect(clampT0(g, -1, 0.8)).toBeCloseTo(T / 2, 6);
    expect(clampT0(g, 9, 0.8)).toBeCloseTo(4 - T / 2 - 0.8, 6);
  });
});

describe("cotas e vestígios", () => {
  it("cotas externas: com parede interna saem trechos e o total", () => {
    let d = rect(doc0(), 0, 0, 4, 3);
    expect(autoDims(d).filter((x) => x.out.y === -1)).toHaveLength(1);
    d = addWall(d, { x: 2, y: 0 }, { x: 2, y: 3 }, {}, tol).doc;
    const top = autoDims(d).filter((x) => x.out.y === -1);
    expect(top).toHaveLength(3);
    const total = top.reduce((m, x) => Math.max(m, x.value), 0);
    expect(total).toBeCloseTo(4 + T, 6);
  });

  it("medida do vestígio até duas paredes perpendiculares, com nome pelo norte", () => {
    const d = rect(doc0(), 0, 0, 4, 3);
    const m = wallMeasures(d, { x: 1, y: 0.8 });
    expect(m).toHaveLength(2);
    expect(m[0]!.value).toBeCloseTo(0.8 - T / 2, 6);
    expect(m[0]!.ref).toBe("parede norte");
    expect(m[1]!.value).toBeCloseTo(1 - T / 2, 6);
    expect(m[1]!.ref).toBe("parede oeste");
  });

  it("rótulos e pontos cardeais", () => {
    expect([0, 1, 25, 26, 27].map((i) => labelFor("letra", i))).toEqual(["A", "B", "Z", "AA", "AB"]);
    expect(labelFor("numero", 4)).toBe("5");
    expect(cardinal({ x: 0, y: -1 }, 0)).toBe("norte");
    expect(cardinal({ x: 1, y: 0 }, 0)).toBe("leste");
    expect(cardinal({ x: 1, y: 0 }, 90)).toBe("norte");
  });

  it("encontro em T tem miolo preenchido", () => {
    let d = rect(doc0(), 0, 0, 4, 3);
    d = addWall(d, { x: 2, y: 0 }, { x: 2, y: 3 }, {}, tol).doc;
    const { hubs } = wallGeometryFull(d);
    expect(hubs).toHaveLength(2);
    for (const h of hubs) expect(h.poly).toHaveLength(3);
  });

  it("planta em L: cada lado só cota o trecho que está na borda", () => {
    let d = rect(doc0(), 0, 0, 5.5, 4.5);
    d = rect(d, 5.5, 0, 9.5, 3.5);
    const all = autoDims(d);
    const right = all.filter((x) => x.out.x === 1);
    expect(right).toHaveLength(1);
    expect(right[0]!.value).toBeCloseTo(3.5 + T, 6);
    const bottom = all.filter((x) => x.out.y === 1);
    expect(bottom).toHaveLength(1);
    expect(bottom[0]!.value).toBeCloseTo(5.5 + T, 6);
  });
});
