/**
 * ImportSicroappDialog — modal opened from Home's "Importar .sicroapp" button.
 *
 * Three states, switched on `phase`:
 *   - "idle"      — user just opened the dialog; show "Pick file" UX.
 *   - "running"   — file picked, importer running (no granular progress yet —
 *                   the Rust side is synchronous; we just show a spinner).
 *   - "done"      — import completed; show the ImportReport summary.
 *   - "error"     — import threw; show the error message with retry/cancel.
 *
 * The summary panel mirrors §18 of `SICRO_OPERACIONAL_INTEGRACAO_DESKTOP_2.md`:
 * ocorrência created, type, BO, photos counts, hashes verified, warnings.
 */

import { useEffect, useState } from "react";
import { open as openFileDialog } from "@tauri-apps/plugin-dialog";
import {
  AlertTriangle,
  CheckCircle2,
  FileArchive,
  FolderOpen,
  Loader2,
  XCircle,
} from "lucide-react";
import { Dialog } from "@components/Dialog/Dialog";
import { Button } from "@components/Button/Button";
import { commands } from "@core/commands";
import { toSicroError } from "@core/errors";
import { useWorkspaceStore } from "@stores/workspaceStore";
import type { ImportReport, ImportResult } from "@domain/import";
import styles from "./ImportSicroappDialog.module.css";

type Phase = "idle" | "running" | "done" | "error";

interface Props {
  open: boolean;
  onClose: () => void;
  onOpenWorkspace: (workspacePath: string) => void;
}

export function ImportSicroappDialog({ open, onClose, onOpenWorkspace }: Props) {
  const [phase, setPhase] = useState<Phase>("idle");
  const [filename, setFilename] = useState<string | null>(null);
  const [result, setResult] = useState<ImportResult | null>(null);
  const [error, setError] = useState<string | null>(null);
  const loadRecents = useWorkspaceStore((s) => s.loadRecents);

  // Reset state every time the dialog re-opens.
  useEffect(() => {
    if (open) {
      setPhase("idle");
      setFilename(null);
      setResult(null);
      setError(null);
    }
  }, [open]);

  const handlePickFile = async () => {
    try {
      const selected = await openFileDialog({
        multiple: false,
        title: "Selecione um pacote .sicroapp",
        filters: [
          {
            name: "Pacote SICRO Operacional",
            extensions: ["sicroapp", "sicrocampo"],
          },
        ],
      });
      if (typeof selected !== "string") return;

      // Display the leaf name immediately for visual confirmation.
      const leaf = selected.split(/[\\/]/).pop() ?? selected;
      setFilename(leaf);

      setPhase("running");
      setError(null);
      try {
        const r = await commands.importSicroapp({ package_path: selected });
        setResult(r);
        setPhase("done");
        // Refresh the recents so the just-imported occurrence shows up.
        void loadRecents();
      } catch (err) {
        const e = toSicroError(err);
        setError(e.message);
        setPhase("error");
      }
    } catch (err) {
      setError(toSicroError(err).message);
      setPhase("error");
    }
  };

  return (
    <Dialog
      open={open}
      title="Importar pacote .sicroapp"
      onClose={onClose}
      footer={
        phase === "done" && result ? (
          <>
            <Button variant="secondary" onClick={onClose}>
              Fechar
            </Button>
            <Button
              variant="primary"
              leftIcon={<FolderOpen size={16} />}
              onClick={() => {
                onOpenWorkspace(result.workspace_path);
                onClose();
              }}
            >
              Abrir ocorrência importada
            </Button>
          </>
        ) : (
          <Button variant="secondary" onClick={onClose}>
            Cancelar
          </Button>
        )
      }
    >
      {phase === "idle" && (
        <IdlePanel onPick={handlePickFile} />
      )}
      {phase === "running" && (
        <RunningPanel filename={filename} />
      )}
      {phase === "done" && result && (
        <DonePanel report={result.report} />
      )}
      {phase === "error" && (
        <ErrorPanel error={error} onRetry={handlePickFile} />
      )}
    </Dialog>
  );
}

// ===========================================================================

function IdlePanel({ onPick }: { onPick: () => void }) {
  return (
    <div className={styles.idle}>
      <FileArchive size={48} strokeWidth={1.5} className={styles.icon} />
      <p className={styles.idleHeading}>
        Escolha um <code>.sicroapp</code> exportado pelo SICRO Operacional.
      </p>
      <p className={styles.idleSub}>
        O Desktop validará o ZIP, conferirá os hashes e criará uma nova
        ocorrência no workspace de destino. Pacotes <code>.sicrocampo</code>
        legados também são aceitos.
      </p>
      <Button variant="primary" leftIcon={<FolderOpen size={16} />} onClick={onPick}>
        Selecionar arquivo…
      </Button>
    </div>
  );
}

function RunningPanel({ filename }: { filename: string | null }) {
  return (
    <div className={styles.running}>
      <Loader2 size={48} className={styles.spinner} />
      <p className={styles.runningHeading}>Importando…</p>
      {filename && <p className={styles.runningSub}>{filename}</p>}
      <p className={styles.runningHint}>
        Validando ZIP, lendo manifest, conferindo hashes e copiando fotos para
        o workspace.
      </p>
    </div>
  );
}

