/** Regras da tela "Novo croqui viário": tamanho em metros vira folha em px com a escala já definida. */

/** Lado maior do PNG exportado (A4 a 300 dpi): a folha nasce nessa resolução. */
export const PNG_LADO_MAIOR_PX = 3508;

/** Tamanhos prontos (m); a cena da prévia diz para que serve cada um. */
/** Tamanhos prontos (m); o perito guarda os dele ao lado (`cenasSalvas`). */
export const CENA_PRESETS: ReadonlyArray<{ w: number; h: number }> = [
  { w: 20, h: 20 },
  { w: 30, h: 40 },
  { w: 60, h: 100 },
  { w: 100, h: 100 },
];

const CHAVE_SALVAS = "sicro.croqui.cenasSalvas";

/** Tamanhos que o perito salvou nesta máquina. */
export function cenasSalvas(): { w: number; h: number }[] {
  try {
    const v = JSON.parse(localStorage.getItem(CHAVE_SALVAS) ?? "[]") as unknown;
    return Array.isArray(v) ? v.filter((c): c is { w: number; h: number } => !!c && typeof c.w === "number" && typeof c.h === "number") : [];
  } catch {
    return [];
  }
}

export function salvarCena(w: number, h: number): { w: number; h: number }[] {
  const lista = cenasSalvas().filter((c) => !(c.w === w && c.h === h));
  lista.push({ w, h });
  localStorage.setItem(CHAVE_SALVAS, JSON.stringify(lista));
  return lista;
}

export function esquecerCena(w: number, h: number): { w: number; h: number }[] {
  const lista = cenasSalvas().filter((c) => !(c.w === w && c.h === h));
  localStorage.setItem(CHAVE_SALVAS, JSON.stringify(lista));
  return lista;
}

export const CENA_PADRAO = { w: 30, h: 40 };
export const CENA_MIN_M = 5;
export const CENA_MAX_M = 2000;

/** px/m que faz o lado maior da cena caber no PNG. */
export function pxPorMetroParaCena(wM: number, hM: number): number {
  const maior = Math.max(wM, hM, 1);
  return Math.max(1, Math.floor(PNG_LADO_MAIOR_PX / maior));
}

export function folhaEmPx(wM: number, hM: number): { width_px: number; height_px: number; px_per_m: number } {
  const ppm = pxPorMetroParaCena(wM, hM);
  return { width_px: Math.round(wM * ppm), height_px: Math.round(hM * ppm), px_per_m: ppm };
}

export const limitarCena = (v: number): number => Math.min(CENA_MAX_M, Math.max(CENA_MIN_M, Math.round(v)));

/** Nome sugerido quando o perito não digita um: "Croqui viário N". */
export function nomePadrao(existentes: string[]): string {
  let n = 1;
  while (existentes.includes(`Croqui viário ${n}`)) n++;
  return `Croqui viário ${n}`;
}
