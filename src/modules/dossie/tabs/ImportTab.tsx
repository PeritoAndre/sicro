/**
 * ImportTab — auditoria do pacote `.sicroapp` que originou este workspace.
 *
 * Lê o `import_report.json` mais recente do disco (via Tauri) e mostra
 * contagens declaradas vs. importadas, hashes verificados, divergências
 * e arquivos ausentes. Expõe um botão "Recarregar dados do pacote" que
 * dispara o rehydrate.
 */

import { useEffect, useState } from "react";
import { AlertTriangle, CheckCircle2, FileArchive, RefreshCw } from "lucide-react";
import { Button } from "@components/Button/Button";
import { commands } from "@core/commands";
import { formatDateTime } from "@core/formatters";
import { toSicroError } from "@core/errors";
import type { DossieSummary } from "@domain/dossie";
import type { ImportReport } from "@domain/import";
import shared from "./shared.module.css";

interface Props {
  workspacePath: string;
  summary: DossieSummary;
  onRehydrated: () => void;
}

export function ImportTab({ workspacePath, summary, onRehydrated }: Props) {
  const imp = summary.latest_import;
  const [report, setReport] = useState<ImportReport | null>(null);
  const [reportError, setReportError] = useState<string | null>(null);
  const [rehydrating, setRehydrating] = useState(false);
  const [rehydrateMsg, setRehydrateMsg] = useState<string | null>(null);

  useEffect(() => {
    if (!imp) {
      setReport(null);
      return;
    }
    let cancelled = false;
    commands
      .readImportReport(workspacePath, imp.id)
      .then((r) => {
        if (!cancelled) setReport(r);
      })
      .catch((err) => {
        if (!cancelled) setReportError(toSicroError(err).message);
      });
    return () => {
      cancelled = true;
    };
  }, [workspacePath, imp]);

  if (!imp) {
    return (
      <div className={shared.empty}>
        <FileArchive size={28} aria-hidden />
        <span>
          Sem importação registrada neste workspace. Use{" "}
          <strong>Início → Importar .sicroapp…</strong> para gerar uma.
        </span>
      </div>
    );
  }

  const handleRehydrate = async () => {
    setRehydrating(true);
    setRehydrateMsg(null);
    try {
      const out = await commands.rehydrateDossie(workspacePath);
      if (!out.rehydrated) {
        setRehydrateMsg("Sem pacote disponível para recarregar.");
      } else {
        setRehydrateMsg(
          `Pacote re-lido em ${out.from_package_path ?? "(?)"}. ` +
            `Checklist ${out.checklist_loaded} · Entidades ${out.entities_loaded} · ` +
            `Vestígios ${out.traces_loaded} · Medições ${out.measurements_loaded} · ` +
            `Observações ${out.notes_loaded} · Eventos ${out.timeline_loaded} · ` +
            `Stats ${out.stats_loaded ? "OK" : "—"}.`,
        );
        onRehydrated();
      }
    } catch (err) {
      setRehydrateMsg(`Falha: ${toSicroError(err).message}`);
    } finally {
      setRehydrating(false);
    }
  };

  return (
    <div className={shared.tab}>
      <section className={shared.card}>
        <header className={shared.cardHeader}>
          <h2 className={shared.cardTitle}>Pacote de origem</h2>
          <div style={{ display: "flex", gap: "var(--space-2)" }}>
            <Button
              variant="secondary"
              leftIcon={<RefreshCw size={14} />}
              onClick={() => void handleRehydrate()}
              disabled={rehydrating}
            >
              {rehydrating ? "Recarregando…" : "Recarregar dados do pacote"}
            </Button>
          </div>
        </header>
        <dl className={shared.metaGrid}>
          <Row label="Nome do arquivo" value={imp.original_filename} />
          <Row label="Formato" value={`${imp.format} ${imp.schema_version}`} mono />
          <Row label="App emissor" value={imp.app_name ?? null} />
          <Row label="Versão do app" value={imp.app_version ?? null} mono />
          <Row label="ID mobile da ocorrência" value={imp.mobile_occurrence_id ?? null} mono />
          <Row label="Importado em" value={formatDateTime(imp.imported_at)} />
          <Row label="Status" value={imp.status} mono />
          <Row label="SHA-256 (pacote)" value={imp.package_sha256} mono />
          <Row
            label="Caminho no workspace"
            value={imp.package_relative_path}
            mono
          />
        </dl>
        {rehydrateMsg && (
          <p
            style={{
              margin: 0,
              fontSize: "var(--text-xs)",
              color: rehydrateMsg.startsWith("Falha")
                ? "var(--sicro-danger)"
                : "var(--sicro-fg-muted)",
            }}
          >
            {rehydrateMsg}
          </p>
        )}
      </section>

      {reportError && <p className={shared.error}>{reportError}</p>}

      {report && (
        <>
          <section className={shared.card}>
            <header className={shared.cardHeader}>
              <h2 className={shared.cardTitle}>Integridade</h2>
              {report.status === "imported" ? (
                <span
                  className={`${shared.chip} ${shared.chipOk}`}
                  style={{ display: "inline-flex", gap: 4, alignItems: "center" }}
                >
                  <CheckCircle2 size={11} /> imported
                </span>
              ) : (
                <span
                  className={`${shared.chip} ${shared.chipWarn}`}
                  style={{ display: "inline-flex", gap: 4, alignItems: "center" }}
                >
                  <AlertTriangle size={11} /> {report.status ?? "—"}
                </span>
              )}
            </header>
            <div className={shared.summary}>
              <ChipStat label="Fotos declaradas" value={report.photos_declared} />
              <ChipStat label="Fotos importadas" value={report.photos_imported} />
              <ChipStat
                label="Fotos ausentes"
                value={report.photos_missing}
                warn={report.photos_missing > 0}
              />
              <ChipStat
                label="Hashes OK"
                value={report.hashes_present ? report.hashes_verified_ok : "n/a"}
              />
              <ChipStat
                label="Hashes divergentes"
                value={report.hashes_mismatched.length}
                warn={report.hashes_mismatched.length > 0}
              />
              <ChipStat label="JSONs lidos" value={report.jsons_read.length} />
              <ChipStat label="JSONs ausentes" value={report.jsons_missing.length} />
              <ChipStat label="Arq. ignorados" value={report.files_ignored.length} />
            </div>
          </section>

          {report.warnings.length > 0 && (
            <DetailsBlock title={`Avisos (${report.warnings.length})`} open>
              <ul className={listStyle}>
                {report.warnings.map((w, i) => (
                  <li key={i}>{w}</li>
                ))}
              </ul>
            </DetailsBlock>
          )}
          {report.errors.length > 0 && (
            <DetailsBlock title={`Erros (${report.errors.length})`} open danger>
              <ul className={listStyle}>
                {report.errors.map((e, i) => (
                  <li key={i}>{e}</li>
                ))}
              </ul>
            </DetailsBlock>
          )}
          {report.hashes_mismatched.length > 0 && (
            <DetailsBlock title={`Hashes divergentes (${report.hashes_mismatched.length})`}>
              <ul className={listStyle}>
                {report.hashes_mismatched.map((m, i) => (
                  <li key={i}>
                    <code className={shared.mono}>{m.path}</code> — esperado{" "}
                    <code className={shared.mono}>{m.expected.slice(0, 12)}…</code>, obtido{" "}
                    <code className={shared.mono}>{m.actual.slice(0, 12)}…</code>
                  </li>
                ))}
              </ul>
            </DetailsBlock>
          )}
          {report.jsons_missing.length > 0 && (
            <DetailsBlock title={`JSONs ausentes (${report.jsons_missing.length})`}>
              <ul className={listStyle}>
                {report.jsons_missing.map((f, i) => (
                  <li key={i}>
                    <code className={shared.mono}>{f}</code>
                  </li>
                ))}
              </ul>
            </DetailsBlock>
          )}
          {report.jsons_read.length > 0 && (
            <DetailsBlock title={`JSONs lidos (${report.jsons_read.length})`}>
              <ul className={listStyle}>
                {report.jsons_read.map((f, i) => (
                  <li key={i}>
                    <code className={shared.mono}>{f}</code>
                  </li>
                ))}
              </ul>
            </DetailsBlock>
          )}
        </>
      )}
    </div>
  );
}

