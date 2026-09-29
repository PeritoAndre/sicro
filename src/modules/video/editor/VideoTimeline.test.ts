import { describe, expect, it } from "vitest";

import { buildTicks } from "./VideoTimeline";

describe("buildTicks", () => {
  it("vídeo inteiro: rótulos espaçados (≥ 80 px) e riscos menores (≥ 8 px)", () => {
    const t = buildTicks(0, 38, 800, 25);
    expect(t.major.length).toBeGreaterThan(2);
    expect(t.major.length).toBeLessThanOrEqual(10);
    expect(t.major[0]?.label).toBe("0s");
    const all = [...t.minor, ...t.major.map((m) => m.t)].sort((a, b) => a - b);
    const minGapPx = Math.min(...all.slice(1).map((x, i) => ((x - all[i]!) / 38) * 800));
    expect(minGapPx).toBeGreaterThanOrEqual(7.9);
  });

  it("zoom máximo (12 quadros a 25 fps): um risco por quadro", () => {
    const t = buildTicks(10, 10.48, 800, 25);
    const all = [...t.minor, ...t.major.map((m) => m.t)].sort((a, b) => a - b);
    expect(all.length).toBe(13); // 10.00, 10.04, …, 10.48
    expect(all[1]! - all[0]!).toBeCloseTo(0.04);
    expect(t.major.some((m) => m.label === "10.00s" || m.label === "10.0s")).toBe(true);
  });

  it("rótulo com minutos e janela inválida", () => {
    const t = buildTicks(60, 180, 800, 30);
    expect(t.major.some((m) => m.label === "2:00")).toBe(true);
    expect(buildTicks(5, 5, 800, 30)).toEqual({ minor: [], major: [] });
  });
});
