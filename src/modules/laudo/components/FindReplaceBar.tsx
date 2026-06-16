/**
 * FindReplaceBar — barra flutuante para localizar/substituir no laudo.
 *
 * Aparece logo abaixo da toolbar quando o perito aciona Ctrl+F (localizar)
 * ou Ctrl+H (substituir). Layout compacto, sem cobrir a área de edição.
 *
 * Comportamento:
 *   - Input "Localizar" → cada keystroke executa `findAll`.
 *   - Botões ▲▼          → next/prev.
 *   - Toggle "Aa"        → case-sensitive.
 *   - Toggle "P. inteira"→ whole word.
 *   - Input "Substituir" → ativa botões "Substituir" / "Subst. tudo".
 *   - Esc / X            → fecha a barra.
 *
 * Visual segue o padrão SICRO 2.0 (var(--sicro-surface), border-radius
 * pequeno, ícones lucide). Sem dependência de bibliotecas externas.
 */

import { useEffect, useMemo, useRef, useState } from "react";
import { ChevronDown, ChevronUp, Replace, Search, X } from "lucide-react";
import type { Editor } from "@tiptap/react";
import { useFindReplace } from "../hooks/useFindReplace";
import styles from "./FindReplaceBar.module.css";

interface FindReplaceBarProps {
  editor: Editor | null;
  /** Mostrar o input de substituir? Quando false, exibe apenas localizar. */
  showReplace: boolean;
  onClose: () => void;
}

export function FindReplaceBar({
  editor,
  showReplace,
  onClose,
}: FindReplaceBarProps) {
  const { state, findAll, next, prev, replaceCurrent, replaceAll, close } =
    useFindReplace(editor);

  const [query, setQuery] = useState("");
  const [replacement, setReplacement] = useState("");
  const [caseSensitive, setCaseSensitive] = useState(false);
  const [wholeWord, setWholeWord] = useState(false);
  const queryInputRef = useRef<HTMLInputElement>(null);

  // Foca o input de localizar ao montar.
  useEffect(() => {
    queryInputRef.current?.focus();
    queryInputRef.current?.select();
  }, []);

  // Recompõe matches quando query/options mudam.
  const opts = useMemo(
    () => ({ caseSensitive, wholeWord }),
    [caseSensitive, wholeWord],
  );
  useEffect(() => {
    if (query) {
      findAll(query, opts);
    } else {
      close();
    }
  }, [query, opts, findAll, close]);

  const handleClose = () => {
    close();
    onClose();
  };

  const handleReplaceCurrent = () => {
    replaceCurrent(replacement);
  };
  const handleReplaceAll = () => {
    const n = replaceAll(replacement);
    if (n > 0) {
      // Mantemos a query para que o perito veja "0 ocorrências" depois.
      // Não fechamos automaticamente — ele pode revisar.
    }
  };

  const counterLabel =
    state.matches.length > 0
      ? `${state.current + 1} de ${state.matches.length}`
      : query.length > 0
        ? "0 ocorrências"
        : "";

  return (
    <div
      className={styles.bar}
      role="search"
      aria-label="Localizar e substituir"
    >
      <div className={styles.row}>
        <div className={styles.field}>
          <Search size={14} className={styles.icon} />
          <input
            ref={queryInputRef}
            type="text"
            className={styles.input}
            value={query}
            onChange={(e) => setQuery(e.target.value)}
            placeholder="Localizar…"
            aria-label="Texto a localizar"
            onKeyDown={(e) => {
              if (e.key === "Enter") {
                e.preventDefault();
                if (e.shiftKey) prev();
                else next();
              }
            }}
          />
          <span className={styles.counter}>{counterLabel}</span>
        </div>

        <button
          type="button"
          className={`${styles.toggle} ${caseSensitive ? styles.toggleOn : ""}`}
          onClick={() => setCaseSensitive((v) => !v)}
          title="Diferenciar maiúsculas/minúsculas"
          aria-pressed={caseSensitive}
        >
          Aa
        </button>
        <button
          type="button"
          className={`${styles.toggle} ${wholeWord ? styles.toggleOn : ""}`}
          onClick={() => setWholeWord((v) => !v)}
          title="Palavra inteira"
          aria-pressed={wholeWord}
        >
          ⎯a⎯
        </button>

        <button
          type="button"
          className={styles.iconBtn}
          onClick={prev}
          title="Anterior (Shift+Enter)"
          disabled={state.matches.length === 0}
        >
          <ChevronUp size={14} />
        </button>
        <button
          type="button"
          className={styles.iconBtn}
          onClick={next}
          title="Próximo (Enter)"
          disabled={state.matches.length === 0}
        >
          <ChevronDown size={14} />
        </button>
        <button
          type="button"
          className={styles.iconBtn}
          onClick={handleClose}
          title="Fechar (Esc)"
        >
          <X size={14} />
        </button>
      </div>

      {showReplace && (
        <div className={styles.row}>
          <div className={styles.field}>
            <Replace size={14} className={styles.icon} />
            <input
              type="text"
              className={styles.input}
              value={replacement}
              onChange={(e) => setReplacement(e.target.value)}
              placeholder="Substituir por…"
              aria-label="Texto de substituição"
            />
          </div>
          <button
            type="button"
            className={styles.action}
            onClick={handleReplaceCurrent}
            disabled={state.current < 0}
          >
            Substituir
          </button>
          <button
            type="button"
            className={styles.action}
            onClick={handleReplaceAll}
            disabled={state.matches.length === 0}
          >
            Substituir tudo
          </button>
        </div>
      )}
    </div>
  );
}
