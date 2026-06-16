/**
 * ExportMenu — dropdown next to "Salvar" that triggers Export Engine commands.
 *
 * Spike C: exposes the three supported targets (HTML / PDF / DOCX) and shows
 * the most recent exports below for context.
 */

import { useEffect, useRef, useState } from "react";
import {
  ChevronDown,
  Download,
  FileCode,
  FileText,
  FileType,
} from "lucide-react";
import { commands } from "@core/commands";
import { toSicroError } from "@core/errors";
import { type SicroDoc } from "../document-engine";
import { formatRelative } from "@core/formatters";
import type { Export } from "@domain/export";
import styles from "./ExportMenu.module.css";

interface ExportMenuProps {
  workspacePath: string;
  laudoId: string;
  /** Title currently shown in the editor — surfaced in the menu so it is
   *  unambiguous WHICH laudo will be exported (Spike C runtime feedback). */
  laudoTitle?: string;
  doc: SicroDoc | null;
  /** Active occurrence — feeds the institutional header (MVP 2). */
  occurrence?: Record<string, unknown> | null;
}

type ExportFmt = "html" | "pdf" | "docx" | "pdf_lo" | "pdf_a";

type Status =
  | { kind: "idle" }
  | { kind: "running"; target: ExportFmt }
  | { kind: "success"; path: string }
  | { kind: "error"; message: string };

