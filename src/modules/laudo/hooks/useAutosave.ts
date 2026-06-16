/**
 * useAutosave — debounced autosave do laudo.
 *
 * Funcionamento:
 *   - Observa o conteúdo do editor TipTap via `editor.on("update")`.
 *   - Após `delayMs` (default 3000) sem digitação, dispara `saveFn`.
 *   - Cancela qualquer save pendente quando o editor é destruído ou o
 *     callback muda (evita race condition entre laudos diferentes).
 *   - Skip durante `isExternallySaving` (ex: usuário clicou Salvar manual).
 *
 * Resilência:
 *   - Se `saveFn` rejeitar, registra o erro mas NÃO repete automaticamente
 *     (evita loop). Caller decide se retenta no próximo update.
 *   - Em ambiente sem `window` (SSR/test), no-op silencioso.
 */

import { useEffect, useRef } from "react";
import type { Editor } from "@tiptap/react";
import type { JSONContent } from "@tiptap/core";

export interface UseAutosaveOptions {
  /** Editor TipTap a observar. Quando null/undefined, o hook fica idle. */
  editor: Editor | null;
  /** Função invocada após o delay. Recebe o snapshot mais recente. */
  saveFn: (content: JSONContent) => Promise<unknown> | unknown;
  /** Espera em milissegundos após a última digitação. Default 3000. */
  delayMs?: number;
  /** Pausa o autosave (útil enquanto há um save manual em andamento). */
  paused?: boolean;
  /** Habilita/desabilita o hook completamente. Default true. */
  enabled?: boolean;
  /** Callback opcional para erros de save (logging). */
  onError?: (err: unknown) => void;
}

export function useAutosave({
  editor,
  saveFn,
  delayMs = 3000,
  paused = false,
  enabled = true,
  onError,
}: UseAutosaveOptions): void {
  const timerRef = useRef<ReturnType<typeof setTimeout> | null>(null);
  // Mantemos a função em ref para evitar re-attach do listener a cada
  // re-render do componente pai. Save fn pode ser inline; ainda assim
  // o `editor.on("update")` fica estável.
  const saveFnRef = useRef(saveFn);
  const pausedRef = useRef(paused);
  const onErrorRef = useRef(onError);

  useEffect(() => {
    saveFnRef.current = saveFn;
  }, [saveFn]);
  useEffect(() => {
    pausedRef.current = paused;
  }, [paused]);
  useEffect(() => {
    onErrorRef.current = onError;
  }, [onError]);

  useEffect(() => {
    if (!editor || !enabled) return undefined;

    const clearTimer = () => {
      if (timerRef.current !== null) {
        clearTimeout(timerRef.current);
        timerRef.current = null;
      }
    };

    const schedule = () => {
      if (pausedRef.current) return;
      clearTimer();
      timerRef.current = setTimeout(() => {
        timerRef.current = null;
        if (pausedRef.current) return;
        try {
          const result = saveFnRef.current(editor.getJSON());
          if (result && typeof (result as Promise<unknown>).then === "function") {
            (result as Promise<unknown>).catch((err) => {
              onErrorRef.current?.(err);
            });
          }
        } catch (err) {
          onErrorRef.current?.(err);
        }
      }, delayMs);
    };

    editor.on("update", schedule);
    return () => {
      editor.off("update", schedule);
      clearTimer();
    };
  }, [editor, delayMs, enabled]);
}
