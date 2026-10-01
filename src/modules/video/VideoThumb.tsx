/**
 * Miniatura de um vídeo do caso (lista de vídeos e abas do editor).
 */
import { useEffect, useState } from "react";
import { convertFileSrc } from "@tauri-apps/api/core";
import { Film } from "lucide-react";
import { commands } from "@core/commands";
import styles from "./VideoListView.module.css";

/** URL já resolvida por mídia (a lista abre instantânea na volta ao módulo). */
const thumbCache = new Map<string, string>();

/**
 * Miniatura gerada pelo backend (ffmpeg, cache do app, 1ª vez só). A lista
 * nunca espera: mostra o ícone enquanto a imagem chega em segundo plano.
 */
export function VideoThumb({
  workspacePath,
  mediaId,
  className,
  iconSize = 18,
}: {
  workspacePath: string;
  mediaId: string;
  /** Classe da moldura (tamanho); padrão: a da lista de vídeos. */
  className?: string;
  iconSize?: number;
}) {
  const [src, setSrc] = useState<string | null>(() => thumbCache.get(mediaId) ?? null);
  const [failed, setFailed] = useState(false);
  useEffect(() => {
    if (src || !workspacePath) return;
    let alive = true;
    void commands
      .videoThumbnail(workspacePath, mediaId)
      .then((path) => {
        const url = convertFileSrc(path);
        thumbCache.set(mediaId, url);
        if (alive) setSrc(url);
      })
      .catch(() => alive && setFailed(true));
    return () => {
      alive = false;
    };
  }, [workspacePath, mediaId, src]);
  return (
    <div className={className ?? styles.thumb} title={failed ? "Não foi possível gerar a miniatura" : undefined}>
      {src ? (
        <img src={src} alt="" loading="lazy" onError={() => setFailed(true)} />
      ) : (
        <Film size={iconSize} strokeWidth={1.4} className={failed ? undefined : styles.thumbWait} />
      )}
    </div>
  );
}
