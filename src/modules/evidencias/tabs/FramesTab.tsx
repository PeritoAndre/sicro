/**
 * FramesTab — frames extraídos por FFmpeg (Spike F). Mostra miniaturas,
 * timestamp, vídeo de origem, sidecar JSON e quantos laudos citam cada
 * frame.
 */

import { useMemo, useState } from "react";
import { useNavigate } from "react-router-dom";
import { ImageOff } from "lucide-react";
import { toSicroError } from "@core/errors";
import type { EvidenceRegistryItem } from "@domain/evidence_registry";
import { formatDateTime } from "@core/formatters";
import {
  assetUrl,
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

export function FramesTab({ items, workspacePath }: Props) {
  const navigate = useNavigate();
  const frames = useMemo(
    () => items.filter((i) => i.kind === "storyboard_frame"),
    [items],
  );
  const [feedback, setFeedback] = useState<string | null>(null);
  const [detailItem, setDetailItem] = useState<EvidenceRegistryItem | null>(
    null,
  );
  const [filter, setFilter] = useState<"all" | "linked" | "unlinked">("all");

  const filtered = useMemo(() => {
    if (filter === "linked") return frames.filter((f) => f.linked_laudos_count > 0);
    if (filter === "unlinked") return frames.filter((f) => f.linked_laudos_count === 0);
    return frames;
  }, [frames, filter]);

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

  if (frames.length === 0) {
    return (
      <p className={styles.tip}>
        Nenhum frame coletado nesta ocorrência. Frames são gerados pelo
        módulo <strong>Vídeo</strong> via FFmpeg.
      </p>
    );
  }

  return (
    <div className={styles.section}>
      <div className={styles.toolbar}>
        <label htmlFor="fr-filter">Filtro</label>
        <select
          id="fr-filter"
          value={filter}
          onChange={(e) => setFilter(e.target.value as never)}
        >
          <option value="all">Todos ({frames.length})</option>
          <option value="linked">Inseridos em laudo</option>
          <option value="unlinked">Não inseridos</option>
        </select>
        <span className={styles.dim}>{filtered.length} mostrando</span>
        {feedback && <span className={styles.dim}>· {feedback}</span>}
      </div>

      <div className={styles.card} style={{ padding: 0 }}>
        <table className={styles.table}>
          <thead>
            <tr>
              <th></th>
              <th>Título</th>
              <th>Caminho</th>
              <th>Sidecar JSON</th>
              <th>Vídeo (hash)</th>
              <th>Status</th>
              <th>Em laudo</th>
              <th>Capturado em</th>
              <th>Ações</th>
            </tr>
          </thead>
          <tbody>
            {filtered.map((f) => (
              <FrameRow
                key={f.id}
                item={f}
                workspacePath={workspacePath}
                onFeedback={showFeedback}
                onShowDetail={setDetailItem}
                onOpenTitle={() => void openTitle(f)}
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

function FrameRow({
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
  const src = item.relative_path ? assetUrl(workspacePath, item.relative_path) : null;
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
        {moduleTargetFor(item) ? (
          <button
            type="button"
            className={styles.titleLink}
            onClick={onOpenTitle}
            title="Abrir no módulo"
          >
            {item.title ?? <span className={styles.dim}>—</span>}
          </button>
        ) : (
          (item.title ?? <span className={styles.dim}>—</span>)
        )}
      </td>
      <td>
        <code>{item.relative_path ?? "—"}</code>
      </td>
      <td>
        <code>{item.sidecar_relative_path ?? "—"}</code>
      </td>
      <td>
        <code title={item.original_id ?? ""}>{shortHash(item.original_id)}</code>
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
