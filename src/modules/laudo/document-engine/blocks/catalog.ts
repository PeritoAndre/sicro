/**
 * Catálogo de blocos de texto reutilizáveis (F10).
 *
 * Cada bloco gera um JSONContent compatível com TipTap. Categorias:
 *   - "abertura":   blocos para iniciar laudos (intro padrão, contextualização)
 *   - "exames":     metodologias clássicas (vistoria local, análise documental)
 *   - "conclusao":  fórmulas conclusivas comuns
 *   - "ressalvas":  notas de ressalva (limitações da prova, conservação)
 *   - "anexos":     transições para anexos / pranchas
 *
 * Blocos personalizados do perito ficam no localStorage como JSON,
 * carregados em paralelo aos blocos built-in via `loadCustomBlocks`.
 */

import type { JSONContent } from "@tiptap/core";

export type BlockCategory =
  | "abertura"
  | "exames"
  | "conclusao"
  | "ressalvas"
  | "anexos";

export interface BlockDefinition {
  id: string;
  label: string;
  description: string;
  category: BlockCategory;
  /** Build the TipTap content (array de nós para inserir após o cursor). */
  build: () => JSONContent[];
  /** Quando true, é bloco do usuário (vindo do localStorage). */
  custom?: boolean;
}

const LOCAL_STORAGE_KEY = "sicro-laudo-blocks-custom-v1";

// ---------------------------------------------------------------------------
// Helpers de construção.

function p(text: string, style?: string): JSONContent {
  const attrs: Record<string, unknown> = {};
  if (style) attrs["data-laudo-style"] = style;
  return {
    type: "paragraph",
    attrs: Object.keys(attrs).length ? attrs : undefined,
    content: text ? [{ type: "text", text }] : [],
  };
}

/** Parágrafo com inlines mistos (strings → texto; objetos → nodes). Usado
 *  quando o bloco precisa de uma pílula `fieldPlaceholder` no meio do texto. */
function pInline(
  parts: ReadonlyArray<string | JSONContent>,
  style?: string,
): JSONContent {
  const attrs: Record<string, unknown> = {};
  if (style) attrs["data-laudo-style"] = style;
  return {
    type: "paragraph",
    attrs: Object.keys(attrs).length ? attrs : undefined,
    content: parts
      .map((part) =>
        typeof part === "string"
          ? part
            ? ({ type: "text", text: part } as JSONContent)
            : null
          : part,
      )
      .filter((n): n is JSONContent => n !== null),
  };
}

/** Pílula de campo automático (FieldPlaceholder). */
function field(key: string): JSONContent {
  return { type: "fieldPlaceholder", attrs: { field: key } };
}

function h(level: 1 | 2 | 3, text: string, style?: string): JSONContent {
  const attrs: Record<string, unknown> = { level };
  if (style) attrs["data-laudo-style"] = style;
  return {
    type: "heading",
    attrs,
    content: [{ type: "text", text }],
  };
}

// ---------------------------------------------------------------------------
// Blocos built-in.

const BUILTIN: BlockDefinition[] = [
  {
    id: "abertura_padrao",
    category: "abertura",
    label: "Abertura padrão",
    description:
      "Texto introdutório clássico (atendimento à requisição, perito designado).",
    build: () => [
      p(
        "Em atendimento à requisição constante nos autos, o(a) perito(a) signatário(a), no exercício de suas funções, procedeu ao exame pericial descrito a seguir.",
      ),
    ],
  },
  {
    id: "abertura_local",
    category: "abertura",
    label: "Contextualização de local",
    description: "Descrição da chegada ao local da ocorrência.",
    build: () => [
      pInline([
        "O exame foi realizado em ",
        field("municipio"),
        ", no endereço informado pela autoridade requisitante, conforme se descreve nos itens subsequentes.",
      ]),
    ],
  },
  {
    id: "exames_vistoria",
    category: "exames",
    label: "Vistoria do local",
    description: "Cabeçalho + parágrafo introdutório da seção de vistoria.",
    build: () => [
      h(2, "DA VISTORIA DO LOCAL", "titulo_2"),
      p(
        "No local sob exame, foram identificados e registrados fotograficamente os vestígios de interesse pericial, conforme descrição a seguir.",
      ),
    ],
  },
  {
    id: "exames_documental",
    category: "exames",
    label: "Análise documental",
    description: "Cabeçalho + intro para análise de documentos/mídias.",
    build: () => [
      h(2, "DA ANÁLISE DOCUMENTAL", "titulo_2"),
      p(
        "Foram analisados os documentos e/ou mídias entregues à perícia, sendo seu conteúdo confrontado com o conjunto probatório disponível.",
      ),
    ],
  },
  {
    id: "exames_quesitos_intro",
    category: "exames",
    label: "Intro de quesitos",
    description: "Texto antes do bloco de quesitos.",
    build: () => [
      h(2, "DOS QUESITOS", "titulo_2"),
      p(
        "Passa-se a responder, com base nos exames realizados, aos quesitos formulados pela autoridade requisitante.",
      ),
    ],
  },
  {
    id: "conclusao_padrao",
    category: "conclusao",
    label: "Conclusão padrão",
    description: "Frase de fechamento padrão para laudos completos.",
    build: () => [
      h(2, "DA CONCLUSÃO", "titulo_2"),
      p(
        "Com base nos exames realizados e nos elementos técnicos analisados, conclui-se o que foi exposto nas seções anteriores deste laudo, ficando à disposição da autoridade requisitante para os esclarecimentos que se fizerem necessários.",
        "conclusao",
      ),
    ],
  },
  {
    id: "conclusao_inconclusiva",
    category: "conclusao",
    label: "Conclusão inconclusiva",
    description: "Texto quando a perícia não pode afirmar/negar com segurança.",
    build: () => [
      h(2, "DA CONCLUSÃO", "titulo_2"),
      p(
        "Diante das limitações apontadas e do estado dos elementos analisados, não foi possível à perícia firmar conclusão categórica, restringindo-se às observações descritas neste laudo.",
        "conclusao",
      ),
    ],
  },
  {
    id: "ressalva_conservacao",
    category: "ressalvas",
    label: "Ressalva de conservação",
    description: "Indica preservação parcial do local/objeto.",
    build: () => [
      p(
        "Ressalta-se que o local/objeto submetido a exame encontrava-se parcialmente preservado, fato que pode ter influenciado o resultado dos exames descritos.",
        "observacao",
      ),
    ],
  },
  {
    id: "ressalva_limitacao",
    category: "ressalvas",
    label: "Limitação técnica",
    description: "Indica limitação dos meios/equipamentos disponíveis.",
    build: () => [
      p(
        "Considere-se que os exames foram limitados aos meios técnicos disponíveis ao perito signatário e às condições do material submetido a análise.",
        "observacao",
      ),
    ],
  },
  {
    id: "anexos_pranchas",
    category: "anexos",
    label: "Transição para pranchas",
    description: "Frase de encaminhamento para o anexo fotográfico.",
    build: () => [
      h(2, "ANEXO – PRANCHAS FOTOGRÁFICAS", "titulo_2"),
      p(
        "Seguem as pranchas fotográficas referenciadas no corpo deste laudo. As fotografias foram realizadas pelo perito signatário, durante o exame.",
      ),
    ],
  },
  {
    id: "anexos_croqui",
    category: "anexos",
    label: "Transição para croqui",
    description: "Frase de encaminhamento para o anexo de croqui.",
    build: () => [
      h(2, "ANEXO – CROQUI DO LOCAL", "titulo_2"),
      p(
        "Segue o croqui do local de exame, elaborado pelo perito signatário com base nas medições in loco e nas referências geográficas levantadas.",
      ),
    ],
  },
];

