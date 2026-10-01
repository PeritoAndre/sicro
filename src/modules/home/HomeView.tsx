/**
 * HomeView — tela inicial do SICRO 4.
 *
 * Uma pergunta só: o que você vai fazer agora? Um nome para o caso e os três
 * módulos. Clicar num módulo cria o caso (quando não há um aberto) e já entra
 * nele; abrir um caso recente volta ao módulo em que ele foi trabalhado por
 * último. Dados do caso, backup, integridade, concluir e excluir ficam no
 * menu ⋯ — o SICRO não é cadastro, é bancada.
 *
 * §13 (KNOWN_LIMITATIONS): nada inventado na tela — contagens vêm do banco do
 * caso, datas do que foi gravado.
 */

import {
  useEffect,
  useMemo,
  useRef,
  useState,
  type KeyboardEvent,
  type ReactNode,
} from "react";
import { createPortal } from "react-dom";
import { useNavigate } from "react-router-dom";
import { open as openDirDialog } from "@tauri-apps/plugin-dialog";
import {
  CheckCircle2,
  Clapperboard,
  Download,
  FolderArchive,
  FolderOpen,
  ImagePlus,
  ListX,
  LogOut,
  Map as MapIcon,
  MoreHorizontal,
  Pencil,
  Plus,
  RotateCcw,
  Search,
  ShieldCheck,
  Trash2,
  type LucideIcon,
} from "lucide-react";
import { ConfirmDialog } from "@components/Dialog/ConfirmDialog";
import { pushToast } from "@/components/toast/toastStore";
import { commands } from "@core/commands";
import { toSicroError } from "@core/errors";
import { formatDate, formatRelative } from "@core/formatters";
import {
  selectActiveOccurrence,
  selectActiveWorkspacePath,
  selectRecents,
  useWorkspaceStore,
} from "@stores/workspaceStore";
import { occurrenceLabel, type RecentOccurrence } from "@domain/occurrence";
import type { WorkspaceCounters } from "@domain/alpha";
import { lastMidiaAba, lastWorkModuleOf } from "@modules/midia/midiaNav";
import { CaseDataDialog } from "./CaseDataDialog";
import { ImportSicroappDialog } from "./ImportSicroappDialog";
import styles from "./HomeView.module.css";

interface ModuleEntry {
  label: string;
  icon: LucideIcon;
  /** Rota de entrada (Vídeo e Áudio volta à aba usada por último). */
  to: () => string;
  /** "3 croquis", "2 vídeos · 1 áudio"… `null` quando não há nada. */
  summary: (c: WorkspaceCounters) => string | null;
}

// Os três módulos do 4.0, na ordem do trilho. Enter no nome abre no primeiro.
const MODULES: ModuleEntry[] = [
  {
    label: "Croqui",
    icon: MapIcon,
    to: () => "/croqui",
    summary: (c) => count(c.croquis, "croqui", "croquis"),
  },
  {
    label: "Vídeo e Áudio",
    icon: Clapperboard,
    to: lastMidiaAba,
    summary: (c) =>
      [count(c.videos, "vídeo", "vídeos"), count(c.audios, "áudio", "áudios")]
        .filter(Boolean)
        .join(" · ") || null,
  },
  {
    label: "Imagem",
    icon: ImagePlus,
    to: () => "/imagem",
    summary: (c) => count(c.image_analyses, "imagem", "imagens"),
  },
];

function count(n: number, one: string, many: string): string | null {
  return n > 0 ? `${n} ${n === 1 ? one : many}` : null;
}

