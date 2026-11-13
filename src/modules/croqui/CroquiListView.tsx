/** Central de Croquis: três lâminas para criar e a lista dos croquis do caso. */

import { useEffect, useState } from "react";
import type { MouseEvent } from "react";
import { Plus, Trash2 } from "lucide-react";
import { ConfirmDialog } from "@components/Dialog/ConfirmDialog";
import { formatDateTime } from "@core/formatters";
import { toSicroError } from "@core/errors";
import type { CroquiKind } from "@domain/croqui";
import { selectActiveWorkspacePath, useWorkspaceStore } from "@stores/workspaceStore";
import { useCroquiStore } from "./store/croquiStore";
import { urlMiniatura } from "./thumb";
import viarioImg from "./assets/central/viario.webp";
import corporalImg from "./assets/central/corporal.webp";
import plantaImg from "./assets/central/planta.webp";
import styles from "./CroquiListView.module.css";

interface PendingDeleteCroqui {
  id: string;
  title: string;
}

const ARTE: Record<CroquiKind, string> = { viario: viarioImg, corporal: corporalImg, planta: plantaImg };
const TIPO: Record<CroquiKind, string> = { viario: "viário", corporal: "corporal", planta: "planta" };
const STATUS: Record<string, string> = { draft: "rascunho", final: "final", exported: "exportado" };

export function CroquiListView() {
  const workspacePath = useWorkspaceStore(selectActiveWorkspacePath);
  const list = useCroquiStore((s) => s.list);
  const isLoadingList = useCroquiStore((s) => s.isLoadingList);
  const loadList = useCroquiStore((s) => s.loadList);
  const abrirNovoViario = useCroquiStore((s) => s.abrirNovoViario);
  const abrirNovoCorporal = useCroquiStore((s) => s.abrirNovoCorporal);
  const abrirNovoPlanta = useCroquiStore((s) => s.abrirNovoPlanta);
  const openCroqui = useCroquiStore((s) => s.openCroqui);
  const openCorpo = useCroquiStore((s) => s.openCorpo);
  const openPlanta = useCroquiStore((s) => s.openPlanta);
  const deleteCroqui = useCroquiStore((s) => s.deleteCroqui);
  const thumbRev = useCroquiStore((s) => s.thumbRev);

  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [semThumb, setSemThumb] = useState<Record<string, boolean>>({});
  const [pendingDelete, setPendingDelete] =
    useState<PendingDeleteCroqui | null>(null);

  useEffect(() => {
    if (workspacePath) void loadList(workspacePath);
  }, [workspacePath, loadList]);

  const handleOpen = async (croquiId: string, kind: CroquiKind) => {
    if (!workspacePath) return;
    if (kind === "corporal") {
      openCorpo(croquiId);
      return;
    }
    if (kind === "planta") {
      openPlanta(croquiId);
      return;
    }
    setBusy(true);
    setError(null);
    try {
      await openCroqui(workspacePath, croquiId);
    } catch (err) {
      setError(toSicroError(err).message);
    } finally {
      setBusy(false);
    }
  };

  const handleDeleteClick = (
    e: MouseEvent<HTMLButtonElement>,
    croquiId: string,
    croquiTitle: string,
  ) => {
    e.stopPropagation();
    setPendingDelete({ id: croquiId, title: croquiTitle });
  };

  const handleConfirmDelete = async () => {
    if (!pendingDelete || !workspacePath) return;
    const target = pendingDelete;
    setBusy(true);
    setError(null);
    try {
      await deleteCroqui(workspacePath, target.id);
      setPendingDelete(null);
    } catch (err) {
      setError(toSicroError(err).message);
      // Popup fica aberto para tentar de novo.
    } finally {
      setBusy(false);
    }
  };

  const vazio = !isLoadingList && list.length === 0;

  const laminas = (
    <div className={styles.laminas}>
      {([
        ["viario", "Croqui viário", abrirNovoViario],
        ["corporal", "Croqui corporal", abrirNovoCorporal],
        ["planta", "Croqui de planta", abrirNovoPlanta],
      ] as const).map(([kind, nome, abrir]) => (
        <button key={kind} type="button" className={styles.lamina} onClick={abrir} disabled={busy}>
          <span className={styles.arte}>
            <img src={ARTE[kind]} alt="" />
          </span>
          <span className={styles.nome}>
            {nome}
            <Plus size={20} />
          </span>
        </button>
      ))}
    </div>
  );

  return (
    <div className={`${styles.wrap} ${vazio ? styles.vazio : ""}`}>
      <div className={styles.container}>
        <h1 className={styles.title}>Central de Croquis</h1>
        {laminas}
        {error && <p className={styles.error}>{error}</p>}

        {!vazio && (
          <section className={styles.lista}>
            <h2 className={styles.sectionTitle}>Neste caso</h2>
            {isLoadingList && list.length === 0 ? (
              <p className={styles.dim}>Carregando…</p>
            ) : (
              <ul className={styles.list}>
                {list.map((c) => {
                  const versao = `${thumbRev[c.id] ?? 0}-${c.updated_at}`;
                  return (
                    <li key={c.id} className={styles.row}>
                      <button
                        type="button"
                        className={styles.rowBtn}
                        onClick={() => void handleOpen(c.id, c.kind)}
                        disabled={busy}
                        aria-label={`Abrir ${c.title}`}
                      >
                        <span className={styles.mini}>
                          <img
                            key={versao}
                            src={
                              workspacePath && !semThumb[c.id]
                                ? urlMiniatura(workspacePath, c.id, versao)
                                : ARTE[c.kind]
                            }
                            alt=""
                            onError={() => setSemThumb((m) => ({ ...m, [c.id]: true }))}
                          />
                        </span>
                        <span className={styles.rowMain}>
                          <strong className={styles.rowTitle}>{c.title}</strong>
                          <span className={styles.rowMeta}>
                            <span className={styles.chip}>{TIPO[c.kind]}</span>
                            {STATUS[c.status] ?? c.status}
                            {c.last_export_relative_path ? " · PNG exportado" : ""}
                          </span>
                        </span>
                        <span className={styles.quando}>{formatDateTime(c.updated_at)}</span>
                      </button>
                      <button
                        type="button"
                        className={styles.deleteBtn}
                        title="Excluir croqui"
                        aria-label={`Excluir croqui ${c.title}`}
                        disabled={busy}
                        onClick={(e) => handleDeleteClick(e, c.id, c.title)}
                      >
                        <Trash2 size={14} />
                      </button>
                    </li>
                  );
                })}
              </ul>
            )}
          </section>
        )}
      </div>

      <ConfirmDialog
        open={pendingDelete !== null}
        title="Excluir croqui"
        destructive
        confirmLabel="Excluir croqui"
        cancelLabel="Cancelar"
        busy={busy}
        message={
          pendingDelete ? (
            <>
              Tem certeza que deseja excluir o croqui{" "}
              <strong>"{pendingDelete.title}"</strong>?
            </>
          ) : (
            ""
          )
        }
        detail="Esta ação não pode ser desfeita. O arquivo .sicrocroqui e o PNG exportado serão removidos do workspace."
        onCancel={() => {
          if (!busy) setPendingDelete(null);
        }}
        onConfirm={() => void handleConfirmDelete()}
      />
    </div>
  );
}
