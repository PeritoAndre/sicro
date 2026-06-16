/**
 * Toaster — renderiza a pilha de toasts na raiz do app.
 *
 * F12.10 — Posicionado bottom-right (canto direito inferior) com
 * stacking vertical. Cada toast tem botão X para dismiss manual.
 */

import {
  AlertTriangle,
  CheckCircle2,
  Info,
  Loader2,
  X,
  XCircle,
} from "lucide-react";
import { useToastStore, type Toast } from "./toastStore";
import styles from "./Toaster.module.css";

export function Toaster() {
  const toasts = useToastStore((s) => s.toasts);
  const dismiss = useToastStore((s) => s.dismiss);

  if (toasts.length === 0) return null;

  return (
    <div className={styles.container} role="region" aria-label="Notificações">
      {toasts.map((t) => (
        <ToastItem key={t.id} toast={t} onClose={() => dismiss(t.id)} />
      ))}
    </div>
  );
}

interface ToastItemProps {
  toast: Toast;
  onClose: () => void;
}

function ToastItem({ toast, onClose }: ToastItemProps) {
  return (
    <div
      className={`${styles.toast} ${styles[toast.kind]}`}
      role={toast.kind === "error" || toast.kind === "warn" ? "alert" : "status"}
    >
      <div className={styles.icon}>{iconFor(toast)}</div>
      <div className={styles.body}>
        {toast.title && <strong className={styles.title}>{toast.title}</strong>}
        <span className={styles.message}>{toast.message}</span>
      </div>
      {toast.kind !== "progress" && (
        <button
          type="button"
          className={styles.close}
          onClick={onClose}
          aria-label="Fechar notificação"
        >
          <X size={12} />
        </button>
      )}
    </div>
  );
}

function iconFor(t: Toast) {
  switch (t.kind) {
    case "success":
      return <CheckCircle2 size={16} />;
    case "warn":
      return <AlertTriangle size={16} />;
    case "error":
      return <XCircle size={16} />;
    case "progress":
      return <Loader2 size={16} className={styles.spin} />;
    case "info":
    default:
      return <Info size={16} />;
  }
}
