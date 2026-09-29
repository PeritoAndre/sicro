/** Small formatters shared by the video panels. */

export function formatDuration(seconds: number): string {
  if (!Number.isFinite(seconds) || seconds < 0) return "—";
  const total = Math.round(seconds * 1000);
  const ms = total % 1000;
  const totalS = Math.floor(total / 1000);
  const s = totalS % 60;
  const m = Math.floor(totalS / 60) % 60;
  const h = Math.floor(totalS / 3600);
  if (h > 0) {
    return `${pad2(h)}:${pad2(m)}:${pad2(s)}`;
  }
  return `${pad2(m)}:${pad2(s)}.${pad3(ms)}`;
}

export function prettyBytes(n: number): string {
  if (n < 1024) return `${n} B`;
  if (n < 1024 * 1024) return `${(n / 1024).toFixed(1)} KB`;
  if (n < 1024 * 1024 * 1024) return `${(n / 1024 / 1024).toFixed(1)} MB`;
  return `${(n / 1024 / 1024 / 1024).toFixed(2)} GB`;
}

export function parseWarnings(raw: string): string[] {
  try {
    const parsed = JSON.parse(raw);
    if (Array.isArray(parsed)) return parsed.filter((x) => typeof x === "string");
  } catch {
    /* fall through */
  }
  return [];
}

function pad2(n: number): string {
  return n < 10 ? `0${n}` : String(n);
}
function pad3(n: number): string {
  if (n < 10) return `00${n}`;
  if (n < 100) return `0${n}`;
  return String(n);
}

/**
 * Índice ESTIMADO do quadro no instante `t` — mesma convenção do Rust
 * (`estimate_frame_index`: round(t × fps), contado do 0:00 da mídia), para o
 * número do player bater com o do storyboard/sidecar.
 */
export function estimateFrameIndex(t: number, fps: number | null | undefined): number | null {
  if (!fps || !Number.isFinite(fps) || fps <= 0 || !Number.isFinite(t)) return null;
  return Math.round(t * fps);
}

/**
 * Instante do PRIMEIRO quadro, lido do ffprobe (start_time do stream de vídeo,
 * ou do container). Vídeos recortados podem ter um trecho vazio no início
 * (edit list) — o 1º quadro chega depois do 0:00. Sem dado → 0.
 */
export function probeStartTime(rawProbeJson: string | null | undefined): number {
  if (!rawProbeJson) return 0;
  try {
    const p = JSON.parse(rawProbeJson) as {
      streams?: { codec_type?: string; start_time?: string | number }[];
      format?: { start_time?: string | number };
    };
    const vs = p.streams?.find((s) => s.codec_type === "video");
    const raw = vs?.start_time ?? p.format?.start_time;
    const n = typeof raw === "number" ? raw : parseFloat(raw ?? "");
    return Number.isFinite(n) && n > 0 ? n : 0;
  } catch {
    return 0;
  }
}

/**
 * Lê o que o perito digitou em "ir para": tempo ou número de quadro.
 *   "12" · "12.48" · "12,48" · "1:02.5" · "01:02:03.250"  → segundos
 *   "#312" · "q312" · "quadro 312"                       → quadro (÷ fps)
 * Retorna null se não entender (ou se pedir quadro sem fps conhecido).
 */
export function parseTimeInput(
  input: string,
  fps: number | null | undefined,
): number | null {
  const s = input.trim().toLowerCase().replace(/\s+/g, " ");
  if (!s) return null;
  const frame = /^(?:#|q|quadro\s?)\s*(\d+)$/.exec(s);
  if (frame) {
    if (!fps || !Number.isFinite(fps) || fps <= 0) return null;
    return Number(frame[1]) / fps;
  }
  const parts = s.replace(/,/g, ".").split(":");
  if (parts.length > 3 || parts.some((p) => !/^\d+(\.\d+)?$/.test(p))) return null;
  const nums = parts.map(Number);
  // Só o último campo pode ter fração; minutos/segundos intermediários < 60.
  if (nums.slice(0, -1).some((n) => !Number.isInteger(n))) return null;
  if (nums.length >= 2 && nums.slice(1).some((n) => n >= 60)) return null;
  return nums.reduce((acc, n) => acc * 60 + n, 0);
}
