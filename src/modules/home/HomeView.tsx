/**
 * HomeView — tela inicial "Início" do SICRO 2.0.
 *
 * Reformulação (W22): a Home deixa de ser um dashboard estatístico (isso vive
 * na aba Estatísticas) e passa a ser uma CENTRAL DE TRABALHO operacional:
 *   1. Workspace ativo — o card dominante da tela;
 *   2. Ações rápidas — criar / abrir / importar / verificar / backup;
 *   3. Atalhos dos módulos da ocorrência ativa;
 *   4. Ocorrências recentes — tabela enxuta;
 *   5. Avisos do sistema — apenas o essencial e acionável.
 *
 * §13 (KNOWN_LIMITATIONS): só exibimos o que conseguimos sustentar. Nada de
 * "último backup às 08:15" fabricado, métricas de produtividade ou contadores
 * globais — o status de integridade vem do snapshot REAL; o backup é manual,
 * por caso; o modo é local/offline de fato.
 */

import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { createPortal } from "react-dom";
import type { ReactNode } from "react";
import { useNavigate } from "react-router-dom";
import { open as openDirDialog } from "@tauri-apps/plugin-dialog";
import {
  AlertTriangle,
  ArrowRight,
  Building2,
  Calendar,
  Car,
  CheckCircle2,
  Clock,
  Crosshair,
  Database,
  Download,
  FileText,
  FolderArchive,
  FolderOpen,
  Info,
  LogOut,
  MapPin,
  MoreHorizontal,
  Plus,
  RotateCcw,
  Search,
  Shield,
  ShieldAlert,
  ShieldCheck,
  Trash2,
  WifiOff,
  X,
} from "lucide-react";
import { Button } from "@components/Button/Button";
import { StatusPill } from "@components/StatusPill/StatusPill";
import { ConfirmDialog } from "@components/Dialog/ConfirmDialog";
import { FeedbackButton } from "./FeedbackButton";
import { commands } from "@core/commands";
import { toSicroError } from "@core/errors";
import { formatDateTime, formatRelative } from "@core/formatters";
import {
  selectActiveOccurrence,
  selectActiveWorkspacePath,
  selectRecents,
  useWorkspaceStore,
} from "@stores/workspaceStore";
import type { CaseIndexEntry } from "@domain/case_index";
import { NewOccurrenceDialog } from "./NewOccurrenceDialog";
import { ImportSicroappDialog } from "./ImportSicroappDialog";
import type { Occurrence, OccurrenceStatus } from "@domain/occurrence";
import type { SystemHealthSnapshot } from "@domain/alpha";
import styles from "./HomeView.module.css";

