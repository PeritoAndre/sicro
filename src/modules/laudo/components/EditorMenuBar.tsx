/**
 * EditorMenuBar — barra fina abaixo do título com os botões de
 * configuração que antes eram abas do Inspector.
 *
 * F4.1 — O Inspector lateral agora foca em PROVAS (Estrutura +
 * Evidências). Validações, Estilos, Cabeçalho, Página e Dados
 * passam a ser popovers ancorados nesta barra superior.
 *
 * Cada botão:
 *   - Mostra ícone + label.
 *   - Abre um popover (`ToolbarPopover`) ancorado abaixo.
 *   - Apenas um popover aberto por vez (controlado por `openId`).
 *   - "Validações" exibe badge com contagem de warnings.
 *
 * A barra é compacta — não compete com o `EditorToolbar` de formatação
 * que vem logo abaixo.
 */

import { useMemo, useState } from "react";
import type { Editor } from "@tiptap/react";
import {
  AlertTriangle,
  Braces,
  Clock,
  Image as ImageIcon,
  LayoutTemplate,
  Library,
  ListTree,
  MessageSquare,
  Palette,
  ScrollText,
  ShieldCheck,
  Sparkles,
  Table as TableIcon,
} from "lucide-react";
import {
  countActiveComments,
  findMissingRequiredFields,
  LAUDO_FIELDS,
  validateSicroDoc,
  type SicroDoc,
} from "../document-engine";
import { useWorkspaceStore } from "@stores/workspaceStore";
import {
  HeaderPanel,
  MetaPanel,
  PagePanel,
  ValidationPanel,
} from "./Inspector";
import { StylesPanel } from "./StylesPanel";
import { FieldsPanel } from "./FieldsPanel";
import { FiguresPanel } from "./FiguresPanel";
import { TablePanel } from "./TablePanel";
import { CommentsPanel } from "./CommentsPanel";
import { VersionsPanel } from "./VersionsPanel";
import { StatusPanel } from "./StatusPanel";
import { BlocksPanel } from "./BlocksPanel";
import { SummaryPanel } from "./SummaryPanel";
import { ToolbarPopover } from "./ToolbarPopover";
import styles from "./EditorMenuBar.module.css";

type PopoverId =
  | "validation"
  | "styles"
  | "fields"
  | "figures"
  | "tables"
  | "blocks"
  | "summary"
  | "comments"
  | "versions"
  | "status"
  | "header"
  | "page"
  | "meta";

interface EditorMenuBarProps {
  doc: SicroDoc | null;
  editor: Editor | null;
  /** Editor da REGIÃO ATIVA (cabeçalho/rodapé/corpo). Usado SÓ pelo painel de
   *  Tabela, pra inserir/editar a tabela ONDE o cursor está (ex.: cabeçalho).
   *  Os demais painéis (documento: comentários, sumário, etc.) continuam no
   *  `editor` do corpo. */
  activeEditor?: Editor | null;
}

