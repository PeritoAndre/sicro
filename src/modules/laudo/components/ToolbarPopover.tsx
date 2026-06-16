/**
 * ToolbarPopover — wrapper genérico para botões da barra superior do laudo
 * que precisam abrir um popover ancorado (Validações / Estilos / Cabeçalho
 * / Página / Dados).
 *
 * F4.1 — substituiu o uso dessas seções no Inspector lateral. O Inspector
 * lateral agora foca em **provas** (Estrutura + Evidências), e estes
 * popovers oferecem acesso rápido sem competir por espaço de aba.
 *
 * Comportamento:
 *   - Click no botão → toggle do popover.
 *   - Click fora → fecha.
 *   - Esc → fecha.
 *   - Posicionamento: fixo abaixo do botão, alinhado à direita.
 *   - Largura padrão 320px, altura máxima 60vh com scroll vertical.
 *
 * Estado controlado externamente (`open`/`onOpenChange`) para permitir
 * que a barra coordene abrir/fechar entre popovers (ex: abrir Estilos
 * fecha automaticamente Validações).
 */

import {
  type ReactNode,
  useCallback,
  useEffect,
  useRef,
} from "react";
import styles from "./ToolbarPopover.module.css";

export interface ToolbarPopoverProps {
  /** Conteúdo do botão (ícone + opcionalmente label). */
  trigger: ReactNode;
  /** Rótulo aria-label / title do botão. */
  label: string;
  /** Conteúdo do popover. */
  children: ReactNode;
  /** Estado externo do popover. */
  open: boolean;
  /** Callback de mudança de estado (controlado). */
  onOpenChange: (open: boolean) => void;
  /**
   * Badge opcional (ex: contagem de warnings) exibido no canto do botão
   * quando o popover está fechado.
   */
  badge?: string | number;
  /** Largura do popover em pixels. Default 320. */
  width?: number;
  /**
   * Posicionamento horizontal — alinhamento da borda do popover ao
   * botão. Default "right" (borda direita do popover = borda direita
   * do botão). Use "left" se o botão estiver no canto esquerdo da barra.
   */
  align?: "left" | "right";
}

export function ToolbarPopover({
  trigger,
  label,
  children,
  open,
  onOpenChange,
  badge,
  width = 320,
  align = "right",
}: ToolbarPopoverProps) {
  const wrapRef = useRef<HTMLDivElement>(null);

  // Click outside fecha.
  useEffect(() => {
    if (!open) return undefined;
    const handler = (e: MouseEvent) => {
      if (wrapRef.current && !wrapRef.current.contains(e.target as Node)) {
        onOpenChange(false);
      }
    };
    window.addEventListener("mousedown", handler);
    return () => window.removeEventListener("mousedown", handler);
  }, [open, onOpenChange]);

  // Esc fecha (apenas quando o popover está aberto).
  useEffect(() => {
    if (!open) return undefined;
    const handler = (e: KeyboardEvent) => {
      if (e.key === "Escape") {
        e.preventDefault();
        e.stopPropagation();
        onOpenChange(false);
      }
    };
    window.addEventListener("keydown", handler);
    return () => window.removeEventListener("keydown", handler);
  }, [open, onOpenChange]);

  const toggle = useCallback(
    () => onOpenChange(!open),
    [open, onOpenChange],
  );

  const popoverStyle: React.CSSProperties = {
    width: `${width}px`,
    [align === "right" ? "right" : "left"]: 0,
  };

  return (
    <div className={styles.wrap} ref={wrapRef}>
      <button
        type="button"
        className={`${styles.triggerBtn} ${open ? styles.triggerActive : ""}`}
        onClick={toggle}
        title={label}
        aria-label={label}
        aria-haspopup="dialog"
        aria-expanded={open}
      >
        {trigger}
        {badge !== undefined && badge !== 0 && badge !== "0" && (
          <span className={styles.badge}>{badge}</span>
        )}
      </button>
      {open && (
        <div
          className={styles.popover}
          style={popoverStyle}
          role="dialog"
          aria-label={label}
        >
          <div className={styles.popoverBody}>{children}</div>
        </div>
      )}
    </div>
  );
}