export function HomeView() {
  const navigate = useNavigate();
  const occurrence = useWorkspaceStore(selectActiveOccurrence);
  const workspacePath = useWorkspaceStore(selectActiveWorkspacePath);
  const recents = useWorkspaceStore(selectRecents);
  const openOccurrence = useWorkspaceStore((s) => s.openOccurrence);
  const setActiveStatus = useWorkspaceStore((s) => s.setActiveStatus);
  const closeActiveOccurrence = useWorkspaceStore((s) => s.closeOccurrence);
  const mutating = useWorkspaceStore((s) => s.isMutating);

  const [dialogOpen, setDialogOpen] = useState(false);
  const [importOpen, setImportOpen] = useState(false);
  const [propsOpen, setPropsOpen] = useState(false);
  const [concludeOpen, setConcludeOpen] = useState(false);
  const [openError, setOpenError] = useState<string | null>(null);
  const [feedback, setFeedback] = useState<string | null>(null);
  const [snapshot, setSnapshot] = useState<SystemHealthSnapshot | null>(null);
  const [loadingSnap, setLoadingSnap] = useState(false);
  const [busy, setBusy] = useState<"none" | "backup" | "health">("none");

  const reloadSnapshot = useCallback(async () => {
    if (!workspacePath) {
      setSnapshot(null);
      return;
    }
    setLoadingSnap(true);
    try {
      setSnapshot(await commands.getSystemHealthSnapshot(workspacePath));
    } catch (e) {
      setSnapshot(null);
      setOpenError(toSicroError(e).message);
    } finally {
      setLoadingSnap(false);
    }
  }, [workspacePath]);

  useEffect(() => {
    void reloadSnapshot();
  }, [reloadSnapshot]);

  const openWs = async (path: string) => {
    setOpenError(null);
    try {
      await openOccurrence(path);
      navigate("/");
    } catch (e) {
      setOpenError(toSicroError(e).message);
    }
  };

  const handleBrowse = async () => {
    setOpenError(null);
    try {
      const sel = await openDirDialog({
        directory: true,
        multiple: false,
        title: "Selecione um workspace .sicro",
      });
      if (typeof sel === "string") await openWs(sel);
    } catch (e) {
      setOpenError(toSicroError(e).message);
    }
  };

  const handleBackup = async () => {
    if (!workspacePath) return;
    setBusy("backup");
    setFeedback(null);
    setOpenError(null);
    try {
      const bo = occurrence?.numero_bo?.trim() || undefined;
      const a = await commands.generateWorkspaceBackup(workspacePath, undefined, bo);
      setFeedback(
        `Backup gerado: ${a.filename} (${a.file_count} arquivo(s), ${prettyBytes(a.size_bytes)}).`,
      );
      void reloadSnapshot();
    } catch (e) {
      setOpenError(toSicroError(e).message);
    } finally {
      setBusy("none");
    }
  };

  const handleHealth = async () => {
    if (!workspacePath) return;
    setBusy("health");
    setFeedback(null);
    setOpenError(null);
    try {
      const a = await commands.generateSystemHealthReport(workspacePath);
      setFeedback(`Relatório de saúde salvo em ${a.relative_path}.`);
    } catch (e) {
      setOpenError(toSicroError(e).message);
    } finally {
      setBusy("none");
    }
  };

  const handleReveal = async () => {
    if (!workspacePath) return;
    try {
      await commands.revealPathInExplorer(workspacePath);
    } catch (e) {
      setOpenError(toSicroError(e).message);
    }
  };

  // --- Ciclo de vida da ocorrência (concluir / reabrir / fechar) ------------
  // Concluir/reabrir mudam só o status (comando dedicado, sem zerar o cabeçalho).
  // Fechar apenas desativa o caso ativo (não apaga nada) → volta ao estado vazio,
  // de onde dá pra criar/abrir outra.
  const handleConclude = async () => {
    setFeedback(null);
    setOpenError(null);
    try {
      await setActiveStatus("concluida");
      setFeedback("Ocorrência concluída. Você pode reabri-la quando precisar.");
    } catch (e) {
      setOpenError(toSicroError(e).message);
    } finally {
      setConcludeOpen(false);
    }
  };

  const handleReopen = async () => {
    setFeedback(null);
    setOpenError(null);
    try {
      await setActiveStatus("aberta");
      setFeedback("Ocorrência reaberta.");
    } catch (e) {
      setOpenError(toSicroError(e).message);
    }
  };

  const handleCloseOccurrence = () => {
    setFeedback(null);
    setOpenError(null);
    closeActiveOccurrence();
  };

  const hasWs = !!workspacePath && !!occurrence;
  const lastOpened = useMemo(() => {
    if (!occurrence) return null;
    return recents.find((r) => r.workspace_id === occurrence.id)?.last_opened_at ?? null;
  }, [recents, occurrence]);

  const integrity = snapshot?.workspace?.integrity_overall_status ?? null;

  return (
    <div className={styles.page}>
      <div className={styles.container}>
        {/* 1. Cabeçalho da página + status resumido */}
        <header className={styles.topBar}>
          <div className={styles.titleBlock}>
            <h1 className={styles.title}>Início</h1>
            <p className={styles.subtitle}>Central de ocorrências e workspaces locais.</p>
          </div>
          <div
            style={{ display: "flex", alignItems: "center", gap: "var(--space-3)" }}
          >
            <FeedbackButton />
            <HeaderStatus
              hasWs={hasWs}
              loading={loadingSnap}
              integrity={integrity}
              onVerify={() => navigate("/dossie?modo=integridade")}
            />
          </div>
        </header>

        {(feedback || openError) && (
          <div className={openError ? styles.bannerError : styles.bannerOk}>
            {openError ? <AlertTriangle size={14} /> : <CheckCircle2 size={14} />}
            <span>{openError ?? feedback}</span>
            <button
              type="button"
              className={styles.bannerClose}
              onClick={() => {
                setOpenError(null);
                setFeedback(null);
              }}
              aria-label="Fechar aviso"
            >
              <X size={13} />
            </button>
          </div>
        )}

        {/* 2. Linha principal: workspace ativo (dominante) + ações rápidas */}
        <div className={styles.primaryRow}>
          {hasWs && occurrence && workspacePath ? (
            <WorkspaceCard
              occurrence={occurrence}
              workspacePath={workspacePath}
              lastOpened={lastOpened}
              onContinue={() => navigate("/dossie")}
            />
          ) : (
            <EmptyWorkspaceCard
              onNew={() => setDialogOpen(true)}
              onBrowse={() => void handleBrowse()}
            />
          )}

          <QuickActions
            hasWs={hasWs}
            status={occurrence?.status ?? null}
            backupBusy={busy === "backup"}
            healthBusy={busy === "health"}
            onNew={() => setDialogOpen(true)}
            onBrowse={() => void handleBrowse()}
            onImport={() => setImportOpen(true)}
            onVerify={() => navigate("/dossie?modo=integridade")}
            onBackup={() => void handleBackup()}
            onProperties={() => setPropsOpen(true)}
            onReveal={() => void handleReveal()}
            onHealth={() => void handleHealth()}
            onConclude={() => setConcludeOpen(true)}
            onReopen={() => void handleReopen()}
            onCloseOccurrence={handleCloseOccurrence}
          />
        </div>

        {/* 4. Histórico completo de ocorrências — busca por texto + data */}
        <HistoryCard
          activeId={occurrence?.id ?? null}
          onOpen={(path) => void openWs(path)}
          onNew={() => setDialogOpen(true)}
          onBrowse={() => void handleBrowse()}
        />
      </div>

      <NewOccurrenceDialog
        open={dialogOpen}
        onClose={() => setDialogOpen(false)}
        onCreated={() => navigate("/")}
      />
      <ImportSicroappDialog
        open={importOpen}
        onClose={() => setImportOpen(false)}
        onOpenWorkspace={(p) => void openWs(p)}
      />
      {propsOpen && occurrence && workspacePath && (
        <PropertiesModal
          occurrence={occurrence}
          workspacePath={workspacePath}
          onReveal={() => void handleReveal()}
          onClose={() => setPropsOpen(false)}
        />
      )}

      <ConfirmDialog
        open={concludeOpen}
        busy={mutating}
        title="Concluir ocorrência?"
        confirmLabel="Concluir"
        message={
          <>
            A ocorrência <strong>{occurrence ? occLabel(occurrence) : ""}</strong>{" "}
            será marcada como <strong>Concluída</strong> e a data de encerramento
            registrada.
          </>
        }
        detail="Você pode reabri-la depois — nada é apagado."
        onCancel={() => setConcludeOpen(false)}
        onConfirm={() => void handleConclude()}
      />
    </div>
  );
}

