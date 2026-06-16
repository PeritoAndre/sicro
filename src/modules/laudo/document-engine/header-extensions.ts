/**
 * N — Extensões TipTap dedicadas ao cabeçalho (Word-style).
 *
 * Subset REDUZIDO das `laudoExtensions()`. Princípio: o header é uma
 * região pequena e visual; não faz sentido carregar paginação, comentários,
 * revisão, sistema de quesitos, blocos dinâmicos, etc.
 *
 * O que ESTÁ aqui:
 *   - StarterKit (paragraph, heading H1/H2, bold, italic, strike, listas,
 *     hardBreak, history, dropcursor, gapcursor — SEM blockquote/code).
 *   - Underline, TextAlign (left/center/right/justify).
 *   - TextStyle + Color + FontFamily + FontSize + Highlight (pra reproduzir
 *     o "look institucional" se desejado, ex: brasão + linhas em maiúsculas
 *     centralizadas).
 *   - Image (logos/brasões institucionais).
 *   - Table (simples) — útil quando o cabeçalho precisa de coluna com
 *     brasão à esquerda + texto à direita.
 *   - Placeholder pra ajudar quando vazio.
 *
 * O que FICOU DE FORA:
 *   - Pagination (header não pagina — é replicado em cada pg).
 *   - CommentMark, RevisionMark (header não tem fluxo de revisão).
 *   - Figure/Storyboard/EvidenceTable/PhotoPlate (peso desnecessário).
 *   - QuesitoList/Signature (são do corpo).
 *   - CrossReference + DynamicLists + AutoNumbering (numeração de body).
 *   - FieldPlaceholder (campos automáticos podem ser adicionados depois
 *     se a UX pedir).
 *   - LaudoStyleAttribute (sistema de estilos do body).
 *
 * Usado tanto pelo editor interativo do header (instância TipTap dedicada
 * em EditorPage) quanto pelo renderer HTML/DOCX que reusa `generateHTML`
 * com a mesma lista.
 */

import type { Extensions } from "@tiptap/core";
import StarterKit from "@tiptap/starter-kit";
import Underline from "@tiptap/extension-underline";
import TextAlign from "@tiptap/extension-text-align";
import Image from "@tiptap/extension-image";
import Placeholder from "@tiptap/extension-placeholder";
import { TextStyle } from "@tiptap/extension-text-style";
import { Color } from "@tiptap/extension-color";
import { Highlight } from "@tiptap/extension-highlight";
import { FontFamily } from "@tiptap/extension-font-family";
import { FontSize } from "./marks/FontSize";
// Pós-laudo S — Figure node + helpers de parágrafo (indent + spacing)
// agora também disponíveis no header, pra que fotos no cabeçalho
// recebam o mesmo tratamento (overlay, handles, wrap modes) que as
// fotos do corpo.
import { Figure, FigCaption } from "./nodes/Figure";
// U — TextBox: caixa de texto editável também disponível no cabeçalho.
import { TextBox } from "./nodes/TextBox";
// Q — Shape: formas geométricas (retângulo, elipse, seta, linha) também no
// cabeçalho. Sem o nó aqui, `insertShape` não existia no editor do header e a
// forma simplesmente não era criada. Necessário também pro renderer (que usa
// estas mesmas extensões pra serializar o cabeçalho no HTML/PDF exportado).
import { Shape } from "./nodes/Shape";
// F1.2/F4 — Tabela de primeira classe também no cabeçalho/rodapé (mesmo
// resize + legenda + bordas). Configurada com o marcador do header pra o
// CSS legado do bloco de registro continuar valendo.
import { SicroTable } from "./nodes/SicroTable";
import {
  SicroTableRow,
  SicroTableCell,
  SicroTableHeader,
} from "./nodes/SicroTableParts";
import { ParagraphFirstLineIndent } from "./paragraph-indent";
import { ParagraphSpacing } from "./paragraph-spacing";
// Campos automáticos `{campo}` + autocomplete por `{` — também no cabeçalho/
// rodapé, pra escrever "Folha {page} de {pages}", "{numero_laudo}", etc.
import { FieldPlaceholder, FieldSuggestion } from "./fields";

export interface HeaderExtensionsOptions {
  placeholder?: string;
}

export function headerExtensions(
  opts?: HeaderExtensionsOptions,
): Extensions {
  return [
    StarterKit.configure({
      heading: { levels: [1, 2] },
      blockquote: false,
      code: false,
      codeBlock: false,
      // History é necessária para o Ctrl+Z funcionar dentro do header.
    }),
    Underline,
    TextAlign.configure({
      types: ["heading", "paragraph"],
      alignments: ["left", "center", "right", "justify"],
      defaultAlignment: "center", // headers institucionais geralmente
                                   // centralizam o conteúdo.
    }),
    TextStyle,
    Color,
    Highlight.configure({
      multicolor: true,
      HTMLAttributes: { class: "sicro-highlight" },
    }),
    FontFamily.configure({ types: ["textStyle"] }),
    FontSize,
    Image.configure({
      inline: false,
      allowBase64: true,
      HTMLAttributes: { class: "sicro-header-image" },
    }),
    SicroTable.configure({
      HTMLAttributes: { "data-sicro-header-table": "true" },
    }),
    SicroTableRow,
    SicroTableHeader,
    SicroTableCell,
    // Pós-laudo S — Figure + FigCaption + paragraph helpers no header.
    Figure,
    FigCaption,
    // U — TextBox no header também (mesmo tratamento que no body).
    TextBox,
    // Q — Shape no header também: habilita `insertShape` + serialização.
    Shape,
    ParagraphFirstLineIndent,
    ParagraphSpacing,
    // Campos automáticos + autocomplete por `{` no cabeçalho/rodapé.
    FieldPlaceholder,
    FieldSuggestion,
    Placeholder.configure({
      placeholder:
        opts?.placeholder ??
        "Cabeçalho do laudo — clique duplo para editar",
      includeChildren: false,
    }),
  ];
}
