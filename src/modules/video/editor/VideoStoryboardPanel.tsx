/**
 * VideoStoryboardPanel — cards com as fotos extraídas pelo ffmpeg.
 * Cada card mostra miniatura (servida via Tauri asset protocol),
 * timestamp, índice de frame (sempre estimado neste spike) e ações.
 */

import { useState } from "react";
import { convertFileSrc } from "@tauri-apps/api/core";
import { ImageOff, Trash2, Eye } from "lucide-react";
import type { VideoEvent, VideoStoryboardFrame } from "@domain/video";
import { formatDuration } from "./format";
import styles from "./VideoStoryboardPanel.module.css";

interface Props {
  workspacePath: string;
  frames: VideoStoryboardFrame[];
  events: VideoEvent[];
  onSelectFrame: (f: VideoStoryboardFrame) => void;
  onDelete: (frameId: string, deletePng: boolean) => Promise<void> | void;
}

export function VideoStoryboardPanel({
  workspacePath,
  frames,
  events,
  onSelectFrame,
  onDelete,
}: Props) {
  return (
    <section className={styles.panel}>
      <h3 className={styles.title}>Storyboard ({frames.length})</h3>
      {frames.length === 0 ? (
        <p className={styles.empty}>
          Nenhum frame coletado ainda. Use <strong>Coletar frame atual</strong>{" "}
          ou o ícone <em>ImagePlus</em> em um evento.
        </p>
      ) : (
        <div className={styles.grid}>
          {frames.map((f) => (
            <FrameCard
              key={f.id}
              frame={f}
              workspacePath={workspacePath}
              eventLabel={
                f.event_id
                  ? events.find((e) => e.id === f.event_id)?.title ?? "(evento removido)"
                  : null
              }
              onSelect={() => onSelectFrame(f)}
              onDelete={(deletePng) => void onDelete(f.id, deletePng)}
            />
          ))}
        </div>
      )}
    </section>
  );
}

function FrameCard({
  frame,
  workspacePath,
  eventLabel,
  onSelect,
  onDelete,
}: {
  frame: VideoStoryboardFrame;
  workspacePath: string;
  eventLabel: string | null;
  onSelect: () => void;
  onDelete: (deletePng: boolean) => void;
}) {
  const src = (() => {
    try {
      const sep = workspacePath.includes("\\") ? "\\" : "/";
      const abs = `${workspacePath}${sep}${frame.output_path.replace(/\//g, sep)}`;
      return convertFileSrc(abs);
    } catch {
      return null;
    }
  })();
  const [failed, setFailed] = useState(false);

  return (
    <figure className={styles.card}>
      <button
        type="button"
        className={styles.thumbBtn}
        onClick={onSelect}
        title="Mover o player para o timestamp deste frame"
      >
        {!src || failed ? (
          <div className={styles.failed}>
            <ImageOff size={20} />
          </div>
        ) : (
          <img
            src={src}
            alt={frame.title}
            className={styles.thumb}
            loading="lazy"
            onError={() => setFailed(true)}
          />
        )}
      </button>
      <figcaption className={styles.caption}>
        <span className={styles.captionTitle}>{frame.title}</span>
        <div className={styles.captionMeta}>
          <code>{formatDuration(frame.requested_timestamp_s)}</code>
          {frame.observed_frame_index != null && (
            <span title="Índice de frame estimado a partir do FPS declarado">
              ~frame {frame.observed_frame_index}
              {frame.frame_index_is_estimated && (
                <span className={styles.estChip}>est.</span>
              )}
            </span>
          )}
          {frame.delta_s != null && Math.abs(frame.delta_s) > 0.001 && (
            <span
              className={styles.deltaChip}
              // O extrator (ffmpeg) pode entregar o keyframe mais próximo do tempo pedido.
              title="Diferença entre o tempo solicitado e o frame entregue (ajuste ao keyframe mais próximo)"
            >
              Δ {frame.delta_s.toFixed(3)}s
            </span>
          )}
        </div>
        {eventLabel && (
          <span className={styles.eventLink}>↳ evento: {eventLabel}</span>
        )}
        <div className={styles.actions}>
          <button
            type="button"
            title="Ir para o frame no player"
            onClick={onSelect}
          >
            <Eye size={11} />
          </button>
          <button
            type="button"
            title="Remover do storyboard (mantém PNG no disco)"
            onClick={() => onDelete(false)}
          >
            <Trash2 size={11} />
          </button>
        </div>
      </figcaption>
    </figure>
  );
}
