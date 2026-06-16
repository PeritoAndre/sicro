/**
 * extractOutline — extrai a estrutura de seções do documento.
 *
 * Percorre o JSONContent do TipTap procurando por nós `heading` em
 * qualquer profundidade. Para cada heading encontrado, registra:
 *
 *   - `level`        — 1, 2 ou 3 (atributo nativo do TipTap heading);
 *   - `text`         — texto plano concatenado dos descendentes;
 *   - `pos`          — posição ProseMirror do nó no documento, útil
 *                       para `setTextSelection` no clique do
 *                       NavigationPanel;
 *   - `laudoStyle`   — atributo `data-laudo-style` quando presente
 *                       (Título 1/2/3 ou Subtítulo).
 *
 * Não modifica o documento — leitura pura. Pode ser chamado a cada
 * `editor.on("update")` sem problema (com debounce no consumidor).
 *
 * Limitações conscientes:
 *   - Subtítulo (paragraph com `data-laudo-style="subtitulo"`) também
 *     entra no outline mas com `level: 0` para sinalizar que NÃO é
 *     heading nativo — útil para mostrar no painel mas pular na
 *     numeração automática.
 *   - Headings dentro de tabelas são incluídos.
 */

import type { JSONContent } from "@tiptap/core";

export interface OutlineEntry {
  /** 0 (subtítulo), 1, 2 ou 3 (heading levels). */
  level: 0 | 1 | 2 | 3;
  /** Texto puro do heading. Strings vazias mantidas para preservar gaps. */
  text: string;
  /**
   * Posição ProseMirror do nó. Quando o documento é montado num editor
   * real, `editor.commands.setTextSelection(entry.pos + 1)` posiciona
   * o cursor logo após a tag de abertura do heading.
   */
  pos: number;
  /** Atributo `data-laudo-style` se houver. */
  laudoStyle?: string;
}

/**
 * Caminha pelo JSONContent acumulando posições. Como o JSONContent
 * não carrega posições absolutas (diferente do Doc ProseMirror), nós
 * computamos via offset semelhante ao algoritmo do ProseMirror:
 *
 *   - Um nó "bloco" abre uma tag de 1 ponto.
 *   - Cada texto consome `text.length` pontos.
 *   - Nós aninhados são percorridos recursivamente.
 *
 * O resultado é compatível com `editor.state.doc.descendants` (mesma
 * semântica de offset).
 */
export function extractOutline(content: JSONContent): OutlineEntry[] {
  const out: OutlineEntry[] = [];
  walk(content, 0, out);
  return out;
}

function walk(node: JSONContent, basePos: number, out: OutlineEntry[]): number {
  const type = node.type;
  if (!type) return basePos;

  // O nó raiz (`doc`) não conta como "posição" — entramos direto
  // nos filhos sem incrementar.
  if (type === "doc") {
    let p = basePos;
    for (const child of node.content ?? []) {
      p = walk(child, p, out);
    }
    return p;
  }

  // Texto puro: avança `text.length`, nada para registrar.
  if (type === "text") {
    return basePos + (node.text?.length ?? 0);
  }

  // Nó "bloco" — abrimos com offset +1 (igual ProseMirror).
  const openPos = basePos;
  let inner = basePos + 1;

  // Heading nativo → registra.
  if (type === "heading") {
    const lvl = clampHeadingLevel(node.attrs?.["level"]);
    if (lvl !== null) {
      out.push({
        level: lvl,
        text: collectText(node),
        pos: openPos,
        ...(node.attrs?.["laudoStyle"] && typeof node.attrs["laudoStyle"] === "string"
          ? { laudoStyle: node.attrs["laudoStyle"] as string }
          : {}),
      });
    }
  }

  // Subtítulo (paragraph com data-laudo-style="subtitulo") → registra com level 0.
  if (
    type === "paragraph" &&
    typeof node.attrs?.["laudoStyle"] === "string" &&
    node.attrs["laudoStyle"] === "subtitulo"
  ) {
    out.push({
      level: 0,
      text: collectText(node),
      pos: openPos,
      laudoStyle: "subtitulo",
    });
  }

  // Recursão nos filhos.
  for (const child of node.content ?? []) {
    inner = walk(child, inner, out);
  }
  // O fechamento do bloco também consome +1.
  return inner + 1;
}

function clampHeadingLevel(value: unknown): 1 | 2 | 3 | null {
  if (typeof value !== "number") return null;
  if (value === 1 || value === 2 || value === 3) return value;
  return null;
}

function collectText(node: JSONContent): string {
  let buf = "";
  const visit = (n: JSONContent) => {
    if (n.type === "text" && typeof n.text === "string") {
      buf += n.text;
    }
    if (n.content) for (const c of n.content) visit(c);
  };
  visit(node);
  return buf.trim();
}