// ---------------------------------------------------------------------------
// Cabeçalho — status resumido (integridade real, sem backup fabricado)
// ---------------------------------------------------------------------------

function HeaderStatus({
  hasWs,
  loading,
  integrity,
  onVerify,
}: {
  hasWs: boolean;
  loading: boolean;
  integrity: string | null;
  onVerify: () => void;
}) {
  let icon: ReactNode = <Shield size={15} />;
  let label = "Sem ocorrência ativa";
  let cls = styles.chipNeutral;
  if (loading) {
    label = "Verificando…";
  } else if (hasWs && integrity === "ok") {
    icon = <ShieldCheck size={15} />;
    label = "Sistema íntegro";
    cls = styles.chipOk;
  } else if (hasWs && integrity === "warning") {
    icon = <ShieldAlert size={15} />;
    label = "Atenção na integridade";
    cls = styles.chipWarn;
  } else if (hasWs && integrity === "critical") {
    icon = <ShieldAlert size={15} />;
    label = "Integridade crítica";
    cls = styles.chipCrit;
  } else if (hasWs) {
    icon = <ShieldCheck size={15} />;
    label = "Workspace ativo";
    cls = styles.chipOk;
  }
  return (
    <div className={styles.headerStatus}>
      <span className={`${styles.statusChip} ${cls}`}>
        {icon}
        {label}
      </span>
      <Button
        variant="secondary"
        leftIcon={<Shield size={15} />}
        onClick={onVerify}
        disabled={!hasWs}
      >
        Verificar integridade
      </Button>
    </div>
  );
}

// ---------------------------------------------------------------------------
// Card dominante: Workspace Ativo
// ---------------------------------------------------------------------------

function WorkspaceCard({
  occurrence,
  workspacePath,
  lastOpened,
  onContinue,
}: {
  occurrence: Occurrence;
  workspacePath: string;
  lastOpened: string | null;
  onContinue: () => void;
}) {
  return (
    <section className={styles.wsCard} aria-label="Workspace ativo">
      <div className={styles.sectionLabel}>Workspace ativo</div>
      <div className={styles.wsHeader}>
        <span className={styles.wsIcon}>
          <FolderOpen size={26} />
        </span>
        <div className={styles.wsHeadText}>
          <h2 className={styles.wsTitle}>{occLabel(occurrence)}</h2>
          <code className={styles.wsPath} title={workspacePath}>
            {workspacePath}
          </code>
        </div>
      </div>

      <div className={styles.wsChips}>
        <span className={styles.chip}>
          <Database size={12} aria-hidden /> Local
        </span>
        <span className={styles.chip}>
          <WifiOff size={12} aria-hidden /> Offline
        </span>
        <span className={styles.chip}>
          <Clock size={12} aria-hidden /> Atualizado {formatRelative(occurrence.updated_at)}
        </span>
        <StatusPill status={occurrence.status} />
      </div>

      <div className={styles.wsMeta}>
        <MetaItem
          icon={<Calendar size={14} />}
          label="Última abertura"
          value={lastOpened ? formatDateTime(lastOpened) : "—"}
        />
        <MetaItem
          icon={<FileText size={14} />}
          label="Tipo de perícia"
          value={occurrence.tipo_pericia || "—"}
        />
        <MetaItem icon={<MapPin size={14} />} label="Local" value={localLabel(occurrence)} />
      </div>

      <div className={styles.wsActions}>
        <Button variant="primary" leftIcon={<ArrowRight size={16} />} onClick={onContinue}>
          Continuar ocorrência
        </Button>
      </div>
    </section>
  );
}

