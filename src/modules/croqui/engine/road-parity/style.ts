/**
 * Estilo de desenho das vias (tema do croqui). Só aparência: o modelo da via
 * não muda. Fica em `doc.style` como parcial; ausente ⇒ planta técnica.
 */

export type ParityTema = "tecnico" | "pb" | "escuro";

export type ParityCalcadaEstilo = "hachura" | "cinza" | "linha" | "nenhuma";

export interface ParityStyle {
  tema: ParityTema;
  /** Cor do asfalto. */
  asfalto: string;
  /** Cor do meio-fio (borda da pista). */
  borda: string;
  /** Espessura do meio-fio, em px de canvas. */
  borda_px: number;
  /** Espessura das linhas de sinalização, em px de canvas. */
  marcacao_px: number;
  amarela: string;
  branca: string;
  calcada: ParityCalcadaEstilo;
  /** Largura padrão da calçada (m); a via pode sobrescrever. */
  calcada_m: number;
  /** Cadência do tracejado (m) — CONTRAN urbano: 2 / 4. */
  traco_m: number;
  espaco_m: number;
  /** Divide a pista em faixas de ~3,5 m com branca tracejada. */
  faixas_auto: boolean;
}

export const PARITY_TEMAS: Record<ParityTema, ParityStyle> = {
  tecnico: {
    tema: "tecnico",
    asfalto: "#e6e6e6",
    borda: "#111111",
    borda_px: 1.5,
    marcacao_px: 2,
    amarela: "#d6a200",
    branca: "#ffffff",
    calcada: "hachura",
    calcada_m: 2,
    traco_m: 2,
    espaco_m: 4,
    faixas_auto: true,
  },
  pb: {
    tema: "pb",
    asfalto: "#ffffff",
    borda: "#000000",
    borda_px: 1.5,
    marcacao_px: 1.5,
    amarela: "#000000",
    branca: "#000000",
    calcada: "hachura",
    calcada_m: 2,
    traco_m: 2,
    espaco_m: 4,
    faixas_auto: true,
  },
  // A pele do SICRO 1.0–4.1.
  escuro: {
    tema: "escuro",
    asfalto: "#1c1c1c",
    borda: "#ffffff",
    borda_px: 2,
    marcacao_px: 2,
    amarela: "#f5c518",
    branca: "#ffffff",
    calcada: "cinza",
    calcada_m: 2,
    traco_m: 1.2,
    espaco_m: 0.8,
    faixas_auto: false,
  },
};

export const PARITY_STYLE_DEFAULT: ParityStyle = PARITY_TEMAS.tecnico;

/** Completa um estilo parcial com o tema dele (ou o técnico). */
export function resolveParityStyle(
  partial?: Partial<ParityStyle> | null,
): ParityStyle {
  const base = PARITY_TEMAS[partial?.tema ?? "tecnico"] ?? PARITY_STYLE_DEFAULT;
  if (!partial) return base;
  const out: ParityStyle = { ...base };
  for (const k of Object.keys(base) as (keyof ParityStyle)[]) {
    const v = partial[k];
    if (v === undefined || v === null) continue;
    if (typeof v === typeof base[k]) (out as unknown as Record<string, unknown>)[k] = v;
  }
  return out;
}

/** Cores derivadas do tema para o que não tem campo próprio. */
export function parityTemaColors(style: ParityStyle): {
  calcada: string;
  hachuraLinha: string;
  hachuraFundo: string;
  ilha: string;
  terra: string;
} {
  switch (style.tema) {
    case "pb":
      return { calcada: "#eeeeee", hachuraLinha: "#000000", hachuraFundo: "#ffffff", ilha: "#ffffff", terra: "#f2f2f2" };
    case "escuro":
      return { calcada: "#7c7460", hachuraLinha: "#5c5648", hachuraFundo: "#7c7460", ilha: "#3a6535", terra: "#9c7a4e" };
    default:
      return { calcada: "#dcdcdc", hachuraLinha: "#9a9a9a", hachuraFundo: "#f5f5f5", ilha: "#d9e8cf", terra: "#d8c9a8" };
  }
}
