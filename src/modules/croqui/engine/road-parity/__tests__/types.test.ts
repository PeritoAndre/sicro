/**
 * Python Parity Engine — testes de tipos + factories.
 *
 * Garantia mínima Fase H.1:
 *   - factories produzem objetos com o shape esperado;
 *   - validação de limites (clamp) funciona;
 *   - inner_color opcional comporta-se corretamente.
 *
 * Renderer NÃO existe ainda — H.2.
 */

import { describe, expect, it } from "vitest";
import {
  makeParityRoad,
  makeParityRoadBezier,
  makeParityRoundabout,
  PARITY_ENGINE_TAG,
  PARITY_ROAD_LARGURA_MAX_M,
  PARITY_ROAD_LARGURA_MIN_M,
  PARITY_ROAD_LARGURA_PADRAO_M,
  PARITY_ROUNDABOUT_LARGURA_PADRAO_M,
  PARITY_ROUNDABOUT_R_MAX_M,
  PARITY_ROUNDABOUT_R_MIN_M,
} from "../index";

describe("road-parity / makeParityRoad — defaults", () => {
  it("cria via reta entre A e B com 8 campos geométricos corretos", () => {
    const r = makeParityRoad(0, 0, 100, 0);
    expect(r.ax).toBe(0);
    expect(r.ay).toBe(0);
    expect(r.bx).toBe(100);
    expect(r.by).toBe(0);
    // Controles default a 1/3 e 2/3 do segmento.
    expect(r.cx1).toBeCloseTo(33.33, 1);
    expect(r.cy1).toBe(0);
    expect(r.cx2).toBeCloseTo(66.67, 1);
    expect(r.cy2).toBe(0);
  });

  it("kind = 'road_parity' (distinto do legacy)", () => {
    const r = makeParityRoad(0, 0, 10, 0);
    expect(r.kind).toBe("road_parity");
  });

  it("engine = 'parity'", () => {
    const r = makeParityRoad(0, 0, 10, 0);
    expect(r.engine).toBe(PARITY_ENGINE_TAG);
    expect(r.engine).toBe("parity");
  });

  it("category = 'vias'", () => {
    expect(makeParityRoad(0, 0, 10, 0).category).toBe("vias");
  });

  it("defaults visuais: asfalto / mão dupla / amarela / 7m", () => {
    const r = makeParityRoad(0, 0, 10, 0);
    expect(r.superficie).toBe("asfalto");
    expect(r.mao_dupla).toBe(true);
    expect(r.marcacao).toBe("amarela");
    expect(r.largura_m).toBe(PARITY_ROAD_LARGURA_PADRAO_M);
  });

  it("visible default true, locked default false", () => {
    const r = makeParityRoad(0, 0, 10, 0);
    expect(r.visible).toBe(true);
    expect(r.locked).toBe(false);
  });

  it("label e metadata_json default null", () => {
    const r = makeParityRoad(0, 0, 10, 0);
    expect(r.label).toBeNull();
    expect(r.metadata_json).toBeNull();
  });

  it("id auto-gerado com prefix 'rdp_'", () => {
    const r1 = makeParityRoad(0, 0, 10, 0);
    const r2 = makeParityRoad(0, 0, 10, 0);
    expect(r1.id).toMatch(/^rdp_/);
    expect(r2.id).toMatch(/^rdp_/);
    expect(r1.id).not.toBe(r2.id);
  });
});

