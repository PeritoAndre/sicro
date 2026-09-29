/**
 * "Continuar de onde parou" — última posição de cada vídeo, por SHA-256.
 *
 * Preferência de UI por máquina (localStorage, como o keymap): não entra no
 * `.sicro`, não é dado pericial. Guarda só as 200 mais recentes.
 */

const LS_KEY = "sicro.video.resume.v1";
const MAX_ENTRIES = 200;

type Store = Record<string, { t: number; at: number }>;

function load(): Store {
  try {
    const raw = localStorage.getItem(LS_KEY);
    const obj = raw ? (JSON.parse(raw) as unknown) : null;
    return obj && typeof obj === "object" ? (obj as Store) : {};
  } catch {
    return {};
  }
}

export function loadPosition(mediaKey: string): number | null {
  const e = load()[mediaKey];
  return e && Number.isFinite(e.t) ? e.t : null;
}

export function savePosition(mediaKey: string, t: number): void {
  if (!mediaKey || !Number.isFinite(t)) return;
  try {
    const s = load();
    s[mediaKey] = { t, at: Date.now() };
    const keys = Object.keys(s);
    if (keys.length > MAX_ENTRIES) {
      keys
        .sort((a, b) => (s[a]?.at ?? 0) - (s[b]?.at ?? 0))
        .slice(0, keys.length - MAX_ENTRIES)
        .forEach((k) => delete s[k]);
    }
    localStorage.setItem(LS_KEY, JSON.stringify(s));
  } catch {
    /* localStorage indisponível — só não lembra */
  }
}
