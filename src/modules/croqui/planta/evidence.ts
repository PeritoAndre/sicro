/**
 * Marcadores de evidência/vestígio do croqui de planta — taxonomia + legenda.
 *
 * Camada PERICIAL sobre o motor de planta (arcada): o perito marca vestígios
 * sobre a planta (sangue, projétil, arma, ponto de entrada/arrombamento, corpo…)
 * com rótulo sequencial (letra A,B,C… ou número 1,2,3…) e uma legenda é montada
 * deterministicamente. §13: o sistema só rotula/organiza; não infere nada.
 */

export type EvidenceTipo =
  | "sangue"
  | "projetil"
  | "estojo"
  | "arma"
  | "entrada"
  | "fuga"
  | "corpo"
  | "arrombamento"
  | "vestigio"
  | "outro";

export type EvidenceLabelKind = "letra" | "numero";

export interface EvidenceMeta {
  tipo: EvidenceTipo;
  label: string; // nome por extenso (legenda)
  short: string; // abreviação
  color: string; // cor do marcador (#rrggbb)
}

export const EVIDENCE_TIPOS: EvidenceMeta[] = [
  { tipo: "sangue", label: "Mancha de sangue", short: "Sangue", color: "#b91c1c" },
  { tipo: "projetil", label: "Projétil / fragmento", short: "Projétil", color: "#1d4ed8" },
  { tipo: "estojo", label: "Estojo / cápsula deflagrada", short: "Estojo", color: "#0e7490" },
  { tipo: "arma", label: "Arma", short: "Arma", color: "#7c2d12" },
  { tipo: "entrada", label: "Ponto de entrada", short: "Entrada", color: "#15803d" },
  { tipo: "fuga", label: "Rota de fuga / saída", short: "Fuga", color: "#a16207" },
  { tipo: "corpo", label: "Corpo / cadáver", short: "Corpo", color: "#111827" },
  { tipo: "arrombamento", label: "Arrombamento / escalada", short: "Arromb.", color: "#9333ea" },
  { tipo: "vestigio", label: "Vestígio genérico", short: "Vestígio", color: "#475569" },
  { tipo: "outro", label: "Outro", short: "Outro", color: "#525252" },
];

const EVIDENCE_INDEX: Record<string, EvidenceMeta> = EVIDENCE_TIPOS.reduce(
  (acc, m) => {
    acc[m.tipo] = m;
    return acc;
  },
  {} as Record<string, EvidenceMeta>,
);

export function evidenceMeta(tipo: EvidenceTipo): EvidenceMeta {
  return EVIDENCE_INDEX[tipo] ?? EVIDENCE_INDEX["outro"]!;
}

export function isEvidenceTipo(v: unknown): v is EvidenceTipo {
  return typeof v === "string" && v in EVIDENCE_INDEX;
}

/**
 * Rótulo a partir da ordem (1-based): "letra" → A,B,…,Z,AA,AB…; "numero" → 1,2…
 */
export function evidenceLabelFor(seq: number, kind: EvidenceLabelKind): string {
  if (kind === "numero") return String(seq);
  // base-26 (A=1)
  let n = Math.max(1, Math.floor(seq));
  let out = "";
  while (n > 0) {
    const rem = (n - 1) % 26;
    out = String.fromCharCode(65 + rem) + out;
    n = Math.floor((n - 1) / 26);
  }
  return out;
}
