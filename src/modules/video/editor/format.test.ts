import { describe, expect, it } from "vitest";

import { estimateFrameIndex, parseTimeInput, probeStartTime } from "./format";

describe("parseTimeInput", () => {
  it("entende segundos, vírgula e formatos com dois-pontos", () => {
    expect(parseTimeInput("12", 25)).toBe(12);
    expect(parseTimeInput("12.48", 25)).toBeCloseTo(12.48);
    expect(parseTimeInput("12,48", 25)).toBeCloseTo(12.48);
    expect(parseTimeInput("1:02.5", 25)).toBeCloseTo(62.5);
    expect(parseTimeInput("01:02:03.250", 25)).toBeCloseTo(3723.25);
  });

  it("entende número de quadro com #, q ou 'quadro'", () => {
    expect(parseTimeInput("#312", 25)).toBeCloseTo(12.48);
    expect(parseTimeInput("q312", 25)).toBeCloseTo(12.48);
    expect(parseTimeInput("quadro 50", 25)).toBe(2);
    expect(parseTimeInput("#10", null)).toBeNull();
  });

  it("recusa lixo e campos fora do lugar", () => {
    expect(parseTimeInput("", 25)).toBeNull();
    expect(parseTimeInput("abc", 25)).toBeNull();
    expect(parseTimeInput("1:75", 25)).toBeNull();
    expect(parseTimeInput("1.5:10", 25)).toBeNull();
    expect(parseTimeInput("1:2:3:4", 25)).toBeNull();
  });
});

describe("estimateFrameIndex", () => {
  it("usa round(t × fps), como o Rust", () => {
    expect(estimateFrameIndex(1, 30)).toBe(30);
    expect(estimateFrameIndex(2.5, 30)).toBe(75);
    expect(estimateFrameIndex(1, null)).toBeNull();
    expect(estimateFrameIndex(1, 0)).toBeNull();
  });
});

describe("probeStartTime", () => {
  it("lê o start_time do stream de vídeo (edit list com trecho vazio)", () => {
    const raw = JSON.stringify({
      streams: [{ codec_type: "audio", start_time: "0.0" }, { codec_type: "video", start_time: "3.965951" }],
      format: { start_time: "0.0" },
    });
    expect(probeStartTime(raw)).toBeCloseTo(3.965951);
  });

  it("cai para o container e para 0", () => {
    expect(probeStartTime(JSON.stringify({ format: { start_time: "1.5" } }))).toBe(1.5);
    expect(probeStartTime("{}")).toBe(0);
    expect(probeStartTime("não é json")).toBe(0);
    expect(probeStartTime(null)).toBe(0);
  });
});
