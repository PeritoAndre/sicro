/**
 * LaudoListView — landing screen / registro do módulo Laudo (SICRO 3.0).
 *
 * Lista os laudos `.docx` da ocorrência. "Novo laudo" cria um `.docx` a partir
 * de um template; "Registrar .docx existente" importa um documento já escrito.
 * Clicar num card abre a BridgeView; "Abrir no Word" abre o `.docx` no editor
 * externo (Word/LibreOffice).
 */

import { useEffect, useState } from "react";
import type { MouseEvent } from "react";
import {
  FileOutput,
  FileText,
  FileUp,
  Images,
  LayoutTemplate,
  PenLine,
  Plus,
  SquareArrowOutUpRight,
  Trash2,
} from "lucide-react";
import { open as openFileDialog } from "@tauri-apps/plugin-dialog";
import { Button } from "@components/Button/Button";
import { Card } from "@components/Card/Card";
import { ConfirmDialog } from "@components/Dialog/ConfirmDialog";
import {
  ModuleLanding,
  type ModuleLandingFeature,
} from "@components/ModuleLanding/ModuleLanding";
import { StatusPill } from "@components/StatusPill/StatusPill";
import { formatRelative } from "@core/formatters";
import { useLaudoStore } from "../store/laudoStore";
import { useWorkspaceStore } from "@stores/workspaceStore";
import { NewLaudoDialog } from "../components/NewLaudoDialog";
import { SignatureBadge } from "../components/SignatureBadge";
import type { OccurrenceContext } from "../document-engine";
import type { Laudo, LaudoStatus } from "@domain/laudo";
import type { Occurrence, OccurrenceStatus } from "@domain/occurrence";
import styles from "./LaudoListView.module.css";

interface LaudoListViewProps {
  workspacePath: string;
  onOpen: (laudo: Laudo) => void;
  onCreate: () => void;
}

const LAUDO_FEATURES: ModuleLandingFeature[] = [
  {
    icon: <FileText size={18} />,
    title: "Laudo em .docx",
    desc: "O laudo é um documento do Word/LibreOffice, editado no seu editor de sempre.",
  },
  {
    icon: <LayoutTemplate size={18} />,
    title: "Templates institucionais",
    desc: "Cabeçalho/timbre e campos da ocorrência já preenchidos no esqueleto inicial.",
  },
  {
    icon: <Images size={18} />,
    title: "Evidências no laudo",
    desc: "Copie fotos, croqui exportado e frames direto para o documento (Ctrl+V).",
  },
  {
    icon: <PenLine size={18} />,
    title: "Consulta do caso",
    desc: "Os dados da ocorrência sempre à mão enquanto você redige.",
  },
  {
    icon: <FileUp size={18} />,
    title: "Registrar existente",
    desc: "Já tem o .docx pronto? Registre-o no caso sem reescrever nada.",
  },
  {
    icon: <FileOutput size={18} />,
    title: "Abertura externa",
    desc: "Um clique abre o .docx no Word/LibreOffice instalado.",
  },
];

