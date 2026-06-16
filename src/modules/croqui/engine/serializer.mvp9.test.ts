/**
 * Migration & schema tests for MVP 9 — bumped CURRENT_SCHEMA_VERSION to
 * "0.3", added view_settings / export_settings / stamp_metadata sections
 * + new vehicle / marker / line subtypes + mobiliario_urbano category.
 *
 * Confirms backward compatibility with v0.1 (Spike E) and v0.2 (MVP 6).
 */

import { describe, expect, it } from "vitest";
import {
  CURRENT_SCHEMA_VERSION,
  coerceCroquiDoc,
  serializeCroquiDoc,
} from "./index";

const MIN = {
  croqui_id: "11111111-1111-4111-8111-111111111111",
  occurrence_id: "22222222-2222-4222-8222-222222222222",
  title: "Croqui MVP 9",
  created_at: "2026-05-25T13:00:00.000Z",
  updated_at: "2026-05-25T13:00:00.000Z",
};

describe("MVP 9 — schema 0.3", () => {
  it("CURRENT_SCHEMA_VERSION is '0.3'", () => {
    expect(CURRENT_SCHEMA_VERSION).toBe("0.3");
  });

  it("loads a v0.1 envelope (Spike E) and provides MVP 9 defaults", () => {
    const d = coerceCroquiDoc({ ...MIN, schema_version: "0.1" });
    // novos campos aditivos saem com defaults sãos
    expect(d.view_settings?.show_grid).toBe(true);
    expect(d.view_settings?.grid_size).toBe(50);
    expect(d.view_settings?.snap_to_grid).toBe(false);
    expect(d.export_settings?.with_stamp).toBe(true);
    expect(d.export_settings?.default_kind).toBe("tecnico");
    expect(d.stamp_metadata).toEqual({
      bo: null,
      protocolo: null,
      tipo_pericia: null,
      municipio: null,
      perito: null,
      custom_note: null,
    });
  });

  it("loads a v0.2 envelope (MVP 6) without losing existing fields", () => {
    const d = coerceCroquiDoc({
      ...MIN,
      schema_version: "0.2",
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
          body_type: "sedan",
          category: "veiculos",
        },
      ],
      scale: { px_per_m: 33 },
    });
    expect(d.objects).toHaveLength(1);
    expect(d.objects[0]?.kind).toBe("vehicle");
    expect(d.scale?.px_per_m).toBe(33);
    expect(d.view_settings).toBeDefined();
  });

  it("preserves view_settings / export_settings provided by a v0.3 envelope", () => {
    const d = coerceCroquiDoc({
      ...MIN,
      schema_version: "0.3",
      view_settings: {
        show_grid: false,
        grid_size: 80,
        snap_to_grid: true,
        show_rulers: false,
        show_labels: false,
        show_measurements: false,
      },
      export_settings: {
        with_stamp: false,
        with_background: false,
        with_legend: true,
        default_kind: "limpo",
      },
      stamp_metadata: {
        bo: "12/2026",
        municipio: "Macapá",
      },
    });
    expect(d.view_settings?.show_grid).toBe(false);
    expect(d.view_settings?.grid_size).toBe(80);
    expect(d.view_settings?.snap_to_grid).toBe(true);
    expect(d.export_settings?.default_kind).toBe("limpo");
    expect(d.stamp_metadata?.bo).toBe("12/2026");
    expect(d.stamp_metadata?.municipio).toBe("Macapá");
    expect(d.stamp_metadata?.tipo_pericia).toBeNull();
  });

  it("round-trips a populated v0.3 envelope through serialize → coerce", () => {
    const base = coerceCroquiDoc({
      ...MIN,
      schema_version: "0.3",
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
          body_type: "carreta", // MVP 9 subtype
        },
        {
          id: "p1",
          layer_id: "layer_objects",
          kind: "marker",
          subtype: "semaforo", // MVP 9 mobiliário
          x: 0,
          y: 0,
          size: 22,
        },
        {
          id: "l1",
          layer_id: "layer_objects",
          kind: "line",
          subtype: "canteiro", // MVP 9 línea
          points: [0, 0, 100, 0],
          stroke_width: 8,
        },
      ],
    });
    const stamped = serializeCroquiDoc(base);
    const reparsed = coerceCroquiDoc(JSON.parse(JSON.stringify(stamped)));
    expect(reparsed.objects[0]).toMatchObject({ body_type: "carreta" });
    expect(reparsed.objects[1]).toMatchObject({
      subtype: "semaforo",
      category: "mobiliario_urbano",
    });
    expect(reparsed.objects[2]).toMatchObject({ subtype: "canteiro" });
  });

  it("inferCategory routes new marker subtypes correctly via reparse", () => {
    const d = coerceCroquiDoc({
      ...MIN,
      schema_version: "0.2",
      objects: [
        // Sem `category` declarada — deve ser inferida.
        {
          id: "x1",
          layer_id: "layer_objects",
          kind: "marker",
          subtype: "skid_curve",
          x: 0,
          y: 0,
          size: 70,
        },
        {
          id: "y1",
          layer_id: "layer_objects",
          kind: "marker",
          subtype: "poste",
          x: 0,
          y: 0,
          size: 14,
        },
      ],
    });
    expect(d.objects[0]?.category).toBe("vestigios");
    expect(d.objects[1]?.category).toBe("mobiliario_urbano");
  });
});
