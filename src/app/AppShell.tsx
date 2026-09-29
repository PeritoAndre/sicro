import type { ReactNode } from "react";
import { useShortcuts } from "@core/useShortcuts";
import { UI_ZOOM_DEFAULT, getUiZoom, stepUiZoom } from "@core/uiZoom";
import { useSettingsStore } from "@stores/settingsStore";
import { ActivityRail } from "./ActivityRail";
import { TitleBar } from "./TitleBar";
import { TopBar } from "./TopBar";
import { StatusBar } from "./StatusBar";
import styles from "./AppShell.module.css";

interface AppShellProps {
  children: ReactNode;
}

/**
 * AppShell — layout principal do SICRO.
 *
 * Grid: activity rail (esquerda) + topbar (topo) + main + statusbar (base).
 *
 * J — A integração SIGDOC usa "cover mode": um webview borderless do
 * Tauri é posicionado por cima da área de conteúdo do laudo (`.body`
 * do LaudoEditorView). O AppShell NÃO precisa colapsar — o webview
 * fica em cima do React, dando a impressão de que o site abriu "no
 * lugar" do laudo. Quando o user clica em "Fechar" no header do
 * cover, o webview some e o React reaparece naturalmente.
 */
export function AppShell({ children }: AppShellProps) {
  // Zoom da interface inteira — vale em qualquer tela, inclusive com o foco
  // num campo de texto (Ctrl+Shift+= não digita nada).
  const setUiZoom = useSettingsStore((s) => s.setUiZoom);
  useShortcuts(
    {
      "geral.uiZoomIn": () => void setUiZoom(stepUiZoom(getUiZoom(), 1)),
      "geral.uiZoomOut": () => void setUiZoom(stepUiZoom(getUiZoom(), -1)),
      "geral.uiZoomReset": () => void setUiZoom(UI_ZOOM_DEFAULT),
    },
    { allowInInputs: true },
  );

  return (
    <div className={styles.shell}>
      <div className={styles.titlebar}>
        <TitleBar />
      </div>
      <div className={styles.rail}>
        <ActivityRail />
      </div>
      <div className={styles.top}>
        <TopBar />
      </div>
      <main className={styles.main}>{children}</main>
      <div className={styles.status}>
        <StatusBar />
      </div>
    </div>
  );
}
