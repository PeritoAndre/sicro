/**
 * NavigationPanel — outline clicável do laudo.
 *
 * F4 — substitui o `OutlinePanel` legado do Inspector com:
 *
 *   - Numeração automática (1, 1.1, 1.1.1) calculada via `numberOutline`.
 *   - Indentação visual por nível (h1 → h2 → h3 → subtítulo).
 *   - Click → `editor.commands.setTextSelection(pos)` + `scrollIntoView()`.
 *   - Indicador do heading que contém o cursor atualmente (highlight).
 *   - Estado vazio explicativo quando não há headings.
 *
 * Atualiza ao vivo via `editor.on("update" | "selectionUpdate")`. A
 * recomputação é leve (O(n) sobre o JSONContent) — sem debounce
 * necessário até documentos com 200+ headings.
 */

import { useEffect, useMemo, useState } from "react";
import type { Editor } from "@tiptap/react";
import { ListTree } from "lucide-react";
import {
  extractOutline,
  numberOutline,
  type NumberedOutlineEntry,
} from "../document-engine";
import styles from "./NavigationPanel.module.css";

interface NavigationPanelProps {
  editor: Editor | null;
}

export function NavigationPanel({ editor }: NavigationPanelProps) {
  const [outline, setOutline] = useState<NumberedOutlineEntry[]>([]);
  const [currentPos, setCurrentPos] = useState<number | null>(null);

  // Recomputa o outline sempre que o documento muda.
  useEffect(() => {
    if (!editor) {
      setOutline([]);
      return undefined;
    }
    const recompute = () => {
      const raw = extractOutline(editor.getJSON());
      setOutline(numberOutline(raw));
    };
    recompute();
    editor.on("update", recompute);
    return () => {
      editor.off("update", recompute);
    };
  }, [editor]);

  // Acompanha a posição do cursor para destacar o heading "atual".
  useEffect(() => {
    if (!editor) return undefined;
    const sync = () => setCurrentPos(editor.state.selection.from);
    sync();
    editor.on("selectionUpdate", sync);
    return () => {
      editor.off("selectionUpdate", sync);
    };
  }, [editor]);

  // Calcula qual entrada do outline contém o cursor.
  // O heading "atual" é o último cuja `pos` é <= cursor.
  const activeIndex = useMemo(() => {
    if (currentPos == null || outline.length === 0) return -1;
    let active = -1;
    for (let i = 0; i < outline.length; i++) {
      if ((outline[i] as NumberedOutlineEntry).pos <= currentPos) active = i;
      else break;
    }
    return active;
  }, [currentPos, outline]);

  if (!editor) {
    return (
      <p className={styles.empty}>
        Abra um laudo para visualizar a estrutura.
      </p>
    );
  }

  if (outline.length === 0) {
    return (
      <>
        <h3 className={styles.sectionTitle}>
          <ListTree size={14} /> Estrutura
        </h3>
        <p className={styles.empty}>
          Nenhum título declarado ainda. Use a barra de ferramentas
          (Texto/Título/Seção) ou o painel de estilos para criar a
          espinha do laudo.
        </p>
      </>
    );
  }

  const handleJump = (pos: number) => {
    if (!editor) return;
    // Selecionamos `pos + 1` para posicionar o cursor dentro do heading
    // (não na fronteira anterior).
    editor.commands.focus();
    editor.commands.setTextSelection(pos + 1);
    editor.commands.scrollIntoView();
  };

  return (
    <>
      <h3 className={styles.sectionTitle}>
        <ListTree size={14} /> Estrutura
      </h3>
      <p className={styles.intro}>
        Click para ir até a seção. Números 1, 1.1, 1.1.1 são automáticos.
      </p>

      <div className={styles.list}>
        {outline.map((entry, idx) => (
          <button
            key={`${entry.pos}-${idx}`}
            type="button"
            className={`${styles.item} ${
              entry.level === 0
                ? styles.itemSubtitle
                : entry.level === 1
                  ? styles.itemH1
                  : entry.level === 2
                    ? styles.itemH2
                    : styles.itemH3
            } ${idx === activeIndex ? styles.itemActive : ""}`}
            onClick={() => handleJump(entry.pos)}
            title={entry.text || "(sem título)"}
          >
            {entry.numero && (
              <span className={styles.itemNumber}>{entry.numero}</span>
            )}
            <span className={styles.itemText}>
              {entry.text || (
                <em className={styles.itemEmpty}>(sem título)</em>
              )}
            </span>
          </button>
        ))}
      </div>
    </>
  );
}
