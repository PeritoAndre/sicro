/**
 * CommentsPanel — popover de comentários (F8).
 *
 * Sections:
 *   1. Botão "Comentar seleção" — visível quando há texto selecionado;
 *      cria um novo comment + aplica `commentMark` na seleção.
 *   2. Lista de comentários — ativos primeiro, resolvidos depois.
 *   3. Filtros: todos / ativos / resolvidos.
 *   4. Cada item: corpo + autor + timestamp + ações (resolver, excluir,
 *      saltar para o anchor).
 *
 * Limitações desta versão:
 *   - Sem editor rich-text no corpo (apenas textarea simples).
 *   - Sem threading visual aprofundado (replies mostradas em lista plana
 *     abaixo do comment principal).
 */

import { useMemo, useState } from "react";
import type { Editor } from "@tiptap/react";
import {
  Check,
  CornerDownRight,
  MessageSquare,
  MessageSquarePlus,
  RotateCcw,
  Trash2,
} from "lucide-react";
import {
  addComment as addCommentSvc,
  addReply as addReplySvc,
  countActiveComments,
  createComment,
  deleteComment as deleteCommentSvc,
  extractCommentAnchors,
  resolveComment as resolveCommentSvc,
  unresolveComment as unresolveCommentSvc,
  type SicroDoc,
  type SicroDocComment,
} from "../document-engine";
import { useLaudoStore } from "../store/laudoStore";
import { useWorkspaceStore } from "@stores/workspaceStore";
import styles from "./CommentsPanel.module.css";

interface CommentsPanelProps {
  editor: Editor | null;
  doc: SicroDoc | null;
}

type Filter = "ativos" | "resolvidos" | "todos";

