/**
 * GlobalBackupCard — "Backup geral" (todos os casos), em Configurações.
 *
 * Mantém uma PASTA-ESPELHO num destino escolhido (ex.: HD externo), com 1
 * `.sicrobackup` por caso + um índice. INCREMENTAL: só recopia os casos que
 * mudaram desde o último backup (o backend compara um fingerprint barato).
 *
 * §13: o original nunca é tocado; o backup é COMPLETO (inclui vídeos/drone) e
 * casos não encontrados (HD desconectado / movido) são reportados sem apagar
 * o backup anterior. Backup é deliberado/manual — sem nuvem.
 */

import { useCallback, useEffect, useRef, useState } from "react";
import {
  AlertTriangle,
  CheckCircle2,
  Database,
  FolderArchive,
  FolderOpen,
  RotateCcw,
} from "lucide-react";
import { open as openDirDialog } from "@tauri-apps/plugin-dialog";
import { listen } from "@tauri-apps/api/event";
import { Button } from "@components/Button/Button";
import { commands } from "@core/commands";
import { toSicroError } from "@core/errors";
import {
  getLastAutoBackupAt,
  isAutoBackupOnCloseEnabled,
  setAutoBackupOnClose,
} from "@core/autoBackup";
import { reindexCaseIndexFromRecents } from "@core/caseIndex";
import type {
  GlobalBackupProgress,
  GlobalBackupReport,
  GlobalCaseInput,
  RestoreProgress,
  RestoreReport,
} from "@domain/alpha";
import type { CaseIndexEntry } from "@domain/case_index";
import cfg from "./ConfiguracoesModule.module.css";
import styles from "./GlobalBackupCard.module.css";

const DEST_KEY = "sicro.globalBackup.destination";

