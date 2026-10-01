/**
 * "Vídeo e Áudio" — um módulo só no SICRO 4.0, com duas abas (Vídeos, Áudios)
 * na barra do topo. Cada aba é a tela de sempre (/video, /audio); aqui fica
 * qual aba foi usada por último, para a entrada do trilho voltar a ela.
 */
const KEY = "sicro.midia.aba.v1";

export type MidiaAba = "/video" | "/audio";

export function isMidiaPath(pathname: string): boolean {
  return pathname === "/video" || pathname === "/audio" || pathname.startsWith("/audio/");
}

export function lastMidiaAba(): MidiaAba {
  try {
    return localStorage.getItem(KEY) === "/audio" ? "/audio" : "/video";
  } catch {
    return "/video";
  }
}

export function rememberMidiaAba(pathname: string): void {
  if (!isMidiaPath(pathname)) return;
  try {
    localStorage.setItem(KEY, pathname.startsWith("/audio") ? "/audio" : "/video");
  } catch {
    /* só não lembra */
  }
}

// ---- último módulo de trabalho (para o "Continuar ocorrência" do Início) ----
const WORK_KEY = "sicro.ultimoModulo.v1";
const WORK_PATHS = ["/croqui", "/imagem", "/video", "/audio"];

export function rememberWorkModule(pathname: string): void {
  const hit = WORK_PATHS.find((p) => pathname === p || pathname.startsWith(p + "/"));
  if (!hit) return;
  try {
    localStorage.setItem(WORK_KEY, hit);
  } catch {
    /* só não lembra */
  }
}

/** Onde o "Continuar ocorrência" leva: o último módulo usado (padrão: Vídeo e Áudio). */
export function lastWorkModule(): string {
  try {
    const v = localStorage.getItem(WORK_KEY);
    return v && WORK_PATHS.includes(v) ? v : lastMidiaAba();
  } catch {
    return "/video";
  }
}
