import { describe, expect, it } from "vitest";
import type { AudioDiarization } from "@domain/audio";
import { assignSpeakers, speakerName, type SpeakerSeg } from "../speakers";

const diar = (names: string[] = []): AudioDiarization => ({
  id: "d",
  occurrence_id: "o",
  audio_sha256: "h",
  turns: [
    { t_start: 1, t_end: 6, speaker: 1 },
    { t_start: 7, t_end: 12, speaker: 2 },
    { t_start: 12, t_end: 14, speaker: 1 },
  ],
  names,
  params: {},
  created_at: "",
});

describe("assignSpeakers", () => {
  it("preenche pelo locutor que fala mais tempo no trecho", () => {
    const segs: SpeakerSeg[] = [
      { t_start: 1.2, t_end: 5.8, speaker: "" },
      { t_start: 7.1, t_end: 11.5, speaker: "" },
    ];
    const out = assignSpeakers(segs, diar());
    expect(out.map((s) => s.speaker)).toEqual(["Locutor 1", "Locutor 2"]);
    expect(out.every((s) => s.speakerAuto && !s.speakerMixed)).toBe(true);
  });

  it("não sobrescreve o que o perito escreveu, mas marca 2+ vozes", () => {
    const segs: SpeakerSeg[] = [{ t_start: 7, t_end: 14, speaker: "Vítima" }];
    const [s] = assignSpeakers(segs, diar());
    expect(s!.speaker).toBe("Vítima");
    expect(s!.speakerAuto).toBeFalsy();
    // 5 s do locutor 2 e 2 s do locutor 1 (2/7 ≥ 25%).
    expect(s!.speakerMixed).toBe(true);
  });

  it("refaz o que a própria separação preencheu e usa os nomes dados", () => {
    const segs: SpeakerSeg[] = [{ t_start: 1, t_end: 6, speaker: "Locutor 2", speakerAuto: true }];
    const [s] = assignSpeakers(segs, diar(["Entrevistador"]));
    expect(s!.speaker).toBe("Entrevistador");
  });

  it("trecho sem fim usa o início do próximo; fora da fala não mexe", () => {
    const segs: SpeakerSeg[] = [
      { t_start: 7, t_end: null, speaker: "" },
      { t_start: 12.5, t_end: null, speaker: "" },
      { t_start: 30, t_end: 31, speaker: "" },
    ];
    const out = assignSpeakers(segs, diar());
    expect(out.map((s) => s.speaker)).toEqual(["Locutor 2", "Locutor 1", ""]);
  });

  it("nome vazio volta a ser Locutor N", () => {
    expect(speakerName(diar(["", " "]), 2)).toBe("Locutor 2");
  });
});
