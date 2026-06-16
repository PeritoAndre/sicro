/**
 * TablePanel — popover "Tabela" do menu superior.
 *
 * F7 — 3 seções:
 *
 *   1. **Lista de tabelas** já presentes no documento, com numeração
 *      automática + click para navegar.
 *   2. **Inserir tabela-modelo pericial** — galeria de templates +
 *      botão "Criar tabela personalizada" (N×M custom via dialog).
 *   3. **Edição contextual** — quando o cursor está dentro de uma
 *      tabela, mostramos botões para inserir/remover linhas/colunas,
 *      mesclar/separar células, alternar header, propriedades e excluir.
 *
 * F7.1 — Substituído o template "Tabela 3×3 em branco" pelo botão
 * "Criar nova tabela…" que abre o `InsertTableDialog` (N×M custom).
 * Botão "Propriedades…" na toolbar contextual abre o
 * `TablePropertiesDialog` (4 abas: Tabela/Linha/Coluna/Célula).
 *
 * O painel reage ao `selectionUpdate` para alternar entre (2) e (3).
 */

import type { Editor } from "@tiptap/react";
import { useEffect, useMemo, useState } from "react";
import {
  ArrowDown,
  ArrowDownToLine,
  ArrowRight,
  ArrowRightToLine,
  ArrowUp,
  ArrowUpToLine,
  Columns2,
  CornerDownRight,
  Merge,
  Plus,
  Rows2,
  Settings2,
  Split,
  Table as TableIcon,
  Trash2,
  XCircle,
} from "lucide-react";
import {
  buildSeededTableJson,
  buildTableList,
  extractTables,
  findTableTemplate,
  generateTableId,
  TABLE_TEMPLATES,
  type NumberedTableEntry,
} from "../document-engine";
import { InsertTableDialog } from "./InsertTableDialog";
import { TablePropertiesDialog } from "./TablePropertiesDialog";
import styles from "./TablePanel.module.css";

interface TablePanelProps {
  editor: Editor | null;
}

export function TablePanel({ editor }: TablePanelProps) {
  const [tables, setTables] = useState<NumberedTableEntry[]>([]);
  const [inTable, setInTable] = useState(false);
  const [insertOpen, setInsertOpen] = useState(false);
  const [propsOpen, setPropsOpen] = useState(false);

  // Recompute table list + check if cursor is inside a table.
  useEffect(() => {
    if (!editor) {
      setTables([]);
      setInTable(false);
      return undefined;
    }
    const refresh = () => {
      setTables(buildTableList(extractTables(editor.getJSON())));
      setInTable(editor.isActive("table"));
    };
    refresh();
    editor.on("update", refresh);
    editor.on("selectionUpdate", refresh);
    return () => {
      editor.off("update", refresh);
      editor.off("selectionUpdate", refresh);
    };
  }, [editor]);

  const insertTemplate = (id: string) => {
    if (!editor) return;
    const def = findTableTemplate(id);
    if (!def) return;
    editor.chain().focus().insertContent(def.build()).run();
  };

  const handleInsertCustom = ({
    rows,
    cols,
    withHeaderRow,
  }: {
    rows: number;
    cols: number;
    withHeaderRow: boolean;
  }) => {
    if (!editor) {
      setInsertOpen(false);
      return;
    }
    // F1.2 — Em vez do `insertTable` nativo (que cria células com
    // colwidth=null → colapsam com table-layout:fixed), inserimos JSON com
    // colwidths semeados + id estável (cross-refs/auto-numeração).
    const table = buildSeededTableJson({ rows, cols, withHeaderRow });
    table.attrs = { ...(table.attrs ?? {}), id: generateTableId() };
    editor.chain().focus().insertContent(table).run();
    setInsertOpen(false);
  };

  const handleJump = (pos: number) => {
    if (!editor) return;
    editor.commands.focus();
    editor.commands.setTextSelection(pos + 1);
    editor.commands.scrollIntoView();
  };

  if (!editor) {
    return (
      <p className={styles.empty}>
        Abra um laudo para inserir e editar tabelas.
      </p>
    );
  }

  return (
    <>
      <h3 className={styles.sectionTitle}>
        <TableIcon size={14} /> Tabelas
      </h3>

      {/* Edição contextual — só aparece quando o cursor está dentro de tabela. */}
      {inTable && (
        <ContextualToolbar
          editor={editor}
          onOpenProperties={() => setPropsOpen(true)}
        />
      )}

      {/* F7.1 — Botão principal para criar tabela personalizada */}
      <div className={styles.section}>
        <div className={styles.sectionLabel}>Nova tabela</div>
        <button
          type="button"
          className={styles.primaryAction}
          onClick={() => setInsertOpen(true)}
        >
          <Plus size={14} /> Criar tabela (N×M)…
        </button>
        <p className={styles.intro}>
          Define linhas, colunas e se a primeira linha é cabeçalho.
        </p>
      </div>

      {/* Inserir templates pré-formatados */}
      <div className={styles.section}>
        <div className={styles.sectionLabel}>
          Modelos periciais ({TABLE_TEMPLATES.length})
        </div>
        <p className={styles.intro}>
          Click para inserir um modelo pré-formatado.
        </p>
        <div className={styles.templateGrid}>
          {TABLE_TEMPLATES.map((t) => (
            <button
              key={t.id}
              type="button"
              className={styles.templateBtn}
              onClick={() => insertTemplate(t.id)}
              title={t.description}
            >
              <span className={styles.templateLabel}>{t.label}</span>
              <span className={styles.templateDesc}>{t.description}</span>
            </button>
          ))}
        </div>
      </div>

      {/* Lista de tabelas */}
      {tables.length > 0 && (
        <div className={styles.section}>
          <div className={styles.sectionLabel}>
            Tabelas no documento ({tables.length})
          </div>
          <div className={styles.list}>
            {tables.map((t) => (
              <button
                key={t.pos}
                type="button"
                className={styles.item}
                onClick={() => handleJump(t.pos)}
              >
                <div className={styles.itemHeader}>
                  <span className={styles.itemLabel}>{t.label}</span>
                  <span className={styles.itemDims}>
                    {t.rowCount}×{t.colCount}
                  </span>
                </div>
                <div className={styles.itemFirstCell}>
                  <CornerDownRight size={11} />
                  {t.firstCell ? (
                    <span>{t.firstCell}</span>
                  ) : (
                    <span className={styles.itemEmpty}>(sem título)</span>
                  )}
                </div>
              </button>
            ))}
          </div>
        </div>
      )}

      {/* F7.1 — Dialog de inserção N×M */}
      <InsertTableDialog
        open={insertOpen}
        onCancel={() => setInsertOpen(false)}
        onConfirm={handleInsertCustom}
      />

      {/* F7.1 — Dialog de propriedades estilo Word */}
      <TablePropertiesDialog
        open={propsOpen}
        editor={editor}
        onClose={() => setPropsOpen(false)}
      />
    </>
  );
}

