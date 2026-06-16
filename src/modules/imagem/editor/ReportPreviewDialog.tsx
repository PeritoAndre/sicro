/**
 * ReportPreviewDialog — modal com preview do relatório de análise
 * pericial em HTML.
 *
 * G12.22 — Renderiza o HTML retornado por `generate_image_analysis_report`
 * em um iframe sandbox isolado. Botões: Recarregar, Imprimir
 * (`window.print()` dentro do iframe), Abrir no arquivo (revela o
 * .html no explorador via comando `open_evidence_file`).
 */

import { useEffect, useState } from "react";
import { Loader2, Printer, RefreshCw, X, FileText, FolderOpen } from "lucide-react";
import { commands } from "@core/commands";
import type { ImageAnalysisReportArtifact } from "@domain/image_analysis";
import styles from "./ReportPreviewDialog.module.css";

interface Props {
  open: boolean;
  workspacePath: string;
  analysisId: string;
  onClose: () => void;
}

export function ReportPreviewDialog({
  open,
  workspacePath,
  analysisId,
  onClose,
}: Props) {
  const [artifact, setArtifact] =
    useState<ImageAnalysisReportArtifact | null>(null);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const generate = async () => {
    setLoading(true);
    setError(null);
    setArtifact(null);
    try {
      const r = await commands.generateImageAnalysisReport(
        workspacePath,
        analysisId,
      );
      setArtifact(r);
    } catch (e) {
      setError(String((e as Error)?.message ?? e));
    } finally {
      setLoading(false);
    }
  };

  useEffect(() => {
    if (open) void generate();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [open, analysisId, workspacePath]);

  useEffect(() => {
    if (!open) return undefined;
    const handler = (e: KeyboardEvent) => {
      if (e.key === "Escape") {
        e.preventDefault();
        onClose();
      }
    };
    window.addEventListener("keydown", handler);
    return () => window.removeEventListener("keydown", handler);
  }, [open, onClose]);

  if (!open) return null;

  const handlePrint = () => {
    const iframe = document.getElementById(
      "sicro-report-iframe",
    ) as HTMLIFrameElement | null;
    iframe?.contentWindow?.print();
  };

  const handleReveal = () => {
    if (!artifact) return;
    void commands
      .revealEvidenceInFolder(workspacePath, artifact.output_relative_path)
      .catch(() => {
        /* ignored */
      });
  };

  return (
    <div
      className={styles.overlay}
      role="dialog"
      aria-modal="true"
      onClick={(e) => {
        if (e.target === e.currentTarget) onClose();
      }}
    >
      <div className={styles.dialog}>
        <header className={styles.head}>
          <strong>
            <FileText size={14} /> Relatório de análise pericial
          </strong>
          <div className={styles.actions}>
            <button
              type="button"
              onClick={() => void generate()}
              disabled={loading}
              title="Regenerar"
            >
              {loading ? (
                <Loader2 size={12} className={styles.spin} />
              ) : (
                <RefreshCw size={12} />
              )}{" "}
              Regenerar
            </button>
            <button
              type="button"
              onClick={handlePrint}
              disabled={!artifact}
              title="Imprimir / Salvar como PDF"
            >
              <Printer size={12} /> Imprimir / PDF
            </button>
            <button
              type="button"
              onClick={handleReveal}
              disabled={!artifact}
              title="Abrir pasta do arquivo"
            >
              <FolderOpen size={12} /> Abrir pasta
            </button>
            <button
              type="button"
              onClick={onClose}
              aria-label="Fechar"
              className={styles.closeBtn}
            >
              <X size={13} />
            </button>
          </div>
        </header>
        <div className={styles.body}>
          {error && <div className={styles.error}>{error}</div>}
          {loading && (
            <div className={styles.loadingState}>
              <Loader2 size={20} className={styles.spin} />
              Gerando relatório…
            </div>
          )}
          {artifact && (
            <iframe
              id="sicro-report-iframe"
              title="Pré-visualização do relatório"
              srcDoc={artifact.html}
              sandbox="allow-same-origin allow-modals allow-popups"
              className={styles.iframe}
            />
          )}
        </div>
        {artifact && (
          <footer className={styles.foot}>
            Gravado em <code>{artifact.output_relative_path}</code>
          </footer>
        )}
      </div>
    </div>
  );
}
