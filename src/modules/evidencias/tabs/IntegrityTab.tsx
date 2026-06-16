/**
 * IntegrityTab — relatório de integridade completo + verificação
 * profunda sob demanda. Cumpre o critério 9-13 do MVP 5.
 */

import { useState } from "react";
import { AlertTriangle, FileText, RefreshCw, ShieldCheck } from "lucide-react";
import { Button } from "@components/Button/Button";
import { commands } from "@core/commands";
import { toSicroError } from "@core/errors";
import type {
  IntegrityReportArtifact,
  RegistrySummary,
  WorkspaceIntegrityReport,
} from "@domain/evidence_registry";
import { formatDateTime } from "@core/formatters";
import { kindLabel, statusClass, statusLabel } from "../shared";
import styles from "../EvidenciasModule.module.css";

interface Props {
  workspacePath: string;
  summary: RegistrySummary;
  report: WorkspaceIntegrityReport | null;
  onReportRefresh: (r: WorkspaceIntegrityReport) => void;
}

export function IntegrityTab({
  workspacePath,
  summary,
  report,
  onReportRefresh,
}: Props) {
  const [busy, setBusy] = useState<"none" | "light" | "deep" | "save">("none");
  const [feedback, setFeedback] = useState<string | null>(null);
  const [artifact, setArtifact] = useState<IntegrityReportArtifact | null>(null);

  const runVerify = async (deep: boolean) => {
    setBusy(deep ? "deep" : "light");
    setFeedback(deep ? "Recomputando hashes…" : "Verificando integridade…");
    try {
      const r = await commands.verifyWorkspaceIntegrity(workspacePath, { deep });
      onReportRefresh(r);
      setFeedback(
        deep
          ? "Verificação profunda concluída."
          : "Verificação leve concluída.",
      );
    } catch (err) {
      setFeedback(`Falha: ${toSicroError(err).message}`);
    } finally {
      setBusy("none");
      setTimeout(() => setFeedback(null), 4000);
    }
  };

  const generateReport = async () => {
    setBusy("save");
    setFeedback(null);
    try {
      const r = await commands.generateWorkspaceIntegrityReport(workspacePath);
      setArtifact(r);
      setFeedback(`Relatório salvo em ${r.relative_path}`);
    } catch (err) {
      setFeedback(`Falha: ${toSicroError(err).message}`);
    } finally {
      setBusy("none");
    }
  };
  const openReport = async () => {
    if (!artifact) return;
    try {
      await commands.openEvidenceFile(workspacePath, artifact.relative_path);
    } catch (err) {
      setFeedback(`Falha ao abrir: ${toSicroError(err).message}`);
    }
  };

  const issueItems = (report?.items ?? []).filter(
    (i) => i.integrity_status !== "ok" && i.integrity_status !== "unknown",
  );

  return (
    <div className={styles.section}>
      <div className={styles.card}>
        <h3 className={styles.cardTitle}>Verificação</h3>
        <p className={styles.tip}>
          Modo <strong>leve</strong>: confere existência do arquivo, sidecars
          esperados e segurança do caminho. Roda automaticamente sempre
          que esta tela é aberta.
          <br />
          Modo <strong>profundo</strong>: além disso, recomputa SHA-256
          para cada item que carrega um hash registrado. Pode ser lento
          em vídeos longos.
        </p>
        <div className={styles.toolbar}>
          <Button
            variant="secondary"
            leftIcon={<RefreshCw size={14} />}
            onClick={() => void runVerify(false)}
            disabled={busy !== "none"}
          >
            {busy === "light" ? "Verificando…" : "Verificação leve"}
          </Button>
          <Button
            variant="secondary"
            leftIcon={<ShieldCheck size={14} />}
            onClick={() => void runVerify(true)}
            disabled={busy !== "none"}
          >
            {busy === "deep" ? "Recomputando hashes…" : "Verificação profunda"}
          </Button>
          <Button
            variant="primary"
            leftIcon={<FileText size={14} />}
            onClick={() => void generateReport()}
            disabled={busy !== "none"}
          >
            {busy === "save" ? "Gerando…" : "Gerar relatório HTML"}
          </Button>
          {artifact && (
            <Button
              variant="secondary"
              leftIcon={<FileText size={14} />}
              onClick={() => void openReport()}
            >
              Abrir relatório salvo
            </Button>
          )}
          {feedback && <span className={styles.tip} style={{ marginLeft: 12 }}>{feedback}</span>}
        </div>
        {report && (
          <p className={styles.tip} style={{ marginTop: 12 }}>
            Última verificação:{" "}
            {formatDateTime(report.generated_at)} —{" "}
            {report.deep_check_executed ? "profunda" : "leve"} · {" "}
            {report.app_version}
          </p>
        )}
      </div>

      {(report?.warnings.length ?? 0) > 0 && (
        <div className={styles.card}>
          <h3 className={styles.cardTitle}>
            Alertas do verificador
          </h3>
          <ul style={{ margin: 0, paddingLeft: 20 }}>
            {report!.warnings.map((w, i) => (
              <li key={i} className={styles.tip}>
                <AlertTriangle size={12} style={{ verticalAlign: "middle" }} /> {w}
              </li>
            ))}
          </ul>
        </div>
      )}

      <div className={styles.card}>
        <h3 className={styles.cardTitle}>
          Itens com problema ({issueItems.length} de {summary.total_items})
        </h3>
        {issueItems.length === 0 ? (
          <p className={styles.tip}>
            Nenhum item com problema. {report?.deep_check_executed ? "Hashes conferem." : ""}
          </p>
        ) : (
          <table className={styles.table}>
            <thead>
              <tr>
                <th>Tipo</th>
                <th>Título</th>
                <th>Caminho</th>
                <th>Status</th>
                <th>Detalhe</th>
              </tr>
            </thead>
            <tbody>
              {issueItems.map((it) => (
                <tr key={it.id}>
                  <td>{kindLabel(it.kind)}</td>
                  <td>{it.title ?? "—"}</td>
                  <td>
                    <code>{it.relative_path ?? "—"}</code>
                  </td>
                  <td>
                    <span className={statusClass(it.integrity_status, styles)}>
                      {statusLabel(it.integrity_status)}
                    </span>
                  </td>
                  <td>{it.integrity_detail ?? "—"}</td>
                </tr>
              ))}
            </tbody>
          </table>
        )}
      </div>
    </div>
  );
}
