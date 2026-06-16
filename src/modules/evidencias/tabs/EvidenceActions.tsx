/**
 * EvidenceActions — linha de ações padrão de uma prova na Central de Provas.
 *
 * Ação primária: **Abrir no módulo** (carrega o item no módulo de origem). Em
 * seguida: Revelar na pasta, Abrir no app do sistema (externo, secundário),
 * Copiar referência técnica (JSON, p/ laudo) e Ver metadados. "Copiar caminho"
 * foi removido (redundante — o caminho aparece na linha e dá pra abrir a pasta).
 */
import { useState } from "react";
import { useNavigate } from "react-router-dom";
import { ArrowUpRight, ExternalLink, Eye, Folder } from "lucide-react";

import { commands } from "@core/commands";
import { toSicroError } from "@core/errors";
import type { EvidenceRegistryItem } from "@domain/evidence_registry";

import { moduleTargetFor, openInModule } from "../openInModule";
import styles from "../EvidenciasModule.module.css";

interface Props {
  item: EvidenceRegistryItem;
  workspacePath: string;
  onFeedback: (msg: string) => void;
  onShowDetail: (item: EvidenceRegistryItem) => void;
}

export function EvidenceActions({
  item,
  workspacePath,
  onFeedback,
  onShowDetail,
}: Props) {
  const navigate = useNavigate();
  const [opening, setOpening] = useState(false);
  const target = moduleTargetFor(item);

  const openModule = async () => {
    if (!target) return;
    setOpening(true);
    try {
      await openInModule(item, workspacePath, navigate);
    } catch (err) {
      onFeedback(`Falha ao abrir no módulo: ${toSicroError(err).message}`);
    } finally {
      setOpening(false);
    }
  };

  const openFile = async () => {
    if (!item.relative_path) return;
    try {
      await commands.openEvidenceFile(workspacePath, item.relative_path);
    } catch (err) {
      onFeedback(`Falha ao abrir: ${toSicroError(err).message}`);
    }
  };

  const reveal = async () => {
    if (!item.relative_path) return;
    try {
      await commands.revealEvidenceInFolder(workspacePath, item.relative_path);
    } catch (err) {
      onFeedback(`Falha ao revelar: ${toSicroError(err).message}`);
    }
  };

  const copyRef = async () => {
    try {
      await navigator.clipboard.writeText(
        JSON.stringify(
          {
            kind: item.kind,
            id: item.id,
            original_id: item.original_id,
            relative_path: item.relative_path,
            sha256: item.hash_sha256,
            title: item.title,
          },
          null,
          2,
        ),
      );
      onFeedback("Referência técnica copiada.");
    } catch {
      onFeedback("Falha ao copiar referência.");
    }
  };

  return (
    <div className={styles.actionsRow}>
      {target && (
        <button
          type="button"
          className={`${styles.actionsBtn} ${styles.actionsPrimary}`}
          onClick={() => void openModule()}
          disabled={opening}
          title={`Abrir no módulo ${target.moduleLabel}`}
        >
          <ArrowUpRight size={11} /> Abrir
        </button>
      )}
      <button
        type="button"
        className={styles.actionsBtn}
        onClick={() => void reveal()}
        disabled={!item.relative_path}
        title="Revelar na pasta"
      >
        <Folder size={11} />
      </button>
      <button
        type="button"
        className={styles.actionsBtn}
        onClick={() => void openFile()}
        disabled={!item.relative_path}
        title="Abrir no aplicativo do sistema (externo)"
      >
        <ExternalLink size={11} />
      </button>
      <button
        type="button"
        className={styles.actionsBtn}
        onClick={() => void copyRef()}
        title="Copiar referência técnica (JSON) — para citar no laudo"
      >
        JSON
      </button>
      <button
        type="button"
        className={styles.actionsBtn}
        onClick={() => onShowDetail(item)}
        title="Ver metadados"
      >
        <Eye size={11} />
      </button>
    </div>
  );
}
