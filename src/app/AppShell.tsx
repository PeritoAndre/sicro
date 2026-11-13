import { useEffect, useState, type ReactNode } from "react";
import { useLocation } from "react-router-dom";
import { useShortcuts } from "@core/useShortcuts";
import { UI_ZOOM_DEFAULT, getUiZoom, stepUiZoom } from "@core/uiZoom";
import { normalizeBinding } from "@core/keymap";
import { resolveBinding, useKeymapStore } from "@stores/keymapStore";
import { CasosDialogos } from "@modules/home/CasosDialogos";
import { useDialogosCaso } from "@modules/home/dialogosStore";
import { useSettingsStore } from "@stores/settingsStore";
import { useImmersiveStore } from "@stores/immersiveStore";
import { ActivityRail } from "./ActivityRail";
import { TitleBar } from "./TitleBar";
import { TopBar } from "./TopBar";
import { StatusBar } from "./StatusBar";
import styles from "./AppShell.module.css";
import { AskHost } from "@components/Dialog/ask";

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

  // Ctrl+O / Ctrl+N valem em qualquer tela. No Vídeo o Ctrl+O é "adicionar vídeos" (mesma tecla): lá ele manda.
  const { pathname } = useLocation();
  const overrides = useKeymapStore((s) => s.overrides);
  const abrirTrocar = useDialogosCaso((s) => s.abrirTrocar);
  const abrirNovo = useDialogosCaso((s) => s.abrirNovo);
  useShortcuts(
    {
      "geral.trocarCaso": () => {
        const doVideo = normalizeBinding(resolveBinding(overrides, "video.addVideo"));
        const minha = normalizeBinding(resolveBinding(overrides, "geral.trocarCaso"));
        if (pathname === "/video" && doVideo === minha) return;
        abrirTrocar();
      },
      "geral.novoCaso": () => abrirNovo(),
    },
    { allowInInputs: true },
  );

  // Fora do Início o menu lateral some para sobrar tela; a borda esquerda revela, sair dele esconde.
  const railAuto = useImmersiveStore((s) => s.on);
  const [railOpen, setRailOpen] = useState(false);
  useEffect(() => setRailOpen(false), [pathname, railAuto]);
  const closeRail = () => {
    if (railAuto) setRailOpen(false);
  };

  return (
    <div className={`${styles.shell} ${railAuto ? styles.shellCompact : ""}`}>
      <div className={styles.titlebar}>
        <TitleBar />
      </div>
      {railAuto && (
        <div className={styles.railHotZone} onMouseEnter={() => setRailOpen(true)} aria-hidden />
      )}
      <div
        className={[
          styles.rail,
          railAuto ? styles.railFloating : "",
          railAuto && !railOpen ? styles.railHidden : "",
        ].join(" ")}
        onMouseLeave={closeRail}
      >
        <ActivityRail />
      </div>
      <div className={styles.top} onMouseEnter={closeRail}>
        <TopBar />
      </div>
      <main className={styles.main} onMouseEnter={closeRail}>
        {children}
      </main>
      <div className={styles.status}>
        <StatusBar />
      </div>
      <AskHost />
      <CasosDialogos />
    </div>
  );
}