export function HomeView() {
  const navigate = useNavigate();
  const occurrence = useWorkspaceStore(selectActiveOccurrence);
  const workspacePath = useWorkspaceStore(selectActiveWorkspacePath);
  const recents = useWorkspaceStore(selectRecents);
  const createOccurrence = useWorkspaceStore((s) => s.createOccurrence);
  const openOccurrence = useWorkspaceStore((s) => s.openOccurrence);
  const closeOccurrence = useWorkspaceStore((s) => s.closeOccurrence);
  const setActiveStatus = useWorkspaceStore((s) => s.setActiveStatus);
  const forgetRecent = useWorkspaceStore((s) => s.forgetRecent);
  const mutating = useWorkspaceStore((s) => s.isMutating);

  const hasCase = !!occurrence && !!workspacePath;
  // "Novo caso" com um caso aberto: o campo de nome toma o lugar do cabeçalho.
  const [newMode, setNewMode] = useState(false);
  const creating = !hasCase || newMode;

  const [name, setName] = useState("");
  const [busy, setBusy] = useState(false);
  const [counts, setCounts] = useState<WorkspaceCounters | null>(null);
  const [dataOpen, setDataOpen] = useState(false);
  const [importOpen, setImportOpen] = useState(false);
  const [concludeOpen, setConcludeOpen] = useState(false);
  const [pendingDelete, setPendingDelete] = useState<RecentOccurrence | null>(null);
  const [deleting, setDeleting] = useState(false);
  const [deleteError, setDeleteError] = useState<string | null>(null);
  const nameRef = useRef<HTMLInputElement>(null);

  // Trocou/fechou o caso → sai do modo "novo caso".
  useEffect(() => {
    setNewMode(false);
  }, [workspacePath]);

  useEffect(() => {
    if (creating) nameRef.current?.focus();
  }, [creating]);

  // Quantos croquis, vídeos… o caso aberto tem (best-effort: sem isso a tela
  // só fica sem os números).
  useEffect(() => {
    if (!workspacePath) {
      setCounts(null);
      return;
    }
    let cancelled = false;
    commands
      .getOccurrenceCounts(workspacePath)
      .then((c) => {
        if (!cancelled) setCounts(c);
      })
      .catch(() => {});
    return () => {
      cancelled = true;
    };
  }, [workspacePath]);

  /** Entra no módulo; sem caso aberto (ou em "novo caso"), cria o caso antes. */
  const enter = async (m: ModuleEntry) => {
    if (busy) return;
    if (!creating) {
      navigate(m.to());
      return;
    }
    setBusy(true);
    try {
      await createOccurrence({ titulo: name.trim() || null });
      setName("");
      setNewMode(false);
      navigate(m.to());
    } catch (e) {
      pushToast("error", toSicroError(e).message);
    } finally {
      setBusy(false);
    }
  };

  const onNameKey = (e: KeyboardEvent<HTMLInputElement>) => {
    if (e.key === "Enter") {
      e.preventDefault();
      void enter(MODULES[0]!);
    } else if (e.key === "Escape" && hasCase) {
      setNewMode(false);
      setName("");
    }
  };

  /** Abre o caso e volta ao módulo em que ele foi trabalhado por último. */
  const openCase = async (path: string) => {
    if (busy) return;
    setBusy(true);
    try {
      const loaded = await openOccurrence(path);
      const last = lastWorkModuleOf(loaded.occurrence.id);
      if (last) navigate(last);
    } catch (e) {
      pushToast("error", toSicroError(e).message);
    } finally {
      setBusy(false);
    }
  };

  const browse = async () => {
    try {
      const sel = await openDirDialog({
        directory: true,
        multiple: false,
        title: "Abrir uma pasta .sicro",
      });
      if (typeof sel === "string") await openCase(sel);
    } catch (e) {
      pushToast("error", toSicroError(e).message);
    }
  };

  const reveal = (path: string) => {
    void commands
      .revealPathInExplorer(path)
      .catch((e) => pushToast("error", toSicroError(e).message));
  };

  const backup = async () => {
    if (!workspacePath) return;
    setBusy(true);
    try {
      const a = await commands.generateWorkspaceBackup(
        workspacePath,
        undefined,
        occurrence?.numero_bo?.trim() || undefined,
      );
      pushToast("success", `Backup gerado: ${a.filename}`);
    } catch (e) {
      pushToast("error", toSicroError(e).message);
    } finally {
      setBusy(false);
    }
  };

  const conclude = async () => {
    try {
      await setActiveStatus("concluida");
      pushToast("success", "Caso concluído. Dá para reabrir quando precisar.");
    } catch (e) {
      pushToast("error", toSicroError(e).message);
    } finally {
      setConcludeOpen(false);
    }
  };

  const reopen = async () => {
    try {
      await setActiveStatus("aberta");
      pushToast("success", "Caso reaberto.");
    } catch (e) {
      pushToast("error", toSicroError(e).message);
    }
  };

  // Apaga a pasta .sicro do disco e tira o caso das listas. Se era o caso
  // aberto, fecha. Em erro, o popup fica aberto com a mensagem.
  const confirmDelete = async () => {
    if (!pendingDelete) return;
    const target = pendingDelete;
    setDeleting(true);
    setDeleteError(null);
    try {
      await commands.deleteOccurrence(target.workspace_path);
      await commands.removeCaseIndex(target.workspace_id).catch(() => {});
      await forgetRecent(target.workspace_id).catch(() => {});
      if (target.workspace_path === workspacePath) closeOccurrence();
      setPendingDelete(null);
    } catch (e) {
      setDeleteError(toSicroError(e).message);
    } finally {
      setDeleting(false);
    }
  };

  // O caso aberto não repete na lista — ele já está no cabeçalho.
  const activeRecent = useMemo(
    () => recents.find((r) => r.workspace_path === workspacePath) ?? null,
    [recents, workspacePath],
  );
  const others = useMemo(
    () => recents.filter((r) => r.workspace_path !== workspacePath),
    [recents, workspacePath],
  );

  const caseMenu: MenuItem[] = occurrence
    ? [
        { label: "Dados do caso", icon: <Pencil size={14} />, onClick: () => setDataOpen(true) },
        { label: "Abrir pasta", icon: <FolderOpen size={14} />, onClick: () => reveal(workspacePath!) },
        { label: "Integridade", icon: <ShieldCheck size={14} />, onClick: () => navigate("/integridade") },
        { label: "Gerar backup", icon: <FolderArchive size={14} />, onClick: () => void backup() },
        occurrence.status === "concluida"
          ? { label: "Reabrir caso", icon: <RotateCcw size={14} />, onClick: () => void reopen() }
          : { label: "Concluir caso", icon: <CheckCircle2 size={14} />, onClick: () => setConcludeOpen(true) },
        { label: "Fechar caso", icon: <LogOut size={14} />, onClick: closeOccurrence },
        ...(activeRecent
          ? [
              {
                label: "Excluir do disco",
                icon: <Trash2 size={14} />,
                danger: true,
                onClick: () => {
                  setDeleteError(null);
                  setPendingDelete(activeRecent);
                },
              },
            ]
          : []),
      ]
    : [];

  return (
    <div className={styles.page}>
      <div className={styles.column}>
        {creating || !occurrence ? (
          <section className={styles.hero} aria-label="Novo caso">
            <div className={styles.heroHead}>
              <span className={styles.sectionLabel}>Novo caso</span>
              {hasCase && (
                <button
                  type="button"
                  className={styles.linkBtn}
                  onClick={() => {
                    setNewMode(false);
                    setName("");
                  }}
                >
                  Cancelar
                </button>
              )}
            </div>
            <input
              ref={nameRef}
              type="text"
              className={styles.nameInput}
              placeholder="Nome do caso — ex.: Laudo 63404/26, Km 09 Duca Serra"
              aria-label="Nome do novo caso"
              value={name}
              onChange={(e) => setName(e.target.value)}
              onKeyDown={onNameKey}
              disabled={busy}
              spellCheck={false}
            />
            <p className={styles.hint}>
              Escolha por onde começar: o caso é criado e já abre lá. Enter abre no
              Croqui. O nome é opcional e pode mudar depois.
            </p>
          </section>
        ) : (
          <section className={styles.hero} aria-label="Caso aberto">
            <div className={styles.caseHead}>
              <button
                type="button"
                className={styles.caseName}
                onClick={() => setDataOpen(true)}
                title="Dados do caso"
              >
                <span className={styles.caseNameText}>{occurrenceLabel(occurrence)}</span>
                <Pencil size={15} className={styles.casePencil} aria-hidden />
              </button>
              <PopMenu items={caseMenu} label="Mais ações do caso" />
            </div>
            <p className={styles.caseSub}>
              {occurrence.status === "concluida" && (
                <span className={styles.badge}>concluído</span>
              )}
              criado {formatDate(occurrence.created_at)}
              <span className={styles.caseSep} aria-hidden>
                ·
              </span>
              <button
                type="button"
                className={styles.pathBtn}
                onClick={() => reveal(workspacePath!)}
                title={workspacePath!}
              >
                {compactPath(workspacePath!)}
              </button>
            </p>
          </section>
        )}

        <div className={styles.modules} role="group" aria-label="Módulos">
          {MODULES.map((m) => {
            const summary = !creating && counts ? m.summary(counts) : null;
            return (
              <button
                key={m.label}
                type="button"
                className={styles.module}
                onClick={() => void enter(m)}
                disabled={busy}
              >
                <m.icon size={30} strokeWidth={1.4} className={styles.moduleIcon} aria-hidden />
                <span className={styles.moduleName}>{m.label}</span>
                {summary && <span className={styles.moduleMeta}>{summary}</span>}
              </button>
            );
          })}
        </div>

        <RecentsList
          items={others}
          title={hasCase ? "Outros casos" : "Casos recentes"}
          busy={busy}
          action={
            hasCase && !newMode ? (
              <button
                type="button"
                className={styles.linkBtn}
                onClick={() => setNewMode(true)}
              >
                <Plus size={13} aria-hidden /> Novo caso
              </button>
            ) : null
          }
          onOpen={(r) => void openCase(r.workspace_path)}
          onReveal={(r) => reveal(r.workspace_path)}
          onForget={(r) => void forgetRecent(r.workspace_id)}
          onDelete={(r) => {
            setDeleteError(null);
            setPendingDelete(r);
          }}
        />

        <footer className={styles.footer}>
          <button type="button" className={styles.linkBtn} onClick={() => void browse()}>
            <FolderOpen size={13} aria-hidden /> Abrir pasta .sicro…
          </button>
          <button type="button" className={styles.linkBtn} onClick={() => setImportOpen(true)}>
            <Download size={13} aria-hidden /> Importar .sicroapp…
          </button>
        </footer>
      </div>

      <CaseDataDialog open={dataOpen} onClose={() => setDataOpen(false)} />
      <ImportSicroappDialog
        open={importOpen}
        onClose={() => setImportOpen(false)}
        onOpenWorkspace={(p) => void openCase(p)}
      />

      <ConfirmDialog
        open={concludeOpen}
        busy={mutating}
        title="Concluir caso?"
        confirmLabel="Concluir"
        message={
          <>
            <strong>{occurrence ? occurrenceLabel(occurrence) : ""}</strong> fica marcado como
            concluído, com a data de hoje.
          </>
        }
        detail="Nada é apagado — dá para reabrir depois."
        onCancel={() => setConcludeOpen(false)}
        onConfirm={() => void conclude()}
      />

      <ConfirmDialog
        open={!!pendingDelete}
        destructive
        busy={deleting}
        title="Excluir caso do disco?"
        confirmLabel="Excluir definitivamente"
        message={
          pendingDelete ? (
            <>
              Excluir <strong>{pendingDelete.occurrence_label}</strong>? A pasta{" "}
              <code>.sicro</code> some do disco com tudo o que há nela — croquis,
              vídeos, áudios, imagens.
            </>
          ) : (
            ""
          )
        }
        detail={deleteError ? `Falha ao excluir: ${deleteError}` : "Não dá para desfazer."}
        onCancel={() => {
          if (!deleting) {
            setPendingDelete(null);
            setDeleteError(null);
          }
        }}
        onConfirm={() => void confirmDelete()}
      />
    </div>
  );
}

