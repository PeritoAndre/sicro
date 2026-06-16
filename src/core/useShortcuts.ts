/**
 * Hook que liga AÇÕES de atalho a handlers. Resolve a combinação efetiva
 * (override do usuário ?? padrão), ignora digitação em campos de texto e só
 * dispara para ações cujo handler foi fornecido (escopo natural por tela).
 */
import { useEffect, useRef } from "react";

import { eventToBinding, isEditableTarget, normalizeBinding } from "@core/keymap";
import { ACTION_BY_ID } from "@core/keymapActions";
import { useKeymapStore } from "@stores/keymapStore";

export type ShortcutHandlers = Record<string, (e: KeyboardEvent) => void>;

interface Options {
  /** Desliga temporariamente os atalhos (ex.: enquanto um modal está aberto). */
  enabled?: boolean;
  /** Permitir disparo mesmo com foco em campo de texto (raro; padrão false). */
  allowInInputs?: boolean;
}

export function useShortcuts(handlers: ShortcutHandlers, opts: Options = {}): void {
  const overrides = useKeymapStore((s) => s.overrides);
  const enabled = opts.enabled ?? true;
  const allowInInputs = opts.allowInInputs ?? false;

  // Ref com os handlers atuais — evita re-assinar o listener a cada render
  // (handlers normalmente são objeto literal recriado em cada render).
  const handlersRef = useRef<ShortcutHandlers>(handlers);
  handlersRef.current = handlers;

  useEffect(() => {
    if (!enabled) return;
    const onKey = (e: KeyboardEvent) => {
      if (!allowInInputs && isEditableTarget(e.target)) return;
      const combo = eventToBinding(e);
      if (!combo) return;
      const hs = handlersRef.current;
      for (const id of Object.keys(hs)) {
        const raw = overrides[id] ?? ACTION_BY_ID[id]?.defaultBinding ?? "";
        if (raw && normalizeBinding(raw) === combo) {
          const fn = hs[id];
          if (fn) {
            e.preventDefault();
            fn(e);
          }
          return;
        }
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [overrides, enabled, allowInInputs]);
}
