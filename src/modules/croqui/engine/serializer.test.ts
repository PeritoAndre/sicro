/**
 * Unit tests for the `.sicrocroqui` serializer — Spike E.
 *
 * Confirms:
 *   - the coercer hardens missing/defaulted fields;
 *   - round-trip (coerce → JSON.stringify → JSON.parse → coerce) is stable;
 *   - shape that can't possibly be a croqui throws clearly.
 */

import { describe, expect, it } from "vitest";
import {
  CURRENT_SCHEMA_VERSION,
  coerceCroquiDoc,
  serializeCroquiDoc,
} from "./index";

const VALID_MINIMUM = {
  schema_version: "0.1",
  croqui_id: "11111111-1111-4111-8111-111111111111",
  occurrence_id: "22222222-2222-4222-8222-222222222222",
  title: "Croqui de teste",
  created_at: "2026-05-25T13:00:00.000Z",
  updated_at: "2026-05-25T13:00:00.000Z",
};

describe("coerceCroquiDoc", () => {
  it("fills canvas / layers / objects with defaults when missing", () => {
    const d = coerceCroquiDoc(VALID_MINIMUM);
    expect(d.canvas.width_px).toBe(1600);
    expect(d.canvas.height_px).toBe(1000);
    expect(d.canvas.grid?.enabled).toBe(true);
    expect(d.layers.length).toBe(2);
    expect(d.layers[0]?.kind).toBe("background");
    expect(d.layers[1]?.kind).toBe("objects");
    expect(d.objects).toEqual([]);
    expect(d.scale).toBeNull();
    expect(d.background_image).toBeNull();
  });

  it("preserves canvas customisation", () => {
    const d = coerceCroquiDoc({
      ...VALID_MINIMUM,
      canvas: { width_px: 2000, height_px: 1200, background_color: "#000" },
    });
    expect(d.canvas.width_px).toBe(2000);
    expect(d.canvas.background_color).toBe("#000");
  });

  it("keeps a valid scale", () => {
    const d = coerceCroquiDoc({
      ...VALID_MINIMUM,
      scale: {
        px_per_m: 47.5,
        definition: { p1: { x: 0, y: 0 }, p2: { x: 100, y: 0 }, real_distance_m: 2.1 },
      },
    });
    expect(d.scale?.px_per_m).toBe(47.5);
    expect(d.scale?.definition?.real_distance_m).toBe(2.1);
  });

  it("drops invalid scale silently (graceful)", () => {
    const d = coerceCroquiDoc({
      ...VALID_MINIMUM,
      scale: { px_per_m: 0 },
    });
    expect(d.scale).toBeNull();
  });

  it("throws when croqui_id is missing", () => {
    const broken = { ...VALID_MINIMUM, croqui_id: "" };
    expect(() => coerceCroquiDoc(broken)).toThrow();
  });

  it("throws when input is not an object", () => {
    expect(() => coerceCroquiDoc(null)).toThrow();
    expect(() => coerceCroquiDoc("not a croqui")).toThrow();
    expect(() => coerceCroquiDoc(42)).toThrow();
  });

  it("preserves objects array as-is", () => {
    const d = coerceCroquiDoc({
      ...VALID_MINIMUM,
      objects: [
        {
          id: "v1",
          layer_id: "layer_objects",
          kind: "vehicle",
          x: 10,
          y: 20,
          width: 80,
          height: 40,
          rotation: 0,
          label: "V1",
        },
      ],
    });
    expect(d.objects).toHaveLength(1);
    expect(d.objects[0]?.kind).toBe("vehicle");
  });

  // MVP 6 — backward compatibility with v0.1 envelopes.
  it("loads a v0.1 envelope without crashing and assigns categories", () => {
    const d = coerceCroquiDoc({
      schema_version: "0.1",
      croqui_id: "11111111-1111-4111-8111-111111111111",
      occurrence_id: "22222222-2222-4222-8222-222222222222",
      title: "Croqui legado",
      created_at: "2026-05-25T13:00:00.000Z",
      updated_at: "2026-05-25T13:00:00.000Z",
      objects: [
        {
          id: "x1",
          layer_id: "layer_objects",
          kind: "marker",
          subtype: "collision_x",
          x: 100,
          y: 50,
          size: 24,
        },
        {
          id: "v1",
          layer_id: "layer_objects",
          kind: "vehicle",
          x: 200,
          y: 200,
          width: 80,
          height: 40,
          rotation: 0,
        },
        {
          id: "r1",
          layer_id: "layer_objects",
          kind: "line",
          subtype: "r1",
          points: [0, 0, 100, 0],
          stroke_width: 4,
        },
      ],
    });
    expect(d.objects).toHaveLength(3);
    // Aggregator/UI relies on `category` — v0.1 objects must surface a
    // defaulted value.
    const cats = d.objects.map((o) => o.category);
    expect(cats).toContain("vestigios");
    expect(cats).toContain("veiculos");
    expect(cats).toContain("referenciais");
  });
});

