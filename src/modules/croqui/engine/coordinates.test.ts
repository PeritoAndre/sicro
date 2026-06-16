/**
 * Coordinate parsing — unit tests (MVP 10).
 */

import { describe, expect, it } from "vitest";
import {
  bboxFromCenterRadius,
  coordinateParseErrorMessage,
  estimatePxPerMeter,
  formatCoordinates,
  parseCoordinates,
} from "./coordinates";

describe("parseCoordinates", () => {
  it("parses the canonical decimal-comma form", () => {
    const r = parseCoordinates("-0.0345, -51.0694");
    expect(r.ok).toBe(true);
    expect(r.value).not.toBeNull();
    expect(r.value!.lat).toBeCloseTo(-0.0345);
    expect(r.value!.lon).toBeCloseTo(-51.0694);
  });

  it("parses space-separated values", () => {
    const r = parseCoordinates("-0.0345 -51.0694");
    expect(r.ok).toBe(true);
    expect(r.value!.lat).toBeCloseTo(-0.0345);
  });

  it("parses Brazilian comma decimals (one comma per token)", () => {
    const r = parseCoordinates("0,0345 51,0694");
    expect(r.ok).toBe(true);
    expect(r.value!.lat).toBeCloseTo(0.0345);
    expect(r.value!.lon).toBeCloseTo(51.0694);
  });

  it("respects N/S/E/W hemisphere letters", () => {
    const r = parseCoordinates("0.0345 S 51.0694 W");
    expect(r.ok).toBe(true);
    expect(r.value!.lat).toBeCloseTo(-0.0345);
    expect(r.value!.lon).toBeCloseTo(-51.0694);
  });

  it("accepts parentheses and labels", () => {
    const r = parseCoordinates("(lat: -0.0345, lon: -51.0694)");
    expect(r.ok).toBe(true);
    expect(r.value!.lat).toBeCloseTo(-0.0345);
    expect(r.value!.lon).toBeCloseTo(-51.0694);
  });

  it("rejects empty input", () => {
    const r = parseCoordinates("");
    expect(r.ok).toBe(false);
    expect(r.error).toBe("empty");
  });

  it("rejects single number with missing_separator", () => {
    const r = parseCoordinates("-0.0345");
    expect(r.ok).toBe(false);
    expect(r.error).toBe("missing_separator");
  });

  it("rejects out-of-range latitude", () => {
    const r = parseCoordinates("95, 50");
    expect(r.ok).toBe(false);
    expect(r.error).toBe("lat_out_of_range");
  });

  it("rejects out-of-range longitude", () => {
    const r = parseCoordinates("0, 200");
    expect(r.ok).toBe(false);
    expect(r.error).toBe("lon_out_of_range");
  });

  it("provides a human-readable error message for every error kind", () => {
    expect(coordinateParseErrorMessage("empty")).toMatch(/coordenadas/i);
    expect(coordinateParseErrorMessage("missing_separator")).toMatch(/vírgula/i);
    expect(coordinateParseErrorMessage("not_a_number")).toMatch(/número/i);
    expect(coordinateParseErrorMessage("lat_out_of_range")).toMatch(/latitude/i);
    expect(coordinateParseErrorMessage("lon_out_of_range")).toMatch(/longitude/i);
  });
});

describe("formatCoordinates", () => {
  it("round-trips through parseCoordinates", () => {
    const orig = { lat: -0.034123, lon: -51.069456 };
    const formatted = formatCoordinates(orig);
    const re = parseCoordinates(formatted);
    expect(re.ok).toBe(true);
    expect(re.value!.lat).toBeCloseTo(orig.lat, 5);
    expect(re.value!.lon).toBeCloseTo(orig.lon, 5);
  });
});

describe("bboxFromCenterRadius", () => {
  it("returns a symmetric bbox at the equator", () => {
    const view = bboxFromCenterRadius(
      { lat: 0, lon: 0 },
      1000,
      800,
      600,
    );
    // At the equator, latitude span ≈ longitude span (since cos(0) = 1).
    const latSpan = view.max_lat - view.min_lat;
    const lonSpan = view.max_lon - view.min_lon;
    expect(latSpan).toBeCloseTo(lonSpan, 5);
    // 1 km / earth radius * 180/π * 2 ≈ 0.018°
    expect(latSpan).toBeCloseTo(0.0180, 3);
  });

  it("widens longitude span at higher latitudes (cos correction)", () => {
    const equator = bboxFromCenterRadius({ lat: 0, lon: 0 }, 1000, 800, 600);
    const brasilia = bboxFromCenterRadius({ lat: -15.78, lon: -47.92 }, 1000, 800, 600);
    const eqLonSpan = equator.max_lon - equator.min_lon;
    const bsbLonSpan = brasilia.max_lon - brasilia.min_lon;
    expect(bsbLonSpan).toBeGreaterThan(eqLonSpan);
  });

  it("keeps lat span the same regardless of longitude", () => {
    const a = bboxFromCenterRadius({ lat: -15, lon: 0 }, 500, 100, 100);
    const b = bboxFromCenterRadius({ lat: -15, lon: 100 }, 500, 100, 100);
    expect(a.max_lat - a.min_lat).toBeCloseTo(b.max_lat - b.min_lat, 6);
  });

  it("propagates the canvas width/height", () => {
    const view = bboxFromCenterRadius({ lat: 0, lon: 0 }, 100, 1200, 900);
    expect(view.width_px).toBe(1200);
    expect(view.height_px).toBe(900);
  });
});

describe("estimatePxPerMeter", () => {
  it("returns positive scale for a typical city-block viewport", () => {
    const view = bboxFromCenterRadius({ lat: 0, lon: 0 }, 200, 800, 600);
    const px = estimatePxPerMeter(view);
    expect(px).not.toBeNull();
    // 200 m radius → bbox is ~400 m wide → 800 px / 400 m ≈ 2 px/m
    expect(px!).toBeCloseTo(2, 0);
  });

  it("returns null for a degenerate viewport", () => {
    expect(
      estimatePxPerMeter({
        min_lat: 0,
        max_lat: 0,
        min_lon: 0,
        max_lon: 0,
        width_px: 0,
        height_px: 0,
      }),
    ).toBeNull();
  });
});
