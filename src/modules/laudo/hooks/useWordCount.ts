/**
 * useWordCount — contagem de palavras, caracteres e parágrafos do laudo.
 *
 * Reage ao evento `editor.on("update")` mas debouncea o cálculo em ~250ms
 * para não custar a cada keystroke. Computa:
 *
 *   - words:      palavras separadas por espaço/quebra de linha.
 *   - chars:      caracteres totais (com espaços).
 *   - charsNoSpaces:  caracteres sem espaços.
 *   - paragraphs: número de nós `paragraph` no documento.
 *
 * Para textos longos o cálculo é O(n) em chars; aceitável até centenas de
 * milhares de chars. Acima disso o debounce protege a UI.
 */

import { useEffect, useRef, useState } from "react";
import type { Editor } from "@tiptap/react";

export interface WordCountStats {
  words: number;
  chars: number;
  charsNoSpaces: number;
  paragraphs: number;
}

const EMPTY: WordCountStats = {
  words: 0,
  chars: 0,
  charsNoSpaces: 0,
  paragraphs: 0,
};

export function useWordCount(editor: Editor | null): WordCountStats {
  const [stats, setStats] = useState<WordCountStats>(EMPTY);
  const timerRef = useRef<ReturnType<typeof setTimeout> | null>(null);

  useEffect(() => {
    if (!editor) {
      setStats(EMPTY);
      return undefined;
    }

    const recompute = () => {
      const text = editor.getText({ blockSeparator: "\n" });
      const chars = text.length;
      const charsNoSpaces = text.replace(/\s/g, "").length;
      const words =
        text
          .trim()
          .split(/\s+/)
          .filter((w) => w.length > 0).length;
      // Contagem de parágrafos: cruza o documento procurando nós paragraph.
      let paragraphs = 0;
      editor.state.doc.descendants((node) => {
        if (node.type.name === "paragraph") paragraphs += 1;
        return true;
      });
      setStats({ words, chars, charsNoSpaces, paragraphs });
    };

    // Snapshot inicial.
    recompute();

    const onUpdate = () => {
      if (timerRef.current !== null) clearTimeout(timerRef.current);
      timerRef.current = setTimeout(recompute, 250);
    };
    editor.on("update", onUpdate);

    return () => {
      editor.off("update", onUpdate);
      if (timerRef.current !== null) clearTimeout(timerRef.current);
    };
  }, [editor]);

  return stats;
}
