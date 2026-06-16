/**
 * O4 — Overlay visual durante drag-and-drop de fotos sobre o editor.
 *
 * Posicionado absolute sobre TODO o scroll container do editor; aparece
 * só quando `state === "hover"` ou `state === "uploading"`. Em modo
 * hover, mostra borda azul tracejada + mensagem; em uploading, troca
 * pra spinner + "Importando...". Estados terminais (done/error) somem
 * sozinhos via `reset` no caller depois de um beat.
 *
 * Não bloqueia clique do user em estados terminais — `pointer-events:
 * none` no <div root>.
 */

import { ImageDown, Loader2 } from "lucide-react";
import type { DragState } from "../hooks/useDragDropPhotos";
import styles from "./PhotoDropOverlay.module.css";

export interface PhotoDropOverlayProps {
  state: DragState;
  errorMessage?: string | null;
}

export function PhotoDropOverlay({
  state,
  errorMessage,
}: PhotoDropOverlayProps) {
  if (state === "idle" || state === "done") return null;

  const isHover = state === "hover";
  const isUploading = state === "uploading";
  const isError = state === "error";

  return (
    <div
      className={`${styles.overlay} ${
        isHover ? styles.hover : isUploading ? styles.uploading : styles.error
      }`}
      role="status"
      aria-live="polite"
    >
      <div className={styles.card}>
        {isHover && (
          <>
            <ImageDown size={42} strokeWidth={1.5} className={styles.icon} />
            <div className={styles.title}>Solte para inserir a foto</div>
            <div className={styles.subtitle}>
              Arquivos JPG, PNG, WEBP, GIF, BMP ou TIFF
            </div>
          </>
        )}
        {isUploading && (
          <>
            <Loader2 size={42} strokeWidth={1.5} className={styles.spinner} />
            <div className={styles.title}>Importando foto…</div>
            <div className={styles.subtitle}>
              Copiando, calculando hash e lendo EXIF
            </div>
          </>
        )}
        {isError && (
          <>
            <div className={`${styles.title} ${styles.errorTitle}`}>
              Não foi possível importar
            </div>
            <div className={styles.subtitle}>
              {errorMessage ?? "Erro desconhecido"}
            </div>
          </>
        )}
      </div>
    </div>
  );
}
