/**
 * toastStore — store global de toasts.
 *
 * F12.10 — Sistema minimalista de notificações temporárias. Zustand
 * store + helper functional `pushToast(...)` para chamar de qualquer
 * lugar. Renderizado uma única vez pelo `<Toaster />` na raiz do app.
 *
 * Variantes:
 *   - info     → azul, padrão
 *   - success  → verde, confirmações
 *   - warn     → âmbar, atenções não-fatais
 *   - error    → vermelho, falhas
 *   - progress → spinner em vez de ícone, default sticky
 *
 * Auto-dismiss após `durationMs` (default 4s). Progress toasts ficam
 * até serem explicitamente removidos via `dismissToast(id)`.
 */

import { create } from "zustand";

export type ToastKind = "info" | "success" | "warn" | "error" | "progress";

export interface Toast {
  id: number;
  kind: ToastKind;
  message: string;
  /** Optional title/header. */
  title?: string;
  /** Auto-dismiss in ms. 0 = sticky. */
  durationMs: number;
  createdAt: number;
}

interface ToastState {
  toasts: Toast[];
  push: (toast: Omit<Toast, "id" | "createdAt">) => number;
  dismiss: (id: number) => void;
  clear: () => void;
}

let nextId = 1;

export const useToastStore = create<ToastState>((set) => ({
  toasts: [],
  push: (toast) => {
    const id = nextId++;
    const full: Toast = { ...toast, id, createdAt: Date.now() };
    set((s) => ({ toasts: [...s.toasts, full] }));
    if (toast.durationMs > 0) {
      window.setTimeout(() => {
        set((s) => ({ toasts: s.toasts.filter((t) => t.id !== id) }));
      }, toast.durationMs);
    }
    return id;
  },
  dismiss: (id) =>
    set((s) => ({ toasts: s.toasts.filter((t) => t.id !== id) })),
  clear: () => set({ toasts: [] }),
}));

interface PushOptions {
  title?: string;
  durationMs?: number;
}

/** Helper imperativo para qualquer arquivo TS — sem precisar do hook. */
export function pushToast(
  kind: ToastKind,
  message: string,
  options: PushOptions = {},
): number {
  const defaultDuration = kind === "progress" ? 0 : 4000;
  return useToastStore.getState().push({
    kind,
    message,
    title: options.title,
    durationMs: options.durationMs ?? defaultDuration,
  });
}

/** Dismiss manual — usado para fechar toasts progressivos. */
export function dismissToast(id: number): void {
  useToastStore.getState().dismiss(id);
}
