/**
 * LaudoModule — module root. Decides between list and editor views based on
 * laudoStore state, and gates everything behind the requirement of an
 * active workspace (no occurrence → empty state explaining what to do).
 */

import { useEffect } from "react";
import { Briefcase } from "lucide-react";
import { NoOccurrenceState } from "@components/NoOccurrenceState/NoOccurrenceState";
import { useWorkspaceStore } from "@stores/workspaceStore";
import { useLaudoStore } from "./store/laudoStore";
import { LaudoListView } from "./views/LaudoListView";
import { LaudoEditorView } from "./views/LaudoEditorView";
import { loadBrandingAssets } from "./document-engine";
import { toSicroError } from "@core/errors";

export function LaudoModule() {
  const workspacePath = useWorkspaceStore((s) => s.activeWorkspacePath);
  const currentLaudo = useLaudoStore((s) => s.currentLaudo);
  const clearCurrent = useLaudoStore((s) => s.clearCurrent);

  // Pre-load institutional branding assets so the first export doesn't pay
  // the fetch/data-URI conversion cost on the user's critical path.
  useEffect(() => {
    void loadBrandingAssets();
  }, []);

  // Reset transient editor state when switching workspaces.
  useEffect(() => {
    return () => {
      // intentional: leave currentLaudo as-is if user just navigates away
      // and back; only Voltar clears it explicitly.
    };
  }, [workspacePath]);

  if (!workspacePath) {
    return (
      <NoOccurrenceState
        icon={<Briefcase size={36} strokeWidth={1.5} />}
        moduleName="Laudos"
      />
    );
  }

  if (currentLaudo) {
    return <LaudoEditorView workspacePath={workspacePath} onBack={clearCurrent} />;
  }

  return (
    <LaudoListView
      workspacePath={workspacePath}
      onOpen={async (laudo) => {
        const path = workspacePath;
        try {
          // Use the store's openLaudo which fetches the document + sets it as current.
          // Importing the store action via getState() to avoid re-rendering this component.
          await useLaudoStore.getState().openLaudo(path, laudo.id);
        } catch (err) {
          // Errors are already on the store's lastError; just log here for the spike.
          // eslint-disable-next-line no-console
          console.warn("openLaudo failed", toSicroError(err));
        }
      }}
      onCreate={() => {
        /* createLaudo in the store already sets currentLaudo as a side effect */
      }}
    />
  );
}

