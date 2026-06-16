/**
 * VersionsPanel — popover de snapshots (F8).
 *
 * Lista as snapshots do laudo (buffer rolling de 20). Cada item:
 *   - autor + label opcional + timestamp;
 *   - stats (palavras / parágrafos);
 *   - botão "Restaurar" (substitui o conteúdo atual pelo da snapshot);
 *   - botão "Excluir" (remove da lista).
 *
 * Botão grande no topo: "Criar snapshot" — captura o conteúdo atual com
 * um label opcional. Atalhar para "checkpoint pré-revisão" etc.
 */

import { useMemo, useState } from "react";
import type { Editor } from "@tiptap/react";
import {
  Camera,
  Check,
  Clock,
  Minus,
  Plus,
  RotateCcw,
  Trash2,
  X,
} from "lucide-react";
import {
  createSnapshot,
  deleteSnapshot as deleteSnapshotSvc,
  pushSnapshot,
  type SicroDoc,
} from "../document-engine";
import { useLaudoStore } from "../store/laudoStore";
import { useWorkspaceStore } from "@stores/workspaceStore";
import styles from "./VersionsPanel.module.css";

interface VersionsPanelProps {
  editor: Editor | null;
  doc: SicroDoc | null;
}

export function VersionsPanel({ editor, doc }: VersionsPanelProps) {
  const activeWorkspacePath = useWorkspaceStore((s) => s.activeWorkspacePath);
  const setSnapshots = useLaudoStore((s) => s.setSnapshots);
  const saveCurrent = useLaudoStore((s) => s.saveCurrent);

  const [label, setLabel] = useState("");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const snapshots = doc?.snapshots ?? [];

  const handleCreate = async () => {
    if (!editor || !activeWorkspacePath || !doc) return;
    setBusy(true);
    setError(null);
    try {
      const snap = createSnapshot({
        author: "Perito",
        label: label.trim() || undefined,
        content: editor.getJSON(),
      });
      const next = pushSnapshot(snapshots, snap);
      await setSnapshots(activeWorkspacePath, next);
      setLabel("");
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
    } finally {
      setBusy(false);
    }
  };

  const handleRestore = async (id: string) => {
    if (!editor || !activeWorkspacePath || !doc) return;
    const snap = snapshots.find((s) => s.id === id);
    if (!snap) return;
    if (
      !window.confirm(
        `Restaurar para a snapshot "${snap.label ?? short(snap.created_at)}"? Isso substitui todo o conteúdo atual (uma nova snapshot pode ser criada antes).`,
      )
    ) {
      return;
    }
    setBusy(true);
    setError(null);
    try {
      // Antes de restaurar, criamos uma snapshot do estado ATUAL.
      const beforeRestore = createSnapshot({
        author: "Perito",
        label: "Antes de restaurar",
        content: editor.getJSON(),
      });
      const withBackup = pushSnapshot(snapshots, beforeRestore);
      // Aplica o conteúdo da snapshot no editor.
      editor.commands.setContent(snap.content, { emitUpdate: true });
      // Persiste o novo conteúdo + a lista atualizada de snapshots.
      await setSnapshots(activeWorkspacePath, withBackup);
      await saveCurrent(activeWorkspacePath, editor.getJSON());
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
    } finally {
      setBusy(false);
    }
  };

  const handleDelete = async (id: string) => {
    if (!activeWorkspacePath) return;
    if (!window.confirm("Excluir esta snapshot? A operação é irreversível.")) {
      return;
    }
    setBusy(true);
    try {
      const next = deleteSnapshotSvc(snapshots, id);
      await setSnapshots(activeWorkspacePath, next);
    } finally {
      setBusy(false);
    }
  };

  if (!editor) {
    return (
      <p className={styles.empty}>
        Abra um laudo para criar snapshots históricos.
      </p>
    );
  }

  return (
    <>
      <h3 className={styles.sectionTitle}>
        <Clock size={14} /> Histórico & Revisões
      </h3>

      {/* F8 — Track-changes lite controls */}
      <RevisionsSection editor={editor} />

      <div className={styles.sectionLabel}>Snapshots</div>
      <div className={styles.createBox}>
        <input
          type="text"
          className={styles.input}
          placeholder="Rótulo (opcional, ex: pré-revisão)"
          value={label}
          onChange={(e) => setLabel(e.target.value)}
          disabled={busy}
        />
        <button
          type="button"
          className={styles.primaryBtn}
          onClick={handleCreate}
          disabled={busy}
        >
          <Camera size={12} /> Criar snapshot
        </button>
      </div>

      <p className={styles.hint}>
        Mantemos as últimas 20 snapshots. Restaurar substitui o conteúdo
        do laudo pelo da snapshot — uma snapshot de backup é feita
        automaticamente antes.
      </p>

      {error && <div className={styles.errorBanner}>{error}</div>}

      {snapshots.length === 0 ? (
        <p className={styles.empty}>
          Nenhuma snapshot ainda. Crie a primeira clicando acima.
        </p>
      ) : (
        <ul className={styles.list}>
          {snapshots.map((s) => (
            <li key={s.id} className={styles.item}>
              <div className={styles.itemHeader}>
                <strong>{s.label ?? "(sem rótulo)"}</strong>
                <span className={styles.itemTime}>
                  {shortDate(s.created_at)}
                </span>
              </div>
              <div className={styles.itemMeta}>
                <span>{s.author}</span>
                {s.stats && (
                  <>
                    <span>·</span>
                    <span>
                      {s.stats.words} palavras · {s.stats.paragraphs}{" "}
                      parágrafos
                    </span>
                  </>
                )}
              </div>
              <div className={styles.itemActions}>
                <button
                  type="button"
                  className={styles.smallBtn}
                  onClick={() => handleRestore(s.id)}
                  disabled={busy}
                  title="Restaurar este snapshot"
                >
                  <RotateCcw size={11} /> Restaurar
                </button>
                <button
                  type="button"
                  className={`${styles.smallBtn} ${styles.smallBtnDanger}`}
                  onClick={() => handleDelete(s.id)}
                  disabled={busy}
                  title="Excluir snapshot"
                >
                  <Trash2 size={11} />
                </button>
              </div>
            </li>
          ))}
        </ul>
      )}
    </>
  );
}