export function CommentsPanel({ editor, doc }: CommentsPanelProps) {
  const activeWorkspacePath = useWorkspaceStore((s) => s.activeWorkspacePath);
  const setComments = useLaudoStore((s) => s.setComments);

  const [draft, setDraft] = useState("");
  const [filter, setFilter] = useState<Filter>("ativos");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const author = useMemo(() => guessCurrentAuthor(), []);
  const comments = doc?.comments ?? [];
  const anchors = useMemo(
    () => extractCommentAnchors(doc?.content),
    [doc?.content],
  );

  const filtered = useMemo(() => {
    const sorted = [...comments].sort((a, b) => {
      // ativos antes; depois mais recente primeiro.
      if (a.resolved !== b.resolved) return a.resolved ? 1 : -1;
      return b.created_at.localeCompare(a.created_at);
    });
    if (filter === "todos") return sorted;
    if (filter === "ativos") return sorted.filter((c) => !c.resolved);
    return sorted.filter((c) => c.resolved);
  }, [comments, filter]);

  const activeCount = countActiveComments(comments);

  const hasSelection =
    !!editor && !editor.state.selection.empty;

  const handleAdd = async () => {
    if (!editor || !activeWorkspacePath || !doc) return;
    const text = draft.trim();
    if (!text) return;
    if (!hasSelection) {
      setError("Selecione um trecho de texto antes de comentar.");
      return;
    }
    setBusy(true);
    setError(null);
    try {
      const comment = createComment({ author, body: text });
      // 1. Aplica o mark TipTap na seleção.
      editor.chain().focus().addComment(comment.id).run();
      // 2. Persiste o comment na coleção.
      const next = addCommentSvc(comments, comment);
      await setComments(activeWorkspacePath, next);
      setDraft("");
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
    } finally {
      setBusy(false);
    }
  };

  const handleResolve = async (c: SicroDocComment) => {
    if (!activeWorkspacePath) return;
    setBusy(true);
    try {
      const next = c.resolved
        ? unresolveCommentSvc(comments, c.id)
        : resolveCommentSvc(comments, c.id);
      await setComments(activeWorkspacePath, next);
    } finally {
      setBusy(false);
    }
  };

  const handleDelete = async (c: SicroDocComment) => {
    if (!activeWorkspacePath || !editor) return;
    if (
      !window.confirm(
        `Excluir comentário "${c.body.slice(0, 60)}..." definitivamente?`,
      )
    ) {
      return;
    }
    setBusy(true);
    try {
      // 1. Remove o mark.
      editor.chain().focus().removeComment(c.id).run();
      // 2. Remove da coleção.
      const next = deleteCommentSvc(comments, c.id);
      await setComments(activeWorkspacePath, next);
    } finally {
      setBusy(false);
    }
  };

  const handleJump = (id: string) => {
    if (!editor) return;
    const anchor = anchors.find((a) => a.id === id);
    if (!anchor) return;
    editor.commands.focus();
    editor.commands.setTextSelection(anchor.pos);
    editor.commands.scrollIntoView();
  };

  const handleReply = async (c: SicroDocComment, body: string) => {
    if (!activeWorkspacePath) return;
    const text = body.trim();
    if (!text) return;
    setBusy(true);
    try {
      const next = addReplySvc(comments, c.id, { author, body: text });
      await setComments(activeWorkspacePath, next);
    } finally {
      setBusy(false);
    }
  };

  if (!editor) {
    return (
      <p className={styles.empty}>
        Abra um laudo para comentar trechos.
      </p>
    );
  }

  return (
    <>
      <h3 className={styles.sectionTitle}>
        <MessageSquare size={14} />
        Comentários
        {activeCount > 0 && (
          <span className={styles.countBadge}>{activeCount} ativo{activeCount === 1 ? "" : "s"}</span>
        )}
      </h3>

      <div className={styles.composeBox}>
        <textarea
          className={styles.textarea}
          rows={3}
          value={draft}
          onChange={(e) => setDraft(e.target.value)}
          placeholder={
            hasSelection
              ? "Digite o comentário e clique em adicionar…"
              : "Selecione um trecho do texto primeiro."
          }
          disabled={busy}
        />
        <button
          type="button"
          className={styles.addBtn}
          onClick={handleAdd}
          disabled={busy || !hasSelection || !draft.trim()}
          title={
            hasSelection
              ? "Adicionar comentário na seleção"
              : "Selecione um trecho do texto para comentar"
          }
        >
          <MessageSquarePlus size={12} />
          Adicionar
        </button>
      </div>

      {error && <div className={styles.errorBanner}>{error}</div>}

      <div className={styles.filterRow}>
        {(["ativos", "resolvidos", "todos"] as Filter[]).map((f) => (
          <button
            key={f}
            type="button"
            className={`${styles.filterChip} ${
              filter === f ? styles.filterChipActive : ""
            }`}
            onClick={() => setFilter(f)}
          >
            {capitalize(f)}
          </button>
        ))}
      </div>

      {filtered.length === 0 ? (
        <p className={styles.empty}>
          {filter === "ativos"
            ? "Nenhum comentário ativo."
            : filter === "resolvidos"
              ? "Nenhum comentário resolvido."
              : "Nenhum comentário ainda. Selecione um trecho e comente."}
        </p>
      ) : (
        <ul className={styles.list}>
          {filtered.map((c) => {
            const anchor = anchors.find((a) => a.id === c.id);
            return (
              <CommentItem
                key={c.id}
                comment={c}
                anchorExcerpt={anchor?.excerpt}
                disabled={busy}
                onJump={() => handleJump(c.id)}
                onToggleResolved={() => handleResolve(c)}
                onDelete={() => handleDelete(c)}
                onReply={(body) => handleReply(c, body)}
              />
            );
          })}
        </ul>
      )}
    </>
  );
}

