/**
 * Global navigation guard — lets editors that hold unsaved work
 * intercept navigation triggered by the ActivityRail (or any other
 * NavLink) without coupling those editors to the navigation layer.
 *
 * Flow:
 *
 *   1. The editor registers an async `guard()` function via
 *      `useNavGuard.getState().register(...)` while it has unsaved
 *      changes.
 *   2. When the user clicks an ActivityRail link, the link calls
 *      `attemptNavigation(targetFn)`. The store invokes the guard
 *      (typically the editor shows a "Salvar antes de sair?" modal),
 *      then runs `targetFn` only if the guard resolves to `true`.
 *   3. When the editor unmounts or no longer has unsaved changes, it
 *      calls `unregister()`.
 *
 * Only one guard at a time — that matches our UI (one editor open at
 * a time) and keeps the API tiny.
 */

import { create } from "zustand";

export type NavGuardCallback = () => Promise<boolean>;

interface NavGuardState {
  guard: NavGuardCallback | null;
  /** Register an unsaved-changes guard. Replaces any previous one. */
  register: (cb: NavGuardCallback) => void;
  /** Drop the active guard. Safe to call when none is registered. */
  unregister: () => void;
  /**
   * Ask the active guard (if any) whether navigation should proceed.
   * Always resolves; defaults to `true` when no guard is registered.
   */
  attemptNavigation: (target: () => void) => Promise<void>;
}

export const useNavGuard = create<NavGuardState>((set, get) => ({
  guard: null,
  register(cb) {
    set({ guard: cb });
  },
  unregister() {
    set({ guard: null });
  },
  async attemptNavigation(target) {
    const guard = get().guard;
    if (!guard) {
      target();
      return;
    }
    const proceed = await guard();
    if (proceed) target();
  },
}));
