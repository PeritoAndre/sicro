/**
 * VideoMetadataPanel — verdade técnica do ffprobe. Exibe os campos
 * principais + chip de aviso se algum warning de probe foi reportado.
 */

import type { VideoMedia } from "@domain/video";
import { formatDuration, prettyBytes } from "./format";
import styles from "./VideoMetadataPanel.module.css";

interface Props {
  media: VideoMedia;
  warnings: string[];
}

export function VideoMetadataPanel({ media, warnings }: Props) {
  return (
    <section className={styles.panel}>
      {/* Fonte dos metadados: ffprobe (FFmpeg). Rótulo da UI neutro. */}
      <h3 className={styles.title}>Metadados técnicos</h3>
      <dl className={styles.grid}>
        <Row label="Codec" value={media.codec ?? "—"} mono />
        <Row
          label="Resolução"
          value={media.width && media.height ? `${media.width}×${media.height}` : "—"}
        />
        <Row label="Pixel format" value={media.pixel_format ?? "—"} mono />
        <Row label="Duração" value={formatDuration(media.duration_s ?? 0)} mono />
        <Row
          label="FPS declarado"
          value={media.fps_declared ? media.fps_declared.toFixed(3) : "—"}
          mono
        />
        <Row
          label="avg_frame_rate"
          value={media.avg_frame_rate ?? "—"}
          mono
        />
        <Row
          label="r_frame_rate"
          value={media.r_frame_rate ?? "—"}
          mono
        />
        <Row label="time_base" value={media.time_base ?? "—"} mono />
        <Row
          label="Frames (nb_frames)"
          value={media.frame_count != null ? String(media.frame_count) : "—"}
          mono
        />
        <Row
          label="Bitrate"
          value={
            media.bitrate != null
              ? `${(media.bitrate / 1000).toFixed(0)} kbps`
              : "—"
          }
          mono
        />
        <Row label="Tamanho" value={prettyBytes(media.size_bytes)} />
        <Row
          label="SHA-256"
          value={media.sha256}
          mono
          title={media.sha256}
        />
      </dl>
      {warnings.length > 0 && (
        <details className={styles.warnings}>
          <summary>{warnings.length} aviso(s) técnico(s)</summary>
          <ul>
            {warnings.map((w, i) => (
              <li key={i}>{w}</li>
            ))}
          </ul>
        </details>
      )}
    </section>
  );
}

function Row({
  label,
  value,
  mono,
  title,
}: {
  label: string;
  value: string;
  mono?: boolean;
  title?: string;
}) {
  return (
    <>
      <dt>{label}</dt>
      <dd className={mono ? styles.mono : undefined} title={title}>
        {value}
      </dd>
    </>
  );
}