function CommentItem({
  comment,
  anchorExcerpt,
  disabled,
  onJump,
  onToggleResolved,
  onDelete,
  onReply,
}: {
  comment: SicroDocComment;
  anchorExcerpt?: string;
  disabled: boolean;
  onJump: () => void;
  onToggleResolved: () => void;
  onDelete: () => void;
  onReply: (body: string) => void;
}) {
  const [replyOpen, setReplyOpen] = useState(false);
  const [replyDraft, setReplyDraft] = useState("");

  const handleSendReply = () => {
    const text = replyDraft.trim();
    if (!text) return;
    onReply(text);
    setReplyDraft("");
    setReplyOpen(false);
  };

  return (
    <li
      className={`${styles.item} ${comment.resolved ? styles.itemResolved : ""}`}
    >
      <div className={styles.itemHeader}>
        <span className={styles.itemAuthor}>{comment.author}</span>
        <span className={styles.itemTime}>{shortTime(comment.created_at)}</span>
      </div>
      {anchorExcerpt && (
        <button
          type="button"
          className={styles.anchorExcerpt}
          onClick={onJump}
          title="Ir para o trecho ancorado"
        >
          <CornerDownRight size={11} />
          <span>"{anchorExcerpt}"</span>
        </button>
      )}
      <p className={styles.itemBody}>{comment.body}</p>
      {comment.replies && comment.replies.length > 0 && (
        <ul className={styles.replies}>
          {comment.replies.map((r) => (
            <li key={r.id} className={styles.reply}>
              <div className={styles.itemHeader}>
                <span className={styles.itemAuthor}>{r.author}</span>
                <span className={styles.itemTime}>
                  {shortTime(r.created_at)}
                </span>
              </div>
              <p className={styles.itemBody}>{r.body}</p>
            </li>
          ))}
        </ul>
      )}
      {replyOpen ? (
        <div className={styles.replyBox}>
          <textarea
            rows={2}
            className={styles.textarea}
            value={replyDraft}
            onChange={(e) => setReplyDraft(e.target.value)}
            placeholder="Responder…"
            disabled={disabled}
          />
          <div className={styles.replyActions}>
            <button
              type="button"
              className={styles.smallBtn}
              onClick={() => {
                setReplyOpen(false);
                setReplyDraft("");
              }}
            >
              Cancelar
            </button>
            <button
              type="button"
              className={`${styles.smallBtn} ${styles.smallBtnPrimary}`}
              onClick={handleSendReply}
              disabled={disabled || !replyDraft.trim()}
            >
              Enviar
            </button>
          </div>
        </div>
      ) : (
        <div className={styles.itemActions}>
          <button
            type="button"
            className={styles.smallBtn}
            onClick={() => setReplyOpen(true)}
            disabled={disabled}
            title="Responder"
          >
            Responder
          </button>
          <button
            type="button"
            className={styles.smallBtn}
            onClick={onToggleResolved}
            disabled={disabled}
            title={comment.resolved ? "Reabrir" : "Marcar como resolvido"}
          >
            {comment.resolved ? (
              <>
                <RotateCcw size={11} /> Reabrir
              </>
            ) : (
              <>
                <Check size={11} /> Resolver
              </>
            )}
          </button>
          <button
            type="button"
            className={`${styles.smallBtn} ${styles.smallBtnDanger}`}
            onClick={onDelete}
            disabled={disabled}
            title="Excluir comentário"
          >
            <Trash2 size={11} />
          </button>
        </div>
      )}
    </li>
  );
}

function capitalize(s: string): string {
  return s.charAt(0).toUpperCase() + s.slice(1);
}

function shortTime(iso: string): string {
  try {
    const d = new Date(iso);
    return d.toLocaleString("pt-BR", {
      day: "2-digit",
      month: "2-digit",
      hour: "2-digit",
      minute: "2-digit",
    });
  } catch {
    return iso;
  }
}

function guessCurrentAuthor(): string {
  // Heurística simples: usa o nome do usuário do SO via env, ou "perito".
  // O dia que tivermos auth real, substitui aqui.
  try {
    // No browser/Tauri, navigator.userAgent não inclui usuário; deixamos
    // estático com fallback.
    return "Perito";
  } catch {
    return "Perito";
  }
}
