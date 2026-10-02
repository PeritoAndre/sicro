/** Store global de toasts; `pushToast(...)` serve de qualquer arquivo. Progress fica até `dismissToast(id)`. */

import { create } from "zustand";

export type ToastKind = "info" | "success" | "warn" | "error" | "progress";

export interface Toast {
  id: number;
  kind: ToastKind;
  message: string;
  title?: string;
  /** 0 = fica até ser fechado. */
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

export function dismissToast(id: number): void {
  useToastStore.getState().dismiss(id);
}