/**
 * Toolbar contextual de edição de tabela — só visível quando
 * `editor.isActive("table")`.
 */
function ContextualToolbar({
  editor,
  onOpenProperties,
}: {
  editor: Editor;
  onOpenProperties: () => void;
}) {
  const canMerge = editor.can().mergeCells();
  const canSplit = editor.can().splitCell();

  return (
    <div className={styles.contextual}>
      <div className={styles.contextualTitle}>
        Você está dentro de uma tabela
      </div>

      <div className={styles.contextualGroup}>
        <div className={styles.contextualGroupLabel}>
          <Rows2 size={11} /> Linhas
        </div>
        <ContextualBtn
          onClick={() => editor.chain().focus().addRowBefore().run()}
          icon={<ArrowUpToLine size={12} />}
          label="Inserir linha acima"
        />
        <ContextualBtn
          onClick={() => editor.chain().focus().addRowAfter().run()}
          icon={<ArrowDownToLine size={12} />}
          label="Inserir linha abaixo"
        />
        <ContextualBtn
          onClick={() => editor.chain().focus().deleteRow().run()}
          icon={<Trash2 size={12} />}
          label="Remover linha"
        />
      </div>

      <div className={styles.contextualGroup}>
        <div className={styles.contextualGroupLabel}>
          <Columns2 size={11} /> Colunas
        </div>
        <ContextualBtn
          onClick={() => editor.chain().focus().addColumnBefore().run()}
          icon={<ArrowUp size={12} />}
          label="Inserir coluna à esquerda"
        />
        <ContextualBtn
          onClick={() => editor.chain().focus().addColumnAfter().run()}
          icon={<ArrowDown size={12} />}
          label="Inserir coluna à direita"
        />
        <ContextualBtn
          onClick={() => editor.chain().focus().deleteColumn().run()}
          icon={<Trash2 size={12} />}
          label="Remover coluna"
        />
      </div>

      <div className={styles.contextualGroup}>
        <div className={styles.contextualGroupLabel}>Células</div>
        <ContextualBtn
          onClick={() => editor.chain().focus().mergeCells().run()}
          icon={<Merge size={12} />}
          label="Mesclar células selecionadas"
          disabled={!canMerge}
        />
        <ContextualBtn
          onClick={() => editor.chain().focus().splitCell().run()}
          icon={<Split size={12} />}
          label="Dividir célula"
          disabled={!canSplit}
        />
        <ContextualBtn
          onClick={() => editor.chain().focus().toggleHeaderRow().run()}
          icon={<ArrowRightToLine size={12} />}
          label="Alternar cabeçalho na linha"
        />
        <ContextualBtn
          onClick={() => editor.chain().focus().toggleHeaderColumn().run()}
          icon={<ArrowRight size={12} />}
          label="Alternar cabeçalho na coluna"
        />
      </div>

      <div className={styles.contextualActions}>
        <button
          type="button"
          className={styles.propsBtn}
          onClick={onOpenProperties}
          title="Abrir Propriedades da Tabela"
        >
          <Settings2 size={12} /> Propriedades…
        </button>
        <button
          type="button"
          className={styles.deleteTableBtn}
          onClick={() => editor.chain().focus().deleteTable().run()}
          title="Remover tabela inteira"
        >
          <XCircle size={12} /> Remover tabela
        </button>
      </div>
    </div>
  );
}

function ContextualBtn({
  onClick,
  icon,
  label,
  disabled,
}: {
  onClick: () => void;
  icon: React.ReactNode;
  label: string;
  disabled?: boolean;
}) {
  return (
    <button
      type="button"
      className={styles.contextualBtn}
      onClick={onClick}
      disabled={disabled}
      title={label}
    >
      {icon}
      <span>{label}</span>
    </button>
  );
}

// Suprime warning unused
void useMemo;
