/**
 * Central list of TipTap extensions used by the SICRO laudo editor.
 *
 * Two callers must use the same list:
 *   1. The interactive editor (`useEditor({ extensions: laudoExtensions() })`);
 *   2. The HTML renderer (`generateHTML(doc, laudoExtensions())`).
 *
 * Keep this list canonical — diverging the two sides causes silent rendering
 * mismatches that are painful to debug.
 */

import type { Extensions } from "@tiptap/core";
import StarterKit from "@tiptap/starter-kit";
import Underline from "@tiptap/extension-underline";
import TextAlign from "@tiptap/extension-text-align";
import Image from "@tiptap/extension-image";
import Placeholder from "@tiptap/extension-placeholder";
// F2 — Edição rica: marca cor de fonte + realce + sub/sobrescrito + família/tamanho.
//
// `TextStyle` é a marca base que armazena estilos inline (cor, fontFamily,
// fontSize). `Color` / `FontFamily` decoram `TextStyle` com comandos
// específicos (`setColor`, `setFontFamily`). `Highlight` é independente —
// equivalente ao marca-texto. `Subscript` e `Superscript` são marcas
// próprias com comandos `toggleSubscript` / `toggleSuperscript`.
import { TextStyle } from "@tiptap/extension-text-style";
import { Color } from "@tiptap/extension-color";
import { Highlight } from "@tiptap/extension-highlight";
import { Subscript } from "@tiptap/extension-subscript";
import { Superscript } from "@tiptap/extension-superscript";
import { FontFamily } from "@tiptap/extension-font-family";
import {
  CrossReference,
  DynamicFigureList,
  DynamicSummary,
  DynamicTableList,
  EvidenceTable,
  FigCaption,
  Figure,
  MathInline,
  MathBlock,
  PhotoPlate,
  Shape,
  SicroTable,
  SicroTableRow,
  SicroTableCell,
  SicroTableHeader,
  TextBox,
  QuesitoAnswer,
  QuesitoItem,
  QuesitoList,
  QuesitoQuestion,
  Signature,
  Storyboard,
  StoryboardItem,
  SystemData,
} from "./nodes";
import { FontSize } from "./marks/FontSize";
// F8 — Marca para ancorar comentários a um intervalo de texto.
import { CommentMark } from "./marks/CommentMark";
// F8 — Marca de revisão (insertion/deletion) — track-changes lite.
import { RevisionMark } from "./marks/RevisionMark";
// F4 — Sistema de estilos documentais.
//
// `LaudoStyleAttribute` adiciona o atributo `data-laudo-style` aos nós
// paragraph + heading. Junto com `styles.css` (importado pelo EditorPage)
// e o helper `applyLaudoStyle`, isso cobre os 12+ estilos do catálogo
// pericial. Não cria novos nodes — mantém persistência simples.
import { LaudoStyleAttribute } from "./styles";
// F5 — Campos automáticos `{{var}}`. Node atômico inline que armazena
// apenas a chave do campo; resolução de valor acontece em runtime
// (renderer / painel / validação). Permite que dados do caso atualizem
// AUTOMATICAMENTE em todos os placeholders sem re-edição manual.
import { FieldPlaceholder, FieldSuggestion } from "./fields";
// F7.3 — Paginação real via plugin ProseMirror: insere spacers entre
// blocos para empurrar conteúdo para a próxima página. O editor continua
// sendo UM contenteditable mas visualmente vira "Word-like" — cada
// página fica fisicamente separada.
import { Pagination } from "./pagination";
// F12.1 — Auto-numeração dinâmica de figuras/tabelas/quesitos via
// Decorations. Re-numera ao inserir/remover. Map IDs→ordinal exposto
// para cross-references consumirem.
import { AutoNumbering } from "./auto-numbering";
// R — Recuo da primeira linha estilo Word (text-indent).
import { ParagraphFirstLineIndent } from "./paragraph-indent";
// Pós-laudo S — line-height + space-before/after estilo Word.
import { ParagraphSpacing } from "./paragraph-spacing";

export interface LaudoExtensionsOptions {
  placeholder?: string;
  /** F7.3 — Habilita paginação real (default true para edição). Quando
   *  false (renderer HTML/PDF), os spacers somem e o conteúdo fica
   *  numa coluna contínua. */
  pagination?: {
    enabled?: boolean;
    pageHeightCm?: number;
    marginTopCm?: number;
    marginBottomCm?: number;
    gapCm?: number;
  };
}

