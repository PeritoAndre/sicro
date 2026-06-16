/**
 * AllItemsTab — uma tabela única com todas as evidências da ocorrência.
 *
 * Filtros: tipo, status de integridade, módulo, "inserido em laudo",
 * busca livre por nome/caminho.
 */

import { useMemo, useState } from "react";
import { useNavigate } from "react-router-dom";
import { toSicroError } from "@core/errors";
import type {
  EvidenceRegistryItem,
  IntegrityStatus,
} from "@domain/evidence_registry";
import { formatDateTime } from "@core/formatters";
import { kindLabel, statusClass, statusLabel } from "../shared";
import { moduleTargetFor, openInModule } from "../openInModule";
import { EvidenceActions } from "./EvidenceActions";
import styles from "../EvidenciasModule.module.css";

interface Props {
  items: EvidenceRegistryItem[];
  workspacePath: string;
  onReload?: () => void;
}

const KIND_OPTIONS = [
  "all",
  "photo",
  "croqui",
  "croqui_export",
  "video",
  "storyboard_frame",
  "audio",
  "image_analysis",
  "image_export",
  "document",
  "laudo",
  "laudo_export",
  "imported_package",
];

const STATUS_OPTIONS: ("all" | IntegrityStatus)[] = [
  "all",
  "ok",
  "missing_file",
  "missing_sidecar",
  "broken_link",
  "hash_mismatch",
  "unsafe_path",
  "unknown",
];

export function AllItemsTab({ items, workspacePath }: Props) {
  const [kind, setKind] = useState<string>("all");
  const [status, setStatus] = useState<"all" | IntegrityStatus>("all");
  const [module, setModule] = useState<string>("all");
  const [linked, setLinked] = useState<"all" | "yes" | "no">("all");
  const [query, setQuery] = useState<string>("");
  const [feedback, setFeedback] = useState<string | null>(null);
  const [detailItem, setDetailItem] = useState<EvidenceRegistryItem | null>(
    null,
  );
  const navigate = useNavigate();

  const modules = useMemo(() => {
    const set = new Set<string>();
    items.forEach((i) => set.add(i.source_module));
    return ["all", ...Array.from(set).sort()];
  }, [items]);

  const filtered = useMemo(() => {
    return items.filter((i) => {
      if (kind !== "all" && i.kind !== kind) return false;
      if (status !== "all" && i.integrity_status !== status) return false;
      if (module !== "all" && i.source_module !== module) return false;
      if (linked === "yes" && i.linked_laudos_count === 0) return false;
      if (linked === "no" && i.linked_laudos_count > 0) return false;
      if (query.trim()) {
        const q = query.toLowerCase();
        const hay = [
          i.title ?? "",
          i.relative_path ?? "",
          i.description ?? "",
          i.original_id ?? "",
          i.source_module,
        ]
          .join(" ")
          .toLowerCase();
        if (!hay.includes(q)) return false;
      }
      return true;
    });
  }, [items, kind, status, module, linked, query]);

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
        <label htmlFor="all-kind">Tipo</label>
        <select id="all-kind" value={kind} onChange={(e) => setKind(e.target.value)}>
          {KIND_OPTIONS.map((k) => (
            <option key={k} value={k}>
              {k === "all" ? "Todos" : kindLabel(k)}
            </option>
          ))}
        </select>

        <label htmlFor="all-status">Status</label>
        <select
          id="all-status"
          value={status}
          onChange={(e) => setStatus(e.target.value as never)}
        >
          {STATUS_OPTIONS.map((s) => (
            <option key={s} value={s}>
              {s === "all" ? "Todos" : statusLabel(s)}
            </option>
          ))}
        </select>

        <label htmlFor="all-mod">Módulo</label>
        <select
          id="all-mod"
          value={module}
          onChange={(e) => setModule(e.target.value)}
        >
          {modules.map((m) => (
            <option key={m} value={m}>
              {m === "all" ? "Todos" : m}
            </option>
          ))}
        </select>

        <label htmlFor="all-linked">Em laudo</label>
        <select
          id="all-linked"
          value={linked}
          onChange={(e) => setLinked(e.target.value as never)}
        >
          <option value="all">Todos</option>
          <option value="yes">Sim</option>
          <option value="no">Não</option>
        </select>

        <input
          type="search"
          placeholder="Busca por título / caminho"
          value={query}
          onChange={(e) => setQuery(e.target.value)}
          style={{ minWidth: 240 }}
        />

        <span className={styles.dim}>
          {filtered.length} de {items.length}
        </span>
        {feedback && <span className={styles.dim}>· {feedback}</span>}
      </div>

      <div className={styles.card} style={{ padding: 0 }}>
        <table className={styles.table}>
          <thead>
            <tr>
              <th>Tipo</th>
              <th>Título</th>
              <th>Origem</th>
              <th>Caminho relativo</th>
              <th>Status</th>
              <th>Em laudo</th>
              <th>Data</th>
              <th>Ações</th>
            </tr>
          </thead>
          <tbody>
            {filtered.length === 0 && (
              <tr>
                <td colSpan={8} className={styles.dim} style={{ textAlign: "center", padding: 16 }}>
                  Nenhum item bate com os filtros atuais.
                </td>
              </tr>
            )}
            {filtered.map((item) => (
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
                <td>{item.source_module}</td>
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
                <td>{item.linked_laudos_count}</td>
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
        <DetailDrawer
          item={detailItem}
          onClose={() => setDetailItem(null)}
        />
      )}
    </div>
  );
}

function DetailDrawer({
  item,
  onClose,
}: {
  item: EvidenceRegistryItem;
  onClose: () => void;
}) {
  let metadataPretty: string;
  try {
    metadataPretty = JSON.stringify(JSON.parse(item.metadata_json), null, 2);
  } catch {
    metadataPretty = item.metadata_json;
  }
  return (
    <div className={styles.card} style={{ marginTop: 16 }}>
      <h3 className={styles.cardTitle}>
        Metadados — {item.title ?? item.id}
        <button
          type="button"
          className={styles.actionsBtn}
          onClick={onClose}
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
        {metadataPretty}
      </pre>
    </div>
  );
}
