/**
 * Python Parity Engine — testes do OSM Adapter (Fase H.5).
 *
 * Cobre:
 *   - largura_m por classe OSM (primary 10.5, secondary 8.5, tertiary 7.5,
 *     residential 6.0, service 4.5);
 *   - marcação por classe (amarela em arteriais, branca em residential/service);
 *   - oneway → mao_dupla=false + marcacao=nenhuma;
 *   - non-vehicle highways ignoradas;
 *   - polyline → Bezier 4-point preserva endpoints;
 *   - detecção de rotatória via `junction=roundabout`;
 *   - fit uniforme + recentre ao centro do canvas;
 *   - metadata preservada (source=osm, osm_id, raw_tags, etc.);
 *   - resultados em coords de mundo (metros) — não pixels.
 */

import { describe, expect, it } from "vitest";
import {
  convertOsmDatasetToParityObjects,
  isNonVehicleHighway,
  isOsmRoundaboutForParity,
  parityRoadMarkingByHighway,
  parityRoadWidthMetersByHighway,
  polylineToParityBezier,
  projectLatLonToLocalMeters,
} from "../osmAdapter";
import type { OsmNode, OsmWay } from "../../osm";

// ---------------------------------------------------------------------------
// Largura por classe.

describe("road-parity / parityRoadWidthMetersByHighway — tabela _LARG_CLASSE", () => {
  it("primary e trunk = 10.5 m", () => {
    expect(parityRoadWidthMetersByHighway("primary")).toBe(10.5);
    expect(parityRoadWidthMetersByHighway("trunk")).toBe(10.5);
    expect(parityRoadWidthMetersByHighway("primary_link")).toBe(10.5);
    expect(parityRoadWidthMetersByHighway("motorway")).toBe(10.5);
  });
  it("secondary = 8.5 m", () => {
    expect(parityRoadWidthMetersByHighway("secondary")).toBe(8.5);
    expect(parityRoadWidthMetersByHighway("secondary_link")).toBe(8.5);
  });
  it("tertiary = 7.5 m", () => {
    expect(parityRoadWidthMetersByHighway("tertiary")).toBe(7.5);
    expect(parityRoadWidthMetersByHighway("tertiary_link")).toBe(7.5);
  });
  it("residential / unclassified / living_street = 6.0 m", () => {
    expect(parityRoadWidthMetersByHighway("residential")).toBe(6.0);
    expect(parityRoadWidthMetersByHighway("unclassified")).toBe(6.0);
    expect(parityRoadWidthMetersByHighway("living_street")).toBe(6.0);
  });
  it("service / parking_aisle = 4.5 m", () => {
    expect(parityRoadWidthMetersByHighway("service")).toBe(4.5);
    expect(parityRoadWidthMetersByHighway("parking_aisle")).toBe(4.5);
  });
  it("classe desconhecida cai em fallback 6.5 m", () => {
    expect(parityRoadWidthMetersByHighway("xpto")).toBe(6.5);
  });
  it("highway ausente cai em 7.0 m (default Python LARGURA_PADRAO)", () => {
    expect(parityRoadWidthMetersByHighway(undefined)).toBe(7.0);
  });
});

// ---------------------------------------------------------------------------
// Marcação por classe.

describe("road-parity / parityRoadMarkingByHighway", () => {
  it("oneway sempre = nenhuma (não importa highway)", () => {
    expect(parityRoadMarkingByHighway("primary", true)).toBe("nenhuma");
    expect(parityRoadMarkingByHighway("residential", true)).toBe("nenhuma");
    expect(parityRoadMarkingByHighway("service", true)).toBe("nenhuma");
  });
  it("primary/secondary/tertiary mão dupla = amarela", () => {
    expect(parityRoadMarkingByHighway("primary", false)).toBe("amarela");
    expect(parityRoadMarkingByHighway("secondary", false)).toBe("amarela");
    expect(parityRoadMarkingByHighway("tertiary", false)).toBe("amarela");
    expect(parityRoadMarkingByHighway("trunk", false)).toBe("amarela");
  });
  it("residential/service mão dupla = branca", () => {
    expect(parityRoadMarkingByHighway("residential", false)).toBe("branca");
    expect(parityRoadMarkingByHighway("unclassified", false)).toBe("branca");
    expect(parityRoadMarkingByHighway("service", false)).toBe("branca");
    expect(parityRoadMarkingByHighway("living_street", false)).toBe("branca");
  });
});