export function laudoExtensions(opts?: LaudoExtensionsOptions): Extensions {
  return [
    // StarterKit ships paragraph, heading, bold, italic, strike, lists, blockquote,
    // code, codeBlock, hardBreak, history, dropcursor, gapcursor, horizontalRule.
    StarterKit.configure({
      heading: { levels: [1, 2, 3] },
    }),
    Underline,
    TextAlign.configure({
      types: ["heading", "paragraph"],
      alignments: ["left", "center", "right", "justify"],
      defaultAlignment: "left",
    }),
    // F2 — base para cor + fontFamily + fontSize.
    //
    // Sem `TextStyle` registrado, `Color` e `FontFamily` não conseguem
    // armazenar atributos inline. Mantemos a configuração padrão (aplica
    // sobre nós `paragraph` e `heading`).
    TextStyle,
    Color,
    Highlight.configure({
      multicolor: true,
      HTMLAttributes: { class: "sicro-highlight" },
    }),
    Subscript,
    Superscript,
    FontFamily.configure({
      types: ["textStyle"],
    }),
    FontSize,
    // F8 — Marca de comentário (data-comment-id).
    CommentMark,
    // F8 — Marca de revisão (data-revision-id, type=insertion|deletion).
    RevisionMark,
    Image.configure({
      // We DON'T use the plain image node directly — figures should go through
      // the Figure custom node so they get captions and numbering. But the
      // image extension still has to be registered for legacy/raw pastes.
      inline: false,
      allowBase64: true,
    }),
    // F1.2/F4 — Tabela de primeira classe: resize de coluna + legenda +
    // bordas/align/padding (ver SicroTable). `resizable: true` exige
    // `table-layout: fixed` (styles.css) e colwidths semeados no insert.
    // Linhas/células estendidas: altura de linha (F3) + valign (F4).
    SicroTable,
    SicroTableRow,
    SicroTableHeader,
    SicroTableCell,
    Figure,
    FigCaption,
    // Q — Shape: formas geométricas flutuantes pra anotação.
    Shape,
    // U — TextBox: caixa de texto editável (objeto), com border/fill/rotation.
    TextBox,
    // F6 — Prancha fotográfica.
    PhotoPlate,
    Storyboard,
    StoryboardItem,
    SystemData,
    // MVP 4 — evidence-driven block
    EvidenceTable,
    // MVP 2 — institutional blocks
    QuesitoList,
    QuesitoItem,
    QuesitoQuestion,
    QuesitoAnswer,
    Signature,
    // F12.2 — Cross-references inline (ver Figura N).
    CrossReference,
    // Fórmula matemática (LaTeX) — inline + bloco, exibida via PNG.
    MathInline,
    MathBlock,
    // F12.3 — Listas dinâmicas (sumário, figuras, tabelas).
    DynamicSummary,
    DynamicFigureList,
    DynamicTableList,
    Placeholder.configure({
      placeholder:
        opts?.placeholder ?? "Comece a escrever o laudo ou insira uma seção…",
      includeChildren: false,
    }),
    // F4 — atributo data-laudo-style nos nós paragraph + heading.
    LaudoStyleAttribute,
    // R — first_line_indent_cm nos nós paragraph + heading (estilo Word).
    ParagraphFirstLineIndent,
    // Pós-laudo S — line_height + space_before_pt + space_after_pt.
    ParagraphSpacing,
    // F5 — placeholder `{var}` reativo (pílula com valor resolvido).
    FieldPlaceholder,
    // Autocomplete estilo IDE: digite `{` pra abrir o menu de campos.
    // Inerte no renderer (generateHTML não cria view → o plugin não roda).
    FieldSuggestion,
    // F12.1 — Auto-numeração dinâmica.
    AutoNumbering,
    // F7.3 — Paginação real (spacers de quebra de página).
    Pagination.configure({
      enabled: opts?.pagination?.enabled ?? true,
      pageHeightCm: opts?.pagination?.pageHeightCm ?? 29.7,
      marginTopCm: opts?.pagination?.marginTopCm ?? 3,
      marginBottomCm: opts?.pagination?.marginBottomCm ?? 2.5,
      gapCm: opts?.pagination?.gapCm ?? 0.7,
    }),
  ];
}
