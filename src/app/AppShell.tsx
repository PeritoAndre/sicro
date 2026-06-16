import type { ReactNode } from "react";
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
