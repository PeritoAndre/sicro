/** Formatadores e leitores de probe compartilhados pelos painéis de vídeo. */

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
    /* não é JSON: sem avisos */
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
 * Quadro estimado = round(t × fps), contado do 0:00 da mídia — mesma convenção
 * do Rust (`estimate_frame_index`), para o player bater com o storyboard.
 */
export function estimateFrameIndex(t: number, fps: number | null | undefined): number | null {
  if (!fps || !Number.isFinite(fps) || fps <= 0 || !Number.isFinite(t)) return null;
  return Math.round(t * fps);
}

/**
 * Instante do 1º quadro (start_time do stream de vídeo, ou do container).
 * Vídeo recortado pode ter trecho vazio no início (edit list). Sem dado → 0.
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
 * "Ir para": tempo ("12,48", "1:02.5", "01:02:03.250") ou quadro ("#312",
 * "q312", "quadro 312" → ÷ fps). Null se não entender ou sem fps para quadro.
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

// relógio da câmera / texto para o laudo

/** Horário impresso pela câmera ("03:36:05", "03:36", "03h36m05s") → s desde 00:00, ou null. */
export function parseClockInput(input: string): number | null {
  const s = input.trim().toLowerCase().replace(/,/g, ".");
  const hms = /^(\d{1,2})\s*[:h]\s*(\d{1,2})(?:\s*[:m]\s*(\d{1,2}(?:\.\d+)?)\s*s?)?$/.exec(s);
  if (!hms) return null;
  const h = Number(hms[1]);
  const m = Number(hms[2]);
  const sec = hms[3] != null ? Number(hms[3]) : 0;
  if (h > 23 || m > 59 || sec >= 60) return null;
  return h * 3600 + m * 60 + sec;
}

/** 13565.48 → "03:46:05.480" (volta em 24 h). */
export function formatClock(seconds: number, withMs = true): string {
  const day = 86400;
  const t = ((seconds % day) + day) % day;
  const totalMs = Math.round(t * 1000);
  const ms = totalMs % 1000;
  const totalS = Math.floor(totalMs / 1000) % day;
  const h = Math.floor(totalS / 3600);
  const m = Math.floor(totalS / 60) % 60;
  const s = totalS % 60;
  const base = `${pad2(h)}:${pad2(m)}:${pad2(s)}`;
  return withMs ? `${base}.${pad3(ms)}` : base;
}

/** Horário da câmera (s desde 00:00, pode passar de 24 h) no instante `t` do vídeo. */
export function cameraClockAt(
  t: number,
  cal: { media_time_s: number; clock_seconds: number },
): number {
  return t - cal.media_time_s + cal.clock_seconds;
}

/** Mesmo instante real em duas câmeras calibradas: tempo_B = tempo_A + offset. */
export function clockSyncOffset(
  a: { media_time_s: number; clock_seconds: number },
  b: { media_time_s: number; clock_seconds: number },
): number {
  return a.clock_seconds - a.media_time_s - (b.clock_seconds - b.media_time_s);
}

/** Tempo no formato de laudo: 12.48 → "00:00:12,480". */
export function formatLaudoTime(seconds: number): string {
  if (!Number.isFinite(seconds) || seconds < 0) return "—";
  const totalMs = Math.round(seconds * 1000);
  const ms = totalMs % 1000;
  const totalS = Math.floor(totalMs / 1000);
  return `${pad2(Math.floor(totalS / 3600))}:${pad2(Math.floor(totalS / 60) % 60)}:${pad2(totalS % 60)},${pad3(ms)}`;
}

/** O ffprobe achou alguma trilha de áudio? */
export function probeHasAudio(rawProbeJson: string | null | undefined): boolean {
  if (!rawProbeJson) return false;
  try {
    const p = JSON.parse(rawProbeJson) as { streams?: { codec_type?: string }[] };
    return (p.streams ?? []).some((s) => s.codec_type === "audio");
  } catch {
    return false;
  }
}

/**
 * Início da trilha de áudio no tempo do vídeo (s): tempo do WAV extraído =
 * tempo do vídeo − este valor. Sem trilha declarada, cai no início do vídeo.
 */
export function audioStreamStart(rawProbeJson: string | null | undefined): number {
  if (!rawProbeJson) return 0;
  try {
    const p = JSON.parse(rawProbeJson) as {
      streams?: { codec_type?: string; start_time?: string | number }[];
    };
    const as = p.streams?.find((s) => s.codec_type === "audio");
    const raw = as?.start_time;
    const n = typeof raw === "number" ? raw : parseFloat(raw ?? "");
    return Number.isFinite(n) && n > 0 ? n : probeStartTime(rawProbeJson);
  } catch {
    return 0;
  }
}

/** Lê o `derivation_json` de um trecho exportado (null se não for trecho). */
export function parseDerivation(
  json: string | null | undefined,
): import("@domain/video").ClipDerivation | null {
  if (!json) return null;
  try {
    const d = JSON.parse(json) as { kind?: string };
    return d && d.kind === "clip" ? (d as import("@domain/video").ClipDerivation) : null;
  } catch {
    return null;
  }
}
