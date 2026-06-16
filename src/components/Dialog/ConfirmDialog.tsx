/**
 * ConfirmDialog — small wrapper around <Dialog> for "are you sure?"
 * confirmation popups. Defaults are safe for destructive actions:
 *   - cancel button is auto-focused
 *   - confirm uses the `danger` variant when `destructive` is true
 *
 * The dialog is fully controlled: parent owns the open state and
 * passes `onCancel` / `onConfirm`. The component does not call
 * `onCancel` automatically after `onConfirm` — the parent decides
 * when to close (typically after the async work completes).
 */

import { useEffect, useRef, type ReactNode } from "react";
import { AlertTriangle } from "lucide-react";
import { Button } from "@components/Button/Button";
import { Dialog } from "./Dialog";
import styles from "./ConfirmDialog.module.css";

interface ConfirmDialogProps {
  open: boolean;
  title: string;
  /** Main confirmation message. Can be a string or rich JSX. */
  message: ReactNode;
  /** Optional secondary line, shown muted under the main message. */
  detail?: ReactNode;
  confirmLabel?: string;
  cancelLabel?: string;
  /** When true, confirm button is red and we lead with a warning icon. */
  destructive?: boolean;
  /** When true, both buttons are disabled (in-flight operation). */
  busy?: boolean;
  onConfirm: () => void;
  onCancel: () => void;
}

export function ConfirmDialog({
  open,
  title,
  message,
  detail,
  confirmLabel = "Confirmar",
  cancelLabel = "Cancelar",
  destructive = false,
  busy = false,
  onConfirm,
  onCancel,
}: ConfirmDialogProps) {
  const bodyRef = useRef<HTMLDivElement>(null);

  // Foca o botão Cancelar quando o popup abre — segurança para
  // ações destrutivas: se o usuário apertar Enter sem ler, ele
  // cancela em vez de confirmar.
  useEffect(() => {
    if (!open) return;
    const id = window.setTimeout(() => {
      const root = bodyRef.current?.closest("[role=dialog]");
      const cancelBtn = root?.querySelector<HTMLButtonElement>(
        "[data-confirm-cancel]",
      );
      cancelBtn?.focus();
    }, 50);
    return () => window.clearTimeout(id);
  }, [open]);

  return (
    <Dialog
      open={open}
      title={title}
      onClose={() => {
        if (!busy) onCancel();
      }}
      footer={
        <>
          <Button
            variant="secondary"
            onClick={onCancel}
            disabled={busy}
            data-confirm-cancel
          >
            {cancelLabel}
          </Button>
          <Button
            variant={destructive ? "danger" : "primary"}
            onClick={onConfirm}
            disabled={busy}
          >
            {busy ? "Aguarde…" : confirmLabel}
          </Button>
        </>
      }
    >
      <div className={styles.body} ref={bodyRef}>
        {destructive && (
          <div className={styles.icon} aria-hidden="true">
            <AlertTriangle size={22} />
          </div>
        )}
        <div className={styles.text}>
          <p className={styles.message}>{message}</p>
          {detail && <p className={styles.detail}>{detail}</p>}
        </div>
      </div>
    </Dialog>
  );
}
