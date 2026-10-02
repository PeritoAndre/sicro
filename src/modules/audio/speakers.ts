/**
 * Separação de locutores na degravação: nome, cor e preenchimento do campo
 * Locutor de cada trecho a partir dos turnos de fala.
 */
import type { AudioDiarization } from "@domain/audio";

/** O mínimo de um trecho que o preenchimento precisa. */
export interface SpeakerSeg {
  t_start: number;
  t_end: number | null;
  speaker: string;
  /** Locutor preenchido pela separação (o perito ainda não mexeu). */
  speakerAuto?: boolean;
  /** Há fala de mais de um locutor no trecho. */
  speakerMixed?: boolean;
}

const SPEAKER_COLORS = [
  "#4fa3e0",
  "#e0864f",
  "#5bc48a",
  "#c46bd6",
  "#e0c84f",
  "#e05b6b",
  "#4fd6c8",
  "#9a9ae0",
];
export const speakerColor = (n: number) => SPEAKER_COLORS[(n - 1) % SPEAKER_COLORS.length]!;
export const speakerName = (d: AudioDiarization, n: number) => d.names[n - 1]?.trim() || `Locutor ${n}`;

/**
 * Preenche o locutor de cada trecho pela separação (quem fala mais tempo nele).
 * Só onde o campo está vazio ou foi preenchido pela própria separação — o que
 * o perito escreveu fica. Marca "mais de uma voz" quando a 2ª ocupa ≥ 25%.
 */
export function assignSpeakers<T extends SpeakerSeg>(segs: T[], d: AudioDiarization | null): T[] {
  if (!d || d.turns.length === 0) return segs;
  return segs.map((s, i) => {
    const end = s.t_end ?? segs[i + 1]?.t_start ?? s.t_start + 5;
    const per = new Map<number, number>();
    for (const t of d.turns) {
      const ov = Math.min(end, t.t_end) - Math.max(s.t_start, t.t_start);
      if (ov > 0) per.set(t.speaker, (per.get(t.speaker) ?? 0) + ov);
    }
    const ranked = [...per.entries()].sort((a, b) => b[1] - a[1]);
    const total = ranked.reduce((acc, [, v]) => acc + v, 0);
    if (ranked.length === 0 || total <= 0) return { ...s, speakerMixed: false };
    const mixed = ranked.length > 1 && ranked[1]![1] >= 0.25 * total;
    const own = s.speaker.trim() !== "" && !s.speakerAuto;
    return own
      ? { ...s, speakerMixed: mixed }
      : { ...s, speaker: speakerName(d, ranked[0]![0]), speakerAuto: true, speakerMixed: mixed };
  });
}
