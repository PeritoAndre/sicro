/**
 * W (fase 2b) — useFooterEditor: instância TipTap dedicada ao RODAPÉ do laudo.
 * Clone direto do `useHeaderEditor` — mesma mecânica (subset reduzido via
 * `headerExtensions()`, editable só em modo "footer", sync externo sem loop).
 * Mantido separado (em vez de generalizar) pra não desestabilizar o header,
 * que teve muitos ajustes finos.
 */

import { useEffect, useMemo, useRef } from "react";
import { useEditor } from "@tiptap/react";
import type { JSONContent } from "@tiptap/core";
import { headerExtensions } from "../document-engine";

export interface UseFooterEditorOptions {
  initialContent: JSONContent;
  /** True quando o usuário está em modo "footer". O editor fica editable. */
  editable: boolean;
  onContentChange: (content: JSONContent) => void;
  placeholder?: string;
}

export function useFooterEditor(options: UseFooterEditorOptions) {
  const { initialContent, editable, onContentChange, placeholder } = options;

  const isExternalUpdateRef = useRef(false);
  const onContentChangeRef = useRef(onContentChange);
  onContentChangeRef.current = onContentChange;

  const extensions = useMemo(
    () => headerExtensions({ placeholder: placeholder ?? "Rodapé do laudo" }),
    [placeholder],
  );

  const editor = useEditor({
    extensions,
    content: initialContent,
    editable,
    // Mesma classe do corpo/cabeçalho — em styles.css `.sicro-editor-content`
    // só governa regras de tabela (bordas, etc.), sem efeito de layout.
    editorProps: {
      attributes: {
        class: "sicro-editor-content",
        // Mesma marcação do cabeçalho: tabelas no rodapé são institucionais/
        // layout e não recebem legenda numerada (vide SicroTableView).
        "data-sicro-region": "footer",
      },
    },
    onUpdate({ editor: ed }) {
      if (isExternalUpdateRef.current) return;
      onContentChangeRef.current(ed.getJSON());
    },
  });

  useEffect(() => {
    if (!editor) return;
    editor.setEditable(editable);
  }, [editor, editable]);

  // Sync externo: aplica novo conteúdo (undo/redo, abrir outro laudo) sem
  // disparar onUpdate.
  useEffect(() => {
    if (!editor) return;
    // Mesmo guard do useHeaderEditor: com o editor FOCADO, initialContent
    // divergente é eco atrasado do próprio persist — repor mataria as
    // teclas mais novas e jogaria o cursor pro fim (cursor "pula" da
    // tabela ao digitar rápido).
    if (editor.isFocused) return;
    const currentJson = editor.getJSON();
    if (jsonContentEqual(currentJson, initialContent)) return;
    isExternalUpdateRef.current = true;
    editor.commands.setContent(initialContent, { emitUpdate: false });
    isExternalUpdateRef.current = false;
  }, [editor, initialContent]);

  return editor;
}

function jsonContentEqual(
  a: JSONContent | undefined,
  b: JSONContent | undefined,
): boolean {
  if (a === b) return true;
  if (!a || !b) return false;
  return JSON.stringify(a) === JSON.stringify(b);
}
