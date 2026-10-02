/**
 * Cabeçalho: módulo atual + ocorrência ativa. Em Vídeo e Áudio leva as abas
 * Vídeos/Áudios (irmãs: com um vídeo aberto, Áudios abre o áudio dele e vice-versa).
 */

import { useEffect } from "react";
import { useLocation, useNavigate } from "react-router-dom";
import { Film, Headphones } from "lucide-react";
import { useWorkspaceStore } from "@stores/workspaceStore";
import { occurrenceLabel } from "@domain/occurrence";
import { isMidiaPath, rememberMidiaAba, rememberWorkModule } from "@modules/midia/midiaNav";
import { prepareTabSwitch } from "@modules/midia/midiaLink";
import { useNavGuard } from "./navGuard";
import styles from "./TopBar.module.css";

const moduleNames: Record<string, string> = {
  "/": "Início",
  "/integridade": "Integridade",
  "/croqui": "Croqui",
  "/imagem": "Imagem",
  "/configuracoes": "Configurações",
  "/ajuda": "Ajuda",
};

export function TopBar() {
  const { pathname } = useLocation();
  const navigate = useNavigate();
  const occurrence = useWorkspaceStore((s) => s.activeOccurrence);
  const midia = isMidiaPath(pathname);

  const caseId = occurrence?.id ?? null;
  useEffect(() => {
    rememberMidiaAba(pathname);
    rememberWorkModule(pathname, caseId);
  }, [pathname, caseId]);

  const moduleLabel = midia ? "Vídeo e Áudio" : (moduleNames[pathname] ?? "—");
  const caseLabel = occurrence ? occurrenceLabel(occurrence) : null;

  const go = (to: "/video" | "/audio") => {
    if (pathname === to) return;
    const switchTab = () => {
      prepareTabSwitch(to);
      navigate(to);
    };
    const guard = useNavGuard.getState().guard;
    if (!guard) switchTab();
    else void useNavGuard.getState().attemptNavigation(switchTab);
  };

  return (
    <header className={styles.bar}>
      <nav className={styles.breadcrumb} aria-label="Localização atual">
        <span className={styles.module}>{moduleLabel}</span>
        {midia && (
          <span className={styles.tabs} role="tablist" aria-label="Vídeo e Áudio">
            <button
              type="button"
              role="tab"
              aria-selected={pathname === "/video"}
              className={pathname === "/video" ? styles.tabOn : ""}
              onClick={() => go("/video")}
            >
              <Film size={13} aria-hidden /> Vídeos
            </button>
            <button
              type="button"
              role="tab"
              aria-selected={pathname.startsWith("/audio")}
              className={pathname.startsWith("/audio") ? styles.tabOn : ""}
              onClick={() => go("/audio")}
            >
              <Headphones size={13} aria-hidden /> Áudios
            </button>
          </span>
        )}
        {caseLabel && (
          <>
            <span className={styles.separator} aria-hidden>
              ▸
            </span>
            <span className={styles.occurrenceLabel} title={caseLabel}>
              {caseLabel}
            </span>
          </>
        )}
      </nav>
      <div className={styles.spacer} />
    </header>
  );
}