// ---------------------------------------------------------------------------
// Filtro de non-vehicle.

describe("road-parity / isNonVehicleHighway", () => {
  it("ignora footway, path, cycleway, pedestrian, steps", () => {
    expect(isNonVehicleHighway("footway")).toBe(true);
    expect(isNonVehicleHighway("path")).toBe(true);
    expect(isNonVehicleHighway("cycleway")).toBe(true);
    expect(isNonVehicleHighway("pedestrian")).toBe(true);
    expect(isNonVehicleHighway("steps")).toBe(true);
    expect(isNonVehicleHighway("bridleway")).toBe(true);
  });
  it("aceita primary, residential, tertiary, service", () => {
    expect(isNonVehicleHighway("primary")).toBe(false);
    expect(isNonVehicleHighway("residential")).toBe(false);
    expect(isNonVehicleHighway("tertiary")).toBe(false);
    expect(isNonVehicleHighway("service")).toBe(false);
  });
  it("ausência (undefined) é tratada como não-veicular (skip)", () => {
    expect(isNonVehicleHighway(undefined)).toBe(true);
  });
});

// ---------------------------------------------------------------------------
// Projeção lat/lon → metros.

describe("road-parity / projectLatLonToLocalMeters", () => {
  it("centro projeta para (0, 0)", () => {
    const p = projectLatLonToLocalMeters(-0.0345, -51.0694, -0.0345, -51.0694);
    expect(p.x).toBeCloseTo(0, 6);
    expect(p.y).toBeCloseTo(0, 6);
  });
  it("longitude positiva (leste) gera x positivo", () => {
    const p = projectLatLonToLocalMeters(
      -0.0345,
      -51.0694 + 0.001, // ~111 m leste no equador
      -0.0345,
      -51.0694,
    );
    expect(p.x).toBeGreaterThan(50);
    expect(p.x).toBeLessThan(200);
  });
  it("latitude positiva (norte) gera y negativo (canvas Y-down)", () => {
    const p = projectLatLonToLocalMeters(
      -0.0345 + 0.001, // norte
      -51.0694,
      -0.0345,
      -51.0694,
    );
    expect(p.y).toBeLessThan(0);
  });
});

// ---------------------------------------------------------------------------
// Bezier 4-point.

describe("road-parity / polylineToParityBezier", () => {
  it("polyline reta → start/end nos endpoints, controles entre eles", () => {
    const fit = polylineToParityBezier([
      { x: 0, y: 0 },
      { x: 30, y: 0 },
      { x: 60, y: 0 },
    ]);
    expect(fit).not.toBeNull();
    expect(fit!.start).toEqual({ x: 0, y: 0 });
    expect(fit!.end).toEqual({ x: 60, y: 0 });
    expect(fit!.c1.x).toBeGreaterThan(0);
    expect(fit!.c1.x).toBeLessThan(30);
    expect(fit!.c2.x).toBeGreaterThan(30);
    expect(fit!.c2.x).toBeLessThan(60);
    expect(fit!.arcLengthM).toBeCloseTo(60, 5);
  });
  it("polyline com 1 ponto → null", () => {
    expect(polylineToParityBezier([{ x: 0, y: 0 }])).toBeNull();
  });
  it("polyline degenerada (arc < 5cm) → null", () => {
    expect(
      polylineToParityBezier([
        { x: 0, y: 0 },
        { x: 0.01, y: 0 },
      ]),
    ).toBeNull();
  });
  it("polyline em L preserva tangente inicial e final", () => {
    // Trecho: vai para leste, depois vira para sul.
    const fit = polylineToParityBezier([
      { x: 0, y: 0 },
      { x: 50, y: 0 },
      { x: 50, y: 50 },
    ]);
    expect(fit).not.toBeNull();
    // Tangente inicial = leste → c1 fica à direita de start.
    expect(fit!.c1.x).toBeGreaterThan(0);
    // Tangente final = sul → c2 fica acima do end (em coords canvas Y-down,
    // "acima" = y menor).
    expect(fit!.c2.y).toBeLessThan(50);
  });
});

