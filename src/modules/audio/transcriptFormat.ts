/**
 * Formatação de degravação para exportação — funções PURAS (sem efeitos).
 *
 * Determinístico: mesma entrada → mesma saída. É auxílio de exportação; o texto
 * é o trabalho revisado do perito (o tool não transcreve nem interpreta).
 */

export interface TranscriptSegmentLike {
  t_start: number;
  t_end: number | null;
  speaker: string;
  text: string;
}

/** Formata um instante (s) como relógio. `withMs` → SRT (HH:MM:SS,mmm). */
function clock(t: number, withMs: boolean): string {
  const safe = Number.isFinite(t) && t > 0 ? t : 0;
  const h = Math.floor(safe / 3600);
  const m = Math.floor((safe % 3600) / 60);
  const s = Math.floor(safe % 60);
  const ss = s.toString().padStart(2, "0");
  if (!withMs) {
    return h > 0 ? `${h}:${m.toString().padStart(2, "0")}:${ss}` : `${m}:${ss}`;
  }
  const ms = Math.floor((safe % 1) * 1000)
    .toString()
    .padStart(3, "0");
  return `${h.toString().padStart(2, "0")}:${m.toString().padStart(2, "0")}:${ss},${ms}`;
}

/** Fim efetivo de um segmento: `t_end`, senão início do próximo, senão +2s. */
function effectiveEnd(
  seg: TranscriptSegmentLike,
  next: TranscriptSegmentLike | undefined,
): number {
  if (seg.t_end != null && seg.t_end > seg.t_start) return seg.t_end;
  if (next && next.t_start > seg.t_start) return next.t_start;
  return seg.t_start + 2;
}

/** Serializa a degravação como texto simples ou legenda SRT. */
export function formatTranscript(
  segments: TranscriptSegmentLike[],
  fmt: "txt" | "srt",
): string {
  if (fmt === "txt") {
    return segments
      .map((s) => {
        const who = s.speaker.trim() ? `${s.speaker.trim()}: ` : "";
        return `[${clock(s.t_start, false)}] ${who}${s.text.trim()}`;
      })
      .join("\n");
  }
  return segments
    .map((s, i) => {
      const end = effectiveEnd(s, segments[i + 1]);
      const who = s.speaker.trim() ? `${s.speaker.trim()}: ` : "";
      return `${i + 1}\n${clock(s.t_start, true)} --> ${clock(end, true)}\n${who}${s.text.trim()}`;
    })
    .join("\n\n");
}
