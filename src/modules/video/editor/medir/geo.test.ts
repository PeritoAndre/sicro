import { describe, expect, it } from "vitest";
import { applyH, buildReference, filaPosicao, invert3, serie } from "./geo";
import { emptyRef } from "./medirStore";
import type { VideoSpeedCalibration } from "@domain/video_speed";

describe("referência → pontos de controle", () => {
  it("retângulo: 4 cantos na ordem perto-esq, perto-dir, longe-dir, longe-esq", () => {
    const r = buildReference({ ...emptyRef(), kind: "retangulo", comprimento: "4,0", largura: "6,6", points: [{ x: 0, y: 10 }, { x: 10, y: 10 }, { x: 8, y: 0 }, { x: 2, y: 0 }] });
    expect(typeof r).not.toBe("string");
    if (typeof r === "string") return;
    expect(r.method).toBe("plane");
    expect(r.control_points.map((c) => [c.world_x_m, c.world_y_m])).toEqual([[0, 0], [6.6, 0], [6.6, 4], [0, 4]]);
  });
  it("sem medida devolve o que falta em texto", () => {
    expect(buildReference({ ...emptyRef(), kind: "medida", points: [{ x: 0, y: 0 }, { x: 1, y: 1 }] })).toMatch(/distância/);
  });
  it("veículo vira linha com a fonte entre-eixos", () => {
    const r = buildReference({ ...emptyRef(), kind: "veiculo", entreEixos: "2,65", points: [{ x: 0, y: 0 }, { x: 100, y: 0 }] });
    expect(r).toMatchObject({ method: "line", reference_source: "entre_eixos" });
  });
  it("tracejado urbano: começo e fim de cada traço (2 + 4 m)", () => {
    expect([0, 1, 2, 3, 4].map((i) => filaPosicao("urbano", i))).toEqual([0, 2, 6, 8, 12]);
    const r = buildReference({ ...emptyRef(), kind: "fila", fila: "urbano", points: [{ x: 0, y: 0 }, { x: 0, y: 1 }, { x: 0, y: 2 }] });
    expect(r).toMatchObject({ method: "cross_ratio" });
  });
});

describe("homografia", () => {
  it("inversa desfaz a ida", () => {
    const H = [0.02, 0.001, -3, 0.0005, 0.03, -2, 0.00001, 0.0002, 1];
    const inv = invert3(H)!;
    const w = applyH(H, 300, 200);
    const p = applyH(inv, w.x, w.y);
    expect(p.x).toBeCloseTo(300, 6);
    expect(p.y).toBeCloseTo(200, 6);
  });
  it("série: 15 m/s com escala de 10 px/m", () => {
    const cal = { homography: [0.1, 0, 0, 0, 0.1, 0, 0, 0, 1] } as unknown as VideoSpeedCalibration;
    const pts = [0, 0.5, 1].map((t) => ({ px: 0, py: 150 * t, actual_timestamp_s: t }));
    expect(serie(cal, pts)!.v).toBeCloseTo(15, 6);
  });
});
