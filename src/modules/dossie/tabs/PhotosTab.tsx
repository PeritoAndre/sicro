/**
 * PhotosTab — galeria de fotos importadas com filtro, lightbox e "Copiar
 * referência para laudo".
 *
 * Lightbox é uma <div> portal-less: fixed-position overlay, fechada com
 * Esc/clique fora. Sem libs externas — o objetivo é o perito conseguir
 * inspecionar uma foto rapidamente, não editar.
 *
 * "Copiar referência" coloca no clipboard um JSON com { id, original_id,
 * relative_path, caption, sha256, mime_type } que o módulo Laudo poderá
 * consumir no futuro (TipTap node `figure` com `data-evidence-id`).
 */

import { useEffect, useMemo, useState } from "react";
import { convertFileSrc } from "@tauri-apps/api/core";
import { Copy, ImageOff, X } from "lucide-react";
import { commands } from "@core/commands";
import { formatDateTime } from "@core/formatters";
import type { MediaAsset } from "@domain/import";
import shared from "./shared.module.css";
import styles from "./PhotosTab.module.css";
import { useDossieList } from "./useDossieList";

interface Props {
  workspacePath: string;
}

export function PhotosTab({ workspacePath }: Props) {
  const { items: photos, loading, error } = useDossieList(
    workspacePath,
    commands.listDossiePhotos,
  );
  const [category, setCategory] = useState<string>("__all__");
  const [active, setActive] = useState<MediaAsset | null>(null);

  const categories = useMemo(() => {
    const set = new Set<string>();
    for (const p of photos) {
      if (p.category) set.add(p.category);
    }
    return Array.from(set).sort();
  }, [photos]);

  const filtered = useMemo(() => {
    if (category === "__all__") return photos;
    return photos.filter((p) => p.category === category);
  }, [photos, category]);

  if (loading && photos.length === 0) {
    return <p className={shared.dim}>Carregando fotos…</p>;
  }
  if (error) {
    return <p className={shared.error}>{error}</p>;
  }
  if (photos.length === 0) {
    return (
      <div className={shared.empty}>
        <ImageOff size={28} aria-hidden />
        <span>Nenhuma foto importada neste workspace.</span>
      </div>
    );
  }

  return (
    <div className={shared.tab}>
      <div className={shared.toolbar}>
        <label htmlFor="cat-filter">Categoria</label>
        <select
          id="cat-filter"
          value={category}
          onChange={(e) => setCategory(e.target.value)}
        >
          <option value="__all__">Todas ({photos.length})</option>
          {categories.map((c) => (
            <option key={c} value={c}>
              {c}
            </option>
          ))}
        </select>
        <span className={shared.dim} style={{ fontSize: "var(--text-xs)" }}>
          {filtered.length} resultado{filtered.length === 1 ? "" : "s"}
        </span>
      </div>

      <div className={styles.gallery}>
        {filtered.map((p) => (
          <PhotoCard
            key={p.id}
            photo={p}
            workspacePath={workspacePath}
            onOpen={() => setActive(p)}
          />
        ))}
      </div>

      {active && (
        <Lightbox
          photo={active}
          workspacePath={workspacePath}
          onClose={() => setActive(null)}
        />
      )}
    </div>
  );
}

function PhotoCard({
  photo,
  workspacePath,
  onOpen,
}: {
  photo: MediaAsset;
  workspacePath: string;
  onOpen: () => void;
}) {
  const src = useMemo(() => assetSrc(workspacePath, photo.relative_path), [
    workspacePath,
    photo.relative_path,
  ]);
  const [failed, setFailed] = useState(false);

  return (
    <figure className={styles.card}>
      <button
        type="button"
        className={styles.thumbBtn}
        onClick={onOpen}
        aria-label={`Abrir ${photo.original_id ?? photo.id}`}
      >
        {failed || !src ? (
          <div className={styles.failed}>
            <ImageOff size={24} />
            <span>Sem visualização</span>
          </div>
        ) : (
          <img
            src={src}
            alt={photo.caption ?? photo.original_filename ?? photo.id}
            onError={() => setFailed(true)}
            className={styles.thumb}
            loading="lazy"
          />
        )}
      </button>
      <figcaption className={styles.caption}>
        <div className={styles.captionTop}>
          <strong className={styles.captionId}>
            {photo.original_id ?? photo.id.slice(0, 8)}
          </strong>
          {photo.category && <span className={shared.chip}>{photo.category}</span>}
        </div>
        {photo.caption && <span className={styles.captionText}>{photo.caption}</span>}
        <div className={styles.captionMeta}>
          {photo.captured_at && <span>{formatDateTime(photo.captured_at)}</span>}
          {photo.sha256 && (
            <code title={photo.sha256}>SHA {photo.sha256.slice(0, 8)}…</code>
          )}
        </div>
        <CopyRefButton photo={photo} />
      </figcaption>
    </figure>
  );
}

