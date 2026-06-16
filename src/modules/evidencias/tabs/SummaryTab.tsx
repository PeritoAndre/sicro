/**
 * SummaryTab — Resumo da ocorrência (contadores + status geral + ações).
 */

import { useState } from "react";
import { FileText, RefreshCw, ShieldCheck } from "lucide-react";
import { commands } from "@core/commands";
import { toSicroError } from "@core/errors";
import { Button } from "@components/Button/Button";
import type {
  IntegrityReportArtifact,
  RegistrySummary,
  WorkspaceIntegrityReport,
} from "@domain/evidence_registry";
import styles from "../EvidenciasModule.module.css";

interface Props {
  summary: RegistrySummary;
  report: WorkspaceIntegrityReport | null;
  workspacePath: string;
  onReload: () => void;
}

export function SummaryTab({ summary, report, workspacePath, onReload }: Props) {
  const [busy, setBusy] = useState<"none" | "report" | "reload">("none");
  const [feedback, setFeedback] = useState<string | null>(null);
  const [artifact, setArtifact] = useState<IntegrityReportArtifact | null>(null);

  const generateReport = async () => {
    setBusy("report");
    setFeedback(null);
    try {
      const r = await commands.generateWorkspaceIntegrityReport(workspacePath);
      setArtifact(r);
      setFeedback(`Relatório salvo em ${r.relative_path}.`);
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

  const overall = summary.overall_status;
  const statusPill =
    overall === "ok" ? (
      <span className={`${styles.statusPill} ${styles.statusOk}`}>íntegro</span>
    ) : overall === "warning" ? (
      <span className={`${styles.statusPill} ${styles.statusWarn}`}>atenção</span>
    ) : overall === "critical" ? (
      <span className={`${styles.statusPill} ${styles.statusCrit}`}>crítico</span>
    ) : (
      <span className={`${styles.statusPill} ${styles.statusUnknown}`}>—</span>
    );

  return (
    <div className={styles.section}>
      <div className={styles.card}>
        <h3 className={styles.cardTitle}>Status geral {statusPill}</h3>
        <p className={styles.tip}>
          A Central de Evidências enxerga, em leitura, tudo o que está
          armazenado no workspace e cruza com os vínculos registrados em
          <code> evidence_links</code>. Use o botão "Gerar relatório" para
          arquivar um HTML auditável dentro de <code>reports/</code>.
        </p>
        <div className={styles.toolbar}>
          <Button
            variant="secondary"
            leftIcon={<RefreshCw size={14} />}
            onClick={() => {
              setBusy("reload");
              onReload();
              setBusy("none");
            }}
            disabled={busy !== "none"}
          >
            Atualizar registro
          </Button>
          <Button
            variant="primary"
            leftIcon={<ShieldCheck size={14} />}
            onClick={() => void generateReport()}
            disabled={busy !== "none"}
          >
            {busy === "report" ? "Gerando…" : "Gerar relatório de integridade"}
          </Button>
          {artifact && (
            <Button
              variant="secondary"
              leftIcon={<FileText size={14} />}
              onClick={() => void openReport()}
            >
              Abrir relatório
            </Button>
          )}
          {feedback && (
            <span className={styles.tip} style={{ marginLeft: 12 }}>
              {feedback}
            </span>
          )}
        </div>
      </div>

      <div className={styles.card}>
        <h3 className={styles.cardTitle}>Contadores</h3>
        <div className={styles.statGrid}>
          <Stat n={summary.photos} label="Fotos" />
          <Stat n={summary.croquis} label="Croquis" />
          <Stat n={summary.croqui_exports} label="Croquis PNG" />
          <Stat n={summary.videos} label="Vídeos" />
          <Stat n={summary.storyboard_frames} label="Frames" />
          <Stat n={summary.laudos} label="Laudos" />
          <Stat n={summary.laudo_exports} label="Exports laudo" />
          <Stat n={summary.imported_packages} label="Pacotes importados" />
          <Stat n={summary.linked_in_laudos} label="Inseridos em laudo" />
        </div>
      </div>

      <div className={styles.card}>
        <h3 className={styles.cardTitle}>Saúde do workspace</h3>
        <div className={styles.statGrid}>
          <Stat n={summary.files_ok} label="Arquivos OK" />
          <Stat
            n={summary.files_missing}
            label="Ausentes"
            alert={summary.files_missing > 0}
          />
          <Stat
            n={summary.broken_links}
            label="Links quebrados"
            alert={summary.broken_links > 0}
          />
          <Stat
            n={summary.hash_mismatches}
            label="Hash divergente"
            critical={summary.hash_mismatches > 0}
          />
          <Stat
            n={summary.unsafe_paths}
            label="Path inseguro"
            critical={summary.unsafe_paths > 0}
          />
        </div>
        {!report?.deep_check_executed && (
          <p className={styles.tip} style={{ marginTop: 12 }}>
            A verificação atual é leve (existência de arquivo + caminho
            seguro). Use a aba <strong>Integridade</strong> para rodar uma
            verificação profunda com recomputo de SHA-256.
          </p>
        )}
      </div>
    </div>
  );
}

function Stat({
  n,
  label,
  alert,
  critical,
}: {
  n: number;
  label: string;
  alert?: boolean;
  critical?: boolean;
}) {
  const cls = critical
    ? `${styles.statCell} ${styles.statCritical}`
    : alert
      ? `${styles.statCell} ${styles.statAlert}`
      : styles.statCell;
  return (
    <div className={cls}>
      <span className={styles.statN}>{n}</span>
      <span className={styles.statL}>{label}</span>
    </div>
  );
}