function EmptyWorkspaceCard({ onNew, onBrowse }: { onNew: () => void; onBrowse: () => void }) {
  return (
    <section className={`${styles.wsCard} ${styles.wsEmpty}`} aria-label="Nenhum workspace ativo">
      <span className={styles.wsIcon}>
        <FolderOpen size={28} />
      </span>
      <h2 className={styles.wsEmptyTitle}>Nenhuma ocorrência ativa</h2>
      <p className={styles.wsEmptyDesc}>
        Crie uma nova ocorrência ou abra um workspace <code>.sicro</code> existente para começar a
        trabalhar.
      </p>
      <div className={styles.wsActions}>
        <Button variant="primary" leftIcon={<Plus size={16} />} onClick={onNew}>
          Nova ocorrência
        </Button>
        <Button variant="secondary" leftIcon={<FolderOpen size={15} />} onClick={onBrowse}>
          Abrir workspace
        </Button>
      </div>
    </section>
  );
}

// ---------------------------------------------------------------------------
// Ações rápidas (área de comando)
// ---------------------------------------------------------------------------

type QAItem = {
  id: string;
  icon: ReactNode;
  title: string;
  desc: string;
  onClick: () => void;
  disabled?: boolean;
};

/**
 * Painel único de ações. Agrega o que antes era "Ações rápidas" + o menu "⋯"
 * (escondido) do card do workspace. Quando há caso ativo, expõe TODAS as ações
 * do workspace (concluir/reabrir, propriedades, integridade, backup, relatório,
 * abrir pasta, fechar). Sem caso ativo, só as ações globais (nova/abrir/importar).
 */
function QuickActions({
  hasWs,
  status,
  backupBusy,
  healthBusy,
  onNew,
  onBrowse,
  onImport,
  onVerify,
  onBackup,
  onProperties,
  onReveal,
  onHealth,
  onConclude,
  onReopen,
  onCloseOccurrence,
}: {
  hasWs: boolean;
  status: OccurrenceStatus | null;
  backupBusy: boolean;
  healthBusy: boolean;
  onNew: () => void;
  onBrowse: () => void;
  onImport: () => void;
  onVerify: () => void;
  onBackup: () => void;
  onProperties: () => void;
  onReveal: () => void;
  onHealth: () => void;
  onConclude: () => void;
  onReopen: () => void;
  onCloseOccurrence: () => void;
}) {
  const globalItems: QAItem[] = [
    { id: "new", icon: <Plus size={18} />, title: "Nova ocorrência", desc: "Criar do zero", onClick: onNew },
    { id: "open", icon: <FolderOpen size={18} />, title: "Abrir workspace", desc: "Abrir existente", onClick: onBrowse },
    { id: "import", icon: <Download size={18} />, title: "Importar .sicroapp", desc: "De outro computador", onClick: onImport },
  ];
  const wsItems: QAItem[] = hasWs
    ? [
        status === "concluida"
          ? { id: "reopen", icon: <RotateCcw size={18} />, title: "Reabrir ocorrência", desc: "Voltar para Aberta", onClick: onReopen }
          : { id: "conclude", icon: <CheckCircle2 size={18} />, title: "Concluir ocorrência", desc: "Marcar como Concluída", onClick: onConclude },
        { id: "props", icon: <Info size={18} />, title: "Propriedades", desc: "Dados da ocorrência", onClick: onProperties },
        { id: "verify", icon: <ShieldCheck size={18} />, title: "Verificar integridade", desc: "Checar arquivos", onClick: onVerify },
        { id: "backup", icon: <FolderArchive size={18} />, title: backupBusy ? "Compactando…" : "Gerar backup", desc: "Do workspace ativo", onClick: onBackup, disabled: backupBusy },
        { id: "health", icon: <FileText size={18} />, title: healthBusy ? "Gerando…" : "Relatório de saúde", desc: "Diagnóstico do caso", onClick: onHealth, disabled: healthBusy },
        { id: "reveal", icon: <FolderOpen size={18} />, title: "Abrir pasta", desc: "No Explorer", onClick: onReveal },
        { id: "close", icon: <LogOut size={18} />, title: "Fechar ocorrência", desc: "Sem excluir nada", onClick: onCloseOccurrence },
      ]
    : [];
  const items = [...globalItems, ...wsItems];
  return (
    <section className={styles.qaCard} aria-label="Ações do workspace">
      <div className={styles.sectionLabel}>
        {hasWs ? "Ações do workspace" : "Ações"}
      </div>
      <div className={styles.qaGrid}>
        {items.map((it) => (
          <button
            key={it.id}
            type="button"
            className={styles.qaItem}
            onClick={it.onClick}
            disabled={it.disabled}
          >
            <span className={styles.qaIcon}>{it.icon}</span>
            <span className={styles.qaTitle}>{it.title}</span>
            <span className={styles.qaDesc}>{it.desc}</span>
          </button>
        ))}
      </div>
    </section>
  );
}

