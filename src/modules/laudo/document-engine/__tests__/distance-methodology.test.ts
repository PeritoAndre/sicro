import { describe, it, expect } from "vitest";
import type { JSONContent } from "@tiptap/core";
import type { VideoSpeedCalibration } from "@domain/video_speed";
import type { VideoDistanceMeasurement } from "@domain/video_distance";
import { buildDistanceMethodologyContent } from "../distance-methodology";

/** Concatena todo o texto do fragmento (para asserts de conteúdo). */
function allText(nodes: JSONContent[]): string {
  let s = "";
  const walk = (n: JSONContent) => {
    if (typeof n.text === "string") s += n.text + " ";
    if (Array.isArray(n.content)) n.content.forEach(walk);
  };
  nodes.forEach(walk);
  return s;
}

const planeCalibration: VideoSpeedCalibration = {
  id: "cal-1",
  occurrence_id: "occ-1",
  media_hash: "abc",
  method: "plane",
  control_points: [],
  reference_source: "campo",
  homography: [0.01, 0, 0, 0, 0.01, 0, 0, 0, 1],
  residuals_px: 0.42,
  distortion_model: null,
  author: "André",
  created_at: "2026-05-31T12:00:00Z",
};

const fullMeasurement: VideoDistanceMeasurement = {
  id: "meas-1",
  occurrence_id: "occ-1",
  media_hash: "abc",
  calibration_id: "cal-1",
  p1_px: 100,
  p1_py: 500,
  p2_px: 600,
  p2_py: 500,
  distance_m: 5.0,
  mc_seed: 4242,
  mc_sigmas: { calibration_px: 0.5, world_m: 0, measure_px: 1.0 },
  mc_n: 5000,
  mc_failed: 2,
  mc_mean_m: 5.01,
  mc_median_m: 5.0,
  mc_p2_5_m: 4.88,
  mc_p97_5_m: 5.14,
  limitations: [
    "Marcação manual dos dois pontos (sem detecção automática).",
    "Possível erro de paralaxe: os pontos devem estar SOBRE o plano calibrado.",
    "Calibração por medição em campo.",
  ],
  audit: { estimator: "homography_point_distance" },
  author: "André",
  created_at: "2026-05-31T12:00:00Z",
};

describe("buildDistanceMethodologyContent", () => {
  it("começa com um heading de seção", () => {
    const out = buildDistanceMethodologyContent(fullMeasurement, planeCalibration);
    expect(out[0]?.type).toBe("heading");
    expect(allText([out[0]!])).toContain("ESTIMATIVA DE DISTÂNCIA");
  });

  it("apresenta o resultado como ESTIMATIVA, nunca como fato", () => {
    const text = allText(
      buildDistanceMethodologyContent(fullMeasurement, planeCalibration),
    );
    expect(text).toContain("ESTIMADA em 5,00 m");
    // NÃO deve afirmar como fato consumado.
    expect(text).not.toContain("a distância é");
    expect(text).toContain("AUXÍLIO DE MEDIÇÃO");
    expect(text).toContain("responsabilidade do(a) perito(a)");
  });

  it("inclui método, tipo de calibração, fonte e RMS", () => {
    const text = allText(
      buildDistanceMethodologyContent(fullMeasurement, planeCalibration),
    );
    expect(text).toContain("modo manual");
    expect(text).toContain("ffmpeg");
    expect(text).toContain("plano (homografia por 4 pontos coplanares)");
    expect(text).toContain("medida obtida em campo");
    expect(text).toContain("0,420 m");
    // pontos A/B em pixel, para reproduzir a marcação.
    expect(text).toContain("A (100, 500)");
    expect(text).toContain("B (600, 500)");
  });

  it("mostra o intervalo Monte Carlo (p2,5–p97,5) quando há MC", () => {
    const text = allText(
      buildDistanceMethodologyContent(fullMeasurement, planeCalibration),
    );
    expect(text).toContain("percentis 2,5 a 97,5");
    expect(text).toContain("4,88 a 5,14 m");
    expect(text).toContain("média de 5,01 m");
    expect(text).toContain("2 descartadas");
  });

  it("distingue σ modelados (>0) dos NÃO incluídos (=0)", () => {
    const text = allText(
      buildDistanceMethodologyContent(fullMeasurement, planeCalibration),
    );
    expect(text).toContain("Fontes de incerteza modeladas");
    expect(text).toContain("marcação dos dois pontos (σ = 1,0 px)");
    expect(text).toContain("calibração (σ = 0,5 px)");
    // world_m = 0 ⇒ NÃO incluída no escopo do intervalo.
    expect(text).toContain("Fontes NÃO incluídas");
    expect(text).toContain("dimensão real da referência");
  });

  it("inclui TODAS as ressalvas, sem omitir nenhuma", () => {
    const text = allText(
      buildDistanceMethodologyContent(fullMeasurement, planeCalibration),
    );
    for (const lim of fullMeasurement.limitations) {
      expect(text).toContain(lim);
    }
  });

  it("cita semente + sigmas + o registro reproduzível (id + data)", () => {
    const text = allText(
      buildDistanceMethodologyContent(fullMeasurement, planeCalibration),
    );
    expect(text).toContain("Semente (seed) do Monte Carlo: 4242");
    expect(text).toContain("marcação 1,0 px");
    expect(text).toContain("calibração 0,5 px");
    expect(text).toContain("medição id meas-1");
    expect(text).toContain("2026-05-31T12:00:00Z");
  });

  it("sem MC: distância pontual, sem intervalo, motivo explícito", () => {
    const noMc: VideoDistanceMeasurement = {
      ...fullMeasurement,
      mc_seed: null,
      mc_sigmas: null,
      mc_n: null,
      mc_failed: null,
      mc_mean_m: null,
      mc_median_m: null,
      mc_p2_5_m: null,
      mc_p97_5_m: null,
      limitations: [
        "Marcação manual dos dois pontos (sem detecção automática).",
        "Sem incerteza: σ não informados pelo(a) perito(a).",
      ],
    };
    const text = allText(buildDistanceMethodologyContent(noMc, planeCalibration));
    expect(text).toContain("SEM intervalo de incerteza");
    expect(text).toContain("não possui intervalo de confiança de regressão");
    expect(text).toContain("Nenhuma fonte de incerteza foi modelada");
    expect(text).toContain("não foi executada (sem semente registrada)");
    // distância pontual ainda presente.
    expect(text).toContain("ESTIMADA em 5,00 m");
  });

  it("razão cruzada: a ressalva 1D (persistida) aparece na seção", () => {
    const crCal: VideoSpeedCalibration = {
      ...planeCalibration,
      method: "cross_ratio",
      reference_source: "norma_viaria",
    };
    const crMeas: VideoDistanceMeasurement = {
      ...fullMeasurement,
      limitations: [
        "Marcação manual dos dois pontos (sem detecção automática).",
        "Distância medida ao longo da linha de referência (modelo 1D por razão cruzada); separação lateral à linha não é capturada.",
      ],
    };
    const text = allText(buildDistanceMethodologyContent(crMeas, crCal));
    expect(text).toContain("razão cruzada");
    expect(text).toContain("dimensão presumida a partir de norma viária");
    expect(text).toContain("modelo 1D por razão cruzada");
  });

  it("calibração ausente: nota explícita em vez de detalhes", () => {
    const text = allText(buildDistanceMethodologyContent(fullMeasurement, null));
    expect(text).toContain("não pôde ser recuperada");
  });
});
