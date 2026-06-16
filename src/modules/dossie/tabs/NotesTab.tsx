/**
 * NotesTab — observações de campo. Cards rolando verticalmente, com
 * prioridade colorida e timestamps lado a lado.
 */

import { StickyNote } from "lucide-react";
import { commands } from "@core/commands";
import { formatDateTime } from "@core/formatters";
import shared from "./shared.module.css";
import { useDossieList } from "./useDossieList";

export function NotesTab({ workspacePath }: { workspacePath: string }) {
  const { items, loading, error } = useDossieList(workspacePath, commands.listDossieNotes);

  if (loading && items.length === 0) return <p className={shared.dim}>Carregando observações…</p>;
  if (error) return <p className={shared.error}>{error}</p>;
  if (items.length === 0) {
    return (
      <div className={shared.empty}>
        <StickyNote size={28} aria-hidden />
        <span>O pacote não trouxe observações.</span>
      </div>
    );
  }

  return (
    <div className={shared.tab}>
      {items.map((n) => (
        <article key={n.id} className={shared.card} style={{ gap: "var(--space-2)" }}>
          <header className={shared.cardHeader} style={{ gap: "var(--space-2)" }}>
            <div style={{ display: "inline-flex", gap: 6, alignItems: "center" }}>
              {n.category && <span className={shared.chip}>{n.category}</span>}
              <PriorityChip priority={n.priority} />
            </div>
            <span className={shared.dim} style={{ fontSize: "var(--text-xs)" }}>
              {n.note_created_at ? formatDateTime(n.note_created_at) : null}
              {n.note_updated_at &&
                n.note_updated_at !== n.note_created_at &&
                ` · ed. ${formatDateTime(n.note_updated_at)}`}
            </span>
          </header>
          <p style={{ margin: 0, fontSize: "var(--text-sm)", whiteSpace: "pre-wrap", lineHeight: 1.5 }}>
            {n.text ?? <span className={shared.dim}>(sem texto)</span>}
          </p>
        </article>
      ))}
    </div>
  );
}

function PriorityChip({ priority }: { priority: string | null }) {
  if (!priority) return null;
  const cls =
    priority === "critica"
      ? shared.chipBad
      : priority === "importante"
        ? shared.chipWarn
        : shared.chipMuted;
  return <span className={`${shared.chip} ${cls}`}>{priority}</span>;
}