// ---------------------------------------------------------------------------
// Detecção de rotatória.

describe("road-parity / isOsmRoundaboutForParity", () => {
  it("junction=roundabout → true (independente de geometria)", () => {
    const way: OsmWay = {
      id: 1,
      node_refs: [1, 2, 3, 1],
      tags: { junction: "roundabout", highway: "tertiary" },
    };
    expect(
      isOsmRoundaboutForParity(way, [
        { x: 0, y: 0 },
        { x: 1, y: 0 },
        { x: 0, y: 1 },
        { x: 0, y: 0 },
      ]),
    ).toBe(true);
  });
  it("ring fechado circular sem tag → detectado por geometria", () => {
    const N = 16;
    const r = 10;
    const pts: { x: number; y: number }[] = [];
    for (let i = 0; i < N; i++) {
      const a = (i / N) * Math.PI * 2;
      pts.push({ x: r * Math.cos(a), y: r * Math.sin(a) });
    }
    pts.push(pts[0] as { x: number; y: number }); // fecha
    const way: OsmWay = {
      id: 1,
      node_refs: [...Array.from({ length: N }, (_, i) => i + 1), 1],
      tags: { highway: "tertiary" },
    };
    expect(isOsmRoundaboutForParity(way, pts)).toBe(true);
  });
  it("polilinha aberta não-circular → false", () => {
    const way: OsmWay = {
      id: 1,
      node_refs: [1, 2, 3],
      tags: { highway: "residential" },
    };
    expect(
      isOsmRoundaboutForParity(way, [
        { x: 0, y: 0 },
        { x: 30, y: 0 },
        { x: 60, y: 0 },
      ]),
    ).toBe(false);
  });
});

// ---------------------------------------------------------------------------
// Adapter completo — way → road_parity.

function buildSimpleWayDataset(
  highway: string,
  oneway: boolean = false,
  name?: string,
): { ways: OsmWay[]; nodes: OsmNode[]; center: { lat: number; lon: number } } {
  const center = { lat: -0.0345, lon: -51.0694 };
  // Cria 3 nodes deslocados ~50 m a oeste/centro/leste do centro.
  // 1 grau de longitude no equador ≈ 111000 m.
  const dLon = 50 / 111000;
  const nodes: OsmNode[] = [
    { id: 1, lat: center.lat, lon: center.lon - dLon },
    { id: 2, lat: center.lat, lon: center.lon },
    { id: 3, lat: center.lat, lon: center.lon + dLon },
  ];
  const tags: Record<string, string> = { highway };
  if (oneway) tags.oneway = "yes";
  if (name) tags.name = name;
  const ways: OsmWay[] = [
    { id: 100, node_refs: [1, 2, 3], tags },
  ];
  return { ways, nodes, center };
}

