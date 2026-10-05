import { describe, expect, it } from "vitest";
import { boxBlur, pixelate, redactionRect, toMoldablePoints } from "./redaction";

const img = (w: number, h: number, f: (x: number, y: number) => number) => {
  const d = new Uint8ClampedArray(w * h * 4);
  for (let y = 0; y < h; y++)
    for (let x = 0; x < w; x++) {
      const i = (y * w + x) * 4;
      d[i] = d[i + 1] = d[i + 2] = f(x, y);
      d[i + 3] = 255;
    }
  return d;
};

describe("tarja", () => {
  it("desfoque mantém área uniforme e espalha um ponto", () => {
    const flat = img(20, 20, () => 100);
    boxBlur(flat, 20, 20, 3);
    expect(flat.every((v, i) => (i % 4 === 3 ? v === 255 : Math.abs(v - 100) <= 1))).toBe(true);
    const dot = img(21, 21, (x, y) => (x === 10 && y === 10 ? 255 : 0));
    boxBlur(dot, 21, 21, 2);
    expect(dot[(10 * 21 + 10) * 4]!).toBeLessThan(255);
    expect(dot[(10 * 21 + 12) * 4]!).toBeGreaterThan(0);
  });

  it("pixelização deixa cada bloco com a média", () => {
    const d = img(4, 2, (x) => (x < 2 ? 0 : 200));
    pixelate(d, 4, 2, 4, 0, 0);
    expect(d[0]).toBe(100);
    expect(d[(1 * 4 + 3) * 4]).toBe(100);
  });

  it("retângulo desenhado para trás é normalizado", () => {
    const r = redactionRect({ x: 50, y: 40, width: -30, height: -10 } as never);
    expect(r).toEqual({ x: 20, y: 30, w: 30, h: 10 });
  });

  it("forma livre usa o envelope dos pontos; elipse vira 32 pontos moldáveis", () => {
    const free = { x: 0, y: 0, redaction_shape: "free", points: [{ x: 10, y: 5 }, { x: 40, y: 8 }, { x: 25, y: 30 }] };
    expect(redactionRect(free as never)).toEqual({ x: 10, y: 5, w: 30, h: 25 });
    const pts = toMoldablePoints({ x: 0, y: 0, width: 100, height: 50, redaction_shape: "ellipse" } as never);
    expect(pts).toHaveLength(32);
    expect(Math.max(...pts.map((p) => p.x))).toBeCloseTo(100, 5);
  });
});
