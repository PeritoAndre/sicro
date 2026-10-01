/**
 * TopBar — cabeçalho contextual. Mostra ONDE você está: módulo atual e, quando
 * há um caso aberto, a ocorrência ativa.
 *
 * No módulo Vídeo e Áudio ela leva as abas Vídeos / Áudios (as duas telas do
 * módulo) — sem tirar altura do player.
 */

import { useEffect } from "react";
import { useLocation, useNavigate } from "react-router-dom";
import { Film, Headphones } from "lucide-react";
import { useWorkspaceStore } from "@stores/workspaceStore";
import { isMidiaPath, rememberMidiaAba, rememberWorkModule } from "@modules/midia/midiaNav";
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

  useEffect(() => {
    rememberMidiaAba(pathname);
    rememberWorkModule(pathname);
  }, [pathname]);

  const moduleLabel = midia ? "Vídeo e Áudio" : (moduleNames[pathname] ?? "—");
  const occurrenceLabel = occurrence ? buildOccurrenceLabel(occurrence) : null;

  const go = (to: string) => {
    if (pathname === to) return;
    const guard = useNavGuard.getState().guard;
    if (!guard) navigate(to);
    else void useNavGuard.getState().attemptNavigation(() => navigate(to));
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
        {occurrenceLabel && (
          <>
            <span className={styles.separator} aria-hidden>
              ▸
            </span>
            <span className={styles.occurrenceLabel} title={occurrenceLabel}>
              {occurrenceLabel}
            </span>
          </>
        )}
      </nav>
      <div className={styles.spacer} />
    </header>
  );
}

function buildOccurrenceLabel(
  o: NonNullable<ReturnType<typeof useWorkspaceStore.getState>["activeOccurrence"]>,
): string {
  const parts: string[] = [];
  if (o.numero_bo) parts.push(`BO ${o.numero_bo}`);
  if (o.tipo_pericia) parts.push(o.tipo_pericia);
  if (o.municipio) parts.push(o.municipio);
  return parts.length > 0 ? parts.join(" — ") : "Ocorrência sem identificação";
}
