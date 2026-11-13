/**
 * Tela inicial: cartão do caso aberto (ou vazio), casos recentes em cartões e,
 * à direita, ações, versão, backup e atalhos. Trocar de caso é na hora.
 */

import { useEffect, useMemo, useState } from "react";
import { useNavigate } from "react-router-dom";
import { open as openDirDialog } from "@tauri-apps/plugin-dialog";
import {
  ArrowLeftRight,
  CheckCircle2,
  Clapperboard,
  FolderArchive,
  FolderOpen,
  ImagePlus,
  ListX,
  LogOut,
  Map as MapIcon,
  Pencil,
  Pin,
  PinOff,
  RotateCcw,
  Search,
  ShieldCheck,
  Star,
  Trash2,
  type LucideIcon,
} from "lucide-react";
import { ConfirmDialog } from "@components/Dialog/ConfirmDialog";
import { Button } from "@components/Button/Button";
import { pushToast } from "@/components/toast/toastStore";
import { commands } from "@core/commands";
import { toSicroError } from "@core/errors";
import { formatDate, formatRelative } from "@core/formatters";
import { useSettingsStore } from "@stores/settingsStore";
import {
  selectActiveOccurrence,
  selectActiveWorkspacePath,
  selectRecents,
  useWorkspaceStore,
} from "@stores/workspaceStore";
import { caseEntryFromOccurrence } from "@domain/case_index";
import type { RecentOccurrence } from "@domain/occurrence";
import type { WorkspaceCounters } from "@domain/alpha";
import { lastMidiaAba } from "@modules/midia/midiaNav";
import { normalizeBinding } from "@core/keymap";
import { resolveBinding, useKeymapStore } from "@stores/keymapStore";
import { CaseDataDialog } from "./CaseDataDialog";
import { ImportSicroappDialog } from "./ImportSicroappDialog";
import { LateralHome } from "./LateralHome";
import { PopMenu, type MenuItem } from "./PopMenu";
import {
  contarEmAndamento,
  dataPorExtenso,
  estaConcluido,
  filtrarOrdenar,
  linhaMiuda,
  resumoConteudo,
  saudacao,
  tituloDoCaso,
  type Contagens,
  type FiltroCasos,
} from "./casosHome";
import { useDialogosCaso } from "./dialogosStore";
import { useFixados } from "./fixadosStore";
import { tituloRecente, useAbrirCaso, useContagens, useIndiceCasos } from "./useCasos";
import styles from "./HomeView.module.css";

interface ModuleEntry {
  label: string;
  icon: LucideIcon;
  /** Rota de entrada (Vídeo e Áudio volta à aba usada por último). */
  to: () => string;
  summary: (c: WorkspaceCounters) => string | null;
}

const count = (n: number, one: string, many: string) => (n > 0 ? `${n} ${n === 1 ? one : many}` : null);

const MODULES: ModuleEntry[] = [
  { label: "Croqui", icon: MapIcon, to: () => "/croqui", summary: (c) => count(c.croquis, "croqui", "croquis") },
  {
    label: "Vídeo e Áudio",
    icon: Clapperboard,
    to: lastMidiaAba,
    summary: (c) =>
      [count(c.videos, "vídeo", "vídeos"), count(c.audios, "áudio", "áudios")].filter(Boolean).join(" · ") || null,
  },
  { label: "Imagem", icon: ImagePlus, to: () => "/imagem", summary: (c) => count(c.image_analyses, "imagem", "imagens") },
];

const FILTROS: Array<{ id: FiltroCasos; label: string }> = [
  { id: "todos", label: "Todos" },
  { id: "andamento", label: "Em andamento" },
  { id: "concluidos", label: "Concluídos" },
];

const VISIVEIS = 6;