describe("road-parity / convertOsmDatasetToParityObjects — way → road_parity", () => {
  it("primary mão dupla → kind road_parity, largura_m=10.5, marcacao=amarela", () => {
    const { ways, nodes, center } = buildSimpleWayDataset("primary");
    const out = convertOsmDatasetToParityObjects({
      ways,
      nodes,
      center,
      radius_m: 100,
      canvas: { width: 1600, height: 1000 },
    });
    expect(out.roads.length).toBe(1);
    const road = out.roads[0]!;
    expect(road.kind).toBe("road_parity");
    expect(road.engine).toBe("parity");
    expect(road.largura_m).toBe(10.5);
    expect(road.marcacao).toBe("amarela");
    expect(road.mao_dupla).toBe(true);
    expect(road.superficie).toBe("asfalto");
  });
  it("residential mão dupla → largura_m=6.0, marcacao=branca", () => {
    const { ways, nodes, center } = buildSimpleWayDataset("residential");
    const out = convertOsmDatasetToParityObjects({
      ways,
      nodes,
      center,
      radius_m: 100,
      canvas: { width: 1600, height: 1000 },
    });
    expect(out.roads.length).toBe(1);
    expect(out.roads[0]!.largura_m).toBe(6.0);
    expect(out.roads[0]!.marcacao).toBe("branca");
  });
  it("oneway → mao_dupla=false, marcacao=nenhuma, largura dividida ao meio", () => {
    const { ways, nodes, center } = buildSimpleWayDataset("primary", true);
    const out = convertOsmDatasetToParityObjects({
      ways,
      nodes,
      center,
      radius_m: 100,
      canvas: { width: 1600, height: 1000 },
    });
    expect(out.roads.length).toBe(1);
    const road = out.roads[0]!;
    expect(road.mao_dupla).toBe(false);
    expect(road.marcacao).toBe("nenhuma");
    // Largura: primary é 10.5 m. Oneway divide ao meio → 5.25 m.
    expect(road.largura_m).toBeCloseTo(5.25, 2);
  });
  it("label vem de tags.name quando presente", () => {
    const { ways, nodes, center } = buildSimpleWayDataset(
      "tertiary",
      false,
      "Av. Manoel Torrinha",
    );
    const out = convertOsmDatasetToParityObjects({
      ways,
      nodes,
      center,
      radius_m: 100,
      canvas: { width: 1600, height: 1000 },
    });
    expect(out.roads[0]!.label).toBe("Av. Manoel Torrinha");
  });
  it("footway é ignorado (não-veicular)", () => {
    const { ways, nodes, center } = buildSimpleWayDataset("footway");
    const out = convertOsmDatasetToParityObjects({
      ways,
      nodes,
      center,
      radius_m: 100,
      canvas: { width: 1600, height: 1000 },
    });
    expect(out.roads.length).toBe(0);
    expect(out.stats.skipped_count).toBeGreaterThanOrEqual(1);
  });
  it("metadata_json preserva osm_id, highway, raw_tags", () => {
    const { ways, nodes, center } = buildSimpleWayDataset("tertiary");
    const out = convertOsmDatasetToParityObjects({
      ways,
      nodes,
      center,
      radius_m: 100,
      canvas: { width: 1600, height: 1000 },
    });
    expect(out.roads.length).toBe(1);
    const meta = JSON.parse(out.roads[0]!.metadata_json!);
    expect(meta.source).toBe("osm");
    expect(meta.osm_id).toBe(100);
    expect(meta.highway).toBe("tertiary");
    expect(meta.raw_tags).toBeDefined();
  });
});

// ---------------------------------------------------------------------------
// Adapter completo — roundabout → roundabout_parity.

describe("road-parity / convertOsmDatasetToParityObjects — roundabout", () => {
  it("junction=roundabout vira SicroRoundaboutObject_parity", () => {
    const center = { lat: -0.0345, lon: -51.0694 };
    // Ring de 8 nodes em círculo, raio ~15 m.
    const N = 8;
    const r = 15 / 111000; // graus
    const nodes: OsmNode[] = [];
    const node_refs: number[] = [];
    for (let i = 0; i < N; i++) {
      const a = (i / N) * Math.PI * 2;
      nodes.push({
        id: 100 + i,
        lat: center.lat + r * Math.sin(a),
        lon: center.lon + r * Math.cos(a),
      });
      node_refs.push(100 + i);
    }
    node_refs.push(100); // fecha o ring
    const ways: OsmWay[] = [
      {
        id: 999,
        node_refs,
        tags: { highway: "tertiary", junction: "roundabout" },
      },
    ];
    const out = convertOsmDatasetToParityObjects({
      ways,
      nodes,
      center,
      radius_m: 100,
      canvas: { width: 1600, height: 1000 },
    });
    expect(out.roundabouts.length).toBe(1);
    expect(out.roads.length).toBe(0);
    const rb = out.roundabouts[0]!;
    expect(rb.kind).toBe("roundabout_parity");
    expect(rb.engine).toBe("parity");
    expect(rb.r_m).toBeGreaterThan(10);
    expect(rb.r_m).toBeLessThan(25);
    // Largura do anel — entre 4 e 9 m por design (paridade Python).
    expect(rb.largura_m).toBeGreaterThanOrEqual(4);
    expect(rb.largura_m).toBeLessThanOrEqual(9);
    expect(rb.superficie).toBe("asfalto");
    // inner_color omitido → renderer aplica default verde Python.
    expect(rb.inner_color).toBeUndefined();
  });

  it("metadata_json da rotatória preserva osm_id e node_refs", () => {
    const center = { lat: -0.0345, lon: -51.0694 };
    const N = 8;
    const r = 12 / 111000;
    const nodes: OsmNode[] = [];
    const node_refs: number[] = [];
    for (let i = 0; i < N; i++) {
      const a = (i / N) * Math.PI * 2;
      nodes.push({
        id: 200 + i,
        lat: center.lat + r * Math.sin(a),
        lon: center.lon + r * Math.cos(a),
      });
      node_refs.push(200 + i);
    }
    node_refs.push(200);
    const ways: OsmWay[] = [
      {
        id: 777,
        node_refs,
        tags: { highway: "tertiary", junction: "roundabout", name: "Praça XYZ" },
      },
    ];
    const out = convertOsmDatasetToParityObjects({
      ways,
      nodes,
      center,
      radius_m: 100,
      canvas: { width: 1600, height: 1000 },
    });
    expect(out.roundabouts.length).toBe(1);
    const meta = JSON.parse(out.roundabouts[0]!.metadata_json!);
    expect(meta.source).toBe("osm");
    expect(meta.osm_id).toBe(777);
    expect(meta.node_refs).toBeDefined();
    expect(out.roundabouts[0]!.label).toBe("Praça XYZ");
  });
});

