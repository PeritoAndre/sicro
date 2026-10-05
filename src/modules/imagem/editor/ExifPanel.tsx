/**
 * Painel de metadados EXIF: resumo (data/câmera/GPS/exposição) + tabela
 * completa colapsável, a partir do `exif_json` do backend.
 */

import { useEffect, useMemo, useState } from "react";
import { ListTree, Loader2, MapPin } from "lucide-react";
import { commands } from "@core/commands";
import type { ImageMetadata } from "@domain/image_analysis";
import styles from "./ExifPanel.module.css";
import { MetadataViewer } from "./MetadataViewer";

interface Props {
  workspacePath: string;
  relativePath: string;
}

interface ExifSummary {
  datetime?: string | null;
  camera?: { make?: string | null; model?: string | null };
  software?: string | null;
  iso?: number | null;
  exposure_time?: string | null;
  f_number?: number | null;
  focal_length_mm?: number | null;
  flash?: string | null;
  orientation?: number | null;
  gps?: { lat?: number; lon?: number; alt_m?: number | null } | null;
}

interface ParsedExif {
  summary: ExifSummary;
  tags: Record<string, string>;
}

export function ExifPanel({ workspacePath, relativePath }: Props) {
  const [meta, setMeta] = useState<ImageMetadata | null>(null);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [viewerOpen, setViewerOpen] = useState(false);

  useEffect(() => {
    let cancelled = false;
    setLoading(true);
    setError(null);
    commands
      .getImageMetadata(workspacePath, relativePath, true)
      .then((m) => {
        if (!cancelled) setMeta(m);
      })
      .catch((e) => {
        if (!cancelled) setError(String((e as Error)?.message ?? e));
      })
      .finally(() => {
        if (!cancelled) setLoading(false);
      });
    return () => {
      cancelled = true;
    };
  }, [workspacePath, relativePath]);

  const parsed: ParsedExif | null = useMemo(() => {
    if (!meta?.exif_json) return null;
    try {
      return JSON.parse(meta.exif_json) as ParsedExif;
    } catch {
      return null;
    }
  }, [meta?.exif_json]);



  const viewer = (
    <>
      <button type="button" className={styles.openAll} onClick={() => setViewerOpen(true)}>
        <ListTree size={14} /> Ver todos os metadados
      </button>
      {viewerOpen && (
        <MetadataViewer
          workspacePath={workspacePath}
          relativePath={relativePath}
          hashSet={meta?.hash_set}
          sizeBytes={meta?.size_bytes}
          onClose={() => setViewerOpen(false)}
        />
      )}
    </>
  );

  if (loading) {
    return (
      <div className={styles.panel}>
        <Loader2 size={14} className={styles.spin} /> Lendo EXIF…
      </div>
    );
  }
  if (error) {
    return <div className={styles.panel}><span className={styles.error}>{error}</span></div>;
  }
  if (!parsed) {
    return (
      <div className={styles.panel}>
        <p className={styles.empty}>
          Esta imagem não possui metadados EXIF (ou são ilegíveis).
        </p>
        {viewer}
      </div>
    );
  }

  const s = parsed.summary;
  const gps = s.gps;

  return (
    <div className={styles.panel}>
      <header className={styles.head}>
        <strong>Metadados</strong>
      </header>

      <dl className={styles.summary}>
        {s.datetime && (
          <>
            <dt>Data/Hora</dt>
            <dd>{s.datetime}</dd>
          </>
        )}
        {(s.camera?.make || s.camera?.model) && (
          <>
            <dt>Câmera</dt>
            <dd>
              {[s.camera?.make, s.camera?.model].filter(Boolean).join(" ")}
            </dd>
          </>
        )}
        {s.software && (
          <>
            <dt>Software</dt>
            <dd>{s.software}</dd>
          </>
        )}
        {typeof s.iso === "number" && (
          <>
            <dt>ISO</dt>
            <dd>{s.iso}</dd>
          </>
        )}
        {s.exposure_time && (
          <>
            <dt>Exposição</dt>
            <dd>{s.exposure_time}</dd>
          </>
        )}
        {typeof s.f_number === "number" && (
          <>
            <dt>Abertura</dt>
            <dd>f/{s.f_number.toFixed(1)}</dd>
          </>
        )}
        {typeof s.focal_length_mm === "number" && (
          <>
            <dt>Focal</dt>
            <dd>{s.focal_length_mm.toFixed(1)} mm</dd>
          </>
        )}
        {s.flash && (
          <>
            <dt>Flash</dt>
            <dd>{s.flash}</dd>
          </>
        )}
        {gps && typeof gps.lat === "number" && typeof gps.lon === "number" && (
          <>
            <dt>GPS</dt>
            <dd className={styles.gps}>
              <MapPin size={11} />{" "}
              <span>
                {gps.lat.toFixed(6)}, {gps.lon.toFixed(6)}
              </span>
              {typeof gps.alt_m === "number" && (
                <small> · {gps.alt_m.toFixed(0)} m</small>
              )}
            </dd>
          </>
        )}
      </dl>

      {viewer}
    </div>
  );
}
