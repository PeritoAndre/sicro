/**
 * WorkspaceProvider — bootstraps global state on app mount.
 *
 * Owns no JSX of its own beyond `{children}`. It just kicks off the initial
 * recents load. Future cross-cutting concerns (event listeners for Tauri
 * window focus, save indicators, etc.) live here.
 */

import { useEffect, type ReactNode } from "react";
import { useWorkspaceStore } from "@stores/workspaceStore";

interface WorkspaceProviderProps {
  children: ReactNode;
}

export function WorkspaceProvider({ children }: WorkspaceProviderProps) {
  const loadRecents = useWorkspaceStore((s) => s.loadRecents);

  useEffect(() => {
    void loadRecents();
  }, [loadRecents]);

  return <>{children}</>;
}