export function HomeView() {
  const navigate = useNavigate();
  const occurrence = useWorkspaceStore(selectActiveOccurrence);
  const workspacePath = useWorkspaceStore(selectActiveWorkspacePath);
  const recents = useWorkspaceStore(selectRecents);
  const closeOccurrence = useWorkspaceStore((s) => s.closeOccurrence);
  const setActiveStatus = useWorkspaceStore((s) => s.setActiveStatus);
  const forgetRecent = useWorkspaceStore((s) => s.forgetRecent);
  const loadRecents = useWorkspaceStore((s) => s.loadRecents);
  const bumpCaseIndex = useWorkspaceStore((s) => s.bumpCaseIndex);
  const mutating = useWorkspaceStore((s) => s.isMutating);
  const nomePerfil = useSettingsStore((s) => s.settings.profile.full_name);
  const abrirTrocar = useDialogosCaso((s) => s.abrirTrocar);
  const fixados = useFixados((s) => s.ids);
  const alternarFixado = useFixados((s) => s.alternar);
  const overrides = useKeymapStore((s) => s.overrides);
  const indice = useIndiceCasos();
  const abrirCaso = useAbrirCaso();

  const hasCase = !!occurrence && !!workspacePath;
  const [counts, setCounts] = useState<WorkspaceCounters | null>(null);
  const [dataOpen, setDataOpen] = useState(false);
  const [importOpen, setImportOpen] = useState(false);
  const [concludeOpen, setConcludeOpen] = useState(false);
  const [pendingDelete, setPendingDelete] = useState<RecentOccurrence | null>(null);
  const [deleting, setDeleting] = useState(false);
  const [deleteError, setDeleteError] = useState<string | null>(null);
  const [filtro, setFiltro] = useState<FiltroCasos>("todos");
  const [expandido, setExpandido] = useState(false);

  // Contagens do caso aberto (best-effort: sem elas a tela só fica sem números).
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

  const reveal = (path: string) => {
    void commands.revealPathInExplorer(path).catch((e) => pushToast("error", toSicroError(e).message));
  };

  const browse = async () => {
    try {
      const defaultPath = await commands.defaultWorkspaceParent().catch(() => undefined);
      const sel = await openDirDialog({ directory: true, multiple: false, title: "Abrir uma pasta .sicro", defaultPath });
      if (typeof sel === "string") await abrirCaso(sel);
    } catch (e) {
      pushToast("error", toSicroError(e).message);
    }
  };

  const backupDoCaso = async () => {
    if (!workspacePath) return;
    try {
      const a = await commands.generateWorkspaceBackup(
        workspacePath,
        undefined,
        occurrence?.numero_bo?.trim() || undefined,
      );
      pushToast("success", `Backup gerado: ${a.filename}`);
    } catch (e) {
      pushToast("error", toSicroError(e).message);
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

  /** Conclui/reabre um caso da lista sem abri-lo. */
  const alternarConcluido = async (r: RecentOccurrence) => {
    try {
      const occ = await commands.setOccurrenceStatus(r.workspace_path, estaConcluido(r.status) ? "aberta" : "concluida");
      await commands.upsertCaseIndex(caseEntryFromOccurrence(occ, r.workspace_path)).catch(() => {});
      await loadRecents();
      bumpCaseIndex();
    } catch (e) {
      pushToast("error", toSicroError(e).message);
    }
  };

  // Em erro, o popup fica aberto com a mensagem.
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

  const activeRecent = useMemo(
    () => recents.find((r) => r.workspace_path === workspacePath) ?? null,
    [recents, workspacePath],
  );

  // O caso aberto não repete na lista — já tem o cartão do alto.
  const lista = useMemo(
    () =>
      filtrarOrdenar(
        recents.filter((r) => r.workspace_path !== workspacePath),
        filtro,
        new Set(fixados),
      ),
    [recents, workspacePath, filtro, fixados],
  );
  const visiveis = expandido ? lista : lista.slice(0, VISIVEIS);
  const contagens = useContagens(visiveis.map((r) => r.workspace_path));

  const agora = new Date();
  const emAndamento = contarEmAndamento(recents);
  const textoAndamento =
    emAndamento === 0
      ? "nenhum caso em andamento"
      : `${emAndamento} ${emAndamento === 1 ? "caso" : "casos"} em andamento`;
  const bindTrocar = normalizeBinding(resolveBinding(overrides, "geral.trocarCaso"));

  const caseMenu: MenuItem[] = occurrence
    ? [
        { label: "Dados do caso", icon: <Pencil size={14} />, onClick: () => setDataOpen(true) },
        { label: "Abrir pasta", icon: <FolderOpen size={14} />, onClick: () => reveal(workspacePath!) },
        { label: "Integridade", icon: <ShieldCheck size={14} />, onClick: () => navigate("/integridade") },
        { label: "Gerar backup", icon: <FolderArchive size={14} />, onClick: () => void backupDoCaso() },
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

  const cardMenu = (r: RecentOccurrence): MenuItem[] => [
    fixados.includes(r.workspace_id)
      ? { label: "Desafixar", icon: <PinOff size={14} />, onClick: () => alternarFixado(r.workspace_id) }
      : { label: "Fixar no topo", icon: <Pin size={14} />, onClick: () => alternarFixado(r.workspace_id) },
    estaConcluido(r.status)
      ? { label: "Reabrir caso", icon: <RotateCcw size={14} />, onClick: () => void alternarConcluido(r) }
      : { label: "Marcar como concluído", icon: <CheckCircle2 size={14} />, onClick: () => void alternarConcluido(r) },
    { label: "Abrir pasta", icon: <FolderOpen size={14} />, onClick: () => reveal(r.workspace_path) },
    { label: "Tirar da lista", icon: <ListX size={14} />, onClick: () => void forgetRecent(r.workspace_id) },
    {
      label: "Excluir do disco",
      icon: <Trash2 size={14} />,
      danger: true,
      onClick: () => {
        setDeleteError(null);
        setPendingDelete(r);
      },
    },
  ];

  return (
    <div className={styles.page}>
      <header className={styles.topo}>
        <div className={styles.saudacao}>
          <h1>{saudacao(agora.getHours(), nomePerfil)}</h1>
          <span>
            {dataPorExtenso(agora)} · {hasCase ? textoAndamento : `nenhum caso aberto · ${emAndamento} em andamento`}
          </span>
        </div>
        <button type="button" className={styles.busca} onClick={abrirTrocar} aria-label="Buscar caso">
          <Search size={16} aria-hidden />
          <span className={styles.buscaTexto}>Buscar caso por nome, laudo ou ocorrência…</span>
          <kbd className={styles.kbd}>{bindTrocar}</kbd>
        </button>
      </header>

      <div className={styles.corpo}>
        <section className={styles.principal} aria-label="Casos">
          {hasCase && occurrence ? (
            <article className={styles.aberto} aria-label="Caso aberto">
              <div className={styles.abertoTopo}>
                <div className={styles.abertoTexto}>
                  <span className={styles.etiqueta}>
                    <span className={styles.ponto} aria-hidden />
                    Caso aberto
                    {occurrence.status === "concluida" && <span className={styles.badge}>concluído</span>}
                  </span>
                  <h2 className={styles.titulo} title={tituloDoCaso(occurrence)}>
                    {tituloDoCaso(occurrence)}
                  </h2>
                  <p className={styles.sub}>
                    criado {formatDate(occurrence.created_at)}
                    <span aria-hidden>·</span>
                    <button
                      type="button"
                      className={styles.pathBtn}
                      onClick={() => reveal(workspacePath!)}
                      title={workspacePath!}
                    >
                      {compactPath(workspacePath!)}
                    </button>
                  </p>
                </div>
                <div className={styles.abertoAcoes}>
                  <Button variant="secondary" leftIcon={<ArrowLeftRight size={16} />} onClick={abrirTrocar}>
                    Trocar de caso
                  </Button>
                  <PopMenu items={caseMenu} label="Mais ações do caso" className={styles.menuBtn} />
                </div>
              </div>
              <div className={styles.modulos} role="group" aria-label="Módulos">
                {MODULES.map((m) => (
                  <button key={m.label} type="button" className={styles.modulo} onClick={() => navigate(m.to())}>
                    <m.icon size={22} aria-hidden />
                    <span className={styles.moduloNome}>{m.label}</span>
                    <span className={styles.moduloMeta}>{(counts && m.summary(counts)) ?? "nada ainda"}</span>
                  </button>
                ))}
              </div>
            </article>
          ) : (
            <article className={`${styles.aberto} ${styles.vazio}`} aria-label="Nenhum caso aberto">
              <div className={styles.abertoTopo}>
                <div className={styles.abertoTexto}>
                  <span className={`${styles.etiqueta} ${styles.etiquetaVazia}`}>
                    <span className={`${styles.ponto} ${styles.pontoVazio}`} aria-hidden />
                    Nenhum caso aberto
                  </span>
                  <h2 className={styles.titulo}>Escolha um caso</h2>
                  <p className={styles.sub}>
                    {recents.length === 0
                      ? "Nenhum caso ainda"
                      : `${recents.length} ${recents.length === 1 ? "caso" : "casos"}`}
                  </p>
                </div>
                <div className={styles.abertoAcoes}>
                  <Button variant="secondary" leftIcon={<FolderOpen size={16} />} onClick={abrirTrocar}>
                    Abrir caso
                  </Button>
                </div>
              </div>
              <div className={styles.modulos} role="group" aria-label="Módulos">
                {MODULES.map((m) => (
                  <div key={m.label} className={`${styles.modulo} ${styles.moduloApagado}`} aria-disabled>
                    <m.icon size={22} aria-hidden />
                    <span className={styles.moduloNome}>{m.label}</span>
                    <span className={styles.moduloMeta}>—</span>
                  </div>
                ))}
              </div>
            </article>
          )}

          <section className={styles.recentes} aria-label="Casos recentes">
            <div className={styles.recentesTopo}>
              <h2 className={styles.secao}>Casos recentes</h2>
              <div className={styles.filtros} role="group" aria-label="Filtro">
                {FILTROS.map((f) => (
                  <button
                    key={f.id}
                    type="button"
                    className={`${styles.filtro} ${filtro === f.id ? styles.filtroOn : ""}`}
                    aria-pressed={filtro === f.id}
                    onClick={() => setFiltro(f.id)}
                  >
                    {f.label}
                  </button>
                ))}
              </div>
            </div>

            {lista.length === 0 ? (
              <p className={styles.nada}>
                {recents.length === 0 ? "Nenhum caso ainda." : "Nenhum caso nesse filtro."}
              </p>
            ) : (
              <ul className={styles.grade}>
                {visiveis.map((r) => {
                  const e = indice.get(r.workspace_path);
                  const titulo = tituloRecente(r, indice);
                  const c = contagens.get(r.workspace_path);
                  const base: Contagens | null = c
                    ? c
                    : e?.counts
                      ? { croquis: e.counts.croquis, videos: e.counts.videos, image_analyses: e.counts.image_analyses }
                      : null;
                  const fixado = fixados.includes(r.workspace_id);
                  return (
                    <li key={r.workspace_id} className={styles.cartao}>
                      <button
                        type="button"
                        className={styles.cartaoBtn}
                        onClick={() => void abrirCaso(r.workspace_path)}
                        disabled={mutating}
                        title={r.workspace_path}
                      >
                        <span className={styles.cartaoTitulo}>{titulo}</span>
                        <span className={styles.cartaoLinha}>
                          {[resumoConteudo(base), `aberto ${formatRelative(r.last_opened_at)}`]
                            .filter(Boolean)
                            .join(" · ")}
                          {estaConcluido(r.status) && <span className={styles.badge}>concluído</span>}
                        </span>
                        <span className={styles.cartaoMiudo}>{linhaMiuda(titulo, e?.created_at)}</span>
                      </button>
                      <div className={styles.cartaoAcoes}>
                        <button
                          type="button"
                          className={`${styles.estrela} ${fixado ? styles.estrelaOn : ""}`}
                          aria-label={fixado ? "Desafixar" : "Fixar no topo"}
                          aria-pressed={fixado}
                          onClick={() => alternarFixado(r.workspace_id)}
                        >
                          <Star size={14} fill={fixado ? "currentColor" : "none"} />
                        </button>
                        <PopMenu items={cardMenu(r)} label={`Mais ações: ${titulo}`} className={styles.cartaoMenu} />
                      </div>
                    </li>
                  );
                })}
              </ul>
            )}

            {lista.length > VISIVEIS && (
              <button type="button" className={styles.verTodos} onClick={() => setExpandido((v) => !v)}>
                {expandido ? "Mostrar menos" : `Ver todos os ${lista.length} casos`}
              </button>
            )}
          </section>
        </section>

        <LateralHome totalCasos={indice.size || recents.length} onBrowse={() => void browse()} onImport={() => setImportOpen(true)} />
      </div>

      <CaseDataDialog open={dataOpen} onClose={() => setDataOpen(false)} />
      <ImportSicroappDialog
        open={importOpen}
        onClose={() => setImportOpen(false)}
        onOpenWorkspace={(p) => void abrirCaso(p)}
      />

      <ConfirmDialog
        open={concludeOpen}
        busy={mutating}
        title="Concluir caso?"
        confirmLabel="Concluir"
        message={
          <>
            <strong>{occurrence ? tituloDoCaso(occurrence) : ""}</strong> fica marcado como concluído, com a data de
            hoje.
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
              Excluir <strong>{pendingDelete.occurrence_label}</strong>? A pasta <code>.sicro</code> some do disco com
              tudo o que há nela — croquis, vídeos, áudios, imagens.
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

/** Só o fim do caminho — o inteiro vai no title. */
function compactPath(p: string, max = 64): string {
  return p.length <= max ? p : "…" + p.slice(-(max - 1));
}
