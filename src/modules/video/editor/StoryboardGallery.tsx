/**
 * Galeria do storyboard em tela grande. As teclas são tratadas aqui (captura)
 * e não chegam ao player enquanto a galeria estiver aberta.
 */
import { useEffect, useRef, useState } from "react";
import { ChevronLeft, ChevronRight, Copy, Crosshair, X } from "lucide-react";
import type { VideoEvent, VideoStoryboardFrame } from "@domain/video";
import { formatDuration } from "./format";
import { frameSrc } from "./VideoStoryboardPanel";
import styles from "./StoryboardGallery.module.css";

interface Props {
  workspacePath: string;
  frames: VideoStoryboardFrame[];
  events: VideoEvent[];
  startIndex: number;
  onClose: () => void;
  onGoto: (f: VideoStoryboardFrame) => void;
  onCopyTime: (f: VideoStoryboardFrame) => void;
}

export function StoryboardGallery({
  workspacePath,
  frames,
  events,
  startIndex,
  onClose,
  onGoto,
  onCopyTime,
}: Props) {
  const [idx, setIdx] = useState(Math.min(Math.max(0, startIndex), frames.length - 1));
  const stripRef = useRef<HTMLDivElement | null>(null);
  const f = frames[idx];

  // Quadro removido enquanto a galeria estava aberta.
  useEffect(() => {
    if (frames.length === 0) onClose();
    else if (idx > frames.length - 1) setIdx(frames.length - 1);
  }, [frames.length, idx, onClose]);

  // A miniatura atual sempre visível na faixa.
  useEffect(() => {
    stripRef.current
      ?.querySelector<HTMLElement>(`[data-i="${idx}"]`)
      ?.scrollIntoView({ block: "nearest", inline: "center" });
  }, [idx]);

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      const map: Record<string, () => void> = {
        ArrowLeft: () => setIdx((i) => Math.max(0, i - 1)),
        ArrowRight: () => setIdx((i) => Math.min(frames.length - 1, i + 1)),
        Home: () => setIdx(0),
        End: () => setIdx(frames.length - 1),
        Escape: onClose,
        Enter: () => {
          const cur = frames[idx];
          if (cur) {
            onGoto(cur);
            onClose();
          }
        },
      };
      const fn = map[e.key];
      if (!fn) return;
      e.preventDefault();
      e.stopPropagation();
      fn();
    };
    window.addEventListener("keydown", onKey, true);
    return () => window.removeEventListener("keydown", onKey, true);
  }, [frames, idx, onClose, onGoto]);

  if (!f) return null;
  const src = frameSrc(workspacePath, f);
  const actual = f.actual_timestamp_s ?? f.requested_timestamp_s;
  const eventTitle = f.event_id ? events.find((e) => e.id === f.event_id)?.title : null;

  return (
    <div className={styles.overlay} role="dialog" aria-label="Storyboard em tela grande">
      <header className={styles.top}>
        <strong className={styles.title}>{f.title}</strong>
        <span className={styles.meta}>
          <code>{formatDuration(actual)}</code>
          {f.observed_frame_index != null && <> · quadro ≈ {f.observed_frame_index}</>}
          {f.delta_s != null && Math.abs(f.delta_s) > 0.001 && (
            <> · pedido {formatDuration(f.requested_timestamp_s)} (Δ {f.delta_s.toFixed(3)} s)</>
          )}
          {eventTitle && <> · evento: {eventTitle}</>}
        </span>
        <span className={styles.count}>
          {idx + 1} / {frames.length}
        </span>
        <button type="button" onClick={() => onCopyTime(f)} title="Copiar o tempo (formato de laudo)">
          <Copy size={14} /> copiar tempo
        </button>
        <button
          type="button"
          onClick={() => {
            onGoto(f);
            onClose();
          }}
          title="Fechar e levar o player a este quadro (Enter)"
        >
          <Crosshair size={14} /> ir para este instante
        </button>
        <button type="button" onClick={onClose} title="Fechar (Esc)">
          <X size={16} />
        </button>
      </header>

      <div className={styles.stage}>
        <button
          type="button"
          className={styles.nav}
          disabled={idx === 0}
          onClick={() => setIdx((i) => Math.max(0, i - 1))}
          title="Anterior (←)"
        >
          <ChevronLeft size={28} />
        </button>
        {src ? <img className={styles.big} src={src} alt={f.title} /> : <div className={styles.big} />}
        <button
          type="button"
          className={styles.nav}
          disabled={idx === frames.length - 1}
          onClick={() => setIdx((i) => Math.min(frames.length - 1, i + 1))}
          title="Seguinte (→)"
        >
          <ChevronRight size={28} />
        </button>
      </div>

      <div ref={stripRef} className={styles.strip}>
        {frames.map((fr, i) => {
          const s = frameSrc(workspacePath, fr);
          return (
            <button
              key={fr.id}
              type="button"
              data-i={i}
              className={`${styles.thumb} ${i === idx ? styles.thumbOn : ""}`}
              onClick={() => setIdx(i)}
              title={`${fr.title} · ${formatDuration(fr.actual_timestamp_s ?? fr.requested_timestamp_s)}`}
            >
              {s && <img src={s} alt="" loading="lazy" />}
            </button>
          );
        })}
      </div>
    </div>
  );
}
