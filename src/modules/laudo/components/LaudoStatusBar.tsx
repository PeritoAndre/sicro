/**
 * LaudoStatusBar — barra inferior fina com contadores e modo do editor.
 *
 * Mostra:
 *   - Palavras / caracteres / parágrafos (via `useWordCount`).
 *   - Status do autosave: "salvo há Xs" | "salvando…" | "alterações pendentes".
 *   - Modo do editor (edição / leitura / etc — F3 introduz isso).
 *
 * Layout fica abaixo do editor, fora do paper canvas. Não interfere na
 * exportação PDF/DOCX (faz parte da UI, não do documento).
 */

import type { Editor } from "@tiptap/react";
import type { ReactNode } from "react";
import { Check, Loader2, Pencil } from "lucide-react";
import { useWordCount } from "../hooks/useWordCount";
import styles from "./LaudoStatusBar.module.css";

export type LaudoSaveState = "saved" | "saving" | "dirty" | "error";

interface LaudoStatusBarProps {
  editor: Editor | null;
  saveState: LaudoSaveState;
  /** Mensagem auxiliar mostrada à direita (ex: "salvo há 5s"). */
  saveLabel?: string;
  /** Modo atual — placeholder até F3 ativar leitura/foco/revisão. */
  mode?: "edicao" | "leitura" | "foco" | "revisao";
  /**
   * F3 — slot opcional para `PageControls` (zoom + modo) à direita da
   * barra. Permite que o `LaudoEditorView` injete os controles sem
   * acoplar a barra com tipos específicos do zoom/modo.
   */
  pageControls?: ReactNode;
  /** F12.5 — Callback para abrir modal de atalhos. */
  onOpenShortcuts?: () => void;
}

export function LaudoStatusBar({
  editor,
  saveState,
  saveLabel,
  mode = "edicao",
  pageControls,
  onOpenShortcuts,
}: LaudoStatusBarProps) {
  const { words, chars, charsNoSpaces, paragraphs } = useWordCount(editor);

  return (
    <div className={styles.bar} role="status" aria-live="polite">
      <div className={styles.left}>
        <span className={styles.modeChip} data-mode={mode}>
          <Pencil size={11} /> {modeLabel(mode)}
        </span>
        <SaveIndicator state={saveState} label={saveLabel} />
      </div>
      <div className={styles.center}>
        <span className={styles.counter}>
          <strong>{words.toLocaleString("pt-BR")}</strong> palavras
        </span>
        <span className={styles.separator}>·</span>
        <span className={styles.counter}>
          <strong>{chars.toLocaleString("pt-BR")}</strong> caracteres
        </span>
        <span className={styles.separator}>·</span>
        <span className={styles.counter}>
          <strong>{charsNoSpaces.toLocaleString("pt-BR")}</strong> sem espaços
        </span>
        <span className={styles.separator}>·</span>
        <span className={styles.counter}>
          <strong>{paragraphs.toLocaleString("pt-BR")}</strong> parágrafos
        </span>
      </div>
      <div className={styles.right}>
        {pageControls}
        {onOpenShortcuts && (
          <button
            type="button"
            className={styles.helpBtn}
            onClick={onOpenShortcuts}
            title="Atalhos de teclado (?)"
            aria-label="Abrir atalhos de teclado"
          >
            ?
          </button>
        )}
      </div>
    </div>
  );
}

function modeLabel(mode: LaudoStatusBarProps["mode"]): string {
  switch (mode) {
    case "leitura":
      return "Leitura";
    case "foco":
      return "Foco";
    case "revisao":
      return "Revisão";
    case "edicao":
    default:
      return "Edição";
  }
}

function SaveIndicator({
  state,
  label,
}: {
  state: LaudoSaveState;
  label?: string;
}) {
  if (state === "saving") {
    return (
      <span className={`${styles.save} ${styles.saveBusy}`}>
        <Loader2 size={11} className={styles.spin} /> Salvando…
      </span>
    );
  }
  if (state === "dirty") {
    return (
      <span className={`${styles.save} ${styles.saveDirty}`}>
        Alterações pendentes
      </span>
    );
  }
  if (state === "error") {
    return (
      <span className={`${styles.save} ${styles.saveError}`}>
        Falha ao salvar
      </span>
    );
  }
  return (
    <span className={`${styles.save} ${styles.saveOk}`}>
      <Check size={11} /> {label ?? "Salvo"}
    </span>
  );
}