// ---------------------------------------------------------------------------
// Casos recentes — uma linha por caso; busca só aparece quando a lista cresce
// ---------------------------------------------------------------------------

function RecentsList({
  items,
  title,
  busy,
  action,
  onOpen,
  onReveal,
  onForget,
  onDelete,
}: {
  items: RecentOccurrence[];
  title: string;
  busy: boolean;
  action: ReactNode;
  onOpen: (r: RecentOccurrence) => void;
  onReveal: (r: RecentOccurrence) => void;
  onForget: (r: RecentOccurrence) => void;
  onDelete: (r: RecentOccurrence) => void;
}) {
  const [query, setQuery] = useState("");
  const searchable = items.length > 8;
  const shown = useMemo(() => {
    const q = normalizeText(query.trim());
    if (!q || !searchable) return items;
    return items.filter((r) => normalizeText(r.occurrence_label).includes(q));
  }, [items, query, searchable]);

  return (
    <section className={styles.recents} aria-label={title}>
      <div className={styles.recentsHead}>
        <span className={styles.sectionLabel}>{title}</span>
        {action}
      </div>

      {searchable && (
        <label className={styles.search}>
          <Search size={14} aria-hidden />
          <input
            type="search"
            placeholder="Buscar caso…"
            value={query}
            onChange={(e) => setQuery(e.target.value)}
            aria-label="Buscar caso"
          />
        </label>
      )}

      {items.length === 0 ? (
        <p className={styles.empty}>Nenhum caso ainda.</p>
      ) : shown.length === 0 ? (
        <p className={styles.empty}>Nenhum caso com esse nome.</p>
      ) : (
        <ul className={styles.recentList}>
          {shown.map((r) => (
            <li key={r.workspace_id} className={styles.recentRow}>
              <button
                type="button"
                className={styles.recentBtn}
                onClick={() => onOpen(r)}
                disabled={busy}
                title={r.workspace_path}
              >
                {r.occurrence_label}
              </button>
              {r.status === "concluida" && <span className={styles.badge}>concluído</span>}
              <span className={styles.recentWhen}>{formatRelative(r.last_opened_at)}</span>
              <span className={styles.recentMenu}>
                <PopMenu
                  label={`Mais ações: ${r.occurrence_label}`}
                  items={[
                    { label: "Abrir pasta", icon: <FolderOpen size={14} />, onClick: () => onReveal(r) },
                    { label: "Tirar da lista", icon: <ListX size={14} />, onClick: () => onForget(r) },
                    { label: "Excluir do disco", icon: <Trash2 size={14} />, danger: true, onClick: () => onDelete(r) },
                  ]}
                />
              </span>
            </li>
          ))}
        </ul>
      )}
    </section>
  );
}

