/**
 * PhotosTab — aba "Fotos" da Central. Lista as fotos importadas com
 * miniatura + categoria + hash + status + inserções em laudo. Não edita
 * — só inspeciona.
 */

import { useMemo, useState } from "react";
import { useNavigate } from "react-router-dom";
import { ImageOff } from "lucide-react";
import { toSicroError } from "@core/errors";
import type { EvidenceRegistryItem } from "@domain/evidence_registry";
import { formatDateTime } from "@core/formatters";
import {
  assetUrl,
  prettyBytes,
  shortHash,
  statusClass,
  statusLabel,
} from "../shared";
import { moduleTargetFor, openInModule } from "../openInModule";
import { EvidenceActions } from "./EvidenceActions";
import styles from "../EvidenciasModule.module.css";

interface Props {
  items: EvidenceRegistryItem[];
  workspacePath: string;
}

export function PhotosTab({ items, workspacePath }: Props) {
  const navigate = useNavigate();
  const photos = useMemo(
    () => items.filter((i) => i.kind === "photo"),
    [items],
  );
  const [feedback, setFeedback] = useState<string | null>(null);
  const [detailItem, setDetailItem] = useState<EvidenceRegistryItem | null>(
    null,
  );

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

  if (photos.length === 0) {
    return (
      <p className={styles.tip}>
        Nenhuma foto importada nesta ocorrência.
      </p>
    );
  }

  return (
    <div className={styles.section}>
      <div className={styles.toolbar}>
        <span className={styles.dim}>{photos.length} fotos</span>
        {feedback && <span className={styles.dim}>· {feedback}</span>}
      </div>
      <div className={styles.card} style={{ padding: 0 }}>
        <table className={styles.table}>
          <thead>
            <tr>
              <th></th>
              <th>Identificador</th>
              <th>Categoria</th>
              <th>Caminho</th>
              <th>Tamanho</th>
              <th>SHA-256</th>
              <th>Status</th>
              <th>Em laudo</th>
              <th>Importada em</th>
              <th>Ações</th>
            </tr>
          </thead>
          <tbody>
            {photos.map((p) => (
              <PhotoRow
                key={p.id}
                item={p}
                workspacePath={workspacePath}
                onFeedback={showFeedback}
                onShowDetail={setDetailItem}
                onOpenTitle={() => void openTitle(p)}
              />
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

function PhotoRow({
  item,
  workspacePath,
  onFeedback,
  onShowDetail,
  onOpenTitle,
}: {
  item: EvidenceRegistryItem;
  workspacePath: string;
  onFeedback: (msg: string) => void;
  onShowDetail: (item: EvidenceRegistryItem) => void;
  onOpenTitle: () => void;
}) {
  const [failed, setFailed] = useState(false);
  const src = item.relative_path
    ? assetUrl(workspacePath, item.relative_path)
    : null;
  const label = item.original_id ?? item.title ?? null;
  return (
    <tr>
      <td style={{ width: 56 }}>
        {src && !failed ? (
          <img
            src={src}
            alt={item.title ?? ""}
            className={styles.thumb}
            loading="lazy"
            onError={() => setFailed(true)}
          />
        ) : (
          <span className={styles.thumbPlaceholder}>
            <ImageOff size={16} />
          </span>
        )}
      </td>
      <td>
        {moduleTargetFor(item) && label ? (
          <button
            type="button"
            className={styles.titleLink}
            onClick={onOpenTitle}
            title="Abrir no módulo"
          >
            {label}
          </button>
        ) : (
          (label ?? <span className={styles.dim}>—</span>)
        )}
      </td>
      <td>{item.description ?? <span className={styles.dim}>—</span>}</td>
      <td>
        <code>{item.relative_path ?? "—"}</code>
      </td>
      <td>{prettyBytes(item.size_bytes)}</td>
      <td>
        <code title={item.hash_sha256 ?? ""}>{shortHash(item.hash_sha256)}</code>
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
      <td>{item.created_at ? formatDateTime(item.created_at) : "—"}</td>
      <td>
        <EvidenceActions
          item={item}
          workspacePath={workspacePath}
          onFeedback={onFeedback}
          onShowDetail={onShowDetail}
        />
      </td>
    </tr>
  );
}

function prettyJson(raw: string): string {
  try {
    return JSON.stringify(JSON.parse(raw), null, 2);
  } catch {
    return raw;
  }
}
