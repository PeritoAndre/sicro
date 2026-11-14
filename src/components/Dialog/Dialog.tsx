import { useEffect, useRef, type ReactNode } from "react";
import { createPortal } from "react-dom";
import { X } from "lucide-react";
import { create } from "zustand";
import styles from "./Dialog.module.css";

interface DialogProps {
  open: boolean;
  title: string;
  onClose: () => void;
  children: ReactNode;
  footer?: ReactNode;
}

/** Diálogos abertos, do mais antigo ao de cima: só o de cima responde ao Esc e prende o Tab. Telas com atalhos
 *  próprios os desligam enquanto houver algum (`useDialogosAbertos`). */
export const useDialogosAbertos = create<{ pilha: number[] }>(() => ({ pilha: [] }));
let proximo = 1;

const FOCAVEIS =
  'button:not([disabled]), [href], input:not([disabled]):not([type="hidden"]), select:not([disabled]), textarea:not([disabled]), [tabindex]:not([tabindex="-1"])';

function focaveis(raiz: HTMLElement): HTMLElement[] {
  return [...raiz.querySelectorAll<HTMLElement>(FOCAVEIS)].filter((e) => !e.hasAttribute("hidden") && e.getAttribute("aria-hidden") !== "true");
}

export function Dialog({ open, title, onClose, children, footer }: DialogProps) {
  const caixa = useRef<HTMLDivElement | null>(null);
  const fechar = useRef(onClose);
  fechar.current = onClose;

  useEffect(() => {
    if (!open) return;
    const id = proximo++;
    useDialogosAbertos.setState((s) => ({ pilha: [...s.pilha, id] }));
    const antes = document.activeElement instanceof HTMLElement ? document.activeElement : null;
    const emCima = () => useDialogosAbertos.getState().pilha.at(-1) === id;
    // foco para dentro, salvo se um filho já pegou (autoFocus)
    const t = window.setTimeout(() => {
      const el = caixa.current;
      if (el && !el.contains(document.activeElement)) (focaveis(el).find((e) => e.getAttribute("aria-label") !== "Fechar") ?? el).focus();
    }, 0);
    const handler = (e: KeyboardEvent) => {
      if (!emCima()) return;
      if (e.key === "Escape") {
        fechar.current();
        return;
      }
      if (e.key !== "Tab") return;
      const el = caixa.current;
      if (!el) return;
      const l = focaveis(el);
      if (l.length === 0) {
        e.preventDefault();
        el.focus();
        return;
      }
      const a = document.activeElement;
      const i = a instanceof HTMLElement ? l.indexOf(a) : -1;
      if (e.shiftKey && (i <= 0 || !el.contains(a))) {
        e.preventDefault();
        l[l.length - 1]!.focus();
      } else if (!e.shiftKey && (i === l.length - 1 || !el.contains(a))) {
        e.preventDefault();
        l[0]!.focus();
      }
    };
    window.addEventListener("keydown", handler);
    return () => {
      window.clearTimeout(t);
      window.removeEventListener("keydown", handler);
      useDialogosAbertos.setState((s) => ({ pilha: s.pilha.filter((x) => x !== id) }));
      // devolve o foco a quem o tinha, se ainda está na página
      if (antes?.isConnected) antes.focus();
    };
  }, [open]);

  // Não fechar quando o arrasto começou dentro (selecionar texto e soltar fora).
  const downOnBackdrop = useRef(false);

  if (!open) return null;

  return createPortal(
    <div
      className={styles.backdrop}
      onMouseDown={(e) => {
        downOnBackdrop.current = e.target === e.currentTarget;
      }}
      onClick={(e) => {
        if (e.target === e.currentTarget && downOnBackdrop.current) onClose();
        downOnBackdrop.current = false;
      }}
      role="presentation"
    >
      <div
        ref={caixa}
        className={styles.dialog}
        role="dialog"
        aria-modal="true"
        aria-label={title}
        tabIndex={-1}
      >
        <header className={styles.header}>
          <h2 className={styles.title}>{title}</h2>
          <button
            type="button"
            className={styles.closeBtn}
            onClick={onClose}
            aria-label="Fechar"
          >
            <X size={16} />
          </button>
        </header>
        <div className={styles.body}>{children}</div>
        {footer && <footer className={styles.footer}>{footer}</footer>}
      </div>
    </div>,
    document.body,
  );
}
