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

// ---- último módulo de trabalho de cada caso (o Início volta direto nele) ----
const WORK_KEY = "sicro.ultimoModulo.v2";
const WORK_PATHS = ["/croqui", "/imagem", "/video", "/audio"];

/** Guarda em que módulo o caso `caseId` foi trabalhado por último. */
export function rememberWorkModule(pathname: string, caseId: string | null | undefined): void {
  if (!caseId) return;
  const hit = WORK_PATHS.find((p) => pathname === p || pathname.startsWith(p + "/"));
  if (!hit) return;
  try {
    localStorage.setItem(`${WORK_KEY}.${caseId}`, hit);
  } catch {
    /* só não lembra */
  }
}

/**
 * Onde abrir o caso `caseId`: o último módulo usado nele, ou `null` se ainda
 * não houve trabalho (o Início fica mostrando os módulos para escolher).
 */
export function lastWorkModuleOf(caseId: string): string | null {
  try {
    const v = localStorage.getItem(`${WORK_KEY}.${caseId}`);
    return v && WORK_PATHS.includes(v) ? v : null;
  } catch {
    return null;
  }
}
