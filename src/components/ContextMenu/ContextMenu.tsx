/**
 * Menu do botão direito do SICRO. Desenhado no lugar, não em portal: a
 * Fullscreen API só mostra a subárvore do elemento em tela cheia.
 */
import { useCallback, useEffect, useLayoutEffect, useRef, useState, type ReactNode } from "react";
import styles from "./ContextMenu.module.css";

export type MenuItem =
  | {
      label: string;
      shortcut?: string;
      icon?: ReactNode;
      disabled?: boolean;
      danger?: boolean;
      onSelect: () => void;
    }
  | "separator";

interface OpenState {
  x: number;
  y: number;
  items: MenuItem[];
}

export function useContextMenu() {
  const [state, setState] = useState<OpenState | null>(null);
  const close = useCallback(() => setState(null), []);

  const open = useCallback((e: { clientX: number; clientY: number; preventDefault: () => void; stopPropagation: () => void }, items: MenuItem[]) => {
    e.preventDefault();
    e.stopPropagation();
    // Tira separadores sobrando (início, fim, duplicados).
    const clean: MenuItem[] = [];
    for (const it of items) {
      if (it === "separator" && (clean.length === 0 || clean[clean.length - 1] === "separator")) continue;
      clean.push(it);
    }
    while (clean[clean.length - 1] === "separator") clean.pop();
    if (clean.length) setState({ x: e.clientX, y: e.clientY, items: clean });
  }, []);

  const element = state ? <MenuView state={state} onClose={close} /> : null;
  return { open, close, element, isOpen: state != null };
}

function MenuView({ state, onClose }: { state: OpenState; onClose: () => void }) {
  const ref = useRef<HTMLDivElement | null>(null);
  const [pos, setPos] = useState({ left: state.x, top: state.y });

  // Mantém o menu dentro da janela.
  useLayoutEffect(() => {
    const el = ref.current;
    if (!el) return;
    const r = el.getBoundingClientRect();
    const left = Math.max(4, Math.min(state.x, window.innerWidth - r.width - 4));
    const top = Math.max(4, Math.min(state.y, window.innerHeight - r.height - 4));
    setPos({ left, top });
  }, [state]);

  useEffect(() => {
    const onDown = (e: PointerEvent) => {
      if (ref.current && !ref.current.contains(e.target as Node)) onClose();
    };
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") {
        e.preventDefault();
        e.stopPropagation();
        onClose();
      }
    };
    const onOther = () => onClose();
    window.addEventListener("pointerdown", onDown, true);
    window.addEventListener("keydown", onKey, true);
    window.addEventListener("blur", onOther);
    window.addEventListener("resize", onOther);
    return () => {
      window.removeEventListener("pointerdown", onDown, true);
      window.removeEventListener("keydown", onKey, true);
      window.removeEventListener("blur", onOther);
      window.removeEventListener("resize", onOther);
    };
  }, [onClose]);

  return (
    <div
      ref={ref}
      role="menu"
      className={styles.menu}
      style={pos}
      data-no-magnify
      onContextMenu={(e) => e.preventDefault()}
    >
      {state.items.map((it, i) =>
        it === "separator" ? (
          <div key={`s${i}`} className={styles.sep} role="separator" />
        ) : (
          <button
            key={`${i}-${it.label}`}
            type="button"
            role="menuitem"
            disabled={it.disabled}
            className={`${styles.item} ${it.danger ? styles.danger : ""}`}
            onClick={() => {
              onClose();
              it.onSelect();
            }}
          >
            <span className={styles.icon}>{it.icon}</span>
            <span className={styles.label}>{it.label}</span>
            {it.shortcut && <kbd className={styles.kbd}>{it.shortcut}</kbd>}
          </button>
        ),
      )}
    </div>
  );
}

/**
 * Bloqueia o menu de navegador no app todo; campos de texto mantêm o nativo
 * (Copiar/Colar) e em DEV fica liberado para o inspetor.
 */
export function installContextMenuGuard(): () => void {
  if (import.meta.env.DEV) return () => {};
  const onMenu = (e: MouseEvent) => {
    const el = e.target as HTMLElement | null;
    const editable =
      !!el &&
      (el.isContentEditable ||
        (el.tagName === "TEXTAREA") ||
        (el.tagName === "INPUT" &&
          !["range", "checkbox", "radio", "button", "submit", "color", "file"].includes(
            (el as HTMLInputElement).type,
          )));
    if (!editable) e.preventDefault();
  };
  document.addEventListener("contextmenu", onMenu);
  return () => document.removeEventListener("contextmenu", onMenu);
}
