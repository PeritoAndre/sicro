/**
 * CroquisTab — aba "Croquis". Lista cada `.sicrocroqui` em par com o
 * seu último PNG exportado (quando existe).
 */

import { useMemo, useState } from "react";
import { useNavigate } from "react-router-dom";
import { Map as MapIcon } from "lucide-react";
import { toSicroError } from "@core/errors";
import type { EvidenceRegistryItem } from "@domain/evidence_registry";
import { formatDateTime } from "@core/formatters";
import { statusClass, statusLabel } from "../shared";
import { moduleTargetFor, openInModule } from "../openInModule";
import { EvidenceActions } from "./EvidenceActions";
import styles from "../EvidenciasModule.module.css";

interface Props {
  items: EvidenceRegistryItem[];
  workspacePath: string;
}

export function CroquisTab({ items, workspacePath }: Props) {
  const navigate = useNavigate();
  const [feedback, setFeedback] = useState<string | null>(null);
  const [detailItem, setDetailItem] = useState<EvidenceRegistryItem | null>(
    null,
  );

  // Group source + export by croqui id suffix.
  const grouped = useMemo(() => {
    const map = new Map<
      string,
      { source: EvidenceRegistryItem | null; export_: EvidenceRegistryItem | null }
    >();
    for (const i of items) {
      if (i.kind === "croqui") {
        const id = i.id.split(":")[1] ?? i.id;
        const entry = map.get(id) ?? { source: null, export_: null };
        entry.source = i;
        map.set(id, entry);
      }
      if (i.kind === "croqui_export") {
        const id = i.id.split(":")[1] ?? i.id;
        const entry = map.get(id) ?? { source: null, export_: null };
        entry.export_ = i;
        map.set(id, entry);
      }
    }
    return Array.from(map.entries());
  }, [items]);

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

  if (grouped.length === 0) {
    return <p className={styles.tip}>Nenhum croqui criado nesta ocorrência.</p>;
  }

  return (
    <div className={styles.section}>
      <div className={styles.toolbar}>
        <span className={styles.dim}>{grouped.length} croqui(s)</span>
        {feedback && <span className={styles.dim}>· {feedback}</span>}
      </div>

      <div className={styles.card} style={{ padding: 0 }}>
        <table className={styles.table}>
          <thead>
            <tr>
              <th>Título</th>
              <th>.sicrocroqui</th>
              <th>Status fonte</th>
              <th>Último PNG</th>
              <th>Status PNG</th>
              <th>Em laudo</th>
              <th>Atualizado em</th>
              <th>Ações</th>
            </tr>
          </thead>
          <tbody>
            {grouped.map(([id, { source, export_ }]) => {
              const title = source?.title ?? export_?.title ?? `Croqui ${id.slice(0, 8)}`;
              const sourceRel = source?.relative_path ?? null;
              const exportRel = export_?.relative_path ?? null;
              const updated =
                source?.updated_at ?? export_?.updated_at ?? source?.created_at ?? null;
              // Item representativo da linha: o .sicrocroqui (origem) quando
              // existe, senão o export PNG. "Abrir no módulo" leva ao Croqui
              // nos dois casos.
              const primary = source ?? export_;
              return (
                <tr key={id}>
                  <td>
                    <span style={{ display: "inline-flex", alignItems: "center", gap: 6 }}>
                      <MapIcon size={12} aria-hidden />{" "}
                      {primary && moduleTargetFor(primary) ? (
                        <button
                          type="button"
                          className={styles.titleLink}
                          onClick={() => void openTitle(primary)}
                          title="Abrir no módulo"
                        >
                          {title}
                        </button>
                      ) : (
                        title
                      )}
                    </span>
                  </td>
                  <td>
                    <code>{sourceRel ?? "—"}</code>
                  </td>
                  <td>
                    {source ? (
                      <span
                        className={statusClass(source.integrity_status, styles)}
                        title={source.integrity_detail ?? undefined}
                      >
                        {statusLabel(source.integrity_status)}
                      </span>
                    ) : (
                      <span className={styles.dim}>—</span>
                    )}
                  </td>
                  <td>
                    <code>{exportRel ?? "—"}</code>
                  </td>
                  <td>
                    {export_ ? (
                      <span
                        className={statusClass(export_.integrity_status, styles)}
                        title={export_.integrity_detail ?? undefined}
                      >
                        {statusLabel(export_.integrity_status)}
                      </span>
                    ) : (
                      <span className={styles.dim}>—</span>
                    )}
                  </td>
                  <td>{export_?.linked_laudos_count ?? source?.linked_laudos_count ?? 0}</td>
                  <td>{updated ? formatDateTime(updated) : "—"}</td>
                  <td>
                    {primary ? (
                      <EvidenceActions
                        item={primary}
                        workspacePath={workspacePath}
                        onFeedback={showFeedback}
                        onShowDetail={setDetailItem}
                      />
                    ) : (
                      <span className={styles.dim}>—</span>
                    )}
                  </td>
                </tr>
              );
            })}
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
