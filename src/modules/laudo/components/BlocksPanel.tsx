/**
 * BlocksPanel — biblioteca de blocos de texto reutilizáveis (F10).
 *
 * Sections:
 *   - Filtro por categoria (chips horizontais).
 *   - Lista de blocos: built-in + custom (localStorage).
 *   - Botão "Inserir" em cada item.
 *   - Botão "Salvar seleção como bloco" no rodapé.
 */

import { useMemo, useState } from "react";
import type { Editor } from "@tiptap/react";
import { Library, Plus, Trash2 } from "lucide-react";
import {
  BLOCK_CATEGORIES,
  deleteCustomBlock,
  listAllBlocks,
  saveCustomBlock,
  type BlockCategory,
} from "../document-engine";
import styles from "./BlocksPanel.module.css";

interface BlocksPanelProps {
  editor: Editor | null;
}

export function BlocksPanel({ editor }: BlocksPanelProps) {
  const [activeCat, setActiveCat] = useState<BlockCategory | "todos">("todos");
  // Trigger refresh quando custom blocks mudam (localStorage).
  const [version, setVersion] = useState(0);
  const blocks = useMemo(
    () => listAllBlocks(),
    // eslint-disable-next-line react-hooks/exhaustive-deps
    [version],
  );

  const [draftLabel, setDraftLabel] = useState("");
  const [draftCat, setDraftCat] = useState<BlockCategory>("abertura");
  const [feedback, setFeedback] = useState<string | null>(null);

  const filtered =
    activeCat === "todos"
      ? blocks
      : blocks.filter((b) => b.category === activeCat);

  const handleInsert = (id: string) => {
    if (!editor) return;
    const block = blocks.find((b) => b.id === id);
    if (!block) return;
    const content = block.build();
    editor.chain().focus().insertContent(content).run();
    setFeedback(`Inserido "${block.label}"`);
    setTimeout(() => setFeedback(null), 2500);
  };

  const handleSaveSelection = () => {
    if (!editor) return;
    const { from, to } = editor.state.selection;
    if (from === to) {
      setFeedback("Selecione um trecho do documento antes de salvar.");
      return;
    }
    if (!draftLabel.trim()) {
      setFeedback("Dê um nome ao bloco antes de salvar.");
      return;
    }
    const slice = editor.state.doc.slice(from, to);
    // Converte o ProseMirror Slice para JSONContent.
    const json = (slice.content as unknown as { toJSON: () => unknown }).toJSON() as { content?: unknown[] };
    const content = Array.isArray(json.content)
      ? (json.content as Array<Record<string, unknown>>)
      : [json as unknown as Record<string, unknown>];
    saveCustomBlock({
      label: draftLabel.trim(),
      category: draftCat,
      content: content as never,
    });
    setDraftLabel("");
    setFeedback(`Bloco "${draftLabel.trim()}" salvo.`);
    setVersion((v) => v + 1);
    setTimeout(() => setFeedback(null), 2500);
  };

  const handleDelete = (id: string) => {
    if (!window.confirm("Excluir este bloco personalizado?")) return;
    deleteCustomBlock(id);
    setVersion((v) => v + 1);
  };

  if (!editor) {
    return (
      <p className={styles.empty}>Abra um laudo para acessar a biblioteca.</p>
    );
  }

  return (
    <>
      <h3 className={styles.sectionTitle}>
        <Library size={14} /> Biblioteca de blocos
      </h3>

      <div className={styles.filterRow}>
        <button
          type="button"
          className={`${styles.chip} ${
            activeCat === "todos" ? styles.chipActive : ""
          }`}
          onClick={() => setActiveCat("todos")}
        >
          Todos ({blocks.length})
        </button>
        {BLOCK_CATEGORIES.map((c) => {
          const count = blocks.filter((b) => b.category === c.id).length;
          return (
            <button
              key={c.id}
              type="button"
              className={`${styles.chip} ${
                activeCat === c.id ? styles.chipActive : ""
              }`}
              onClick={() => setActiveCat(c.id)}
            >
              {c.label} ({count})
            </button>
          );
        })}
      </div>

      {feedback && <div className={styles.feedback}>{feedback}</div>}

      {filtered.length === 0 ? (
        <p className={styles.empty}>Nenhum bloco nesta categoria.</p>
      ) : (
        <div className={styles.list}>
          {filtered.map((b) => (
            <div key={b.id} className={styles.item}>
              <div className={styles.itemHeader}>
                <strong>{b.label}</strong>
                {b.custom && <span className={styles.customBadge}>Personalizado</span>}
              </div>
              {b.description && (
                <p className={styles.itemDesc}>{b.description}</p>
              )}
              <div className={styles.itemActions}>
                <button
                  type="button"
                  className={styles.insertBtn}
                  onClick={() => handleInsert(b.id)}
                >
                  Inserir
                </button>
                {b.custom && (
                  <button
                    type="button"
                    className={styles.deleteBtn}
                    onClick={() => handleDelete(b.id)}
                    title="Excluir bloco"
                  >
                    <Trash2 size={11} />
                  </button>
                )}
              </div>
            </div>
          ))}
        </div>
      )}

      <div className={styles.saveBox}>
        <div className={styles.sectionLabel}>Salvar seleção como bloco</div>
        <input
          className={styles.input}
          placeholder="Nome do bloco"
          value={draftLabel}
          onChange={(e) => setDraftLabel(e.target.value)}
        />
        <select
          className={styles.select}
          value={draftCat}
          onChange={(e) => setDraftCat(e.target.value as BlockCategory)}
        >
          {BLOCK_CATEGORIES.map((c) => (
            <option key={c.id} value={c.id}>
              {c.label}
            </option>
          ))}
        </select>
        <button
          type="button"
          className={styles.saveBtn}
          onClick={handleSaveSelection}
          disabled={!draftLabel.trim()}
        >
          <Plus size={11} /> Salvar bloco
        </button>
      </div>
    </>
  );
}
