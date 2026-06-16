import { describe, expect, it } from "vitest";
import { formatTranscript } from "../transcriptFormat";

describe("formatTranscript", () => {
  const segs = [
    { t_start: 5, t_end: 8, speaker: "Locutor 1", text: "alô" },
    { t_start: 65.5, t_end: null, speaker: "", text: "tudo bem?" },
  ];

  it("txt: [mm:ss] Locutor: texto", () => {
    expect(formatTranscript(segs, "txt")).toBe(
      "[0:05] Locutor 1: alô\n[1:05] tudo bem?",
    );
  });

  it("srt: blocos numerados com HH:MM:SS,mmm", () => {
    const out = formatTranscript(segs, "srt");
    expect(out).toContain("1\n00:00:05,000 --> 00:00:08,000\nLocutor 1: alô");
    // último segmento sem t_end → usa t_start + 2s
    expect(out).toContain("2\n00:01:05,500 --> 00:01:07,500\ntudo bem?");
  });

  it("srt: t_end ausente usa o início do próximo segmento", () => {
    const out = formatTranscript(
      [
        { t_start: 0, t_end: null, speaker: "A", text: "um" },
        { t_start: 3, t_end: null, speaker: "B", text: "dois" },
      ],
      "srt",
    );
    expect(out).toContain("00:00:00,000 --> 00:00:03,000");
  });

  it("trecho sem locutor não imprime prefixo", () => {
    expect(formatTranscript([{ t_start: 0, t_end: 1, speaker: "  ", text: "oi" }], "txt")).toBe(
      "[0:00] oi",
    );
  });
});