export function GlobalBackupCard() {
  const [entries, setEntries] = useState<CaseIndexEntry[]>([]);
  const [loadingEntries, setLoadingEntries] = useState(true);
  const [dest, setDest] = useState<string | null>(null);
  const [running, setRunning] = useState(false);
  const [progress, setProgress] = useState<GlobalBackupProgress | null>(null);
  const [report, setReport] = useState<GlobalBackupReport | null>(null);
  const [error, setError] = useState<string | null>(null);
  const runningRef = useRef(false);

  // Restauração (genérica: HD externo, pendrive, nuvem, rede).
  const [restoring, setRestoring] = useState(false);
  const [restoreProgress, setRestoreProgress] = useState<RestoreProgress | null>(
    null,
  );
  const [restoreReport, setRestoreReport] = useState<RestoreReport | null>(null);
  const [restoreError, setRestoreError] = useState<string | null>(null);
  const restoringRef = useRef(false);

  // Auto-backup ao fechar a ocorrência (DR — Fase 2b).
  const [autoOnClose, setAutoOnClose] = useState(isAutoBackupOnCloseEnabled());
  const lastAuto = getLastAutoBackupAt();

  useEffect(() => {
    setDest(localStorage.getItem(DEST_KEY));
    let cancelled = false;
    commands
      .getCaseIndex()
      .then((idx) => !cancelled && setEntries(idx))
      .catch((e) => !cancelled && setError(toSicroError(e).message))
      .finally(() => !cancelled && setLoadingEntries(false));
    return () => {
      cancelled = true;
    };
  }, []);

  const run = useCallback(
    async (destDir: string) => {
      if (runningRef.current || entries.length === 0) return;
      runningRef.current = true;
      setRunning(true);
      setError(null);
      setReport(null);
      setProgress(null);
      let unlisten: (() => void) | null = null;
      try {
        unlisten = await listen<GlobalBackupProgress>(
          "global-backup-progress",
          (ev) => setProgress(ev.payload),
        );
        const cases: GlobalCaseInput[] = entries.map((e) => ({
          workspace_path: e.workspace_path,
          label: caseLabel(e),
        }));
        const rep = await commands.generateGlobalBackup(cases, destDir);
        setReport(rep);
      } catch (e) {
        setError(toSicroError(e).message);
      } finally {
        if (unlisten) unlisten();
        runningRef.current = false;
        setRunning(false);
        setProgress(null);
      }
    },
    [entries],
  );

  const pickAndRun = useCallback(async () => {
    try {
      const sel = await openDirDialog({
        directory: true,
        multiple: false,
        defaultPath: dest ?? undefined,
        title: "Escolha a pasta de destino do backup geral (ex.: HD externo)",
      });
      if (typeof sel !== "string") return;
      localStorage.setItem(DEST_KEY, sel);
      setDest(sel);
      await run(sel);
    } catch (e) {
      setError(toSicroError(e).message);
    }
  }, [dest, run]);

  const handleReveal = useCallback(() => {
    if (dest) void commands.revealPathInExplorer(dest).catch(() => {});
  }, [dest]);

  const pickAndRestore = useCallback(async () => {
    if (restoringRef.current) return;
    try {
      const sel = await openDirDialog({
        directory: true,
        multiple: false,
        defaultPath: dest ?? undefined,
        title: "Escolha a pasta de backup para restaurar (HD, pendrive, nuvem…)",
      });
      if (typeof sel !== "string") return;
      const ok = window.confirm(
        "Restaurar deste backup?\n\n" +
          "• Os casos serão recriados na pasta local padrão do SICRO.\n" +
          "• Casos que já existem NÃO são sobrescritos.\n" +
          "• Seu perfil e cabeçalhos (config) serão restaurados.\n\n" +
          "A pasta de backup de origem não é alterada.",
      );
      if (!ok) return;
      restoringRef.current = true;
      setRestoring(true);
      setRestoreError(null);
      setRestoreReport(null);
      setRestoreProgress(null);
      let unlisten: (() => void) | null = null;
      try {
        unlisten = await listen<RestoreProgress>(
          "restore-backup-progress",
          (ev) => setRestoreProgress(ev.payload),
        );
        const rep = await commands.restoreBackup(sel, {
          restoreConfig: true,
          overwrite: false,
        });
        // Os casos restaurados voltaram pros recentes; reindexar agora os
        // coloca no Histórico da Home sem precisar abrir cada um.
        await reindexCaseIndexFromRecents().catch(() => {});
        setRestoreReport(rep);
      } finally {
        if (unlisten) unlisten();
        restoringRef.current = false;
        setRestoring(false);
        setRestoreProgress(null);
      }
    } catch (e) {
      restoringRef.current = false;
      setRestoring(false);
      setRestoreError(toSicroError(e).message);
    }
  }, [dest]);

  const total = progress?.total ?? entries.length;
  const pct =
    progress && total > 0
      ? Math.min(100, Math.round(((progress.index + 1) / total) * 100))
      : 0;

  return (
    <section className={cfg.card}>
      <div className={cfg.cardHead}>
        <FolderArchive size={15} aria-hidden />
        <h2 className={cfg.cardTitle}>Backup geral</h2>
      </div>
      <p className={cfg.cardDesc}>
        Copia <strong>todas as ocorrências</strong> para uma pasta de destino
        (ex.: HD externo), com um arquivo <code>.sicrobackup</code> por caso.
        É <strong>incremental</strong>: só recopia os casos que mudaram desde o
        último backup. Backup completo — inclui fotos, vídeos e drone.
      </p>

      <div className={styles.body}>
        {/* Destino + ação */}
        <div className={styles.destRow}>
          <div className={styles.destInfo}>
            <span className={styles.destLabel}>Destino</span>
            {dest ? (
              <code className={styles.destPath} title={dest}>
                {dest}
              </code>
            ) : (
              <span className={styles.destEmpty}>
                Nenhum destino escolhido ainda.
              </span>
            )}
          </div>
          <div className={styles.destActions}>
            {dest && (
              <Button
                variant="ghost"
                size="sm"
                leftIcon={<FolderOpen size={14} />}
                onClick={handleReveal}
                disabled={running}
              >
                Abrir pasta
              </Button>
            )}
            <Button
              variant="primary"
              leftIcon={<FolderArchive size={15} />}
              onClick={() => void pickAndRun()}
              disabled={running || loadingEntries || entries.length === 0}
            >
              {running
                ? "Fazendo backup…"
                : dest
                  ? "Backup geral (trocar destino)"
                  : "Escolher destino e fazer backup"}
            </Button>
            {dest && !running && (
              <Button
                variant="secondary"
                onClick={() => void run(dest)}
                disabled={running || entries.length === 0}
                title={`Refazer no destino salvo: ${dest}`}
              >
                Repetir neste destino
              </Button>
            )}
          </div>
        </div>

        <p className={styles.meta}>
          {loadingEntries
            ? "Carregando acervo…"
            : `${entries.length} ocorrência(s) no acervo.`}
        </p>

        {/* Auto-backup ao fechar a ocorrência */}
        <label
          style={{
            display: "flex",
            alignItems: "center",
            gap: 8,
            fontSize: "var(--text-xs)",
            color: "var(--sicro-fg-dim)",
            cursor: dest ? "pointer" : "default",
          }}
        >
          <input
            type="checkbox"
            checked={autoOnClose}
            disabled={!dest}
            onChange={(e) => {
              setAutoBackupOnClose(e.target.checked);
              setAutoOnClose(e.target.checked);
            }}
          />
          <span>
            Backup automático ao <strong>fechar a ocorrência</strong>
            {!dest && " — defina um destino acima para ativar"}
            {lastAuto &&
              ` · último: ${new Date(lastAuto).toLocaleString("pt-BR")}`}
          </span>
        </label>

        {/* Progresso ao vivo */}
        {running && (
          <div className={styles.progressWrap}>
            <div className={styles.progressTrack}>
              <div className={styles.progressFill} style={{ width: `${pct}%` }} />
            </div>
            <span className={styles.progressLabel}>
              {progress
                ? `Caso ${progress.index + 1} de ${total} — ${progress.label || "—"} (${phaseLabel(progress.phase)})`
                : "Preparando…"}
            </span>
          </div>
        )}

        {error && (
          <p className={styles.error}>
            <AlertTriangle size={14} aria-hidden /> {error}
          </p>
        )}

        {/* Relatório */}
        {report && !running && (
          <div className={styles.report}>
            <div className={styles.summary}>
              <span className={`${styles.chip} ${styles.chipOk}`}>
                <CheckCircle2 size={13} aria-hidden /> {report.backed_up} copiado(s)
              </span>
              <span className={styles.chip}>{report.skipped} sem mudança</span>
              {report.missing > 0 && (
                <span className={`${styles.chip} ${styles.chipWarn}`}>
                  {report.missing} ausente(s)
                </span>
              )}
              {report.errors > 0 && (
                <span className={`${styles.chip} ${styles.chipErr}`}>
                  {report.errors} erro(s)
                </span>
              )}
              <span className={`${styles.chip} ${styles.chipSize}`}>
                <Database size={13} aria-hidden /> {prettyBytes(report.total_size_bytes)} no destino
              </span>
            </div>

            <ul className={styles.caseList}>
              {report.cases.map((c, i) => (
                <li key={`${c.workspace_id ?? c.workspace_path}-${i}`} className={styles.caseRow}>
                  <span className={styles.caseName} title={c.workspace_path}>
                    {c.label || c.workspace_path}
                  </span>
                  <span className={styles.caseInfo}>
                    {c.status === "backed_up" || c.status === "skipped_unchanged"
                      ? prettyBytes(c.size_bytes)
                      : c.error
                        ? c.error
                        : ""}
                  </span>
                  <span className={`${styles.caseStatus} ${styles[statusKey(c.status)] ?? ""}`}>
                    {statusLabel(c.status)}
                  </span>
                </li>
              ))}
            </ul>
          </div>
        )}

        {/* ---- Restaurar de um backup (HD/pendrive/nuvem) ---- */}
        <div className={styles.restoreBlock}>
          <div className={styles.destRow}>
            <div className={styles.destInfo}>
              <span className={styles.destLabel}>Restaurar backup</span>
              <span className={styles.destEmpty}>
                Traz de volta os casos + perfil de uma pasta de backup (ex.:
                computador novo). Não sobrescreve casos que já existem.
              </span>
            </div>
            <Button
              variant="secondary"
              leftIcon={<RotateCcw size={15} />}
              onClick={() => void pickAndRestore()}
              disabled={restoring || running}
            >
              {restoring ? "Restaurando…" : "Restaurar backup…"}
            </Button>
          </div>

          {restoring && (
            <div className={styles.progressWrap}>
              <div className={styles.progressTrack}>
                <div
                  className={styles.progressFill}
                  style={{
                    width: `${
                      restoreProgress && restoreProgress.total > 0
                        ? Math.min(
                            100,
                            Math.round(
                              ((restoreProgress.index + 1) /
                                restoreProgress.total) *
                                100,
                            ),
                          )
                        : 0
                    }%`,
                  }}
                />
              </div>
              <span className={styles.progressLabel}>
                {restoreProgress
                  ? `Caso ${restoreProgress.index + 1} de ${restoreProgress.total} — ${restoreProgress.label || "—"}`
                  : "Lendo o backup…"}
              </span>
            </div>
          )}

          {restoreError && (
            <p className={styles.error}>
              <AlertTriangle size={14} aria-hidden /> {restoreError}
            </p>
          )}

          {restoreReport && !restoring && (
            <div className={styles.report}>
              <div className={styles.summary}>
                <span className={`${styles.chip} ${styles.chipOk}`}>
                  <CheckCircle2 size={13} aria-hidden />{" "}
                  {restoreReport.restored} restaurado(s)
                </span>
                {restoreReport.skipped > 0 && (
                  <span className={styles.chip}>
                    {restoreReport.skipped} já existia(m)
                  </span>
                )}
                {restoreReport.errors > 0 && (
                  <span className={`${styles.chip} ${styles.chipErr}`}>
                    {restoreReport.errors} erro(s)
                  </span>
                )}
              </div>
              <p className={styles.meta}>
                Recarregue o SICRO para ver os casos restaurados e o perfil.
              </p>
              <Button
                variant="ghost"
                size="sm"
                onClick={() => window.location.reload()}
              >
                Recarregar agora
              </Button>
            </div>
          )}
        </div>
      </div>
    </section>
  );
}

