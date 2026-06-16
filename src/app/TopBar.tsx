/**
 * TopBar — cabeçalho contextual. Mostra ONDE você está: módulo atual e, quando
 * há um caso aberto, a ocorrência ativa.
 *
 * A marca (SICRO) e a versão vivem agora na sidebar — por isso aqui ficou só o
 * breadcrumb de localização, sem repetir logo/versão.
 */

import { useLocation } from "react-router-dom";
import { useWorkspaceStore } from "@stores/workspaceStore";
import styles from "./TopBar.module.css";

const moduleNames: Record<string, string> = {
  "/": "Início",
  "/dossie": "Dossiê",
  "/laudo": "Laudo",
  "/croqui": "Croqui",
  "/video": "Vídeo",
  "/imagem": "Imagem",
  "/estatisticas": "Estatísticas",
  "/configuracoes": "Configurações",
};

export function TopBar() {
  const { pathname } = useLocation();
  const occurrence = useWorkspaceStore((s) => s.activeOccurrence);

  const moduleLabel = moduleNames[pathname] ?? "—";
  const occurrenceLabel = occurrence ? buildOccurrenceLabel(occurrence) : null;

  return (
    <header className={styles.bar}>
      <nav className={styles.breadcrumb} aria-label="Localização atual">
        <span className={styles.module}>{moduleLabel}</span>
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
