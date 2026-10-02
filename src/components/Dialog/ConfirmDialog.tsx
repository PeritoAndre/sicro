/**
 * Popup de confirmação, totalmente controlado: não chama `onCancel` depois de
 * `onConfirm` — o pai fecha quando o trabalho assíncrono terminar.
 */

import { useEffect, useRef, type ReactNode } from "react";
import { AlertTriangle } from "lucide-react";
import { Button } from "@components/Button/Button";
import { Dialog } from "./Dialog";
import styles from "./ConfirmDialog.module.css";

interface ConfirmDialogProps {
  open: boolean;
  title: string;
  message: ReactNode;
  /** Linha secundária, apagada, sob a mensagem. */
  detail?: ReactNode;
  confirmLabel?: string;
  cancelLabel?: string;
  /** Botão vermelho + ícone de alerta. */
  destructive?: boolean;
  /** Desabilita os dois botões. */
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

  // Foco no Cancelar: Enter sem ler cancela em vez de confirmar.
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
