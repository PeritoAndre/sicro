/** Central de Croquis: três lâminas para criar e a lista dos croquis do caso. */

import { useEffect, useState } from "react";
import { ConfirmDialog } from "@components/Dialog/ConfirmDialog";
import { Central, type CentralItem, type CentralLamina } from "@components/Central/Central";
import { toSicroError } from "@core/errors";
import type { CroquiKind } from "@domain/croqui";
import { selectActiveWorkspacePath, useWorkspaceStore } from "@stores/workspaceStore";
import { useCroquiStore } from "./store/croquiStore";
import { urlMiniatura } from "./thumb";
import viarioImg from "./assets/central/viario.webp";
import corporalImg from "./assets/central/corporal.webp";
import plantaImg from "./assets/central/planta.webp";

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

  const laminas: CentralLamina[] = [
    { id: "viario", nome: "Croqui viário", arte: viarioImg, onClick: abrirNovoViario },
    { id: "corporal", nome: "Croqui corporal", arte: corporalImg, onClick: abrirNovoCorporal },
    { id: "planta", nome: "Croqui de planta", arte: plantaImg, onClick: abrirNovoPlanta },
  ];

  const itens: CentralItem[] = list.map((c) => {
    const versao = `${thumbRev[c.id] ?? 0}-${c.updated_at}`;
    return {
      id: c.id,
      titulo: c.title,
      miniatura: (
        <img
          key={versao}
          src={
            workspacePath && !semThumb[c.id] ? urlMiniatura(workspacePath, c.id, versao) : ARTE[c.kind]
          }
          alt=""
          onError={() => setSemThumb((m) => ({ ...m, [c.id]: true }))}
        />
      ),
      tipo: TIPO[c.kind],
      resumo: `${STATUS[c.status] ?? c.status}${c.last_export_relative_path ? " · PNG exportado" : ""}`,
      quando: c.updated_at,
      onAbrir: () => void handleOpen(c.id, c.kind),
      onExcluir: () => setPendingDelete({ id: c.id, title: c.title }),
      excluirRotulo: "Excluir croqui",
    };
  });

  return (
    <Central
      titulo="Central de Croquis"
      laminas={laminas}
      itens={itens}
      carregando={isLoadingList}
      busy={busy}
      erro={error}
    >
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
    </Central>
  );
}
