/**
 * vehicleArt — catálogo da frota SVG do designer + recolor por cor-chave.
 */

import { describe, expect, it } from "vitest";
import {
  ART_KEY_COLOR,
  PESSOA_ART,
  VEHICLE_ART,
  darkenHex,
  getPessoaArt,
  getVehicleArtSvg,
  getVehicleRealDims,
} from "./vehicleArt";
import { makeMarker, makeVehicle } from "./factories";

const ENTRIES = Object.entries(VEHICLE_ART) as [
  string,
  NonNullable<(typeof VEHICLE_ART)[keyof typeof VEHICLE_ART]>,
][];

describe("VEHICLE_ART (catálogo)", () => {
  it("todo item tem SVG válido e dimensões reais sãs", () => {
    expect(ENTRIES.length).toBeGreaterThanOrEqual(25);
    for (const [body, entry] of ENTRIES) {
      expect(entry.svg, body).toContain("<svg");
      // Dimensões plausíveis pra veículos: 0,5–6 m de largura, 1–20 m de comprimento.
      expect(entry.widthM, body).toBeGreaterThan(0.5);
      expect(entry.widthM, body).toBeLessThan(6);
      expect(entry.lengthM, body).toBeGreaterThan(1);
      expect(entry.lengthM, body).toBeLessThan(20);
      // Comprimento > largura em todo veículo (vista de topo, retrato).
      expect(entry.lengthM, body).toBeGreaterThan(entry.widthM);
    }
  });

  it("recoloríveis contêm a cor-chave; pintura fixa não precisa", () => {
    for (const [body, entry] of ENTRIES) {
      if (entry.recolorable) {
        expect(entry.svg, body).toContain(ART_KEY_COLOR);
      }
    }
    // Pintura oficial fixa marcada corretamente.
    for (const fixo of ["ambulancia", "taxi", "vtr_pm", "vtr_pc", "vtr_pci", "vtr_bm", "vtr_pp"]) {
      expect(VEHICLE_ART[fixo as keyof typeof VEHICLE_ART]?.recolorable, fixo).toBe(false);
    }
  });
});

describe("getVehicleArtSvg (recolor)", () => {
  it("troca a cor-chave pela cor do objeto nos recoloríveis", () => {
    const out = getVehicleArtSvg("hatch", "#1D4ED8");
    expect(out).not.toBeNull();
    expect(out!).toContain("#1D4ED8");
    expect(out!).not.toContain(ART_KEY_COLOR);
  });

  it("aceita cor sem '#' e ignora cor inválida (mantém a chave)", () => {
    expect(getVehicleArtSvg("hatch", "1D4ED8")!).toContain("#1D4ED8");
    expect(getVehicleArtSvg("hatch", "azul!!")!).toContain(ART_KEY_COLOR);
  });

  it("pintura fixa ignora a cor (mesmo SVG sempre)", () => {
    const a = getVehicleArtSvg("vtr_pm", "#1D4ED8");
    const b = getVehicleArtSvg("vtr_pm", "#22c55e");
    expect(a).toBe(b);
  });

  it("tipos sem arte (pickup/other) devolvem null", () => {
    expect(getVehicleArtSvg("pickup", "#111111")).toBeNull();
    expect(getVehicleArtSvg("other", "#111111")).toBeNull();
  });

  it("sedan: a sombra também é trocada (não sobra B0080A)", () => {
    const out = getVehicleArtSvg("sedan", "#1D4ED8")!;
    expect(out).not.toContain("#B0080A");
    expect(out).toContain(darkenHex("#1D4ED8"));
  });
});

describe("darkenHex", () => {
  it("escurece canais e preserva formato", () => {
    expect(darkenHex("#ffffff", 0.5)).toBe("#808080");
    expect(darkenHex("#000000")).toBe("#000000");
    // Entrada inválida volta intacta.
    expect(darkenHex("azul")).toBe("azul");
  });
});

describe("makeVehicle em escala real", () => {
  it("com pxPerM e arte, usa as dimensões reais (frente = +x → width = comprimento)", () => {
    const v = makeVehicle({ x: 0, y: 0 }, "V1", "sedan", 10);
    const dims = getVehicleRealDims("sedan")!;
    expect(v.width).toBeCloseTo(dims.lengthM * 10, 5);
    expect(v.height).toBeCloseTo(dims.widthM * 10, 5);
  });

  it("sem escala, mantém os presets em px (compat)", () => {
    const v = makeVehicle({ x: 0, y: 0 }, "V1", "sedan");
    expect(v.width).toBe(80);
    expect(v.height).toBe(35);
  });

  it("tipo sem arte ignora a escala e usa preset", () => {
    const v = makeVehicle({ x: 0, y: 0 }, "V1", "pickup", 10);
    expect(v.width).toBe(96);
    expect(v.height).toBe(42);
  });
});

describe("PESSOA_ART (pedestres em decúbito)", () => {
  it("6 poses com SVG válido e dimensões humanas", () => {
    const keys = Object.keys(PESSOA_ART);
    expect(keys).toHaveLength(6);
    for (const k of keys) {
      const e = PESSOA_ART[k]!;
      expect(e.svg, k).toContain("<svg");
      expect(e.widthM, k).toBeGreaterThan(0.3);
      expect(e.widthM, k).toBeLessThan(1);
      expect(e.lengthM, k).toBeGreaterThan(1.4);
      expect(e.lengthM, k).toBeLessThan(2);
    }
    expect(getPessoaArt("pedestre_m_dorsal")).not.toBeNull();
    expect(getPessoaArt("collision_x")).toBeNull();
  });

  it("makeMarker com escala usa a altura humana real; sem escala, preset", () => {
    const comEscala = makeMarker({ x: 0, y: 0 }, "pedestre_f_dorsal", undefined, 10);
    expect(comEscala.size).toBeCloseTo(1.65 * 10, 5);
    const semEscala = makeMarker({ x: 0, y: 0 }, "pedestre_f_dorsal");
    expect(semEscala.size).toBe(56);
    // Markers sem arte não são afetados pela escala.
    const x = makeMarker({ x: 0, y: 0 }, "collision_x", undefined, 10);
    expect(x.size).toBeGreaterThan(0);
  });
});
