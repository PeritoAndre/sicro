/**
 * KindTab — lista genérica de provas filtrada por `kinds`, com título clicável
 * (abre no módulo) + ações padronizadas (EvidenceActions). Usada pelas abas
 * Áudios, Imagens e Documentoscopia da Central de Provas.
 */
import { useState } from "react";
import { useNavigate } from "react-router-dom";

import { toSicroError } from "@core/errors";
import { formatDateTime } from "@core/formatters";
import type {
  EvidenceKind,
  EvidenceRegistryItem,
} from "@domain/evidence_registry";

import { kindLabel, statusClass, statusLabel } from "../shared";
import { moduleTargetFor, openInModule } from "../openInModule";
import { EvidenceActions } from "./EvidenceActions";
import styles from "../EvidenciasModule.module.css";

interface Props {
  items: EvidenceRegistryItem[];
  workspacePath: string;
  kinds: EvidenceKind[];
  emptyHint?: string;
}

export function KindTab({ items, workspacePath, kinds, emptyHint }: Props) {
  const navigate = useNavigate();
  const [feedback, setFeedback] = useState<string | null>(null);
  const [detailItem, setDetailItem] = useState<EvidenceRegistryItem | null>(
    null,
  );
  const rows = items.filter((i) => kinds.includes(i.kind));

  const showFeedback = (msg: string) => {
    setFeedback(msg);
    setTimeout(() => setFeedback(null), 1800);
  };

  const openTitle = async (item: EvidenceRegistryItem) => {
    try {
      await openInModule(item, workspacePath, navigate);
    } catch (err) {
      showFeedback(`Falha ao abrir no módulo: ${toSicroError(err).message}`);
    }
  };

  return (
    <div className={styles.section}>
      <div className={styles.toolbar}>
        <span className={styles.dim}>
          {rows.length} {rows.length === 1 ? "item" : "itens"}
        </span>
        {feedback && <span className={styles.dim}>· {feedback}</span>}
      </div>

      <div className={styles.card} style={{ padding: 0 }}>
        <table className={styles.table}>
          <thead>
            <tr>
              <th>Tipo</th>
              <th>Título</th>
              <th>Caminho relativo</th>
              <th>Status</th>
              <th>Data</th>
              <th>Ações</th>
            </tr>
          </thead>
          <tbody>
            {rows.length === 0 && (
              <tr>
                <td
                  colSpan={6}
                  className={styles.dim}
                  style={{ textAlign: "center", padding: 16 }}
                >
                  {emptyHint ?? "Nenhum item nesta categoria."}
                </td>
              </tr>
            )}
            {rows.map((item) => (
              <tr key={item.id}>
                <td>{kindLabel(item.kind)}</td>
                <td>
                  {moduleTargetFor(item) ? (
                    <button
                      type="button"
                      className={styles.titleLink}
                      onClick={() => void openTitle(item)}
                      title="Abrir no módulo"
                    >
                      {item.title ?? kindLabel(item.kind)}
                    </button>
                  ) : (
                    (item.title ?? <span className={styles.dim}>—</span>)
                  )}
                </td>
                <td>
                  <code>{item.relative_path ?? "—"}</code>
                </td>
                <td>
                  <span
                    className={statusClass(item.integrity_status, styles)}
                    title={item.integrity_detail ?? undefined}
                  >
                    {statusLabel(item.integrity_status)}
                  </span>
                </td>
                <td>
                  {item.updated_at || item.created_at
                    ? formatDateTime(item.updated_at ?? item.created_at!)
                    : "—"}
                </td>
                <td>
                  <EvidenceActions
                    item={item}
                    workspacePath={workspacePath}
                    onFeedback={showFeedback}
                    onShowDetail={setDetailItem}
                  />
                </td>
              </tr>
            ))}
          </tbody>
        </table>
      </div>

      {detailItem && (
        <div className={styles.card} style={{ marginTop: 16 }}>
          <h3 className={styles.cardTitle}>
            Metadados — {detailItem.title ?? detailItem.id}
            <button
              type="button"
              className={styles.actionsBtn}
              onClick={() => setDetailItem(null)}
              style={{ float: "right" }}
            >
              Fechar
            </button>
          </h3>
          <pre
            style={{
              margin: 0,
              padding: 12,
              background: "var(--sicro-surface-2)",
              borderRadius: 4,
              fontFamily: "var(--font-mono)",
              fontSize: 11,
              color: "var(--sicro-fg)",
              overflowX: "auto",
              maxHeight: 320,
            }}
          >
            {prettyJson(detailItem.metadata_json)}
          </pre>
        </div>
      )}
    </div>
  );
}

function prettyJson(raw: string): string {
  try {
    return JSON.stringify(JSON.parse(raw), null, 2);
  } catch {
    return raw;
  }
}
