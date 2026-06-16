/**
 * VideoEventPanel — lista de eventos + formulário rápido para criar
 * um evento no timestamp atual.
 */

import { useState } from "react";
import { Check, Pencil, Plus, Target, Trash2, ImagePlus } from "lucide-react";
import type { VideoEvent } from "@domain/video";
import { formatDuration } from "./format";
import styles from "./VideoEventPanel.module.css";

const CATEGORIES = [
  "colisao",
  "frenagem",
  "impacto",
  "reacao",
  "semaforo",
  "mudanca_faixa",
  "outro",
] as const;

interface Props {
  events: VideoEvent[];
  currentTime: number;
  selectedEventId: string | null;
  onSelect: (id: string) => void;
  onCreate: (category: string, title: string) => Promise<void> | void;
  onUpdate: (
    id: string,
    patch: { title?: string; description?: string; category?: string; reviewed?: boolean },
  ) => Promise<void> | void;
  onDelete: (id: string) => Promise<void> | void;
  onAdjustToCurrent: (id: string) => Promise<void> | void;
  onCollectFrameForEvent: (id: string, title: string) => void;
}

export function VideoEventPanel({
  events,
  currentTime,
  selectedEventId,
  onSelect,
  onCreate,
  onUpdate,
  onDelete,
  onAdjustToCurrent,
  onCollectFrameForEvent,
}: Props) {
  const [newCategory, setNewCategory] = useState<string>("colisao");
  const [newTitle, setNewTitle] = useState<string>("");
  const [editingId, setEditingId] = useState<string | null>(null);
  const [editTitle, setEditTitle] = useState<string>("");

  const handleCreate = async () => {
    await onCreate(newCategory, newTitle);
    setNewTitle("");
  };

  return (
    <section className={styles.panel}>
      <header className={styles.header}>
        <h3 className={styles.title}>Eventos ({events.length})</h3>
        <span className={styles.dim}>
          em <code>{formatDuration(currentTime)}</code>
        </span>
      </header>

      <div className={styles.newForm}>
        <select
          value={newCategory}
          onChange={(e) => setNewCategory(e.target.value)}
        >
          {CATEGORIES.map((c) => (
            <option key={c} value={c}>
              {c}
            </option>
          ))}
        </select>
        <input
          type="text"
          placeholder="Título do evento"
          value={newTitle}
          onChange={(e) => setNewTitle(e.target.value)}
          onKeyDown={(e) => {
            if (e.key === "Enter") void handleCreate();
          }}
        />
        <button
          type="button"
          onClick={() => void handleCreate()}
          title="Criar evento no timestamp atual"
        >
          <Plus size={12} />
        </button>
      </div>

      <ul className={styles.list}>
        {events.map((ev) => (
          <li
            key={ev.id}
            className={`${styles.row} ${
              selectedEventId === ev.id ? styles.rowActive : ""
            }`}
            onClick={() => onSelect(ev.id)}
          >
            <div className={styles.rowMain}>
              <div className={styles.rowTop}>
                <span className={`${styles.chip} ${styles[`cat-${ev.category}`] ?? ""}`}>
                  {ev.category}
                </span>
                <code className={styles.ts}>{ev.timestamp_label}</code>
                {ev.reviewed && (
                  <span className={`${styles.chip} ${styles.chipOk}`}>revisado</span>
                )}
              </div>
              {editingId === ev.id ? (
                <input
                  type="text"
                  className={styles.editInput}
                  value={editTitle}
                  onChange={(e) => setEditTitle(e.target.value)}
                  autoFocus
                  onClick={(e) => e.stopPropagation()}
                  onBlur={() => {
                    void onUpdate(ev.id, { title: editTitle });
                    setEditingId(null);
                  }}
                  onKeyDown={(e) => {
                    if (e.key === "Enter") {
                      void onUpdate(ev.id, { title: editTitle });
                      setEditingId(null);
                    } else if (e.key === "Escape") {
                      setEditingId(null);
                    }
                  }}
                />
              ) : (
                <span className={styles.evTitle}>{ev.title}</span>
              )}
            </div>
            <div
              className={styles.rowActions}
              onClick={(e) => e.stopPropagation()}
            >
              <button
                type="button"
                title="Mover evento para o timestamp atual"
                onClick={() => void onAdjustToCurrent(ev.id)}
              >
                <Target size={12} />
              </button>
              <button
                type="button"
                title="Coletar frame deste evento"
                onClick={() => onCollectFrameForEvent(ev.id, ev.title)}
              >
                <ImagePlus size={12} />
              </button>
              <button
                type="button"
                title="Editar título"
                onClick={() => {
                  setEditingId(ev.id);
                  setEditTitle(ev.title);
                }}
              >
                <Pencil size={12} />
              </button>
              <button
                type="button"
                title={ev.reviewed ? "Marcar como pendente" : "Marcar como revisado"}
                onClick={() => void onUpdate(ev.id, { reviewed: !ev.reviewed })}
              >
                <Check size={12} />
              </button>
              <button
                type="button"
                className={styles.danger}
                title="Excluir evento"
                onClick={() => void onDelete(ev.id)}
              >
                <Trash2 size={12} />
              </button>
            </div>
          </li>
        ))}
      </ul>
    </section>
  );
}
