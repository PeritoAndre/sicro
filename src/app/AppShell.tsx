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

/** Layout principal: rail (esquerda) + topbar + main + statusbar. */
export function AppShell({ children }: AppShellProps) {
  // Zoom da interface vale até com foco em campo de texto (Ctrl+Shift+= não digita nada).
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