// ---------------------------------------------------------------------------
// Custom blocks (localStorage).

export function loadCustomBlocks(): BlockDefinition[] {
  if (typeof localStorage === "undefined") return [];
  try {
    const raw = localStorage.getItem(LOCAL_STORAGE_KEY);
    if (!raw) return [];
    const parsed = JSON.parse(raw) as Array<{
      id: string;
      label: string;
      description?: string;
      category?: string;
      content: JSONContent[];
    }>;
    return parsed.map((b) => ({
      id: b.id,
      label: b.label,
      description: b.description ?? "",
      category: (b.category as BlockCategory) ?? "abertura",
      build: () => b.content,
      custom: true,
    }));
  } catch {
    return [];
  }
}

export function saveCustomBlock(opts: {
  label: string;
  description?: string;
  category: BlockCategory;
  content: JSONContent[];
}): BlockDefinition {
  const existing = loadRaw();
  const id = `custom-${Date.now().toString(36)}-${Math.floor(Math.random() * 1e6).toString(36)}`;
  const next = [
    ...existing,
    {
      id,
      label: opts.label,
      description: opts.description,
      category: opts.category,
      content: opts.content,
    },
  ];
  saveRaw(next);
  return {
    id,
    label: opts.label,
    description: opts.description ?? "",
    category: opts.category,
    build: () => opts.content,
    custom: true,
  };
}

export function deleteCustomBlock(id: string): void {
  const existing = loadRaw();
  saveRaw(existing.filter((b) => b.id !== id));
}

function loadRaw(): Array<{
  id: string;
  label: string;
  description?: string;
  category?: string;
  content: JSONContent[];
}> {
  if (typeof localStorage === "undefined") return [];
  try {
    const raw = localStorage.getItem(LOCAL_STORAGE_KEY);
    return raw ? JSON.parse(raw) : [];
  } catch {
    return [];
  }
}

function saveRaw(
  data: Array<{
    id: string;
    label: string;
    description?: string;
    category?: string;
    content: JSONContent[];
  }>,
) {
  if (typeof localStorage === "undefined") return;
  try {
    localStorage.setItem(LOCAL_STORAGE_KEY, JSON.stringify(data));
  } catch {
    // localStorage may throw QuotaExceeded etc — silently ignore.
  }
}

// ---------------------------------------------------------------------------
// Registry helpers.

export function listAllBlocks(): BlockDefinition[] {
  return [...BUILTIN, ...loadCustomBlocks()];
}

export function listBlocksByCategory(
  cat: BlockCategory,
): BlockDefinition[] {
  return listAllBlocks().filter((b) => b.category === cat);
}

export function findBlock(id: string): BlockDefinition | null {
  return listAllBlocks().find((b) => b.id === id) ?? null;
}

export const BLOCK_CATEGORIES: ReadonlyArray<{
  id: BlockCategory;
  label: string;
}> = [
  { id: "abertura", label: "Abertura" },
  { id: "exames", label: "Exames" },
  { id: "conclusao", label: "Conclusão" },
  { id: "ressalvas", label: "Ressalvas" },
  { id: "anexos", label: "Anexos" },
];

export const BUILTIN_BLOCKS = BUILTIN;