export function EditorMenuBar({ doc, editor, activeEditor }: EditorMenuBarProps) {
  const [openId, setOpenId] = useState<PopoverId | null>(null);
  const activeOccurrence = useWorkspaceStore((s) => s.activeOccurrence);

  const warnings = useMemo(
    () => (doc ? validateSicroDoc(doc) : []),
    [doc],
  );

  // F5 — Conta campos obrigatórios sem valor, para badge no botão "Campos".
  const missingFieldCount = useMemo(() => {
    const ctx = {
      metadata: doc?.metadata as Record<string, unknown> | undefined,
      occurrence: activeOccurrence as unknown as
        | Record<string, unknown>
        | undefined,
    };
    return findMissingRequiredFields(ctx, LAUDO_FIELDS).length;
  }, [doc?.metadata, activeOccurrence]);

  // F8 — Comentários ativos no badge do botão "Comentários".
  const activeCommentsCount = useMemo(
    () => countActiveComments(doc?.comments),
    [doc?.comments],
  );

  // Helper: garante que abrir um popover fecha o anterior.
  const handleOpen = (id: PopoverId) => (next: boolean) => {
    setOpenId(next ? id : openId === id ? null : openId);
  };

  return (
    <div
      className={styles.bar}
      role="menubar"
      aria-label="Configurações do laudo"
    >
      <ToolbarPopover
        label="Validações do laudo"
        open={openId === "validation"}
        onOpenChange={handleOpen("validation")}
        badge={warnings.length > 0 ? warnings.length : undefined}
        align="left"
        width={340}
        trigger={
          <>
            <AlertTriangle size={14} />
            <span className={styles.label}>Validações</span>
          </>
        }
      >
        <ValidationPanel warnings={warnings} hasDoc={!!doc} />
      </ToolbarPopover>

      <ToolbarPopover
        label="Estilos documentais"
        open={openId === "styles"}
        onOpenChange={handleOpen("styles")}
        align="left"
        width={360}
        trigger={
          <>
            <Palette size={14} />
            <span className={styles.label}>Estilos</span>
          </>
        }
      >
        <StylesPanel editor={editor} />
      </ToolbarPopover>

      <ToolbarPopover
        label="Campos automáticos do laudo"
        open={openId === "fields"}
        onOpenChange={handleOpen("fields")}
        align="left"
        width={380}
        badge={missingFieldCount > 0 ? missingFieldCount : undefined}
        trigger={
          <>
            <Braces size={14} />
            <span className={styles.label}>Campos</span>
          </>
        }
      >
        <FieldsPanel editor={editor} doc={doc} />
      </ToolbarPopover>

      <ToolbarPopover
        label="Figuras e pranchas fotográficas"
        open={openId === "figures"}
        onOpenChange={handleOpen("figures")}
        align="left"
        width={400}
        trigger={
          <>
            <ImageIcon size={14} />
            <span className={styles.label}>Figuras</span>
          </>
        }
      >
        <FiguresPanel editor={editor} />
      </ToolbarPopover>

      <ToolbarPopover
        label="Tabelas — inserir, editar, listar"
        open={openId === "tables"}
        onOpenChange={handleOpen("tables")}
        align="left"
        width={380}
        trigger={
          <>
            <TableIcon size={14} />
            <span className={styles.label}>Tabela</span>
          </>
        }
      >
        <TablePanel editor={activeEditor ?? editor} />
      </ToolbarPopover>

      <ToolbarPopover
        label="Biblioteca de blocos reutilizáveis"
        open={openId === "blocks"}
        onOpenChange={handleOpen("blocks")}
        align="left"
        width={380}
        trigger={
          <>
            <Library size={14} />
            <span className={styles.label}>Blocos</span>
          </>
        }
      >
        <BlocksPanel editor={editor} />
      </ToolbarPopover>

      <ToolbarPopover
        label="Sumário, lista de figuras e lista de tabelas"
        open={openId === "summary"}
        onOpenChange={handleOpen("summary")}
        align="left"
        width={360}
        trigger={
          <>
            <ListTree size={14} />
            <span className={styles.label}>Sumário</span>
          </>
        }
      >
        <SummaryPanel editor={editor} doc={doc} />
      </ToolbarPopover>

      <ToolbarPopover
        label="Comentários do laudo"
        open={openId === "comments"}
        onOpenChange={handleOpen("comments")}
        align="left"
        width={400}
        badge={activeCommentsCount > 0 ? activeCommentsCount : undefined}
        trigger={
          <>
            <MessageSquare size={14} />
            <span className={styles.label}>Comentários</span>
          </>
        }
      >
        <CommentsPanel editor={editor} doc={doc} />
      </ToolbarPopover>

      <ToolbarPopover
        label="Histórico de versões"
        open={openId === "versions"}
        onOpenChange={handleOpen("versions")}
        align="left"
        width={380}
        trigger={
          <>
            <Clock size={14} />
            <span className={styles.label}>Histórico</span>
          </>
        }
      >
        <VersionsPanel editor={editor} doc={doc} />
      </ToolbarPopover>

      <ToolbarPopover
        label="Status & Finalização"
        open={openId === "status"}
        onOpenChange={handleOpen("status")}
        align="left"
        width={360}
        badge={
          doc?.status === "final"
            ? "✓"
            : doc?.status === "em_revisao"
              ? "R"
              : undefined
        }
        trigger={
          <>
            <ShieldCheck size={14} />
            <span className={styles.label}>Status</span>
          </>
        }
      >
        <StatusPanel doc={doc} />
      </ToolbarPopover>

      <ToolbarPopover
        label="Cabeçalho institucional"
        open={openId === "header"}
        onOpenChange={handleOpen("header")}
        align="left"
        width={340}
        trigger={
          <>
            <ScrollText size={14} />
            <span className={styles.label}>Cabeçalho</span>
          </>
        }
      >
        <HeaderPanel doc={doc} />
      </ToolbarPopover>

      <ToolbarPopover
        label="Configurações de página"
        open={openId === "page"}
        onOpenChange={handleOpen("page")}
        align="left"
        width={380}
        trigger={
          <>
            <LayoutTemplate size={14} />
            <span className={styles.label}>Página</span>
          </>
        }
      >
        <PagePanel doc={doc} />
      </ToolbarPopover>

      <ToolbarPopover
        label="Metadados do documento"
        open={openId === "meta"}
        onOpenChange={handleOpen("meta")}
        align="left"
        width={340}
        trigger={
          <>
            <Sparkles size={14} />
            <span className={styles.label}>Dados</span>
          </>
        }
      >
        <MetaPanel doc={doc} />
      </ToolbarPopover>
    </div>
  );
}