// ---------------------------------------------------------------------------
// Ocorrências recentes (tabela enxuta)
// ---------------------------------------------------------------------------

function HistoryCard({
  activeId,
  onOpen,
  onNew,
  onBrowse,
}: {
  activeId: string | null;
  onOpen: (workspacePath: string) => void;
  onNew: () => void;
  onBrowse: () => void;
}) {
  const [cases, setCases] = useState<CaseIndexEntry[]>([]);
  const [loading, setLoading] = useState(true);
  const [query, setQuery] = useState("");
  const [from, setFrom] = useState("");
  const [to, setTo] = useState("");
  // Exclusão de ocorrência (destrutivo — exige confirmação).
  const [pendingDelete, setPendingDelete] = useState<CaseIndexEntry | null>(null);
  const [deleting, setDeleting] = useState(false);
  const [deleteError, setDeleteError] = useState<string | null>(null);
  const closeOccurrence = useWorkspaceStore((s) => s.closeOccurrence);
  const forgetRecent = useWorkspaceStore((s) => s.forgetRecent);
  // Gatilho de reload do histórico: incrementa no store quando o índice de casos
  // é (re)gravado (criar/abrir/editar/excluir ocorrência). Sem isto, criar uma
  // ocorrência só aparecia aqui depois de navegar pra outro módulo e voltar.
  const caseIndexVersion = useWorkspaceStore((s) => s.caseIndexVersion);

  // Apaga a pasta .sicro do disco e limpa índice + recentes. Se a ocorrência
  // excluída era a ativa, fecha-a. Mantém o popup aberto com a mensagem se falhar.
  const confirmDelete = async () => {
    if (!pendingDelete) return;
    const target = pendingDelete;
    setDeleting(true);
    setDeleteError(null);
    try {
      await commands.deleteOccurrence(target.workspace_path);
      await commands.removeCaseIndex(target.workspace_id).catch(() => {});
      await forgetRecent(target.workspace_id).catch(() => {});
      if (target.workspace_id === activeId) closeOccurrence();
      setCases((prev) =>
        prev.filter((x) => x.workspace_id !== target.workspace_id),
      );
      setPendingDelete(null);
    } catch (e) {
      setDeleteError(toSicroError(e).message);
    } finally {
      setDeleting(false);
    }
  };

  useEffect(() => {
    let cancelled = false;
    commands
      .getCaseIndex()
      .then((idx) => {
        if (!cancelled) setCases(idx);
      })
      .catch(() => {
        /* índice ausente/ilegível → histórico vazio */
      })
      .finally(() => {
        if (!cancelled) setLoading(false);
      });
    return () => {
      cancelled = true;
    };
    // Recarrega o histórico ao mudar a versão do índice (nova ocorrência etc.).
  }, [caseIndexVersion]);

  const filtered = useMemo(() => {
    const q = normalizeText(query.trim());
    const rows = cases.filter((c) => {
      if (q) {
        const hay = normalizeText(
          [
            c.numero_bo,
            c.tipo_pericia,
            c.natureza,
            c.municipio,
            c.bairro,
            ...(c.peritos ?? []),
            c.workspace_id,
          ]
            .filter(Boolean)
            .join(" "),
        );
        if (!hay.includes(q)) return false;
      }
      if (from || to) {
        const d = (c.data_fato ?? c.created_at ?? "").slice(0, 10);
        if (!d) return false;
        if (from && d < from) return false;
        if (to && d > to) return false;
      }
      return true;
    });
    return rows.sort((a, b) => {
      const da = (a.data_fato ?? a.created_at ?? a.indexed_at ?? "").slice(0, 10);
      const db = (b.data_fato ?? b.created_at ?? b.indexed_at ?? "").slice(0, 10);
      return db.localeCompare(da);
    });
  }, [cases, query, from, to]);

  const hasFilter = !!(query.trim() || from || to);

  return (
    <section className={styles.recentsCard} aria-label="Histórico de ocorrências">
      <div className={styles.cardHead}>
        <h2 className={styles.cardTitle}>Histórico de ocorrências</h2>
        <span className={styles.cardMeta}>
          {loading
            ? "carregando…"
            : hasFilter
              ? `${filtered.length} de ${cases.length}`
              : `${cases.length} ocorrência(s)`}
        </span>
      </div>

      <div className={styles.historyFilters}>
        <div className={styles.searchBox}>
          <Search size={15} aria-hidden />
          <input
            type="search"
            className={styles.searchInput}
            placeholder="Buscar por BO, tipo, natureza, município, bairro, perito…"
            value={query}
            onChange={(e) => setQuery(e.target.value)}
          />
        </div>
        <label className={styles.dateField}>
          <span>De</span>
          <input
            type="date"
            value={from}
            max={to || undefined}
            onChange={(e) => setFrom(e.target.value)}
          />
        </label>
        <label className={styles.dateField}>
          <span>Até</span>
          <input
            type="date"
            value={to}
            min={from || undefined}
            onChange={(e) => setTo(e.target.value)}
          />
        </label>
        {hasFilter && (
          <Button
            variant="ghost"
            size="sm"
            onClick={() => {
              setQuery("");
              setFrom("");
              setTo("");
            }}
          >
            Limpar
          </Button>
        )}
      </div>

      {loading ? (
        <p className={styles.cardMeta} style={{ padding: "var(--space-3)" }}>
          Carregando histórico…
        </p>
      ) : cases.length === 0 ? (
        <div className={styles.recEmpty}>
          <FolderOpen size={26} strokeWidth={1.5} aria-hidden />
          <p>Nenhuma ocorrência no histórico ainda.</p>
          <div className={styles.recEmptyActions}>
            <Button variant="secondary" leftIcon={<FolderOpen size={15} />} onClick={onBrowse}>
              Abrir existente
            </Button>
            <Button variant="primary" leftIcon={<Plus size={15} />} onClick={onNew}>
              Criar nova
            </Button>
          </div>
        </div>
      ) : filtered.length === 0 ? (
        <div className={styles.recEmpty}>
          <Search size={26} strokeWidth={1.5} aria-hidden />
          <p>Nenhuma ocorrência corresponde à busca.</p>
        </div>
      ) : (
        <div className={styles.tableWrap}>
          <table className={styles.table}>
            <thead>
              <tr>
                <th>Nome / número</th>
                <th>Tipo de perícia</th>
                <th>Município</th>
                <th>Data do fato</th>
                <th>Status</th>
                <th className={styles.thAction}>Ação</th>
              </tr>
            </thead>
            <tbody>
              {filtered.map((c) => (
                <tr
                  key={c.workspace_id}
                  className={c.workspace_id === activeId ? styles.rowActive : ""}
                >
                  <td>
                    <div className={styles.recName}>
                      <button
                        type="button"
                        className={styles.recLabelBtn}
                        onClick={() => onOpen(c.workspace_path)}
                        title="Abrir esta ocorrência"
                      >
                        {indexLabel(c)}
                      </button>
                      <code className={styles.recPath} title={c.workspace_path}>
                        {c.workspace_path}
                      </code>
                    </div>
                  </td>
                  <td>
                    <span className={styles.recTipo}>
                      {tipoIcon(c.tipo_pericia)}
                      {c.tipo_pericia || "—"}
                    </span>
                  </td>
                  <td className={styles.recCell}>{c.municipio || "—"}</td>
                  <td className={styles.recDate}>
                    {formatDateOnly(c.data_fato ?? c.created_at)}
                  </td>
                  <td>
                    <StatusPill status={c.status as OccurrenceStatus} />
                  </td>
                  <td className={styles.tdAction}>
                    <div className={styles.tdActionInner}>
                      <Button variant="secondary" size="sm" onClick={() => onOpen(c.workspace_path)}>
                        Abrir
                      </Button>
                      <PopMenu
                        items={[
                          {
                            label: "Abrir pasta",
                            icon: <FolderOpen size={14} />,
                            onClick: () =>
                              void commands
                                .revealPathInExplorer(c.workspace_path)
                                .catch(() => {}),
                          },
                          {
                            label: "Excluir ocorrência",
                            icon: <Trash2 size={14} />,
                            danger: true,
                            onClick: () => {
                              setDeleteError(null);
                              setPendingDelete(c);
                            },
                          },
                        ]}
                      />
                    </div>
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      )}

      <ConfirmDialog
        open={!!pendingDelete}
        destructive
        busy={deleting}
        title="Excluir ocorrência?"
        confirmLabel="Excluir definitivamente"
        message={
          pendingDelete ? (
            <>
              Excluir <strong>{indexLabel(pendingDelete)}</strong>? Isto remove{" "}
              <strong>permanentemente</strong> a pasta <code>.sicro</code> do
              disco — laudos, croquis, fotos e todas as evidências do caso.
            </>
          ) : (
            ""
          )
        }
        detail={
          deleteError
            ? `Falha ao excluir: ${deleteError}`
            : "Esta ação não pode ser desfeita."
        }
        onCancel={() => {
          if (!deleting) {
            setPendingDelete(null);
            setDeleteError(null);
          }
        }}
        onConfirm={() => void confirmDelete()}
      />
    </section>
  );
}

/** Normaliza texto para busca: sem acento, minúsculas. */
function normalizeText(s: string): string {
  return s
    .normalize("NFD")
    .replace(/[̀-ͯ]/g, "")
    .toLowerCase();
}

/** Rótulo de uma entrada do índice (BO / tipo / município, com fallback). */
function indexLabel(c: CaseIndexEntry): string {
  const parts = [
    c.numero_bo ? `BO ${c.numero_bo}` : null,
    c.tipo_pericia,
    c.municipio,
  ].filter((p): p is string => !!p);
  return parts.length
    ? parts.join(" — ")
    : `Ocorrência ${c.workspace_id.slice(0, 8)}`;
}

/** Data (sem hora) em pt-BR; "—" quando ausente/inválida. */
function formatDateOnly(iso: string | null): string {
  if (!iso) return "—";
  const d = new Date(iso);
  return Number.isNaN(d.getTime()) ? "—" : d.toLocaleDateString("pt-BR");
}

// ---------------------------------------------------------------------------
// Auxiliares
// ---------------------------------------------------------------------------

function PopMenu({
  items,
}: {
  items: { label: string; icon?: ReactNode; onClick: () => void; danger?: boolean }[];
}) {
  const [open, setOpen] = useState(false);
  const [pos, setPos] = useState<{ top: number; left: number } | null>(null);
  const btnRef = useRef<HTMLButtonElement | null>(null);

  // Posiciona o menu COM PORTAL no document.body — sai de qualquer container
  // com overflow (ex.: .tableWrap), então não fica cortado nem fecha quando o
  // perito tenta rolar. Reposiciona em scroll/resize e fecha em Esc.
  useEffect(() => {
    if (!open) {
      setPos(null);
      return;
    }
    const MENU_W = 210;
    const MENU_H_EST = 44 * Math.max(items.length, 1) + 12;
    const compute = () => {
      const r = btnRef.current?.getBoundingClientRect();
      if (!r) return;
      let top = r.bottom + 4;
      let left = r.right - MENU_W;
      const vh = window.innerHeight;
      const vw = window.innerWidth;
      if (top + MENU_H_EST > vh - 8) top = Math.max(8, r.top - MENU_H_EST - 4);
      if (left < 8) left = 8;
      if (left + MENU_W > vw - 8) left = vw - MENU_W - 8;
      setPos({ top, left });
    };
    compute();
    const onScroll = () => compute();
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") setOpen(false);
    };
    window.addEventListener("scroll", onScroll, true);
    window.addEventListener("resize", onScroll);
    window.addEventListener("keydown", onKey);
    return () => {
      window.removeEventListener("scroll", onScroll, true);
      window.removeEventListener("resize", onScroll);
      window.removeEventListener("keydown", onKey);
    };
  }, [open, items.length]);

  return (
    <div className={styles.menuWrap}>
      <button
        ref={btnRef}
        type="button"
        className={styles.iconBtn}
        aria-label="Mais opções"
        aria-haspopup="menu"
        aria-expanded={open}
        onClick={() => setOpen((o) => !o)}
      >
        <MoreHorizontal size={16} />
      </button>
      {open &&
        pos &&
        createPortal(
          <>
            <div className={styles.menuBackdrop} onClick={() => setOpen(false)} />
            <div
              className={styles.menu}
              role="menu"
              style={{ top: pos.top, left: pos.left }}
            >
              {items.map((it) => (
                <button
                  key={it.label}
                  type="button"
                  role="menuitem"
                  className={`${styles.menuItem} ${it.danger ? styles.menuItemDanger : ""}`}
                  onClick={() => {
                    setOpen(false);
                    it.onClick();
                  }}
                >
                  {it.icon}
                  <span>{it.label}</span>
                </button>
              ))}
            </div>
          </>,
          document.body,
        )}
    </div>
  );
}

function PropertiesModal({
  occurrence,
  workspacePath,
  onReveal,
  onClose,
}: {
  occurrence: Occurrence;
  workspacePath: string;
  onReveal: () => void;
  onClose: () => void;
}) {
  const rows: [string, string | null][] = [
    ["Número do BO", occurrence.numero_bo],
    ["Protocolo", occurrence.protocolo],
    ["Requisição", occurrence.requisicao],
    ["Ofício", occurrence.oficio],
    ["Delegacia", occurrence.delegacia],
    ["Tipo de perícia", occurrence.tipo_pericia],
    ["Natureza", occurrence.natureza],
    ["Município", occurrence.municipio],
    ["Bairro", occurrence.bairro],
    ["Logradouro", occurrence.logradouro],
    ["Referência", occurrence.referencia],
    ["Data do fato", occurrence.data_fato ? formatDateTime(occurrence.data_fato) : null],
    [
      "Encerrada em",
      occurrence.data_encerramento ? formatDateTime(occurrence.data_encerramento) : null,
    ],
    ["Peritos", occurrence.peritos.length ? occurrence.peritos.join(", ") : null],
    ["Criada em", formatDateTime(occurrence.created_at)],
    ["Atualizada em", formatDateTime(occurrence.updated_at)],
  ];
  const shown = rows.filter(([, v]) => v != null && v !== "");

  return (
    <div className={styles.modalBackdrop} onClick={onClose}>
      <div
        className={styles.modal}
        role="dialog"
        aria-modal="true"
        aria-label="Propriedades da ocorrência"
        onClick={(e) => e.stopPropagation()}
      >
        <div className={styles.modalHead}>
          <h3 className={styles.modalTitle}>Propriedades da ocorrência</h3>
          <button type="button" className={styles.modalClose} onClick={onClose} aria-label="Fechar">
            <X size={16} />
          </button>
        </div>
        <div className={styles.modalBody}>
          <div className={styles.propGrid}>
            <div className={styles.propRow}>
              <span className={styles.propKey}>Status</span>
              <span className={styles.propVal}>
                <StatusPill status={occurrence.status} />
              </span>
            </div>
            {shown.map(([k, v]) => (
              <div key={k} className={styles.propRow}>
                <span className={styles.propKey}>{k}</span>
                <span className={styles.propVal}>{v}</span>
              </div>
            ))}
            <div className={`${styles.propRow} ${styles.propRowFull}`}>
              <span className={styles.propKey}>Workspace</span>
              <code className={styles.propPath}>{workspacePath}</code>
            </div>
          </div>
        </div>
        <div className={styles.modalActions}>
          <Button variant="secondary" leftIcon={<FolderOpen size={15} />} onClick={onReveal}>
            Abrir pasta
          </Button>
          <Button variant="primary" onClick={onClose}>
            Fechar
          </Button>
        </div>
      </div>
    </div>
  );
}

function MetaItem({ icon, label, value }: { icon: ReactNode; label: string; value: string }) {
  return (
    <div className={styles.metaItem}>
      <span className={styles.metaLabel}>
        {icon}
        {label}
      </span>
      <span className={styles.metaValue} title={value}>
        {value}
      </span>
    </div>
  );
}

function occLabel(o: Occurrence): string {
  const parts: string[] = [];
  if (o.numero_bo) parts.push(`BO ${o.numero_bo}`);
  if (o.tipo_pericia) parts.push(o.tipo_pericia);
  if (o.municipio) parts.push(o.municipio);
  return parts.length ? parts.join(" — ") : `Ocorrência ${o.id.slice(0, 8)}`;
}

function localLabel(o: Occurrence): string {
  const parts = [o.municipio, o.bairro].filter((p): p is string => !!p);
  return parts.length ? parts.join(" / ") : "—";
}

function tipoIcon(tipo: string | null): ReactNode {
  const t = (tipo ?? "").toLowerCase();
  if (t.includes("trâns") || t.includes("trans")) return <Car size={15} aria-hidden />;
  if (t.includes("patrim")) return <Building2 size={15} aria-hidden />;
  if (t.includes("crimin")) return <Crosshair size={15} aria-hidden />;
  return <FileText size={15} aria-hidden />;
}

function prettyBytes(n: number): string {
  if (n < 1024) return `${n} B`;
  if (n < 1024 * 1024) return `${(n / 1024).toFixed(1)} KB`;
  if (n < 1024 * 1024 * 1024) return `${(n / 1024 / 1024).toFixed(1)} MB`;
  return `${(n / 1024 / 1024 / 1024).toFixed(2)} GB`;
}