// ---------------------------------------------------------------------------
// Auxiliares
// ---------------------------------------------------------------------------

interface MenuItem {
  label: string;
  icon?: ReactNode;
  onClick: () => void;
  danger?: boolean;
}

function PopMenu({ items, label }: { items: MenuItem[]; label: string }) {
  const [open, setOpen] = useState(false);
  const [pos, setPos] = useState<{ top: number; left: number } | null>(null);
  const btnRef = useRef<HTMLButtonElement | null>(null);

  // Menu em portal no document.body, posicionado pela viewport — não é cortado
  // por nenhum overflow. Reposiciona em scroll/resize e fecha em Esc.
  useEffect(() => {
    if (!open) {
      setPos(null);
      return;
    }
    const MENU_W = 210;
    const MENU_H_EST = 40 * Math.max(items.length, 1) + 12;
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
    const onKey = (e: globalThis.KeyboardEvent) => {
      if (e.key === "Escape") setOpen(false);
    };
    window.addEventListener("scroll", compute, true);
    window.addEventListener("resize", compute);
    window.addEventListener("keydown", onKey);
    return () => {
      window.removeEventListener("scroll", compute, true);
      window.removeEventListener("resize", compute);
      window.removeEventListener("keydown", onKey);
    };
  }, [open, items.length]);

  return (
    <>
      <button
        ref={btnRef}
        type="button"
        className={styles.iconBtn}
        aria-label={label}
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
            <div className={styles.menu} role="menu" style={{ top: pos.top, left: pos.left }}>
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
    </>
  );
}

/** Busca sem acento nem caixa. */
function normalizeText(s: string): string {
  return s
    .normalize("NFD")
    .replace(/[̀-ͯ]/g, "")
    .toLowerCase();
}

/** Só o fim do caminho — o inteiro vai no title. */
function compactPath(p: string, max = 64): string {
  return p.length <= max ? p : "…" + p.slice(-(max - 1));
}
