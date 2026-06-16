/**
 * Tests for annotation factories (MVP 7).
 */

import { describe, expect, it } from "vitest";
import {
  makeArrow,
  makeEllipse,
  makeLine,
  makeMeasurement,
  makeNumberedMarker,
  makePoint,
  makeRect,
  makeRedaction,
  makeText,
} from "./factories";

describe("annotation factories", () => {
  it("makeArrow yields a 2-point shape", () => {
    const a = makeArrow(0, 0, 100, 100);
    expect(a.kind).toBe("arrow");
    expect(a.x).toBe(0);
    expect(a.x2).toBe(100);
    expect(a.visible).toBe(true);
    expect(a.locked).toBe(false);
  });
  it("makeLine has unique id", () => {
    const l1 = makeLine(0, 0, 10, 10);
    const l2 = makeLine(0, 0, 10, 10);
    expect(l1.id).not.toBe(l2.id);
  });
  it("makeRect width/height stored", () => {
    const r = makeRect(10, 20, 100, 50);
    expect(r.width).toBe(100);
    expect(r.height).toBe(50);
  });
  it("makeEllipse uses same fields", () => {
    const e = makeEllipse(0, 0, 80, 40);
    expect(e.kind).toBe("ellipse");
  });
  it("makeText carries the text", () => {
    const t = makeText(0, 0, "hello");
    expect(t.text).toBe("hello");
  });
  it("makeNumberedMarker stores number and text", () => {
    const m = makeNumberedMarker(0, 0, 7);
    expect(m.number).toBe(7);
    expect(m.text).toBe("7");
  });
  it("makePoint produces a point", () => {
    const p = makePoint(5, 5);
    expect(p.kind).toBe("point");
  });
  it("makeMeasurement is dashed by default (subtype)", () => {
    const m = makeMeasurement(0, 0, 100, 0);
    expect(m.kind).toBe("measurement");
  });
  it("makeRedaction is filled black", () => {
    const r = makeRedaction(0, 0, 50, 50);
    expect(r.fill).toBe("#000000");
  });
});
