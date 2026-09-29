/**
 * VideoStoryboardPanel — cards com as fotos extraídas pelo ffmpeg.
 * Cada card mostra miniatura (servida via Tauri asset protocol),
 * timestamp, índice de frame (sempre estimado neste spike) e ações.
 *
 * Tamanho das miniaturas P / M / G (lembrado), galeria em tela grande e menu
 * no botão direito de cada quadro.
 */

import { useEffect, useState } from "react";
import { convertFileSrc } from "@tauri-apps/api/core";
import { Copy, Crosshair, Expand, ImageOff, Trash2, Eye } from "lucide-react";
import type { VideoEvent, VideoStoryboardFrame } from "@domain/video";
import { useContextMenu, type MenuItem } from "@components/ContextMenu/ContextMenu";
import { formatDuration } from "./format";
import styles from "./VideoStoryboardPanel.module.css";

export type ThumbSize = "s" | "m" | "l";
const SIZE_KEY = "sicro.video.storyboardSize.v1";
function loadSize(): ThumbSize {
  try {
    const v = localStorage.getItem(SIZE_KEY);
    return v === "m" || v === "l" ? v : "s";
  } catch {
    return "s";
  }
}

/** URL servível do PNG de um quadro coletado. */
export function frameSrc(workspacePath: string, frame: VideoStoryboardFrame): string | null {
  try {
    const sep = workspacePath.includes("\\") ? "\\" : "/";
    return convertFileSrc(`${workspacePath}${sep}${frame.output_path.replace(/\//g, sep)}`);
  } catch {
    return null;
  }
}

interface Props {
  workspacePath: string;
  frames: VideoStoryboardFrame[];
  events: VideoEvent[];
  onSelectFrame: (f: VideoStoryboardFrame) => void;
  onDelete: (frameId: string, deletePng: boolean) => Promise<void> | void;
  /** Abre a galeria em tela grande no quadro `index`. */
  onOpenGallery?: (index: number) => void;
  onCopyTime?: (f: VideoStoryboardFrame) => void;
}

export function VideoStoryboardPanel({
  workspacePath,
  frames,
  events,
  onSelectFrame,
  onDelete,
  onOpenGallery,
  onCopyTime,
}: Props) {
  const [size, setSize] = useState<ThumbSize>(loadSize);
  const menu = useContextMenu();
  useEffect(() => {
    try {
      localStorage.setItem(SIZE_KEY, size);
    } catch {
      /* só não lembra */
    }
  }, [size]);

  const frameMenu = (e: React.MouseEvent, f: VideoStoryboardFrame, index: number) => {
    const items: MenuItem[] = [];
    if (onOpenGallery)
      items.push({ label: "Ver grande", icon: <Expand size={12} />, onSelect: () => onOpenGallery(index) });
    items.push({ label: "Ir para este instante", icon: <Crosshair size={12} />, onSelect: () => onSelectFrame(f) });
    if (onCopyTime)
      items.push({ label: "Copiar tempo (laudo)", icon: <Copy size={12} />, onSelect: () => onCopyTime(f) });
    items.push("separator", {
      label: "Remover do storyboard",
      icon: <Trash2 size={12} />,
      danger: true,
      onSelect: () => void onDelete(f.id, false),
    });
    menu.open(e, items);
  };

  return (
    <section className={styles.panel}>
      {menu.element}
      <div className={styles.head}>
        <h3 className={styles.title}>Storyboard ({frames.length})</h3>
        <div className={styles.sizeSeg} role="radiogroup" aria-label="Tamanho das miniaturas">
          {(["s", "m", "l"] as const).map((k) => (
            <button
              key={k}
              type="button"
              className={size === k ? styles.sizeOn : ""}
              onClick={() => setSize(k)}
              title={k === "s" ? "Miniaturas pequenas" : k === "m" ? "Miniaturas médias" : "Miniaturas grandes"}
            >
              {k === "s" ? "P" : k === "m" ? "M" : "G"}
            </button>
          ))}
        </div>
        {onOpenGallery && frames.length > 0 && (
          <button
            type="button"
            className={styles.galleryBtn}
            onClick={() => onOpenGallery(0)}
            title="Ver os quadros em tela grande"
          >
            <Expand size={12} />
          </button>
        )}
      </div>
      {frames.length === 0 ? (
        <p className={styles.empty}>
          Nenhum frame coletado ainda. Use <strong>Coletar frame atual</strong>{" "}
          ou o ícone <em>ImagePlus</em> em um evento.
        </p>
      ) : (
        <div
          className={`${styles.grid} ${size === "m" ? styles.gridM : size === "l" ? styles.gridL : ""}`}
        >
          {frames.map((f, i) => (
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
              onContextMenu={(e) => frameMenu(e, f, i)}
              onOpenBig={onOpenGallery ? () => onOpenGallery(i) : undefined}
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
  onContextMenu,
  onOpenBig,
}: {
  frame: VideoStoryboardFrame;
  workspacePath: string;
  eventLabel: string | null;
  onSelect: () => void;
  onDelete: (deletePng: boolean) => void;
  onContextMenu: (e: React.MouseEvent) => void;
  onOpenBig?: () => void;
}) {
  const src = frameSrc(workspacePath, frame);
  const [failed, setFailed] = useState(false);

  return (
    <figure className={styles.card} onContextMenu={onContextMenu}>
      <button
        type="button"
        className={styles.thumbBtn}
        onClick={onSelect}
        onDoubleClick={onOpenBig}
        title="Clique: levar o player a este quadro · duplo clique: ver grande · botão direito: mais opções"
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
              // O quadro coletado é o que o player mostra no instante pedido: ele
              // começa um pouco antes (até 1 quadro). Δ = início do quadro − pedido.
              title="O quadro coletado é o que aparece no instante pedido; ele começa Δ antes (no máximo a duração de 1 quadro)"
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