/**
 * F8 — Subseção "Marcações de revisão (track-changes lite)".
 *
 * Botões para o revisor marcar trechos como `inserção` ou `remoção`.
 * Aceitar/rejeitar todos os marks pendentes.
 */
function RevisionsSection({ editor }: { editor: Editor }) {
  const [pending, setPending] = useState<
    Array<{ id: string; type: "insertion" | "deletion"; excerpt: string }>
  >([]);

  // Recompute em cada update do editor.
  const _ = useMemo(() => {
    const refresh = () => {
      const json = editor.getJSON();
      const list: typeof pending = [];
      const visit = (n: { type?: string; text?: string; marks?: Array<{ type: string; attrs?: Record<string, unknown> }>; content?: unknown[] }) => {
        if (n.type === "text" && Array.isArray(n.marks)) {
          const m = n.marks.find((mk) => mk.type === "revisionMark");
          if (m && typeof m.attrs?.["id"] === "string") {
            const id = m.attrs["id"] as string;
            const type =
              m.attrs["type"] === "deletion" ? "deletion" : "insertion";
            const text = typeof n.text === "string" ? n.text : "";
            const existing = list.find((x) => x.id === id);
            if (existing) {
              existing.excerpt = (existing.excerpt + text).slice(0, 60);
            } else {
              list.push({ id, type, excerpt: text.slice(0, 60) });
            }
          }
        }
        if (Array.isArray(n.content))
          for (const c of n.content as Array<typeof n>) visit(c);
      };
      visit(json as Parameters<typeof visit>[0]);
      setPending(list);
    };
    refresh();
    editor.on("update", refresh);
    return () => editor.off("update", refresh);
  }, [editor]);
  void _;

  const markInsertion = () => editor.chain().focus().addRevisionInsertion().run();
  const markDeletion = () => editor.chain().focus().addRevisionDeletion().run();
  const accept = (id: string) => editor.chain().focus().acceptRevision(id).run();
  const reject = (id: string) => editor.chain().focus().rejectRevision(id).run();
  const acceptAll = () => {
    pending.forEach((p) =>
      editor.chain().focus().acceptRevision(p.id).run(),
    );
  };
  const rejectAll = () => {
    pending.forEach((p) =>
      editor.chain().focus().rejectRevision(p.id).run(),
    );
  };

  return (
    <div className={styles.revisionsBox}>
      <div className={styles.sectionLabel}>Revisão (track-changes)</div>
      <div className={styles.revisionActions}>
        <button
          type="button"
          className={styles.revisionBtnInsertion}
          onClick={markInsertion}
          title="Marcar a seleção como INSERÇÃO (verde)"
        >
          <Plus size={11} /> Inserção
        </button>
        <button
          type="button"
          className={styles.revisionBtnDeletion}
          onClick={markDeletion}
          title="Marcar a seleção como REMOÇÃO (vermelho)"
        >
          <Minus size={11} /> Remoção
        </button>
      </div>
      {pending.length > 0 ? (
        <>
          <div className={styles.pendingList}>
            {pending.map((p) => (
              <div
                key={p.id}
                className={`${styles.pendingItem} ${
                  p.type === "insertion"
                    ? styles.pendingInsertion
                    : styles.pendingDeletion
                }`}
              >
                <span className={styles.pendingExcerpt}>
                  {p.type === "insertion" ? "+ " : "− "}"{p.excerpt}"
                </span>
                <div className={styles.pendingActions}>
                  <button
                    type="button"
                    title="Aceitar"
                    onClick={() => accept(p.id)}
                  >
                    <Check size={10} />
                  </button>
                  <button
                    type="button"
                    title="Rejeitar"
                    onClick={() => reject(p.id)}
                  >
                    <X size={10} />
                  </button>
                </div>
              </div>
            ))}
          </div>
          <div className={styles.bulkActions}>
            <button type="button" onClick={acceptAll}>
              Aceitar todas
            </button>
            <button type="button" onClick={rejectAll}>
              Rejeitar todas
            </button>
          </div>
        </>
      ) : (
        <p className={styles.hint}>
          Selecione um trecho e clique em "Inserção" ou "Remoção" para
          marcá-lo como mudança de revisão.
        </p>
      )}
    </div>
  );
}

function short(iso: string): string {
  return iso.slice(11, 16);
}

function shortDate(iso: string): string {
  try {
    const d = new Date(iso);
    return d.toLocaleString("pt-BR", {
      day: "2-digit",
      month: "2-digit",
      year: "2-digit",
      hour: "2-digit",
      minute: "2-digit",
    });
  } catch {
    return iso;
  }
}
