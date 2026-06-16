/**
 * Tests for the `.sicroimage` serializer (MVP 7).
 */

import { describe, expect, it } from "vitest";
import { coerceSicroImage, serializeSicroImage, CURRENT_SCHEMA_VERSION } from "./index";

const VALID_MIN = {
  schema_version: "0.1",
  image_analysis_id: "aaaaaaaa-aaaa-4aaa-aaaa-aaaaaaaaaaaa",
  occurrence_id: "bbbbbbbb-bbbb-4bbb-bbbb-bbbbbbbbbbbb",
  title: "Análise de teste",
};

describe("coerceSicroImage", () => {
  it("fills source/canvas/adjustments/layers with defaults", () => {
    const d = coerceSicroImage(VALID_MIN);
    expect(d.canvas.zoom).toBe(1);
    expect(d.view_adjustments.gamma).toBe(1);
    expect(d.layers.length).toBeGreaterThanOrEqual(2);
    expect(d.annotations).toEqual([]);
    expect(d.scale).toBeNull();
  });

  it("preserves a populated envelope through serialize → coerce round-trip", () => {
    const d = coerceSicroImage({
      ...VALID_MIN,
      annotations: [
        {
          id: "x1",
          layer_id: "layer_annotations",
          kind: "arrow",
          x: 10,
          y: 10,
          x2: 100,
          y2: 100,
          created_at: "2026-05-25T13:00:00Z",
        },
      ],
      view_adjustments: {
        brightness: 12,
        contrast: -8,
        gamma: 1.1,
        saturation: 5,
        grayscale: true,
        invert: false,
      },
      scale: {
        px_per_unit: 47.5,
        unit: "m",
        calibrated_by: [{ x: 0, y: 0 }, { x: 100, y: 0 }],
        calibration_real_distance: 2.1,
        created_at: "2026-05-25T13:00:00Z",
      },
    });
    const stamped = serializeSicroImage(d);
    expect(stamped.schema_version).toBe(CURRENT_SCHEMA_VERSION);
    const reparsed = coerceSicroImage(JSON.parse(JSON.stringify(stamped)));
    expect(reparsed.annotations[0]?.kind).toBe("arrow");
    expect(reparsed.view_adjustments.grayscale).toBe(true);
    expect(reparsed.scale?.px_per_unit).toBe(47.5);
  });

  it("throws on missing image_analysis_id", () => {
    expect(() => coerceSicroImage({ ...VALID_MIN, image_analysis_id: "" })).toThrow();
  });

  it("rejects non-object input", () => {
    expect(() => coerceSicroImage(null)).toThrow();
    expect(() => coerceSicroImage(42)).toThrow();
  });

  it("normalises unknown unit to 'm' in scale", () => {
    const d = coerceSicroImage({
      ...VALID_MIN,
      scale: {
        px_per_unit: 10,
        unit: "leagues",
        calibrated_by: [],
        calibration_real_distance: 1,
        created_at: "x",
      },
    });
    expect(d.scale?.unit).toBe("m");
  });

  it("drops invalid scale silently", () => {
    const d = coerceSicroImage({
      ...VALID_MIN,
      scale: { px_per_unit: 0 },
    });
    expect(d.scale).toBeNull();
  });

  // W20 — seleção (estilo Photoshop)
  it("round-trips a rect selection (inverted)", () => {
    const d = coerceSicroImage({
      ...VALID_MIN,
      selection: {
        id: "sel1",
        kind: "rect",
        x: 10,
        y: 20,
        width: 100,
        height: 60,
        inverted: true,
        created_at: "2026-06-04T10:00:00Z",
      },
    });
    const reparsed = coerceSicroImage(
      JSON.parse(JSON.stringify(serializeSicroImage(d))),
    );
    expect(reparsed.selection?.kind).toBe("rect");
    expect(reparsed.selection?.width).toBe(100);
    expect(reparsed.selection?.inverted).toBe(true);
  });

  it("round-trips a polygon selection (3+ points)", () => {
    const d = coerceSicroImage({
      ...VALID_MIN,
      selection: {
        id: "sel2",
        kind: "polygon",
        points: [
          { x: 0, y: 0 },
          { x: 50, y: 0 },
          { x: 25, y: 40 },
        ],
        source_tool: "select_lasso",
        inverted: false,
        created_at: "2026-06-04T10:00:00Z",
      },
    });
    expect(d.selection?.kind).toBe("polygon");
    expect(d.selection?.points?.length).toBe(3);
    expect(d.selection?.source_tool).toBe("select_lasso");
  });

  it("drops invalid selections (bad kind / degenerate geometry)", () => {
    expect(coerceSicroImage({ ...VALID_MIN, selection: { kind: "blob" } }).selection).toBeNull();
    expect(
      coerceSicroImage({
        ...VALID_MIN,
        selection: { kind: "rect", x: 0, y: 0, width: 0, height: 10, inverted: false },
      }).selection,
    ).toBeNull();
    expect(
      coerceSicroImage({
        ...VALID_MIN,
        selection: { kind: "polygon", points: [{ x: 0, y: 0 }, { x: 1, y: 1 }] },
      }).selection,
    ).toBeNull();
  });

  it("old docs without selection coerce to null", () => {
    const d = coerceSicroImage(VALID_MIN);
    expect(d.selection ?? null).toBeNull();
  });

  // W20 (S2) — escopo + máscara congelada por operação.
  it("round-trips a selection-scoped processing op with frozen mask", () => {
    const d = coerceSicroImage({
      ...VALID_MIN,
      processing_stack: [
        {
          id: "op1",
          kind: "clahe",
          enabled: true,
          params: { tile_size: 8, clip_limit: 2 },
          scope: "selection",
          mask: {
            id: "m1",
            kind: "rect",
            x: 5,
            y: 5,
            width: 40,
            height: 30,
            inverted: false,
            created_at: "2026-06-04T10:00:00Z",
          },
          created_at: "2026-06-04T10:00:00Z",
        },
      ],
    });
    const reparsed = coerceSicroImage(
      JSON.parse(JSON.stringify(serializeSicroImage(d))),
    );
    const op = reparsed.processing_stack[0];
    expect(op?.scope).toBe("selection");
    expect(op?.mask?.kind).toBe("rect");
    expect(op?.mask?.width).toBe(40);
  });

  // W20 (S3) — camada de pixels (recorte de seleção).
  it("round-trips a pixels layer (offset/dims/bitmap/source)", () => {
    const d = coerceSicroImage({
      ...VALID_MIN,
      layers: [
        {
          id: "layer_base",
          name: "Imagem base",
          kind: "image_base",
          visible: true,
          locked: true,
          opacity: 1,
        },
        {
          id: "pl-1",
          name: "Camada (original)",
          kind: "pixels",
          visible: true,
          locked: false,
          opacity: 1,
          offset_x: 12,
          offset_y: 34,
          width: 200,
          height: 150,
          bitmap_relative_path: "imagens/camadas/pl-1.png",
          pixel_source: "original",
          hash_sha256: "abc",
          created_at: "2026-06-04T10:00:00Z",
        },
      ],
    });
    const reparsed = coerceSicroImage(
      JSON.parse(JSON.stringify(serializeSicroImage(d))),
    );
    const px = reparsed.layers.find((l) => l.kind === "pixels");
    expect(px).toBeTruthy();
    expect(px?.offset_x).toBe(12);
    expect(px?.width).toBe(200);
    expect(px?.bitmap_relative_path).toBe("imagens/camadas/pl-1.png");
    expect(px?.pixel_source).toBe("original");
  });

  it("drops a pixels layer without bitmap/dims (invalid)", () => {
    const d = coerceSicroImage({
      ...VALID_MIN,
      layers: [
        {
          id: "pl-bad",
          name: "Camada quebrada",
          kind: "pixels",
          visible: true,
          locked: false,
          opacity: 1,
          // sem bitmap_relative_path / width / height
        },
      ],
    });
    // sem camadas válidas → cai nos defaults (base + anotações), sem pixels.
    expect(d.layers.some((l) => l.kind === "pixels")).toBe(false);
    expect(d.layers.length).toBeGreaterThanOrEqual(2);
  });

  it("defaults op scope to 'image' and drops mask when scope absent", () => {
    const d = coerceSicroImage({
      ...VALID_MIN,
      processing_stack: [
        {
          id: "op2",
          kind: "edge_sobel",
          enabled: true,
          params: { strength: 1 },
          // sem scope/mask (op antigo)
          created_at: "2026-06-04T10:00:00Z",
        },
      ],
    });
    const op = d.processing_stack[0];
    expect(op?.scope).toBe("image");
    expect(op?.mask ?? null).toBeNull();
  });
});
