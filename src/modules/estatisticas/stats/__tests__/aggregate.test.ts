import { describe, it, expect } from "vitest";
import {
  aggregate,
  bucketTime,
  countBy,
  histogramBins,
  numericSummary,
} from "../aggregate";
import type { StatsRawData } from "../model";
import type { Occurrence } from "@domain/occurrence";
import type { VideoSpeedCalculation } from "@domain/video_speed";
import type { VideoDistanceMeasurement } from "@domain/video_distance";

describe("countBy", () => {
  it("agrupa, ordena desc e mapeia nulos para —", () => {
    const out = countBy(
      [{ t: "a" }, { t: "a" }, { t: "b" }, { t: null }],
      (x) => x.t,
    );
    expect(out[0]).toEqual({ label: "a", value: 2 });
    expect(out.find((s) => s.label === "—")?.value).toBe(1);
  });
});

describe("numericSummary", () => {
  it("retorna null para vazio", () => {
    expect(numericSummary([])).toBeNull();
  });

  it("computa min/max/mean/median/sum", () => {
    const s = numericSummary([2, 4, 6, 8])!;
    expect(s.count).toBe(4);
    expect(s.min).toBe(2);
    expect(s.max).toBe(8);
    expect(s.sum).toBe(20);
    expect(s.mean).toBe(5);
    expect(s.median).toBeCloseTo(5, 6);
  });

  it("lida com valor único", () => {
    const s = numericSummary([42])!;
    expect(s.count).toBe(1);
    expect(s.min).toBe(42);
    expect(s.max).toBe(42);
    expect(s.median).toBe(42);
    expect(s.stdev).toBe(0);
  });

  it("ignora não-finitos", () => {
    const s = numericSummary([1, NaN, 3, Infinity])!;
    expect(s.count).toBe(2);
    expect(s.sum).toBe(4);
  });
});

describe("histogramBins", () => {
  it("vazio → []", () => {
    expect(histogramBins([])).toEqual([]);
  });

  it("valor único → uma faixa com todos os pontos", () => {
    const bins = histogramBins([5, 5, 5]);
    expect(bins).toHaveLength(1);
    expect(bins[0]!.count).toBe(3);
  });

  it("conserva a contagem total entre as faixas", () => {
    const values = [1, 2, 3, 4, 5, 6, 7, 8, 9, 10];
    const bins = histogramBins(values);
    const total = bins.reduce((s, b) => s + b.count, 0);
    expect(total).toBe(values.length);
  });
});

describe("bucketTime", () => {
  it("vazio → []", () => {
    expect(bucketTime([])).toEqual([]);
    expect(bucketTime([null, undefined])).toEqual([]);
  });

  it("agrupa por dia em janelas curtas e ordena", () => {
    const out = bucketTime([
      "2026-05-31T10:00:00Z",
      "2026-05-31T18:00:00Z",
      "2026-06-01T09:00:00Z",
    ]);
    expect(out).toHaveLength(2);
    expect(out[0]!.value).toBe(2);
    expect(out[1]!.value).toBe(1);
    // ordem crescente por key
    expect(out[0]!.key < out[1]!.key).toBe(true);
  });
});

function emptyRaw(): StatsRawData {
  return {
    occurrence: null,
    dossie: null,
    checklist: [],
    entities: [],
    traces: [],
    measurements: [],
    notes: [],
    timeline: [],
    integrity: null,
    laudos: [],
    videos: [],
    speeds: [],
    distances: [],
    imageAnalysesCount: 0,
    croquisCount: 0,
  };
}

describe("aggregate", () => {
  it("sem ocorrência → hasData=false, mas não quebra", () => {
    const m = aggregate(emptyRaw());
    expect(m.hasData).toBe(false);
    expect(m.headline.length).toBeGreaterThan(0);
    expect(m.evidenceByKind).toEqual([]);
    expect(m.speedsSummary).toBeNull();
  });

  it("agrega velocidades e distâncias", () => {
    const raw = emptyRaw();
    raw.occurrence = {
      id: "occ-1",
      numero_bo: "7466/2026",
      tipo_pericia: "Sinistro de trânsito",
      municipio: "Macapá",
      status: "em_andamento",
      peritos: ["André"],
    } as unknown as Occurrence;
    raw.speeds = [
      { id: "s1", velocity_kmh: 60, ci_low: 55, ci_high: 65, created_at: "2026-05-31T12:00:00Z" },
      { id: "s2", velocity_kmh: 80, ci_low: null, ci_high: null, created_at: "2026-05-31T13:00:00Z" },
    ] as unknown as VideoSpeedCalculation[];
    raw.distances = [
      { id: "d1", distance_m: 5, created_at: "2026-05-31T12:30:00Z" },
      { id: "d2", distance_m: 9, created_at: "2026-05-31T12:35:00Z" },
    ] as unknown as VideoDistanceMeasurement[];

    const m = aggregate(raw);
    expect(m.hasData).toBe(true);
    expect(m.occurrence?.numero_bo).toBe("7466/2026");
    expect(m.speedsSummary?.count).toBe(2);
    expect(m.speedsSummary?.mean).toBe(70);
    expect(m.speeds).toHaveLength(2);
    expect(m.distancesSummary?.count).toBe(2);
    expect(m.distancesSummary?.mean).toBe(7);
    expect(m.counts.velocidades).toBe(2);
    expect(m.counts.distancias).toBe(2);
  });
});
