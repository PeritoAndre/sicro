/**
 * useFindReplace — motor de localizar/substituir sobre o TipTap.
 *
 * Estratégia: percorre o documento ProseMirror coletando posições de
 * cada ocorrência da query (case-sensitive ou case-insensitive). Não
 * decora visualmente (sem highlights persistentes) — apenas seleciona
 * a ocorrência atual via `editor.commands.setTextSelection(...)`.
 *
 * Funcionalidades:
 *   - findAll(query, opts)        — recomputa matches.
 *   - next() / prev()             — navega entre matches.
 *   - replaceCurrent(replacement) — substitui só o match atual.
 *   - replaceAll(replacement)     — substitui todos.
 *   - close()                     — limpa estado.
 *
 * Limitações conscientes:
 *   - Não procura dentro de atributos (alt de imagem, src, etc.).
 *   - Quebra de linha entre nós conta como espaço.
 *   - Match em texto rico preserva marks (replacement entra como texto puro).
 */

import { useCallback, useMemo, useRef, useState } from "react";
import type { Editor } from "@tiptap/react";

export interface FindReplaceOptions {
  caseSensitive?: boolean;
  /** Quando true, exige que o match seja palavra inteira (`\bquery\b`). */
  wholeWord?: boolean;
}

export interface FindMatch {
  from: number;
  to: number;
}

export interface FindReplaceState {
  query: string;
  matches: FindMatch[];
  current: number;
  options: FindReplaceOptions;
}

const EMPTY_STATE: FindReplaceState = {
  query: "",
  matches: [],
  current: -1,
  options: {},
};

export function useFindReplace(editor: Editor | null) {
  const [state, setState] = useState<FindReplaceState>(EMPTY_STATE);
  // Guardamos a query+options em ref para reuso dentro de replaceCurrent etc.
  const stateRef = useRef(state);
  stateRef.current = state;

  const findAll = useCallback(
    (query: string, opts: FindReplaceOptions = {}) => {
      if (!editor || !query) {
        setState({ ...EMPTY_STATE, options: opts, query });
        return;
      }
      const matches = collectMatches(editor, query, opts);
      // Se houver matches, selecionamos o primeiro automaticamente.
      const current = matches.length > 0 ? 0 : -1;
      setState({ query, matches, current, options: opts });
      if (current >= 0) {
        const m = matches[0] as FindMatch;
        editor.commands.setTextSelection({ from: m.from, to: m.to });
        editor.commands.scrollIntoView();
      }
    },
    [editor],
  );

  const moveBy = useCallback(
    (delta: 1 | -1) => {
      if (!editor) return;
      const s = stateRef.current;
      if (s.matches.length === 0) return;
      const nextIdx =
        (s.current + delta + s.matches.length) % s.matches.length;
      const m = s.matches[nextIdx] as FindMatch;
      editor.commands.setTextSelection({ from: m.from, to: m.to });
      editor.commands.scrollIntoView();
      setState({ ...s, current: nextIdx });
    },
    [editor],
  );

  const next = useCallback(() => moveBy(1), [moveBy]);
  const prev = useCallback(() => moveBy(-1), [moveBy]);

  const replaceCurrent = useCallback(
    (replacement: string) => {
      if (!editor) return;
      const s = stateRef.current;
      if (s.current < 0 || s.matches.length === 0) return;
      const m = s.matches[s.current] as FindMatch;
      editor
        .chain()
        .focus()
        .insertContentAt({ from: m.from, to: m.to }, replacement)
        .run();
      // Re-coleta — posições mudam após replace.
      const remaining = collectMatches(editor, s.query, s.options);
      const nextCurrent = Math.min(s.current, remaining.length - 1);
      setState({ ...s, matches: remaining, current: nextCurrent });
      if (nextCurrent >= 0) {
        const next = remaining[nextCurrent] as FindMatch;
        editor.commands.setTextSelection({ from: next.from, to: next.to });
      }
    },
    [editor],
  );

  const replaceAll = useCallback(
    (replacement: string): number => {
      if (!editor) return 0;
      const s = stateRef.current;
      if (s.matches.length === 0) return 0;
      const total = s.matches.length;
      // Aplicamos do FIM para o início para preservar posições anteriores.
      const ordered = [...s.matches].sort((a, b) => b.from - a.from);
      let chain = editor.chain().focus();
      for (const m of ordered) {
        chain = chain.insertContentAt({ from: m.from, to: m.to }, replacement);
      }
      chain.run();
      // Após substituir tudo, normalmente não há mais matches.
      const remaining = collectMatches(editor, s.query, s.options);
      setState({
        ...s,
        matches: remaining,
        current: remaining.length > 0 ? 0 : -1,
      });
      return total;
    },
    [editor],
  );

  const close = useCallback(() => {
    setState(EMPTY_STATE);
  }, []);

  return useMemo(
    () => ({
      state,
      findAll,
      next,
      prev,
      replaceCurrent,
      replaceAll,
      close,
    }),
    [state, findAll, next, prev, replaceCurrent, replaceAll, close],
  );
}

// ---------------------------------------------------------------------------
// Helper interno — varre o documento coletando matches.

function collectMatches(
  editor: Editor,
  query: string,
  opts: FindReplaceOptions,
): FindMatch[] {
  if (!query) return [];
  const flags = opts.caseSensitive ? "g" : "gi";
  const pattern = opts.wholeWord
    ? new RegExp(`\\b${escapeRegex(query)}\\b`, flags)
    : new RegExp(escapeRegex(query), flags);

  const matches: FindMatch[] = [];
  editor.state.doc.descendants((node, pos) => {
    if (!node.isText || typeof node.text !== "string") return true;
    const text = node.text;
    pattern.lastIndex = 0;
    let m: RegExpExecArray | null;
    while ((m = pattern.exec(text)) !== null) {
      const from = pos + m.index;
      const to = from + m[0].length;
      matches.push({ from, to });
      // Evita loop infinito em regex zero-width.
      if (pattern.lastIndex === m.index) pattern.lastIndex += 1;
    }
    return true;
  });
  return matches;
}

function escapeRegex(str: string): string {
  return str.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
}
