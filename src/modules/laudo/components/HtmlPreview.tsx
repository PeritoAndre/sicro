/**
 * HtmlPreview — full-document HTML render of the active laudo.
 *
 * Validates Spike B's "criterion 12: HTML intermediário pode ser gerado".
 * The HTML is produced from the in-memory TipTap doc (not the saved
 * `.sicrodoc`) so the preview reflects unsaved edits.
 */

import { useEffect, useMemo, useState } from "react";
import { Copy, Eye, X } from "lucide-react";
import {
  collectEvidencePaths,
  loadBrandingAssets,
  loadEvidenceAssets,
  normalizeEvidenceSrcsForSave,
  renderSicroDocToHtml,
  type BrandingAssets,
  type EvidenceAssetMap,
  type SicroDoc,
} from "../document-engine";
import type { Occurrence } from "@domain/occurrence";
import styles from "./HtmlPreview.module.css";

interface HtmlPreviewProps {
  doc: SicroDoc | null;
  liveContent: SicroDoc["content"] | null;
  /** Optional — feeds the institutional header (MVP 2). */
  occurrence?: Occurrence | null;
  /** Required for MVP 4 — used to fetch evidence bytes for inlining. */
  workspacePath: string | null;
  onClose: () => void;
}

export function HtmlPreview({
  doc,
  liveContent,
  occurrence,
  workspacePath,
  onClose,
}: HtmlPreviewProps) {
  const [branding, setBranding] = useState<BrandingAssets | null>(null);
  const [evidenceAssets, setEvidenceAssets] = useState<EvidenceAssetMap | null>(
    null,
  );

  useEffect(() => {
    let cancelled = false;
    void loadBrandingAssets().then((assets) => {
      if (!cancelled) setBranding(assets);
    });
    return () => {
      cancelled = true;
    };
  }, []);

  // Pre-load evidence assets whenever the doc/liveContent changes. The
  // renderer expects `relative_path` references but the WebView's iframe
  // srcdoc can't reach `tauri://localhost/...` URLs, so we inline
  // everything as data URIs.
  useEffect(() => {
    if (!doc || !workspacePath) {
      setEvidenceAssets(null);
      return;
    }
    let cancelled = false;
    const portable = normalizeEvidenceSrcsForSave(liveContent ?? doc.content);
    const paths = collectEvidencePaths(portable);
    // Passo 2 (imagens) — inclui também as imagens do CABEÇALHO (brasão
    // importado) no preload, senão a prévia/iframe mostra a imagem quebrada.
    if (doc.header?.content) {
      const headerPortable = normalizeEvidenceSrcsForSave(doc.header.content);
      for (const p of collectEvidencePaths(headerPortable)) paths.add(p);
    }
    // W (fase 2b) — idem pro RODAPÉ (brasão da Polícia Científica vindo do
    // .docx). Sem isso o brasão do rodapé aparece quebrado na prévia/PDF.
    if (doc.footer?.content) {
      const footerPortable = normalizeEvidenceSrcsForSave(doc.footer.content);
      for (const p of collectEvidencePaths(footerPortable)) paths.add(p);
    }
    if (paths.size === 0) {
      setEvidenceAssets({ byRelativePath: {} });
      return;
    }
    void loadEvidenceAssets(workspacePath, paths).then((assets) => {
      if (!cancelled) setEvidenceAssets(assets);
    });
    return () => {
      cancelled = true;
    };
  }, [doc, liveContent, workspacePath]);

  const html = useMemo(() => {
    if (!doc) return "";
    // Strip any in-memory convertFileSrc URLs back to relative paths so
    // the renderer's `inlineEvidenceAssets` step can match them against
    // the loaded asset map.
    const portableContent = normalizeEvidenceSrcsForSave(
      liveContent ?? doc.content,
    );
    const docWithLiveContent: SicroDoc = {
      ...doc,
      content: portableContent,
    };
    return renderSicroDocToHtml(docWithLiveContent, {
      fullDocument: true,
      occurrence: (occurrence as unknown as Record<string, unknown>) ?? null,
      branding,
      evidenceAssets,
    });
  }, [doc, liveContent, occurrence, branding, evidenceAssets]);

  const copy = async () => {
    try {
      await navigator.clipboard.writeText(html);
    } catch {
      /* ignored — clipboard may be unavailable in Tauri without permission */
    }
  };

  return (
    <div className={styles.overlay} role="dialog" aria-label="Prévia HTML do laudo">
      <div className={styles.bar}>
        <span className={styles.title}>
          <Eye size={16} /> Prévia HTML (renderização intermediária do Document Engine)
        </span>
        <div className={styles.actions}>
          <button type="button" className={styles.copyBtn} onClick={copy}>
            <Copy size={14} /> Copiar HTML
          </button>
          <button type="button" className={styles.closeBtn} onClick={onClose}>
            <X size={14} /> Fechar
          </button>
        </div>
      </div>
      <div className={styles.iframeWrap}>
        <iframe
          title="Prévia HTML do laudo"
          className={styles.iframe}
          srcDoc={html}
          sandbox=""
        />
      </div>
    </div>
  );
}