// ---------------------------------------------------------------------------

function caseLabel(e: CaseIndexEntry): string {
  const parts = [
    e.numero_bo ? `BO ${e.numero_bo}` : null,
    e.tipo_pericia,
    e.municipio,
  ].filter((p): p is string => !!p);
  return parts.length ? parts.join(" — ") : `Ocorrência ${e.workspace_id.slice(0, 8)}`;
}

function phaseLabel(phase: string): string {
  switch (phase) {
    case "checking":
      return "verificando";
    case "backing_up":
      return "copiando";
    case "skipped":
      return "sem mudança";
    case "done":
      return "copiado";
    case "missing":
      return "ausente";
    case "error":
      return "erro";
    default:
      return phase;
  }
}

function statusLabel(status: string): string {
  switch (status) {
    case "backed_up":
      return "Copiado";
    case "skipped_unchanged":
      return "Sem mudança";
    case "missing":
      return "Ausente";
    case "error":
      return "Erro";
    default:
      return status;
  }
}

function statusKey(status: string): string {
  switch (status) {
    case "backed_up":
      return "stOk";
    case "skipped_unchanged":
      return "stSkip";
    case "missing":
      return "stWarn";
    case "error":
      return "stErr";
    default:
      return "stSkip";
  }
}

function prettyBytes(n: number): string {
  if (n < 1024) return `${n} B`;
  if (n < 1024 * 1024) return `${(n / 1024).toFixed(1)} KB`;
  if (n < 1024 * 1024 * 1024) return `${(n / 1024 / 1024).toFixed(1)} MB`;
  return `${(n / 1024 / 1024 / 1024).toFixed(2)} GB`;
}