export function LaudoListView({
  workspacePath,
  onOpen,
  onCreate,
}: LaudoListViewProps) {
  const list = useLaudoStore((s) => s.list);
  const isLoading = useLaudoStore((s) => s.isLoadingList);
  const isMutating = useLaudoStore((s) => s.isMutating);
  const error = useLaudoStore((s) => s.lastError);
  const loadList = useLaudoStore((s) => s.loadList);
  const deleteLaudo = useLaudoStore((s) => s.deleteLaudo);
  const registerExistingDocx = useLaudoStore((s) => s.registerExistingDocx);
  const openExternal = useLaudoStore((s) => s.openExternal);
  const activeOccurrence = useWorkspaceStore((s) => s.activeOccurrence);

  const [dialogOpen, setDialogOpen] = useState(false);
  const [isImporting, setIsImporting] = useState(false);
  // Laudo aguardando confirmação de exclusão. `null` significa que
  // não há popup aberto. Trocamos `window.confirm` por um modal
  // próprio pra dar ao usuário um segundo pra respirar antes de
  // apagar um trabalho.
  const [pendingDelete, setPendingDelete] = useState<Laudo | null>(null);

  // Botão de exclusão de cada card: abre o popup de confirmação.
  // `stopPropagation` evita que o clique abra o laudo através do
  // `Card interactive` que envolve o conteúdo.
  const handleDeleteClick = (
    e: MouseEvent<HTMLButtonElement>,
    laudo: Laudo,
  ) => {
    e.stopPropagation();
    setPendingDelete(laudo);
  };

  const handleConfirmDelete = async () => {
    if (!pendingDelete) return;
    const target = pendingDelete;
    try {
      await deleteLaudo(workspacePath, target.id);
    } finally {
      // Fechamos sempre, mesmo em caso de erro — o erro fica
      // exibido no banner geral via `lastError`.
      setPendingDelete(null);
    }
  };

  useEffect(() => {
    void loadList(workspacePath);
  }, [workspacePath, loadList]);

  const suggestedTitle = buildSuggestedTitle(activeOccurrence);
  const occurrenceContext = activeOccurrence
    ? toOccurrenceContext(activeOccurrence)
    : null;

  // SICRO 3.0 — Registrar um `.docx` que o perito já escreveu por fora:
  // escolhe o arquivo, copia pro workspace + registra a linha. O store já
  // seleciona o laudo (mostra a BridgeView).
  const handleRegisterDocx = async () => {
    let selected: string | string[] | null = null;
    try {
      selected = await openFileDialog({
        multiple: false,
        filters: [{ name: "Documento do Word (.docx)", extensions: ["docx"] }],
      });
    } catch {
      return;
    }
    if (!selected || typeof selected !== "string") return;
    setIsImporting(true);
    try {
      // Título derivado do nome do arquivo no backend quando vazio.
      const laudo = await registerExistingDocx("", selected);
      onOpen(laudo);
    } catch {
      // O erro fica visível no banner geral via `lastError` do store.
    } finally {
      setIsImporting(false);
    }
  };

  // Abre o `.docx` no Word/LibreOffice direto do card (sem entrar na ponte).
  const handleOpenExternal = (e: MouseEvent<HTMLButtonElement>, laudo: Laudo) => {
    e.stopPropagation();
    void openExternal(laudo.id);
  };

  if (list.length === 0 && !isLoading) {
    return (
      <div className={styles.wrap}>
        <ModuleLanding
          icon={<FileText size={44} strokeWidth={1.2} />}
          title="Laudos da Ocorrência"
          subtitle="O laudo pericial é um documento .docx editado no Word ou LibreOffice. Crie um a partir de um template, registre um já pronto, e use a ponte para consultar o caso e enviar imagens ao documento."
          actions={
            <>
              <Button
                variant="ghost"
                leftIcon={<FileUp size={15} />}
                onClick={() => void handleRegisterDocx()}
                disabled={isMutating || isImporting}
              >
                {isImporting ? "Registrando…" : "Registrar .docx existente"}
              </Button>
              <Button
                variant="primary"
                leftIcon={<Plus size={15} />}
                onClick={() => setDialogOpen(true)}
                disabled={isMutating}
              >
                Novo laudo
              </Button>
            </>
          }
          features={LAUDO_FEATURES}
          note="O SICRO é apoio à redação. O conteúdo técnico, as conclusões e a assinatura são de responsabilidade do perito."
        />
        {error && (
          <p className={styles.error} style={{ textAlign: "center" }}>
            {error.message}
          </p>
        )}
        <NewLaudoDialog
          open={dialogOpen}
          workspacePath={workspacePath}
          suggestedTitle={suggestedTitle}
          occurrence={occurrenceContext}
          onClose={() => setDialogOpen(false)}
          onCreated={() => onCreate()}
        />
      </div>
    );
  }

  return (
    <div className={styles.wrap}>
      <div className={styles.container}>
        <header className={styles.header}>
          <div className={styles.titleBlock}>
            <h1 className={styles.title}>Laudos da ocorrência</h1>
            <p className={styles.subtitle}>
              {isLoading
                ? "carregando…"
                : `${list.length} laudo(s) registrado(s) no workspace.`}
            </p>
          </div>
          <div style={{ display: "flex", gap: 8 }}>
            <Button
              variant="ghost"
              leftIcon={<FileUp size={16} />}
              onClick={() => void handleRegisterDocx()}
              disabled={isMutating || isImporting}
            >
              {isImporting ? "Registrando…" : "Registrar .docx existente"}
            </Button>
            <Button
              variant="primary"
              leftIcon={<Plus size={16} />}
              onClick={() => setDialogOpen(true)}
              disabled={isMutating}
            >
              Novo laudo
            </Button>
          </div>
        </header>

        {error && <p className={styles.error}>{error.message}</p>}

        <div className={styles.grid}>
          {list.map((laudo) => (
              <Card key={laudo.id} interactive onClick={() => onOpen(laudo)}>
                <div className={styles.card}>
                  <div
                    style={{ display: "flex", justifyContent: "space-between", gap: 8 }}
                  >
                    <h3 className={styles.cardTitle}>{laudo.title}</h3>
                    <div
                      style={{
                        display: "flex",
                        alignItems: "center",
                        gap: 6,
                      }}
                    >
                      {/* H — Badge de assinatura digital (gov.br/A1/A3/mock) */}
                      <SignatureBadge type={laudo.signature_type ?? null} />
                      <StatusPill status={mapLaudoStatus(laudo.status)} />
                      <button
                        type="button"
                        className={styles.deleteBtn}
                        title="Abrir no Word"
                        aria-label={`Abrir ${laudo.title} no Word`}
                        disabled={isMutating}
                        onClick={(e) => handleOpenExternal(e, laudo)}
                      >
                        <SquareArrowOutUpRight size={14} />
                      </button>
                      <button
                        type="button"
                        className={styles.deleteBtn}
                        title="Excluir laudo"
                        aria-label={`Excluir laudo ${laudo.title}`}
                        disabled={isMutating}
                        onClick={(e) => handleDeleteClick(e, laudo)}
                      >
                        <Trash2 size={14} />
                      </button>
                    </div>
                  </div>
                  <code className={styles.path}>{laudo.relative_path}</code>
                  <div className={styles.cardMeta}>
                    <span>template: {laudo.template_id}</span>
                    <span>atualizado {formatRelative(laudo.updated_at)}</span>
                  </div>
                </div>
              </Card>
            ))}
          </div>
      </div>

      <NewLaudoDialog
        open={dialogOpen}
        workspacePath={workspacePath}
        suggestedTitle={suggestedTitle}
        occurrence={occurrenceContext}
        onClose={() => setDialogOpen(false)}
        onCreated={() => {
          onCreate();
        }}
      />

      <ConfirmDialog
        open={pendingDelete !== null}
        title="Excluir laudo"
        destructive
        confirmLabel="Excluir laudo"
        cancelLabel="Cancelar"
        busy={isMutating}
        message={
          pendingDelete ? (
            <>
              Tem certeza que deseja excluir o laudo{" "}
              <strong>"{pendingDelete.title}"</strong>?
            </>
          ) : (
            ""
          )
        }
        detail="Esta ação não pode ser desfeita. O arquivo será removido do workspace e o registro será apagado."
        onCancel={() => {
          if (!isMutating) setPendingDelete(null);
        }}
        onConfirm={() => void handleConfirmDelete()}
      />
    </div>
  );
}