describe("road-parity / makeParityRoad — overrides", () => {
  it("respeita id custom", () => {
    expect(makeParityRoad(0, 0, 10, 0, { id: "custom_123" }).id).toBe("custom_123");
  });

  it("respeita largura_m dentro do limite", () => {
    expect(makeParityRoad(0, 0, 10, 0, { largura_m: 14 }).largura_m).toBe(14);
  });

  it("clamp largura_m abaixo do mínimo", () => {
    expect(makeParityRoad(0, 0, 10, 0, { largura_m: 0.1 }).largura_m).toBe(
      PARITY_ROAD_LARGURA_MIN_M,
    );
  });

  it("clamp largura_m acima do máximo", () => {
    expect(makeParityRoad(0, 0, 10, 0, { largura_m: 100 }).largura_m).toBe(
      PARITY_ROAD_LARGURA_MAX_M,
    );
  });

  it("clamp largura_m de Infinity volta para mínimo (Number.isFinite false)", () => {
    expect(makeParityRoad(0, 0, 10, 0, { largura_m: Infinity }).largura_m).toBe(
      PARITY_ROAD_LARGURA_MIN_M,
    );
  });

  it("clamp largura_m NaN volta para mínimo (Number.isFinite false)", () => {
    expect(makeParityRoad(0, 0, 10, 0, { largura_m: NaN }).largura_m).toBe(
      PARITY_ROAD_LARGURA_MIN_M,
    );
  });

  it("mao_dupla false + marcacao nenhuma persistem", () => {
    const r = makeParityRoad(0, 0, 10, 0, {
      mao_dupla: false,
      marcacao: "nenhuma",
    });
    expect(r.mao_dupla).toBe(false);
    expect(r.marcacao).toBe("nenhuma");
  });

  it("label custom é mantido", () => {
    expect(makeParityRoad(0, 0, 10, 0, { label: "BR-101" }).label).toBe(
      "BR-101",
    );
  });
});

describe("road-parity / makeParityRoadBezier — controles explícitos", () => {
  it("aceita 4 control points completos", () => {
    const r = makeParityRoadBezier(0, 0, 30, -50, 70, 50, 100, 0, {
      label: "Av. Curva",
    });
    expect(r.ax).toBe(0);
    expect(r.ay).toBe(0);
    expect(r.cx1).toBe(30);
    expect(r.cy1).toBe(-50);
    expect(r.cx2).toBe(70);
    expect(r.cy2).toBe(50);
    expect(r.bx).toBe(100);
    expect(r.by).toBe(0);
    expect(r.label).toBe("Av. Curva");
  });
});

describe("road-parity / makeParityRoundabout — defaults", () => {
  it("cria rotatória com cx/cy/r_m corretos", () => {
    const rb = makeParityRoundabout(50, 100, 12);
    expect(rb.cx).toBe(50);
    expect(rb.cy).toBe(100);
    expect(rb.r_m).toBe(12);
  });

  it("kind = 'roundabout_parity'", () => {
    expect(makeParityRoundabout(0, 0, 10).kind).toBe("roundabout_parity");
  });

  it("engine = 'parity'", () => {
    expect(makeParityRoundabout(0, 0, 10).engine).toBe(PARITY_ENGINE_TAG);
  });

  it("largura_m default 7m", () => {
    expect(makeParityRoundabout(0, 0, 12).largura_m).toBe(
      PARITY_ROUNDABOUT_LARGURA_PADRAO_M,
    );
  });

  it("clamp r_m abaixo do mínimo", () => {
    expect(makeParityRoundabout(0, 0, 0.5).r_m).toBe(PARITY_ROUNDABOUT_R_MIN_M);
  });

  it("clamp r_m acima do máximo", () => {
    expect(makeParityRoundabout(0, 0, 500).r_m).toBe(PARITY_ROUNDABOUT_R_MAX_M);
  });

  it("clamp largura_m para garantir ilha visível (largura < raio - 1)", () => {
    // r=5, largura tentando ser 7 → seria > r-1 = 4 → clamped pra 4.
    const rb = makeParityRoundabout(0, 0, 5, { largura_m: 7 });
    expect(rb.largura_m).toBeLessThanOrEqual(4);
  });

  it("inner_color ausente quando não especificado (renderer aplica default)", () => {
    const rb = makeParityRoundabout(0, 0, 12);
    expect(rb.inner_color).toBeUndefined();
  });

  it("inner_color presente quando especificado", () => {
    const rb = makeParityRoundabout(0, 0, 12, { inner_color: "#abc123" });
    expect(rb.inner_color).toBe("#abc123");
  });

  it("id auto-gerado com prefix 'rbp_'", () => {
    expect(makeParityRoundabout(0, 0, 10).id).toMatch(/^rbp_/);
  });

  it("label e metadata_json default null", () => {
    const rb = makeParityRoundabout(0, 0, 10);
    expect(rb.label).toBeNull();
    expect(rb.metadata_json).toBeNull();
  });

  it("category = 'vias'", () => {
    expect(makeParityRoundabout(0, 0, 10).category).toBe("vias");
  });

  it("superficie default 'asfalto'", () => {
    expect(makeParityRoundabout(0, 0, 10).superficie).toBe("asfalto");
  });
});
