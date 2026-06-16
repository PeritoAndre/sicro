import { describe, it, expect } from "vitest";
import type { JSONContent } from "@tiptap/core";
import type {
  VideoSpeedCalculation,
  VideoSpeedCalibration,
} from "@domain/video_speed";
import { buildSpeedMethodologyContent } from "../speed-methodology";

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

const fullCalc: VideoSpeedCalculation = {
  id: "calc-1",
  occurrence_id: "occ-1",
  media_hash: "abc",
  calibration_id: "cal-1",
  points: [
    { px: 0, py: 50, u_px: 2.5, actual_timestamp_s: 1.0021, manual: true, storyboard_frame_id: "f1" },
    { px: 100, py: 50, u_px: 2.5, actual_timestamp_s: 1.1, manual: true, storyboard_frame_id: "f2" },
    { px: 200, py: 50, u_px: 2.5, actual_timestamp_s: 1.2, manual: true, storyboard_frame_id: "f3" },
  ],
  velocity_kmh: 57.6,
  vx_m_per_s: 15.9,
  vy_m_per_s: 1.2,
  se_m_per_s: 1.1,
  ci_low: 49.3,
  ci_high: 65.9,
  confidence: 0.95,
  r_squared: 0.991,
  residuals: [0.03, -0.05, 0.01],
  mc_seed: 4242,
  mc_sigmas: { calibration_px: 1.0, world_m: 0, trajectory_px: 2.5, time_s: 0.01 },
  mc_n: 10000,
  mc_failed: 3,
  mc_mean_kmh: 57.4,
  mc_median_kmh: 57.5,
  mc_p2_5_kmh: 48.9,
  mc_p97_5_kmh: 66.2,
  limitations: [
    "Marcação manual da posição do veículo (sem tracking automático).",
    "Possível erro de paralaxe.",
    "Calibração por medição em campo.",
  ],
  audit: { estimator: "per_axis_regression" },
  author: "André",
  created_at: "2026-05-31T12:00:00Z",
};

describe("buildSpeedMethodologyContent", () => {
  it("começa com um heading de seção", () => {
    const out = buildSpeedMethodologyContent(fullCalc, planeCalibration);
    expect(out[0]?.type).toBe("heading");
    expect(allText([out[0]!])).toContain("ESTIMATIVA DE VELOCIDADE");
  });

  it("apresenta o resultado como ESTIMATIVA, nunca como fato", () => {
    const text = allText(buildSpeedMethodologyContent(fullCalc, planeCalibration));
    expect(text).toContain("ESTIMADA em 57,6 km/h");
    // NÃO deve afirmar como fato consumado.
    expect(text).not.toContain("o veículo estava a");
    expect(text).toContain("AUXÍLIO DE MEDIÇÃO");
    expect(text).toContain("responsabilidade do(a) perito(a)");
  });

  it("inclui método, tipo de calibração, fonte e RMS", () => {
    const text = allText(buildSpeedMethodologyContent(fullCalc, planeCalibration));
    expect(text).toContain("modo manual");
    expect(text).toContain("ffmpeg");
    expect(text).toContain("plano (homografia por 4 pontos coplanares)");
    expect(text).toContain("medida obtida em campo");
    expect(text).toContain("0,420 m");
  });

  it("lista os instantes (actual_timestamp_s) e a contagem de pontos", () => {
    const text = allText(buildSpeedMethodologyContent(fullCalc, planeCalibration));
    expect(text).toContain("3 ponto(s)");
    expect(text).toContain("1,002 s");
    expect(text).toContain("1,200 s");
  });

  it("mostra IC da regressão e o intervalo Monte Carlo", () => {
    const text = allText(buildSpeedMethodologyContent(fullCalc, planeCalibration));
    expect(text).toContain("intervalo de confiança de 95%");
    expect(text).toContain("49,3 a 65,9 km/h");
    expect(text).toContain("R² = 0,9910");
    expect(text).toContain("Monte Carlo");
    expect(text).toContain("48,9 a 66,2 km/h");
    expect(text).toContain("3 descartadas");
  });

  it("distingue σ modelados (>0) dos NÃO incluídos (=0)", () => {
    const text = allText(buildSpeedMethodologyContent(fullCalc, planeCalibration));
    expect(text).toContain("Fontes de incerteza modeladas");
    expect(text).toContain("marcação dos pontos (σ = 2,5 px)");
    expect(text).toContain("tempo (σ = 0,010 s)");
    // world_m = 0 ⇒ NÃO incluída.
    expect(text).toContain("Fontes NÃO incluídas");
    expect(text).toContain("dimensão real da referência");
  });

  it("inclui TODAS as ressalvas, sem omitir nenhuma", () => {
    const out = buildSpeedMethodologyContent(fullCalc, planeCalibration);
    const text = allText(out);
    for (const lim of fullCalc.limitations) {
      expect(text).toContain(lim);
    }
  });

  it("cita semente + sigmas para reprodutibilidade", () => {
    const text = allText(buildSpeedMethodologyContent(fullCalc, planeCalibration));
    expect(text).toContain("Semente (seed) do Monte Carlo: 4242");
    expect(text).toContain("marcação 2,5 px");
    expect(text).toContain("calibração 1,0 px");
  });

  it("caso 2 pontos: média sem IC, sem MC, com avisos explícitos", () => {
    const twoPt: VideoSpeedCalculation = {
      ...fullCalc,
      points: fullCalc.points.slice(0, 2),
      ci_low: null,
      ci_high: null,
      confidence: null,
      r_squared: null,
      se_m_per_s: null,
      residuals: [],
      mc_seed: null,
      mc_sigmas: null,
      mc_n: null,
      mc_failed: null,
      mc_mean_kmh: null,
      mc_median_kmh: null,
      mc_p2_5_kmh: null,
      mc_p97_5_kmh: null,
      limitations: ["2 pontos — sem incerteza estatística (mínimo 3 para regressão e Monte Carlo)."],
    };
    const lineCal: VideoSpeedCalibration = {
      ...planeCalibration,
      method: "line",
      reference_source: "entre_eixos",
    };
    const text = allText(buildSpeedMethodologyContent(twoPt, lineCal));
    expect(text).toContain("velocidade média entre os dois instantes");
    expect(text).toContain("Não foi executada simulação de Monte Carlo");
    expect(text).toContain("Nenhuma fonte de incerteza foi modelada");
    expect(text).toContain("não foi executada (sem semente registrada)");
    expect(text).toContain("linha (segmento de comprimento conhecido");
    expect(text).toContain("distância entre-eixos");
    expect(text).toContain("2 pontos — sem incerteza estatística");
  });

  it("calibração ausente: nota explícita em vez de detalhes", () => {
    const text = allText(buildSpeedMethodologyContent(fullCalc, null));
    expect(text).toContain("não pôde ser recuperada");
  });
});
