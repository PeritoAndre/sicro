/** Menu ⋯ em portal no body, posicionado pela viewport (nenhum overflow o corta). */

import { useEffect, useRef, useState, type ReactNode } from "react";
import { createPortal } from "react-dom";
import { MoreHorizontal } from "lucide-react";
import styles from "./PopMenu.module.css";

export interface MenuItem {
  label: string;
  icon?: ReactNode;
  onClick: () => void;
  danger?: boolean;
}

export function PopMenu({ items, label, className }: { items: MenuItem[]; label: string; className?: string }) {
  const [open, setOpen] = useState(false);
  const [pos, setPos] = useState<{ top: number; left: number } | null>(null);
  const btnRef = useRef<HTMLButtonElement | null>(null);

  useEffect(() => {
    if (!open) {
      setPos(null);
      return;
    }
    const MENU_W = 210;
    const MENU_H_EST = 40 * Math.max(items.length, 1) + 12;
    const compute = () => {
      const r = btnRef.current?.getBoundingClientRect();
      if (!r) return;
      let top = r.bottom + 4;
      let left = r.right - MENU_W;
      const vh = window.innerHeight;
      const vw = window.innerWidth;
      if (top + MENU_H_EST > vh - 8) top = Math.max(8, r.top - MENU_H_EST - 4);
      if (left < 8) left = 8;
      if (left + MENU_W > vw - 8) left = vw - MENU_W - 8;
      setPos({ top, left });
    };
    compute();
    const onKey = (e: globalThis.KeyboardEvent) => {
      if (e.key === "Escape") setOpen(false);
    };
    window.addEventListener("scroll", compute, true);
    window.addEventListener("resize", compute);
    window.addEventListener("keydown", onKey);
    return () => {
      window.removeEventListener("scroll", compute, true);
      window.removeEventListener("resize", compute);
      window.removeEventListener("keydown", onKey);
    };
  }, [open, items.length]);

  return (
    <>
      <button
        ref={btnRef}
        type="button"
        className={`${styles.iconBtn} ${className ?? ""}`}
        aria-label={label}
        aria-haspopup="menu"
        aria-expanded={open}
        onClick={() => setOpen((o) => !o)}
      >
        <MoreHorizontal size={16} />
      </button>
      {open &&
        pos &&
        createPortal(
          <>
            <div className={styles.menuBackdrop} onClick={() => setOpen(false)} />
            <div className={styles.menu} role="menu" style={{ top: pos.top, left: pos.left }}>
              {items.map((it) => (
                <button
                  key={it.label}
                  type="button"
                  role="menuitem"
                  className={`${styles.menuItem} ${it.danger ? styles.menuItemDanger : ""}`}
                  onClick={() => {
                    setOpen(false);
                    it.onClick();
                  }}
                >
                  {it.icon}
                  <span>{it.label}</span>
                </button>
              ))}
            </div>
          </>,
          document.body,
        )}
    </>
  );
}
