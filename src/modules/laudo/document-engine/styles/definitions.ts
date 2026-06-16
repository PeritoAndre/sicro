/**
 * Catálogo central de estilos documentais do laudo pericial.
 *
 * Cada estilo é um preset declarativo que mapeia para:
 *   - Um `target` (nó TipTap onde se aplica: `paragraph` ou `heading` + level);
 *   - Uma `cssClass` aplicada ao nó via atributo `data-laudo-style="<id>"`;
 *   - Um `shortcut` opcional (string descritiva apenas — atalhos reais ficam
 *     no `useLaudoShortcuts`).
 *
 * **Filosofia:** a fonte de verdade do estilo é o atributo no nó (persiste
 * no `.sicrodoc`). O CSS é apenas a renderização. Mudar a aparência de
 * "Quesito" no futuro = mudar a regra CSS, sem migrar documentos.
 *
 * Tipos cobertos:
 *   1. Estrutura: Normal, Título 1-3, Subtítulo, Seção técnica
 *   2. Pericial: Quesito, Resposta, Legenda, Citação, Observação,
 *      Conclusão, Advertência, Assinatura
 *
 * Para adicionar um novo estilo:
 *   1. Acrescente entrada em `LAUDO_STYLES`.
 *   2. Adicione regra correspondente em `styles.css`.
 *   3. (Opcional) Adicione atalho em `useLaudoShortcuts`.
 */

export type LaudoStyleId =
  // Estrutura
  | "normal"
  | "titulo_1"
  | "titulo_2"
  | "titulo_3"
  | "subtitulo"
  | "secao_tecnica"
  // Pericial
  | "quesito"
  | "resposta"
  | "legenda"
  | "citacao"
  | "observacao"
  | "conclusao"
  | "advertencia"
  | "assinatura";

/** Onde o estilo pode ser aplicado. */
export type LaudoStyleTarget =
  | { kind: "paragraph" }
  | { kind: "heading"; level: 1 | 2 | 3 };

export interface LaudoStyleDefinition {
  id: LaudoStyleId;
  /** Nome exibido na UI (dropdown, painel de estilos). */
  label: string;
  /** Descrição curta — tooltip / preview. */
  description: string;
  /** Categoria para agrupamento visual no painel. */
  category: "estrutura" | "pericial";
  /** Em qual nó TipTap aplicar. */
  target: LaudoStyleTarget;
  /** Atalho descritivo (string apenas, sem listener). */
  shortcut?: string;
  /**
   * Cor / sample que o painel de estilos usa para mostrar um preview.
   * Não é o CSS real — o CSS real vive em `styles.css`.
   */
  preview?: {
    fontWeight?: number | string;
    fontSize?: string;
    color?: string;
    background?: string;
    fontStyle?: "italic" | "normal";
    borderLeft?: string;
    textAlign?: "left" | "center" | "right" | "justify";
  };
}

// ---------------------------------------------------------------------------
// Catálogo central.

