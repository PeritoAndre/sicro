/**
 * UnsavedChangesModal — the "Salvar antes de sair?" dialog raised by
 * CroquiEditor when the user tries to leave the editor while there
 * are unsaved changes (MVP 9 Round 3).
 *
 * Three outcomes:
 *   - **Salvar e sair**   → save the croqui, then proceed with the
 *                          pending navigation.
 *   - **Sair sem salvar** → discard local edits and proceed.
 *   - **Cancelar**        → close the modal and stay in the editor.
 *
 * Visual styling reuses the existing croqui dialog classes so the
 * modal feels native to the module.
 */

import styles from "./CroquiEditor.module.css";

export interface UnsavedChangesModalProps {
  /** Whether the parent is currently in the middle of a save. */
  saving: boolean;
  /** Whether the parent is currently in the middle of an export. */
  exporting?: boolean;
  /** Optional contextual text — e.g. where the user was trying to go. */
  destinationLabel?: string;
  onSaveAndLeave: () => void;
  onDiscardAndLeave: () => void;
  onCancel: () => void;
}

export function UnsavedChangesModal({
  saving,
  exporting,
  destinationLabel,
  onSaveAndLeave,
  onDiscardAndLeave,
  onCancel,
}: UnsavedChangesModalProps) {
  const busy = saving || !!exporting;
  return (
    <div
      className={styles.overlay}
      role="dialog"
      aria-modal="true"
      aria-labelledby="unsaved-changes-title"
      onClick={(e) => {
        if (e.target === e.currentTarget) onCancel();
      }}
    >
      <div className={styles.dialog} style={{ maxWidth: 460 }}>
        <header className={styles.dialogHeader}>
          <strong id="unsaved-changes-title">
            Alterações não salvas no croqui
          </strong>
        </header>
        <div style={{ padding: "8px 4px 12px", fontSize: 13, lineHeight: 1.5 }}>
          <p style={{ margin: 0 }}>
            Você tem alterações no croqui que ainda não foram salvas.
            {destinationLabel
              ? ` Deseja salvar antes de ir para ${destinationLabel}?`
              : " Deseja salvar antes de sair?"}
          </p>
        </div>
        <div
          style={{
            display: "flex",
            gap: 8,
            justifyContent: "flex-end",
            paddingTop: 4,
          }}
        >
          <button
            type="button"
            className={styles.dialogClose}
            onClick={onCancel}
            disabled={busy}
          >
            Cancelar
          </button>
          <button
            type="button"
            className={styles.dialogClose}
            style={{ color: "#f87171" }}
            onClick={onDiscardAndLeave}
            disabled={busy}
          >
            Sair sem salvar
          </button>
          <button
            type="button"
            className={styles.dialogClose}
            style={{ color: "#5aa9e6", fontWeight: 600 }}
            onClick={onSaveAndLeave}
            disabled={busy}
          >
            {saving ? "Salvando…" : "Salvar e sair"}
          </button>
        </div>
      </div>
    </div>
  );
}
