/** Dispara a carga inicial dos recentes no mount. */

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