export function ExportMenu({
  workspacePath,
  laudoId,
  laudoTitle,
  doc,
  occurrence,
}: ExportMenuProps) {
  const [open, setOpen] = useState(false);
  const [status, setStatus] = useState<Status>({ kind: "idle" });
  const [recent, setRecent] = useState<Export[]>([]);
  // null = ainda não verificado; controla a opção "PDF (LibreOffice)".
  const [loInstalled, setLoInstalled] = useState<boolean | null>(null);
  const rootRef = useRef<HTMLDivElement>(null);

  // Load the recents list + status do LibreOffice na primeira abertura.
  useEffect(() => {
    if (!open) return;
    void commands
      .listLaudoExports(workspacePath, laudoId)
      .then(setRecent)
      .catch(() => {
        /* swallow — the recent list is non-critical */
      });
    void commands
      .getLibreofficeStatus()
      .then((s) => setLoInstalled(s.installed))
      .catch(() => setLoInstalled(false));
  }, [open, workspacePath, laudoId]);

  // Click-outside to close.
  useEffect(() => {
    if (!open) return;
    const handler = (e: MouseEvent) => {
      if (rootRef.current && !rootRef.current.contains(e.target as Node)) {
        setOpen(false);
      }
    };
    window.addEventListener("mousedown", handler);
    return () => window.removeEventListener("mousedown", handler);
  }, [open]);

  const runExport = async (target: ExportFmt) => {
    if (!doc) return;
    const label =
      target === "pdf_lo"
        ? "PDF"
        : target === "pdf_a"
          ? "PDF/A"
          : target.toUpperCase();
    setStatus({ kind: "running", target });
    const { pushToast, dismissToast } = await import(
      "@/components/toast/toastStore"
    );
    const toastId = pushToast("progress", `Exportando ${label}…`, {
      title: "Exportação",
    });
    try {
      // K — Usa o helper unificado `exportLaudo` que (1) chama o
      // pipeline correto pra cada formato e (2) abre o Explorer na
      // pasta do arquivo gerado (revealAfter=true por padrão). O
      // perito já encontra o arquivo aberto pra arrastar pra outro
      // app (SIGDOC, Outlook, etc.).
      const { exportLaudo } = await import("../services/laudoExport");
      const { export: result } = await exportLaudo(
        target,
        workspacePath,
        laudoId,
        doc,
        (occurrence as Record<string, unknown> | null) ?? null,
        { revealAfter: true },
      );
      setStatus({ kind: "success", path: result.relative_path });
      dismissToast(toastId);
      pushToast(
        "success",
        `${label} gerado em ${result.relative_path} — pasta aberta no Explorer.`,
        {
          title: "Exportação concluída",
          durationMs: 6000,
        },
      );
      try {
        setRecent(await commands.listLaudoExports(workspacePath, laudoId));
      } catch {
        /* ignored */
      }
    } catch (err) {
      const msg = toSicroError(err).message;
      setStatus({ kind: "error", message: msg });
      dismissToast(toastId);
      pushToast("error", msg, { title: "Falha na exportação" });
    }
  };

  return (
    <div className={styles.wrap} ref={rootRef}>
      <button
        type="button"
        className={styles.trigger}
        onClick={() => setOpen((v) => !v)}
        disabled={!doc}
        aria-haspopup="menu"
        aria-expanded={open}
      >
        <Download size={14} /> Exportar <ChevronDown size={12} />
      </button>

      {open && (
        <div className={styles.menu} role="menu">
          {(laudoTitle || laudoId) && (
            <>
              <div className={styles.section}>Laudo a exportar</div>
              <div
                className={styles.recentItem}
                title={laudoId}
                style={{ paddingTop: 0, paddingBottom: 4 }}
              >
                <code style={{ flex: 1 }}>
                  {laudoTitle ?? laudoId}
                </code>
              </div>
              <div className={styles.divider} />
            </>
          )}
          <div className={styles.section}>Exportar como…</div>
          <button
            type="button"
            role="menuitem"
            className={styles.item}
            onClick={() => void runExport("pdf_lo")}
            disabled={status.kind === "running" || loInstalled === false}
            title={
              loInstalled === false
                ? "Requer LibreOffice — instale em Configurações › Dependências (ou exporte DOCX e finalize no Word)"
                : "PDF com diagramação estilo Word (paginação fiel ao DOCX)"
            }
          >
            <FileText size={14} />
            PDF
            <span className={styles.itemMeta}>
              {status.kind === "running" && status.target === "pdf_lo"
                ? "convertendo…"
                : loInstalled === false
                  ? "requer LibreOffice"
                  : "estilo Word"}
            </span>
          </button>
          {loInstalled === false && (
            <p
              style={{
                margin: "0 10px 6px",
                fontSize: 11,
                lineHeight: 1.45,
                color: "var(--sicro-fg-dim)",
              }}
            >
              O PDF é gerado pelo <strong>LibreOffice</strong> (diagramação fiel,
              igual ao Word). Instale em <em>Configurações › Dependências</em> —
              ou exporte <strong>DOCX</strong> e finalize/gere o PDF no Word.
            </p>
          )}
          <button
            type="button"
            role="menuitem"
            className={styles.item}
            onClick={() => void runExport("pdf_a")}
            disabled={status.kind === "running" || loInstalled === false}
            title={
              loInstalled === false
                ? "Requer LibreOffice — instale em Configurações › Dependências"
                : "PDF/A — formato de arquivamento de longo prazo (ISO 19005)"
            }
          >
            <FileText size={14} />
            PDF/A (arquivamento)
            <span className={styles.itemMeta}>
              {status.kind === "running" && status.target === "pdf_a"
                ? "convertendo…"
                : loInstalled === false
                  ? "requer LibreOffice"
                  : "preservação ISO 19005"}
            </span>
          </button>
          <p
            style={{
              margin: "2px 10px 6px",
              fontSize: 11,
              lineHeight: 1.45,
              color: "var(--sicro-fg-dim)",
            }}
          >
            PDF/A é o padrão de <strong>arquivamento de longo prazo</strong>{" "}
            (ISO 19005): embute as fontes e imagens no arquivo, sem links
            externos, para abrir <em>idêntico</em> daqui a anos. Exigido por
            vários tribunais e órgãos para documentos oficiais.
          </p>
          <button
            type="button"
            role="menuitem"
            className={styles.item}
            onClick={() => void runExport("docx")}
            disabled={status.kind === "running"}
          >
            <FileType size={14} />
            DOCX
            <span className={styles.itemMeta}>
              {status.kind === "running" && status.target === "docx"
                ? "gerando…"
                : "via docx-rs"}
            </span>
          </button>
          <button
            type="button"
            role="menuitem"
            className={styles.item}
            onClick={() => void runExport("html")}
            disabled={status.kind === "running"}
          >
            <FileCode size={14} />
            HTML
            <span className={styles.itemMeta}>
              {status.kind === "running" && status.target === "html"
                ? "salvando…"
                : "intermediário"}
            </span>
          </button>

          {status.kind === "success" && (
            <>
              <div className={styles.divider} />
              <div className={styles.feedback}>
                Salvo em <code>{status.path}</code>
              </div>
            </>
          )}
          {status.kind === "error" && (
            <>
              <div className={styles.divider} />
              <div className={`${styles.feedback} ${styles.error}`}>
                Falha: {status.message}
              </div>
            </>
          )}

          {recent.length > 0 && (
            <>
              <div className={styles.divider} />
              <div className={styles.section}>Exportações recentes</div>
              <div className={styles.recent}>
                {recent.slice(0, 5).map((e) => (
                  <div key={e.id} className={styles.recentItem}>
                    <span style={{ textTransform: "uppercase" }}>{e.kind}</span>
                    <code title={e.relative_path}>{e.relative_path}</code>
                    <span>{formatRelative(e.created_at)}</span>
                  </div>
                ))}
              </div>
            </>
          )}
        </div>
      )}
    </div>
  );
}
