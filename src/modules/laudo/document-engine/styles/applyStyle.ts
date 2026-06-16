/**
 * applyLaudoStyle — helper centralizado para aplicar um estilo do catálogo
 * a um TipTap Editor.
 *
 * Por que um helper e não chamar `setLaudoStyle` direto?
 *
 *   - O catálogo distingue estilos de PARÁGRAFO (Normal, Quesito, Citação…)
 *     dos estilos de HEADING (Título 1-3). Para um estilo de heading, o
 *     bloco atual precisa virar `heading` com o `level` correto ANTES de
 *     receber o atributo `data-laudo-style`.
 *   - Para estilos de parágrafo, se o bloco atual é heading, primeiro
 *     converte para paragraph.
 *   - Esse helper esconde essa coreografia. Caller só fala "aplica estilo X".
 *
 * Uso típico:
 *
 *   applyLaudoStyle(editor, "quesito");
 *   applyLaudoStyle(editor, "titulo_2");
 *   removeLaudoStyle(editor);
 */

import type { Editor } from "@tiptap/core";
import {
  findLaudoStyle,
  type LaudoStyleDefinition,
  type LaudoStyleId,
} from "./definitions";

/** Aplica um estilo conhecido. No-op silencioso se id desconhecido. */
export function applyLaudoStyle(editor: Editor, id: LaudoStyleId): boolean {
  const def = findLaudoStyle(id);
  if (!def) return false;
  return applyDefinition(editor, def);
}

/** Remove o atributo `data-laudo-style` do bloco atual. */
export function removeLaudoStyle(editor: Editor): boolean {
  return editor.chain().focus().unsetLaudoStyle().run();
}

/**
 * Verifica se o bloco atual carrega um estilo do catálogo. Retorna o id
 * ou null.
 */
export function getCurrentLaudoStyle(editor: Editor): LaudoStyleId | null {
  // Lemos os atributos de paragraph E heading; o que tiver `laudoStyle`
  // setado é o estilo ativo. Se ambos estão setados (não deveria, mas
  // documento legado pode ter), heading ganha porque é mais específico.
  const headingAttrs = editor.getAttributes("heading") as {
    laudoStyle?: string | null;
  };
  if (headingAttrs.laudoStyle) {
    return headingAttrs.laudoStyle as LaudoStyleId;
  }
  const paragraphAttrs = editor.getAttributes("paragraph") as {
    laudoStyle?: string | null;
  };
  if (paragraphAttrs.laudoStyle) {
    return paragraphAttrs.laudoStyle as LaudoStyleId;
  }
  return null;
}

// ---------------------------------------------------------------------------
// Helper interno — coreografia paragraph ↔ heading.

function applyDefinition(
  editor: Editor,
  def: LaudoStyleDefinition,
): boolean {
  const chain = editor.chain().focus();
  if (def.target.kind === "heading") {
    // Converte para heading com o level correto + aplica atributo.
    return chain
      .setNode("heading", { level: def.target.level })
      .setLaudoStyle(def.id)
      .run();
  }
  // Paragraph — se estamos em heading, força paragraph primeiro.
  return chain.setParagraph().setLaudoStyle(def.id).run();
}
