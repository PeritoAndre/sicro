/**
 * CroquiListView — landing inside the Croqui module before any croqui is open.
 * Allows creating a new croqui and opening existing ones.
 */

import { useEffect, useState } from "react";
import type { MouseEvent } from "react";
import {
  Car,
  FileImage,
  Map,
  Plus,
  Route,
  Ruler,
  Shapes,
  Trash2,
} from "lucide-react";
import { Button } from "@components/Button/Button";
import { ConfirmDialog } from "@components/Dialog/ConfirmDialog";
import {
  ModuleLanding,
  type ModuleLandingFeature,
} from "@components/ModuleLanding/ModuleLanding";
import { formatDateTime } from "@core/formatters";
import { toSicroError } from "@core/errors";
import type { CroquiKind } from "@domain/croqui";
import { selectActiveWorkspacePath, useWorkspaceStore } from "@stores/workspaceStore";
import { useCroquiStore } from "./store/croquiStore";
import styles from "./CroquiListView.module.css";

// Linha mínima que precisamos guardar enquanto o popup está aberto.
// Espelha os campos do store sem importar o tipo da row inteira.
interface PendingDeleteCroqui {
  id: string;
  title: string;
}

const CROQUI_FEATURES: ModuleLandingFeature[] = [
  {
    icon: <Route size={18} />,
    title: "Vias e rotatórias",
    desc: "Traçado paramétrico com marcações, cruzamentos e interseções limpas.",
  },
  {
    icon: <Car size={18} />,
    title: "Veículos e vestígios",
    desc: "Silhuetas técnicas, mobiliário urbano, setas e pontos de impacto.",
  },
  {
    icon: <Map size={18} />,
    title: "Importar do OpenStreetMap",
    desc: "Puxe o traçado real por coordenada/raio e ajuste sobre a cena.",
  },
  {
    icon: <FileImage size={18} />,
    title: "Foto / drone de fundo",
    desc: "Use uma imagem do Dossiê como referência, com opacidade e bloqueio.",
  },
  {
    icon: <Ruler size={18} />,
    title: "Escala e medições",
    desc: "Calibre a escala e meça distâncias em unidade real.",
  },
  {
    icon: <FileImage size={18} />,
    title: "Export técnico",
    desc: "PNG com cabeçalho, escala e carimbo — pronto para o laudo.",
  },
];