// ---------------------------------------------------------------------------
// Fit uniforme — objetos centrados no canvas.

describe("road-parity / convertOsmDatasetToParityObjects — fit uniforme", () => {
  it("scale calculada (px_per_m) > 0 quando há vias", () => {
    const { ways, nodes, center } = buildSimpleWayDataset("primary");
    const out = convertOsmDatasetToParityObjects({
      ways,
      nodes,
      center,
      radius_m: 100,
      canvas: { width: 1600, height: 1000 },
    });
    expect(out.stats.px_per_m).toBeGreaterThan(0);
  });
  it("objetos parity ficam em coords de mundo (metros) — não pixels", () => {
    // Ways pequenas (50m de extensão) em canvas 1600×1000 com fit ~10-15 px/m.
    // Coordenadas resultantes devem estar em ordem de magnitude de metros
    // (~50-150), não pixels (~800-1600).
    const { ways, nodes, center } = buildSimpleWayDataset("tertiary");
    const out = convertOsmDatasetToParityObjects({
      ways,
      nodes,
      center,
      radius_m: 100,
      canvas: { width: 1600, height: 1000 },
    });
    const road = out.roads[0]!;
    // O recentre coloca o conjunto no centro do canvas em coords de
    // mundo. Para fit ~16 px/m, canvas/scale ~ 100m → ax/bx devem ficar
    // em torno de 30-90m. NUNCA na ordem de 800-1600 (que seria pixels).
    expect(road.ax).toBeLessThan(500);
    expect(road.bx).toBeLessThan(500);
    expect(road.ay).toBeLessThan(500);
    expect(road.by).toBeLessThan(500);
  });
});

// ---------------------------------------------------------------------------
// Stats + warnings.

describe("road-parity / convertOsmDatasetToParityObjects — stats", () => {
  it("dataset vazio → 0 roads + skipped_count = 0", () => {
    const out = convertOsmDatasetToParityObjects({
      ways: [],
      nodes: [],
      center: { lat: -0.0345, lon: -51.0694 },
      radius_m: 100,
      canvas: { width: 1600, height: 1000 },
    });
    expect(out.roads.length).toBe(0);
    expect(out.roundabouts.length).toBe(0);
    expect(out.stats.imported_road_count).toBe(0);
    expect(out.stats.imported_roundabout_count).toBe(0);
  });
  it("warnings populado quando há ways ignoradas", () => {
    const center = { lat: -0.0345, lon: -51.0694 };
    // 1 way muito curta (10 cm).
    const dLon = 0.0000001;
    const nodes: OsmNode[] = [
      { id: 1, lat: center.lat, lon: center.lon },
      { id: 2, lat: center.lat, lon: center.lon + dLon },
    ];
    const ways: OsmWay[] = [
      { id: 100, node_refs: [1, 2], tags: { highway: "tertiary" } },
    ];
    const out = convertOsmDatasetToParityObjects({
      ways,
      nodes,
      center,
      radius_m: 100,
      canvas: { width: 1600, height: 1000 },
    });
    expect(out.roads.length).toBe(0);
    expect(out.stats.skipped_count).toBeGreaterThanOrEqual(1);
    expect(out.warnings.length).toBeGreaterThanOrEqual(1);
  });
});
