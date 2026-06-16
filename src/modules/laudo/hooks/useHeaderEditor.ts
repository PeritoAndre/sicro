/**
 * N — useHeaderEditor: instância TipTap dedicada ao cabeçalho do laudo.
 *
 * Diferente do editor do body, este editor:
 *   - usa `headerExtensions()` (subset reduzido, sem pagination/comments/
 *     blocos pesados);
 *   - é editable APENAS quando `editingRegion === "header"`;
 *   - sincroniza com `doc.header.content` no envelope `.sicrodoc`;
 *   - dispara `onContentChange` debounced para o caller persistir.
 *
 * A sincronização externa (quando o store atualiza `doc.header.content` por
 * undo/redo ou load de outro laudo) é feita via `editor.commands.setContent`
 * só quando o conteúdo difere — evita loop com o próprio `onContentChange`.
 */

import { useEffect, useMemo, useRef } from "react";
import { useEditor } from "@tiptap/react";
import type { JSONContent } from "@tiptap/core";
import { headerExtensions } from "../document-engine";

export interface UseHeaderEditorOptions {
  /** Conteúdo inicial do cabeçalho (ProseMirror JSON). */
  initialContent: JSONContent;
  /** True quando o usuário está em modo "header". O editor fica editable. */
  editable: boolean;
  /** Disparado quando o conteúdo do header muda por edição do usuário.
   *  Debounce no caller — este hook NÃO debounce, só repassa cada update. */
  onContentChange: (content: JSONContent) => void;
  /** Placeholder mostrado quando o header está vazio. */
  placeholder?: string;
}

export function useHeaderEditor(options: UseHeaderEditorOptions) {
  const {
    initialContent,
    editable,
    onContentChange,
    placeholder,
  } = options;

  // Ref pra evitar loop quando aplicamos setContent vindo de fora.
  const isExternalUpdateRef = useRef(false);
  const onContentChangeRef = useRef(onContentChange);
  onContentChangeRef.current = onContentChange;

  // Extensions criadas uma vez (placeholder estável).
  const extensions = useMemo(
    () => headerExtensions({ placeholder }),
    [placeholder],
  );

  const editor = useEditor({
    extensions,
    content: initialContent,
    editable,
    // V2 — mesma classe do editor do corpo. Em styles.css, `.sicro-editor-content`
    // governa SOMENTE regras de tabela (bordas, padding de célula, célula vazia
    // com min-height, selectedCell). Dar a classe ao header faz tabelas do
    // cabeçalho (ex: bloco de registro 1×3) terem borda igual ao corpo, sem
    // outros efeitos colaterais de layout.
    editorProps: {
      attributes: {
        class: "sicro-editor-content",
        // Marca a REGIÃO no DOM do editor. O NodeView da tabela
        // (SicroTableView) lê isto pra saber que está no cabeçalho — tabelas
        // de cabeçalho/rodapé são institucionais/layout (ex.: bloco de
        // registro) e NÃO recebem a legenda numerada "Tabela N — …".
        "data-sicro-region": "header",
      },
    },
    onUpdate({ editor: ed }) {
      if (isExternalUpdateRef.current) return;
      onContentChangeRef.current(ed.getJSON());
    },
  });

  // Sincroniza editable quando o modo muda.
  useEffect(() => {
    if (!editor) return;
    editor.setEditable(editable);
  }, [editor, editable]);

  // Sincronização externa: quando o `initialContent` muda por algo que NÃO
  // foi este editor (undo de margem que carrega outro header, abertura de
  // outro laudo, etc.), aplicamos o novo conteúdo via setContent sem
  // disparar onUpdate.
  useEffect(() => {
    if (!editor) return;
    // FOCADO = o usuário está digitando AQUI: o editor é a fonte da verdade
    // e qualquer `initialContent` divergente é um ECO ATRASADO do próprio
    // onContentChange (persist debounced → store → prop). Sem este guard,
    // digitar rápido (mais teclas que o eco) fazia o setContent REPOR o
    // conteúdo velho e jogar o cursor pro fim do doc — era o "cursor sai da
    // tabela e escreve embaixo" no cabeçalho. Conteúdo externo legítimo
    // (abrir outro laudo, restaurar versão) chega com o editor desfocado.
    if (editor.isFocused) return;
    const currentJson = editor.getJSON();
    if (jsonContentEqual(currentJson, initialContent)) return;
    isExternalUpdateRef.current = true;
    // emitUpdate: false — não dispara onUpdate ao aplicar conteúdo externo.
    editor.commands.setContent(initialContent, { emitUpdate: false });
    isExternalUpdateRef.current = false;
  }, [editor, initialContent]);

  return editor;
}

/** Comparação shallow-ish entre dois JSONContent. Como ambos vêm do mesmo
 *  ProseMirror, comparar por JSON.stringify é suficiente — é estável e
 *  rápido o bastante pro tamanho típico de um header. */
function jsonContentEqual(
  a: JSONContent | undefined,
  b: JSONContent | undefined,
): boolean {
  if (a === b) return true;
  if (!a || !b) return false;
  return JSON.stringify(a) === JSON.stringify(b);
}