export function CroquiListView() {
  const workspacePath = useWorkspaceStore(selectActiveWorkspacePath);
  const list = useCroquiStore((s) => s.list);
  const isLoadingList = useCroquiStore((s) => s.isLoadingList);
  const loadList = useCroquiStore((s) => s.loadList);
  const createCroqui = useCroquiStore((s) => s.createCroqui);
  const openCroqui = useCroquiStore((s) => s.openCroqui);
  const openCorpo = useCroquiStore((s) => s.openCorpo);
  const openPlanta = useCroquiStore((s) => s.openPlanta);
  const deleteCroqui = useCroquiStore((s) => s.deleteCroqui);

  const [title, setTitle] = useState("");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  // Croqui aguardando confirmação. `null` = nenhum popup aberto.
  const [pendingDelete, setPendingDelete] =
    useState<PendingDeleteCroqui | null>(null);

  useEffect(() => {
    if (workspacePath) void loadList(workspacePath);
  }, [workspacePath, loadList]);

  const handleCreate = async (kind: CroquiKind = "viario") => {
    if (!workspacePath) return;
    setBusy(true);
    setError(null);
    try {
      const created = await createCroqui(workspacePath, title.trim(), kind);
      setTitle("");
      // Viário já abre via store (activeDoc setado). Corporal/planta: manual.
      if (kind === "corporal") openCorpo(created.id);
      else if (kind === "planta") openPlanta(created.id);
    } catch (err) {
      setError(toSicroError(err).message);
    } finally {
      setBusy(false);
    }
  };

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

  // Exclui o croqui após confirmação via popup. `stopPropagation`
  // mantém o padrão do delete do laudo (o botão fica dentro de um
  // `row` que não é clicável hoje, mas o stopPropagation defende
  // contra mudanças futuras).
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
      // Mantemos o popup aberto se o usuário quiser tentar de novo.
    } finally {
      setBusy(false);
    }
  };

  const isEmpty = !isLoadingList && list.length === 0;

  if (isEmpty) {
    return (
      <div className={styles.wrap}>
        <ModuleLanding
          icon={<Shapes size={44} strokeWidth={1.2} />}
          title="Croqui — Reconstrução de Cena"
          subtitle="Desenhe o croqui técnico do sinistro: vias, veículos, vestígios e medidas em escala real. Importe o traçado do OpenStreetMap ou use foto/drone como fundo. Exporta PNG para o laudo."
          actions={
            <div
              className={styles.createRow}
              style={{ width: "100%", maxWidth: 480 }}
            >
              <input
                type="text"
                placeholder="Título (ex.: Sinistro Av. FAB)"
                value={title}
                onChange={(e) => setTitle(e.target.value)}
                disabled={busy}
                className={styles.titleInput}
              />
              <Button
                variant="secondary"
                leftIcon={<Plus size={15} />}
                onClick={() => void handleCreate("viario")}
                disabled={busy}
              >
                Croqui viário
              </Button>
              <Button
                variant="secondary"
                leftIcon={<Plus size={15} />}
                onClick={() => void handleCreate("corporal")}
                disabled={busy}
              >
                Croqui corporal
              </Button>
              <Button
                variant="secondary"
                leftIcon={<Plus size={15} />}
                onClick={() => void handleCreate("planta")}
                disabled={busy}
              >
                Croqui de planta
              </Button>
            </div>
          }
          features={CROQUI_FEATURES}
          note="O croqui é representação técnica de apoio. Medidas e posições refletem o levantamento do perito; os dados de origem (OSM, foto) ficam registrados."
        />
        {error && (
          <p className={styles.error} style={{ textAlign: "center" }}>
            {error}
          </p>
        )}
      </div>
    );
  }

  return (
    <div className={styles.wrap}>
      <div className={styles.container}>
        <header className={styles.header}>
          <div>
            <h1 className={styles.title}>Croqui</h1>
            <p className={styles.subtitle}>
              Representação técnica de apoio — croqui viário, corporal ou planta
              baixa. Cada croqui é um arquivo no workspace; o PNG técnico é
              derivado para uso avulso ou no laudo.
            </p>
          </div>
        </header>

        <section className={styles.createCard}>
          <h2 className={styles.sectionTitle}>Novo croqui</h2>
          <div className={styles.createRow}>
            <input
              type="text"
              placeholder="Título (ex.: Sinistro Av. FAB)"
              value={title}
              onChange={(e) => setTitle(e.target.value)}
              disabled={busy}
              className={styles.titleInput}
            />
            <Button
              variant="secondary"
              leftIcon={<Plus size={14} />}
              onClick={() => void handleCreate("viario")}
              disabled={busy}
            >
              Croqui viário
            </Button>
            <Button
              variant="secondary"
              leftIcon={<Plus size={14} />}
              onClick={() => void handleCreate("corporal")}
              disabled={busy}
            >
              Croqui corporal
            </Button>
            <Button
              variant="secondary"
              leftIcon={<Plus size={14} />}
              onClick={() => void handleCreate("planta")}
              disabled={busy}
            >
              Croqui de planta
            </Button>
          </div>
          {error && <p className={styles.error}>{error}</p>}
        </section>

        <section>
          <h2 className={styles.sectionTitle}>
            Croquis desta ocorrência ({list.length})
          </h2>
          {isLoadingList && list.length === 0 ? (
            <p className={styles.dim}>Carregando…</p>
          ) : (
            <ul className={styles.list}>
              {list.map((c) => (
                <li key={c.id} className={styles.row}>
                  <div className={styles.rowMain}>
                    <strong className={styles.rowTitle}>{c.title}</strong>
                    <span className={styles.rowMeta}>
                      <span
                        className={styles.chip}
                        style={
                          c.kind === "corporal"
                            ? { background: "#7c3aed", color: "#fff" }
                            : c.kind === "planta"
                              ? { background: "#0e7490", color: "#fff" }
                              : undefined
                        }
                      >
                        {c.kind === "corporal"
                          ? "Corporal"
                          : c.kind === "planta"
                            ? "Planta"
                            : "Viário"}
                      </span>
                      <span className={styles.chip}>{c.status}</span>
                      <span>v{c.schema_version}</span>
                      <span>atualizado {formatDateTime(c.updated_at)}</span>
                      {c.last_export_relative_path && (
                        <span title={c.last_export_relative_path}>
                          <FileImage
                            size={11}
                            style={{ verticalAlign: "-1px", marginRight: 3 }}
                          />
                          PNG exportado
                        </span>
                      )}
                    </span>
                  </div>
                  <Button
                    variant="secondary"
                    onClick={() => void handleOpen(c.id, c.kind)}
                    disabled={busy}
                  >
                    Abrir
                  </Button>
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
              ))}
            </ul>
          )}
        </section>
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
