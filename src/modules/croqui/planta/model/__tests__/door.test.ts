import { it, expect } from "vitest";
import { emptyPlanta } from "../schema";
import { addWall, wallGeometry, nodeMap, wallFrame } from "../walls";
import { snapToWall, newOpening, openingDrawing } from "../openings";
it("porta abre para o lado do ponteiro", () => {
  let d = emptyPlanta({ planta_id: "p", occurrence_id: "o", title: "t" });
  const R = (x0: number, y0: number, x1: number, y1: number) => {
    const p = [{ x: x0, y: y0 }, { x: x1, y: y0 }, { x: x1, y: y1 }, { x: x0, y: y1 }];
    for (let i = 0; i < 4; i++) d = addWall(d, p[i]!, p[(i + 1) % 4]!, { thickness: 0.15 }, 0.15).doc;
  };
  R(2, 2, 7.5, 6.5);
  R(7.5, 2, 11.5, 5.5);
  const s = snapToWall(d, { x: 7.7, y: 4 }, 0.6)!;
  const w = d.walls.find((x) => x.id === s.wall)!;
  const f = wallFrame(w, nodeMap(d))!;
  void f;
  const g = wallGeometry(d).get(s.wall)!;
  const o = newOpening("porta", s.wall, s.along - 0.4, s.side);
  const dr = openingDrawing(g, o, 0.15);
  expect(dr.lines[0]![1]!.x).toBeGreaterThan(7.5);
});