describe("serializeCroquiDoc", () => {
  it("stamps schema_version and updated_at", () => {
    const d = coerceCroquiDoc(VALID_MINIMUM);
    const before = d.updated_at;
    const after = serializeCroquiDoc(d);
    expect(after.schema_version).toBe(CURRENT_SCHEMA_VERSION);
    expect(after.updated_at).not.toBe(before);
  });

  it("round-trips through JSON without losing data", () => {
    const d = coerceCroquiDoc({
      ...VALID_MINIMUM,
      objects: [
        {
          id: "x1",
          layer_id: "layer_objects",
          kind: "marker",
          subtype: "collision_x",
          x: 100,
          y: 50,
          size: 24,
        },
      ],
      scale: { px_per_m: 30 },
    });
    const stamped = serializeCroquiDoc(d);
    const text = JSON.stringify(stamped);
    const reparsed = coerceCroquiDoc(JSON.parse(text));
    expect(reparsed.objects).toEqual(stamped.objects);
    expect(reparsed.scale?.px_per_m).toBe(30);
    expect(reparsed.croqui_id).toBe(VALID_MINIMUM.croqui_id);
  });

  // MVP 9 Round 5 — additive background fields.
  it("defaults background.rotation to 0 when the field is missing (legacy doc)", () => {
    const d = coerceCroquiDoc({
      ...VALID_MINIMUM,
      background_image: {
        source_path: "croquis/backgrounds/foo.png",
        x: 10,
        y: 20,
        width: 800,
        height: 600,
        opacity: 0.6,
        locked: true,
      },
    });
    expect(d.background_image).not.toBeNull();
    expect(d.background_image!.rotation).toBe(0);
    expect(d.background_image!.x).toBe(10);
    expect(d.background_image!.opacity).toBe(0.6);
  });

  it("preserves rotation / sidecar_path / original_path round-trip", () => {
    const d = coerceCroquiDoc({
      ...VALID_MINIMUM,
      background_image: {
        source_path: "croquis/backgrounds/drone_corrigido_x.png",
        x: 100,
        y: 50,
        width: 1200,
        height: 800,
        opacity: 0.8,
        locked: false,
        rotation: 45,
        sidecar_path: "croquis/backgrounds/drone_corrigido_x.sidecar.json",
        original_path: "C:/Users/perit/drone_001.JPG",
      },
    });
    const text = JSON.stringify(serializeCroquiDoc(d));
    const reparsed = coerceCroquiDoc(JSON.parse(text));
    expect(reparsed.background_image?.rotation).toBe(45);
    expect(reparsed.background_image?.sidecar_path).toBe(
      "croquis/backgrounds/drone_corrigido_x.sidecar.json",
    );
    expect(reparsed.background_image?.original_path).toBe(
      "C:/Users/perit/drone_001.JPG",
    );
    expect(reparsed.background_image?.locked).toBe(false);
  });

  // Fase S clean cut — Road v1/v2 (`kind: "road"`, `kind: "roundabout"`)
  // são silenciosamente descartados pelo coercer. Croquis pré-S perdem
  // essas primitivas; o único motor de via passa a ser Python Parity
  // Engine (`kind: "road_parity"` / `kind: "roundabout_parity"`).
  it("silently drops legacy v1 road objects (clean cut)", () => {
    const d = coerceCroquiDoc({
      ...VALID_MINIMUM,
      objects: [
        {
          id: "r1",
          layer_id: "layer_objects",
          kind: "road",
          subtype: "spline",
          points: [0, 0, 100, 0],
          width: 80,
        },
        {
          id: "v1",
          layer_id: "layer_objects",
          kind: "vehicle",
          x: 10,
          y: 20,
          width: 80,
          height: 40,
          rotation: 0,
        },
      ],
    });
    // O `road` é descartado; o `vehicle` sobrevive.
    expect(d.objects).toHaveLength(1);
    expect(d.objects[0]?.kind).toBe("vehicle");
  });

  it("silently drops legacy v1 roundabout objects (clean cut)", () => {
    const d = coerceCroquiDoc({
      ...VALID_MINIMUM,
      objects: [
        {
          id: "rb1",
          layer_id: "layer_objects",
          kind: "roundabout",
          cx: 100,
          cy: 100,
          r: 80,
          width: 14,
        },
      ],
    });
    expect(d.objects).toHaveLength(0);
  });

  it("accepts parity road objects through the coercer", () => {
    const d = coerceCroquiDoc({
      ...VALID_MINIMUM,
      objects: [
        {
          id: "rdp1",
          layer_id: "layer_objects",
          kind: "road_parity",
          engine: "parity",
          ax: 0,
          ay: 0,
          bx: 100,
          by: 0,
          cx1: 33,
          cy1: 0,
          cx2: 66,
          cy2: 0,
          largura_m: 7.0,
          superficie: "asfalto",
          mao_dupla: true,
          marcacao: "amarela",
          visible: true,
          locked: false,
          label: null,
          metadata_json: null,
          category: "vias",
        },
      ],
    });
    expect(d.objects).toHaveLength(1);
    expect(d.objects[0]?.kind).toBe("road_parity");
  });

  it("drops invalid background_image (no source_path) without crashing", () => {
    const d = coerceCroquiDoc({
      ...VALID_MINIMUM,
      background_image: {
        // no source_path
        x: 10,
        y: 10,
        width: 100,
        height: 100,
        opacity: 1,
      },
    });
    expect(d.background_image).toBeNull();
  });
});