function toOccurrenceContext(o: Occurrence): OccurrenceContext {
  return {
    numero_bo: o.numero_bo,
    protocolo: o.protocolo,
    requisicao: o.requisicao,
    oficio: o.oficio,
    tipo_pericia: o.tipo_pericia,
    municipio: o.municipio,
    data_fato: o.data_fato,
    peritos: o.peritos,
  };
}

/**
 * Sugestão do nome do laudo no formato pedido pelo perito:
 *   {TIPO} - Laudo Nº {protocolo} - BO {numero_bo} - Ofício nº {oficio}
 * Omite os segmentos cujo dado não existe na ocorrência (ex.: sem ofício → some
 * o "Ofício nº"). Sem nenhum dado, cai no rótulo genérico com a data.
 */
function buildSuggestedTitle(occ: Occurrence | null): string {
  const clean = (v: string | null | undefined) => (v ?? "").trim() || null;
  const tipo = clean(occ?.tipo_pericia);
  const protocolo = clean(occ?.protocolo);
  const bo = clean(occ?.numero_bo);
  const oficio = clean(occ?.oficio);
  const parts = [
    tipo,
    protocolo ? `Laudo Nº ${protocolo}` : null,
    bo ? `BO ${bo}` : null,
    oficio ? `Ofício nº ${oficio}` : null,
  ].filter((p): p is string => !!p);
  if (parts.length) return parts.join(" - ");
  return `Laudo Pericial (${new Date().toLocaleDateString("pt-BR")})`;
}

/** Coerce a LaudoStatus into the StatusPill's OccurrenceStatus look-alike.
 *  The two status sets diverge — this is the lightweight visual mapping. */
function mapLaudoStatus(status: LaudoStatus): OccurrenceStatus {
  switch (status) {
    case "rascunho":
      return "aberta";
    case "revisado":
      return "em_andamento";
    case "exportado":
    case "assinado":
      return "concluida";
    case "arquivado":
      return "arquivada";
    default:
      return "aberta";
  }
}