function Row({
  label,
  value,
  mono,
}: {
  label: string;
  value: string | null;
  mono?: boolean;
}) {
  return (
    <>
      <dt>{label}</dt>
      <dd className={mono ? shared.mono : undefined}>
        {value ?? <span className={shared.dim}>—</span>}
      </dd>
    </>
  );
}

function ChipStat({
  label,
  value,
  warn,
}: {
  label: string;
  value: number | string;
  warn?: boolean;
}) {
  return (
    <div style={warn ? { borderColor: "rgba(230, 180, 80, 0.45)" } : undefined}>
      <strong>{value}</strong>
      <span>{label}</span>
    </div>
  );
}

function DetailsBlock({
  title,
  children,
  open,
  danger,
}: {
  title: string;
  children: React.ReactNode;
  open?: boolean;
  danger?: boolean;
}) {
  return (
    <details
      open={open ?? false}
      className={shared.card}
      style={{ padding: "var(--space-3) var(--space-4)" }}
    >
      <summary
        style={{
          cursor: "pointer",
          fontWeight: 600,
          fontSize: "var(--text-sm)",
          color: danger ? "var(--sicro-danger)" : "var(--sicro-fg)",
        }}
      >
        {title}
      </summary>
      <div style={{ marginTop: "var(--space-2)" }}>{children}</div>
    </details>
  );
}

const listStyle = `${shared.dim}`;