function ErrorPanel({
  error,
  onRetry,
}: {
  error: string | null;
  onRetry: () => void;
}) {
  return (
    <div className={styles.errorPanel}>
      <XCircle size={48} className={styles.errorIcon} />
      <p className={styles.errorHeading}>Falha na importação</p>
      <pre className={styles.errorMessage}>{error ?? "Erro desconhecido."}</pre>
      <Button variant="secondary" onClick={onRetry}>
        Tentar outro arquivo
      </Button>
    </div>
  );
}

function DonePanel({ report }: { report: ImportReport }) {
  const isClean =
    report.errors.length === 0 &&
    report.warnings.length === 0 &&
    report.photos_missing === 0 &&
    report.hashes_mismatched.length === 0;

  const header = isClean ? (
    <div className={styles.statusBannerOk}>
      <CheckCircle2 size={20} />
      <span>Importação concluída</span>
    </div>
  ) : (
    <div className={styles.statusBannerWarn}>
      <AlertTriangle size={20} />
      <span>Importação concluída com avisos</span>
    </div>
  );

  return (
    <div className={styles.done}>
      {header}

      <dl className={styles.summaryGrid}>
        <SummaryRow label="Tipo de perícia" value={report.tipo_pericia ?? "—"} />
        <SummaryRow label="Natureza" value={report.natureza ?? "—"} />
        <SummaryRow label="Resultado" value={report.resultado ?? "—"} />
        <SummaryRow label="BO" value={report.bo ?? "—"} />
        <SummaryRow label="Protocolo" value={report.protocolo ?? "—"} />
        <SummaryRow label="Município" value={report.municipio ?? "—"} />
        <SummaryRow label="Bairro" value={report.bairro ?? "—"} />
        <SummaryRow label="Logradouro" value={report.logradouro ?? "—"} />
        <SummaryRow
          label="Pacote (versão)"
          value={`${report.format ?? "?"} ${report.schema_version ?? "?"}`}
        />
        <SummaryRow
          label="Pacote (SHA-256)"
          value={report.package_sha256?.slice(0, 16) ?? "—"}
          mono
        />
      </dl>

      <div className={styles.countsRow}>
        <CountChip
          label="Fotos importadas"
          value={`${report.photos_imported}/${report.photos_declared}`}
        />
        <CountChip
          label="Fotos ausentes"
          value={report.photos_missing}
          warn={report.photos_missing > 0}
        />
        <CountChip
          label="Hashes OK"
          value={
            report.hashes_present
              ? `${report.hashes_verified_ok}`
              : "n/a"
          }
        />
        <CountChip
          label="Hashes divergentes"
          value={report.hashes_mismatched.length}
          warn={report.hashes_mismatched.length > 0}
        />
        <CountChip label="JSONs lidos" value={report.jsons_read.length} />
        <CountChip
          label="Arquivos ignorados"
          value={report.files_ignored.length}
        />
      </div>

      {report.warnings.length > 0 && (
        <details className={styles.detailsBlock} open>
          <summary>Avisos ({report.warnings.length})</summary>
          <ul className={styles.warningList}>
            {report.warnings.map((w, i) => (
              <li key={i}>{w}</li>
            ))}
          </ul>
        </details>
      )}

      {report.errors.length > 0 && (
        <details className={styles.detailsBlock} open>
          <summary>Erros ({report.errors.length})</summary>
          <ul className={styles.errorList}>
            {report.errors.map((e, i) => (
              <li key={i}>{e}</li>
            ))}
          </ul>
        </details>
      )}

      {report.hashes_mismatched.length > 0 && (
        <details className={styles.detailsBlock}>
          <summary>Hashes divergentes</summary>
          <ul className={styles.warningList}>
            {report.hashes_mismatched.map((h, i) => (
              <li key={i}>
                <code>{h.path}</code> — esperado{" "}
                <code>{h.expected.slice(0, 12)}…</code>, obtido{" "}
                <code>{h.actual.slice(0, 12)}…</code>
              </li>
            ))}
          </ul>
        </details>
      )}

      {report.jsons_missing.length > 0 && (
        <details className={styles.detailsBlock}>
          <summary>JSONs esperados não encontrados</summary>
          <ul className={styles.warningList}>
            {report.jsons_missing.map((f, i) => (
              <li key={i}>
                <code>{f}</code>
              </li>
            ))}
          </ul>
        </details>
      )}

      {report.workspace_path && (
        <p className={styles.workspacePath}>
          Workspace criado em:{" "}
          <code>{report.workspace_path}</code>
        </p>
      )}
    </div>
  );
}

function SummaryRow({
  label,
  value,
  mono,
}: {
  label: string;
  value: string;
  mono?: boolean;
}) {
  return (
    <>
      <dt>{label}</dt>
      <dd className={mono ? styles.mono : undefined}>{value}</dd>
    </>
  );
}

function CountChip({
  label,
  value,
  warn,
}: {
  label: string;
  value: string | number;
  warn?: boolean;
}) {
  return (
    <div className={warn ? styles.chipWarn : styles.chip}>
      <span className={styles.chipValue}>{value}</span>
      <span className={styles.chipLabel}>{label}</span>
    </div>
  );
}