function CopyRefButton({ photo }: { photo: MediaAsset }) {
  const [copied, setCopied] = useState(false);
  const onClick = async (e: React.MouseEvent) => {
    e.stopPropagation();
    const payload = {
      kind: "sicro-evidence",
      id: photo.id,
      original_id: photo.original_id,
      relative_path: photo.relative_path,
      mime_type: photo.mime_type,
      sha256: photo.sha256,
      caption: photo.caption,
    };
    try {
      await navigator.clipboard.writeText(JSON.stringify(payload, null, 2));
      setCopied(true);
      setTimeout(() => setCopied(false), 1800);
    } catch {
      // Fallback: ignore — Tauri WebView has clipboard by default.
    }
  };
  return (
    <button
      type="button"
      className={styles.copyRefBtn}
      onClick={onClick}
      title="Copia uma referência JSON desta foto. O módulo Laudo poderá inseri-la como figura vinculada ao evidence_item num MVP futuro."
    >
      <Copy size={11} /> {copied ? "Copiado!" : "Copiar referência"}
    </button>
  );
}

function Lightbox({
  photo,
  workspacePath,
  onClose,
}: {
  photo: MediaAsset;
  workspacePath: string;
  onClose: () => void;
}) {
  const src = useMemo(() => assetSrc(workspacePath, photo.relative_path), [
    workspacePath,
    photo.relative_path,
  ]);
  useEffect(() => {
    const handler = (e: KeyboardEvent) => {
      if (e.key === "Escape") onClose();
    };
    window.addEventListener("keydown", handler);
    return () => window.removeEventListener("keydown", handler);
  }, [onClose]);

  return (
    <div
      className={styles.backdrop}
      onClick={(e) => {
        if (e.target === e.currentTarget) onClose();
      }}
      role="presentation"
    >
      <div className={styles.lightbox} role="dialog" aria-modal="true">
        <button
          type="button"
          className={styles.closeBtn}
          onClick={onClose}
          aria-label="Fechar"
        >
          <X size={18} />
        </button>
        <div className={styles.lightboxImageWrap}>
          {src ? (
            <img
              src={src}
              alt={photo.caption ?? photo.original_filename ?? photo.id}
            />
          ) : (
            <div className={styles.failed}>Sem visualização</div>
          )}
        </div>
        <div className={styles.lightboxMeta}>
          <dl className={shared.metaGrid}>
            <dt>ID original</dt>
            <dd className={shared.mono}>{photo.original_id ?? "—"}</dd>
            <dt>Categoria</dt>
            <dd>{photo.category ?? "—"}</dd>
            <dt>Legenda</dt>
            <dd>{photo.caption ?? <span className={shared.dim}>—</span>}</dd>
            <dt>Capturada em</dt>
            <dd>{photo.captured_at ? formatDateTime(photo.captured_at) : "—"}</dd>
            <dt>Tamanho</dt>
            <dd>{prettyBytes(photo.size_bytes)}</dd>
            <dt>MIME</dt>
            <dd className={shared.mono}>{photo.mime_type ?? "—"}</dd>
            <dt>SHA-256</dt>
            <dd className={shared.mono} title={photo.sha256 ?? ""}>
              {photo.sha256 ? `${photo.sha256.slice(0, 32)}…` : "—"}
            </dd>
            <dt>Caminho</dt>
            <dd className={shared.mono}>{photo.relative_path}</dd>
          </dl>
          <CopyRefButton photo={photo} />
        </div>
      </div>
    </div>
  );
}

function assetSrc(workspacePath: string, relativePath: string): string | null {
  const sep = workspacePath.includes("\\") ? "\\" : "/";
  const abs = `${workspacePath}${sep}${relativePath.replace(/\//g, sep)}`;
  try {
    return convertFileSrc(abs);
  } catch {
    return null;
  }
}

function prettyBytes(n: number): string {
  if (n < 1024) return `${n} B`;
  if (n < 1024 * 1024) return `${(n / 1024).toFixed(1)} KB`;
  return `${(n / 1024 / 1024).toFixed(1)} MB`;
}
