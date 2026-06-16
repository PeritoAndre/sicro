/**
 * LaudoModule — module root (SICRO 3.0). Decides between the laudo registry
 * (list) and the BRIDGE view (consult the occurrence + push artifacts to the
 * `.docx` opened in Word). Gates everything behind an active workspace.
 *
 * SICRO 3.0 pivot: a laudo IS a `.docx` edited in Word/LibreOffice — there is
 * no in-app text editor anymore. Selecting a laudo opens the BridgeView; the
 * old `LaudoEditorView` is intentionally NOT routed (files kept for a later
 * cleanup pass).
 */

import { useEffect } from "react";
import { Briefcase } from "lucide-react";
import { NoOccurrenceState } from "@components/NoOccurrenceState/NoOccurrenceState";
import { useWorkspaceStore } from "@stores/workspaceStore";
import { useLaudoStore } from "./store/laudoStore";
import { LaudoListView } from "./views/LaudoListView";
import { LaudoBridgeView } from "./views/LaudoBridgeView";
import { loadBrandingAssets } from "./document-engine";

export function LaudoModule() {
  const workspacePath = useWorkspaceStore((s) => s.activeWorkspacePath);
  const currentLaudo = useLaudoStore((s) => s.currentLaudo);
  const clearCurrent = useLaudoStore((s) => s.clearCurrent);
  const selectLaudo = useLaudoStore((s) => s.selectLaudo);

  // Pre-load institutional branding assets so the first DOCX render doesn't pay
  // the fetch/data-URI conversion cost on the user's critical path.
  useEffect(() => {
    void loadBrandingAssets();
  }, []);

  if (!workspacePath) {
    return (
      <NoOccurrenceState
        icon={<Briefcase size={36} strokeWidth={1.5} />}
        moduleName="Laudos"
      />
    );
  }

  if (currentLaudo) {
    return (
      <LaudoBridgeView
        workspacePath={workspacePath}
        laudo={currentLaudo}
        onBack={clearCurrent}
      />
    );
  }

  return (
    <LaudoListView
      workspacePath={workspacePath}
      // SICRO 3.0 — abrir um laudo só seleciona a linha (mostra a BridgeView).
      // NÃO carrega `.sicrodoc` para edição (o editor in-app foi aposentado).
      onOpen={(laudo) => selectLaudo(laudo)}
      onCreate={() => {
        /* createLaudoDocx no store já seta currentLaudo como efeito colateral */
      }}
    />
  );
}

