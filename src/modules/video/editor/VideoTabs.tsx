/**
 * Abas dos vídeos do caso, no topo do editor: troca de vídeo num clique (cada
 * um volta ao instante em que estava), adiciona vídeos sem sair da análise e,
 * no botão direito, compara lado a lado com o vídeo atual.
 */
import { Columns2, Plus, Scissors } from "lucide-react";
import type { VideoMedia } from "@domain/video";
import { useContextMenu } from "@components/ContextMenu/ContextMenu";
import { VideoThumb } from "../VideoThumb";
import { formatDuration, parseDerivation } from "./format";
import styles from "./VideoTabs.module.css";

interface Props {
  workspacePath: string;
  videos: VideoMedia[];
  activeId: string;
  /** Texto de progresso enquanto registra vídeos novos (ou null). */
  adding: string | null;
  onOpen: (id: string) => void;
  onAdd: () => void;
  onCompare: (id: string) => void;
}

function tooltip(v: VideoMedia): string {
  const parts = [
    v.filename,
    [
      v.codec ?? "codec —",
      v.width && v.height ? `${v.width}×${v.height}` : null,
      v.fps_declared ? `${v.fps_declared.toFixed(2)} fps` : null,
      v.duration_s ? formatDuration(v.duration_s) : null,
    ]
      .filter(Boolean)
      .join(" · "),
    `SHA ${v.sha256.slice(0, 16)}…`,
  ];
  return parts.join("\n");
}

export function VideoTabs({ workspacePath, videos, activeId, adding, onOpen, onAdd, onCompare }: Props) {
  const menu = useContextMenu();
  return (
    <nav className={styles.tabs} aria-label="Vídeos do caso">
      {menu.element}
      <div
        className={styles.strip}
        role="tablist"
        onContextMenu={(e) => {
          const id = (e.target as HTMLElement).closest<HTMLElement>("[data-video-id]")?.dataset.videoId;
          const v = videos.find((x) => x.id === id);
          if (!v) return;
          const active = v.id === activeId;
          menu.open(e, [
            { label: "Abrir este vídeo", disabled: active, onSelect: () => onOpen(v.id) },
            {
              label: "Comparar lado a lado com o atual",
              icon: <Columns2 size={12} />,
              disabled: active,
              onSelect: () => onCompare(v.id),
            },
          ]);
        }}
      >
        {videos.map((v) => {
          const active = v.id === activeId;
          const clip = parseDerivation(v.derivation_json) != null;
          return (
            <button
              key={v.id}
              type="button"
              role="tab"
              aria-selected={active}
              data-video-id={v.id}
              className={`${styles.tab} ${active ? styles.tabOn : ""}`}
              title={`${tooltip(v)}\n\nClique: abrir · botão direito: comparar`}
              onClick={() => !active && onOpen(v.id)}
            >
              <VideoThumb
                workspacePath={workspacePath}
                mediaId={v.id}
                className={styles.thumb}
                iconSize={12}
              />
              <span className={styles.text}>
                <span className={styles.name}>
                  {clip && <Scissors size={10} aria-hidden className={styles.clip} />}
                  {v.filename}
                </span>
                {v.duration_s != null && (
                  <span className={styles.dur}>{formatDuration(v.duration_s)}</span>
                )}
              </span>
            </button>
          );
        })}
      </div>
      <button
        type="button"
        className={styles.add}
        onClick={onAdd}
        disabled={adding != null}
        title="Adicionar vídeos ao caso sem sair da análise (Ctrl+O)"
      >
        <Plus size={13} aria-hidden /> {adding ?? "Adicionar"}
      </button>
    </nav>
  );
}
