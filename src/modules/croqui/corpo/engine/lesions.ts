/**
 * Taxonomia de lesões/achados (POP SENASP "Local de Crime" vol. 6): categorias
 * pro perito classificar o que observou; o sistema não diagnostica.
 * `icon` é só uma chave — a UI mapeia pra lucide, o engine fica sem React.
 */

export type LesaoTipo =
  | "faf_entrada"
  | "faf_saida"
  | "arma_branca"
  | "contusao"
  | "queimadura"
  | "asfixia"
  | "mordida"
  | "defesa"
  | "balistico"
  | "outro";

interface LesaoTipoMeta {
  tipo: LesaoTipo;
  label: string;
  /** Abreviatura exibida dentro do marcador, quando cabe. */
  short: string;
  /** Cor do marcador (hex). */
  color: string;
  /** Chave de ícone lucide, resolvida na UI. */
  icon: string;
}

export const LESAO_TIPOS: LesaoTipoMeta[] = [
  {
    tipo: "faf_entrada",
    label: "FAF — orifício de entrada",
    short: "E",
    color: "#dc2626", // red-600
    icon: "Target",
  },
  {
    tipo: "faf_saida",
    label: "FAF — orifício de saída",
    short: "S",
    color: "#991b1b", // red-800
    icon: "CircleDot",
  },
  {
    tipo: "arma_branca",
    label: "Arma branca (perfuro/cortante)",
    short: "AB",
    color: "#2563eb", // blue-600
    icon: "Slice",
  },
  {
    tipo: "contusao",
    label: "Contusão / hematoma / escoriação",
    short: "C",
    color: "#7c3aed", // violet-600
    icon: "Circle",
  },
  {
    tipo: "queimadura",
    label: "Queimadura",
    short: "Q",
    color: "#ea580c", // orange-600
    icon: "Flame",
  },
  {
    tipo: "asfixia",
    label: "Asfixia (sulco / escoriação cervical)",
    short: "Af",
    color: "#059669", // emerald-600
    icon: "Minus",
  },
  {
    tipo: "mordida",
    label: "Mordida",
    short: "M",
    color: "#d97706", // amber-600
    icon: "Brackets",
  },
  {
    tipo: "defesa",
    label: "Lesão de defesa",
    short: "D",
    color: "#0891b2", // cyan-600
    icon: "Shield",
  },
  {
    tipo: "balistico",
    label: "Achado balístico (projétil / estojo / fragmento)",
    short: "B",
    color: "#475569", // slate-600
    icon: "Diamond",
  },
  {
    tipo: "outro",
    label: "Outro / achado genérico",
    short: "O",
    color: "#334155", // slate-700
    icon: "MapPin",
  },
];

const TIPO_INDEX: Record<LesaoTipo, LesaoTipoMeta> = LESAO_TIPOS.reduce(
  (acc, m) => {
    acc[m.tipo] = m;
    return acc;
  },
  {} as Record<LesaoTipo, LesaoTipoMeta>,
);

export function lesaoMeta(tipo: LesaoTipo): LesaoTipoMeta {
  return TIPO_INDEX[tipo] ?? TIPO_INDEX.outro;
}

export function isLesaoTipo(v: unknown): v is LesaoTipo {
  return typeof v === "string" && v in TIPO_INDEX;
}
