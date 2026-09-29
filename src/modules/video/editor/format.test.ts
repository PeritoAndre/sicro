import { describe, expect, it } from "vitest";

import {
  cameraClockAt,
  clockSyncOffset,
  estimateFrameIndex,
  formatClock,
  formatLaudoTime,
  parseClockInput,
  parseTimeInput,
  probeHasAudio,
  probeStartTime,
} from "./format";

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

describe("relógio da câmera", () => {
  it("lê o horário em vários formatos", () => {
    expect(parseClockInput("03:36:05")).toBe(3 * 3600 + 36 * 60 + 5);
    expect(parseClockInput("3:36:05,5")).toBeCloseTo(12965.5);
    expect(parseClockInput("03:36")).toBe(12960);
    expect(parseClockInput("03h36m05s")).toBe(12965);
    expect(parseClockInput("24:00:00")).toBeNull();
    expect(parseClockInput("12:60")).toBeNull();
    expect(parseClockInput("abc")).toBeNull();
  });

  it("formata e dá a volta em 24 h", () => {
    expect(formatClock(12965.48)).toBe("03:36:05.480");
    expect(formatClock(86400 + 5, false)).toBe("00:00:05");
    expect(formatClock(-1, false)).toBe("23:59:59");
  });

  it("horário em qualquer instante e sincronia entre duas câmeras", () => {
    const a = { media_time_s: 5, clock_seconds: 12965 }; // A: 5 s = 03:36:05
    const b = { media_time_s: 2, clock_seconds: 12960 }; // B: 2 s = 03:36:00
    expect(cameraClockAt(12.48, a)).toBeCloseTo(12972.48);
    const off = clockSyncOffset(a, b);
    // mesmo instante real: A em 5 s (03:36:05) ↔ B em 7 s (03:36:05)
    expect(5 + off).toBeCloseTo(7);
    expect(cameraClockAt(5 + off, b)).toBeCloseTo(cameraClockAt(5, a));
  });
});

describe("texto para o laudo", () => {
  it("usa vírgula nos milissegundos", () => {
    expect(formatLaudoTime(12.48)).toBe("00:00:12,480");
    expect(formatLaudoTime(3723.25)).toBe("01:02:03,250");
  });

  it("detecta trilha de áudio no probe", () => {
    expect(probeHasAudio(JSON.stringify({ streams: [{ codec_type: "video" }, { codec_type: "audio" }] }))).toBe(true);
    expect(probeHasAudio(JSON.stringify({ streams: [{ codec_type: "video" }] }))).toBe(false);
    expect(probeHasAudio("x")).toBe(false);
  });
});