export const LAUDO_STYLES: ReadonlyArray<LaudoStyleDefinition> = [
  // ESTRUTURA ----------------------------------------------------------------
  {
    id: "normal",
    label: "Normal",
    description: "Corpo do laudo — parágrafo padrão.",
    category: "estrutura",
    target: { kind: "paragraph" },
    shortcut: "Ctrl+Alt+0",
    preview: { fontSize: "12pt", color: "#1a1a1a" },
  },
  {
    id: "titulo_1",
    label: "Título 1",
    description: "Cabeçalho principal do laudo. Usado uma vez no topo.",
    category: "estrutura",
    target: { kind: "heading", level: 1 },
    shortcut: "Ctrl+Alt+1",
    preview: {
      fontWeight: 700,
      fontSize: "18pt",
      textAlign: "center",
      color: "#0f172a",
    },
  },
  {
    id: "titulo_2",
    label: "Título 2",
    description: "Seções principais (PREÂMBULO, HISTÓRICO, EXAMES…).",
    category: "estrutura",
    target: { kind: "heading", level: 2 },
    shortcut: "Ctrl+Alt+2",
    preview: { fontWeight: 700, fontSize: "14pt", color: "#1e293b" },
  },
  {
    id: "titulo_3",
    label: "Título 3",
    description: "Subseções dentro de uma seção maior.",
    category: "estrutura",
    target: { kind: "heading", level: 3 },
    shortcut: "Ctrl+Alt+3",
    preview: { fontWeight: 600, fontSize: "12.5pt", color: "#334155" },
  },
  {
    id: "subtitulo",
    label: "Subtítulo",
    description: "Linha de subtítulo após o Título 1.",
    category: "estrutura",
    target: { kind: "paragraph" },
    shortcut: "Ctrl+Alt+4",
    preview: {
      fontStyle: "italic",
      fontSize: "13pt",
      textAlign: "center",
      color: "#475569",
    },
  },
  {
    id: "secao_tecnica",
    label: "Seção técnica",
    description: "Título de bloco técnico destacado (tabela, prancha, anexo).",
    category: "estrutura",
    target: { kind: "paragraph" },
    shortcut: "Ctrl+Alt+5",
    preview: {
      fontWeight: 600,
      fontSize: "11.5pt",
      color: "#0f172a",
      background: "#f1f5f9",
    },
  },

  // PERICIAL -----------------------------------------------------------------
  {
    id: "quesito",
    label: "Quesito",
    description: "Pergunta formal do quesito (numerada em série).",
    category: "pericial",
    target: { kind: "paragraph" },
    shortcut: "Ctrl+Alt+6",
    preview: {
      fontWeight: 600,
      fontSize: "12pt",
      borderLeft: "3px solid #3b82f6",
      color: "#1e3a8a",
    },
  },
  {
    id: "resposta",
    label: "Resposta",
    description: "Resposta autoral do perito ao quesito.",
    category: "pericial",
    target: { kind: "paragraph" },
    shortcut: "Ctrl+Alt+7",
    preview: {
      fontSize: "12pt",
      borderLeft: "3px solid #94a3b8",
      color: "#1a1a1a",
    },
  },
  {
    id: "legenda",
    label: "Legenda",
    description: "Texto explicativo abaixo de figura, tabela ou prancha.",
    category: "pericial",
    target: { kind: "paragraph" },
    preview: {
      fontStyle: "italic",
      fontSize: "10pt",
      textAlign: "center",
      color: "#475569",
    },
  },
  {
    id: "citacao",
    label: "Citação",
    description: "Trecho transcrito de norma, regulamento ou laudo externo.",
    category: "pericial",
    target: { kind: "paragraph" },
    preview: {
      fontStyle: "italic",
      fontSize: "11pt",
      borderLeft: "3px solid #cbd5e1",
      color: "#334155",
    },
  },
  {
    id: "observacao",
    label: "Observação",
    description: "Ressalva técnica fora do fluxo principal do raciocínio.",
    category: "pericial",
    target: { kind: "paragraph" },
    preview: {
      fontSize: "10.5pt",
      background: "#fef3c7",
      color: "#78350f",
      borderLeft: "3px solid #f59e0b",
    },
  },
  {
    id: "conclusao",
    label: "Conclusão",
    description: "Síntese conclusiva do perito (autoral, não automatizável).",
    category: "pericial",
    target: { kind: "paragraph" },
    preview: {
      fontWeight: 600,
      fontSize: "12pt",
      borderLeft: "3px solid #16a34a",
      color: "#14532d",
      background: "#f0fdf4",
    },
  },
  {
    id: "advertencia",
    label: "Advertência",
    description: "Aviso técnico crítico (limitação, impossibilidade).",
    category: "pericial",
    target: { kind: "paragraph" },
    preview: {
      fontWeight: 600,
      fontSize: "11pt",
      background: "#fee2e2",
      color: "#7f1d1d",
      borderLeft: "3px solid #dc2626",
    },
  },
  {
    id: "assinatura",
    label: "Assinatura",
    description: "Linha de assinatura (Macapá, data, perito).",
    category: "pericial",
    target: { kind: "paragraph" },
    preview: {
      fontSize: "11pt",
      textAlign: "center",
      color: "#1a1a1a",
    },
  },
];

/** Lookup rápido por id. */
export const LAUDO_STYLES_BY_ID: ReadonlyMap<LaudoStyleId, LaudoStyleDefinition> =
  new Map(LAUDO_STYLES.map((s) => [s.id, s]));

/** Categoria → lista, para agrupar no painel. */
export function laudoStylesByCategory(): {
  estrutura: LaudoStyleDefinition[];
  pericial: LaudoStyleDefinition[];
} {
  return {
    estrutura: LAUDO_STYLES.filter((s) => s.category === "estrutura"),
    pericial: LAUDO_STYLES.filter((s) => s.category === "pericial"),
  };
}

/** Resolve o `data-laudo-style` para um estilo conhecido, ou null. */
export function findLaudoStyle(
  id: string | null | undefined,
): LaudoStyleDefinition | null {
  if (!id) return null;
  return LAUDO_STYLES_BY_ID.get(id as LaudoStyleId) ?? null;
}
