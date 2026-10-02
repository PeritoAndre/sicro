/**
 * Barra de título própria (janela com `decorations: false`). Janela frameless
 * perde o redimensionar pelas bordas no Windows: as alças chamam `startResizeDragging`.
 */

import { useEffect, useState } from "react";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { Brand } from "./Brand";
import styles from "./TitleBar.module.css";

// `as const` deixa cada `dir` literal, casando com o union ResizeDirection.
const RESIZE_HANDLES = [
  { dir: "North", cls: "rN" },
  { dir: "South", cls: "rS" },
  { dir: "East", cls: "rE" },
  { dir: "West", cls: "rW" },
  { dir: "NorthWest", cls: "rNW" },
  { dir: "NorthEast", cls: "rNE" },
  { dir: "SouthWest", cls: "rSW" },
  { dir: "SouthEast", cls: "rSE" },
] as const;

export function TitleBar() {
  const win = getCurrentWindow();
  const [maximized, setMaximized] = useState(false);

  useEffect(() => {
    let unlisten: (() => void) | undefined;
    const sync = () => {
      void win.isMaximized().then(setMaximized).catch(() => {});
    };
    sync();
    win
      .onResized(sync)
      .then((u) => {
        unlisten = u;
      })
      .catch(() => {});
    return () => unlisten?.();
  }, [win]);

  return (
    <>
      <div className={styles.bar}>
        {/* Os filhos têm pointer-events:none para o arrasto cair no drag-region. */}
        <div className={styles.brandSlot} data-tauri-drag-region>
          <Brand />
        </div>
        {/* Duplo-clique maximiza (permissão internal-toggle-maximize). */}
        <div className={styles.drag} data-tauri-drag-region />
        <div className={styles.controls}>
          <button
            type="button"
            className={styles.btn}
            onClick={() => void win.minimize().catch(() => {})}
            aria-label="Minimizar"
            title="Minimizar"
          >
            <svg width="10" height="10" viewBox="0 0 10 10" aria-hidden>
              <path d="M0 5 H10" stroke="currentColor" strokeWidth="1" />
            </svg>
          </button>
          <button
            type="button"
            className={styles.btn}
            onClick={() => void win.toggleMaximize().catch(() => {})}
            aria-label={maximized ? "Restaurar" : "Maximizar"}
            title={maximized ? "Restaurar" : "Maximizar"}
          >
            {maximized ? (
              <svg width="10" height="10" viewBox="0 0 10 10" aria-hidden>
                <rect
                  x="0.5"
                  y="2.5"
                  width="6"
                  height="6"
                  fill="none"
                  stroke="currentColor"
                  strokeWidth="1"
                />
                <path
                  d="M2.5 2.5 V0.5 H9.5 V7.5 H7.5"
                  fill="none"
                  stroke="currentColor"
                  strokeWidth="1"
                />
              </svg>
            ) : (
              <svg width="10" height="10" viewBox="0 0 10 10" aria-hidden>
                <rect
                  x="0.5"
                  y="0.5"
                  width="9"
                  height="9"
                  fill="none"
                  stroke="currentColor"
                  strokeWidth="1"
                />
              </svg>
            )}
          </button>
          <button
            type="button"
            className={`${styles.btn} ${styles.close}`}
            onClick={() => void win.close().catch(() => {})}
            aria-label="Fechar"
            title="Fechar"
          >
            <svg width="10" height="10" viewBox="0 0 10 10" aria-hidden>
              <path d="M0 0 L10 10 M10 0 L0 10" stroke="currentColor" strokeWidth="1" />
            </svg>
          </button>
        </div>
      </div>

      {!maximized && (
        <div className={styles.resizeLayer} aria-hidden>
          {RESIZE_HANDLES.map((h) => (
            <div
              key={h.dir}
              className={`${styles.handle} ${styles[h.cls]}`}
              onPointerDown={(e) => {
                if (e.button !== 0) return;
                e.preventDefault();
                void win.startResizeDragging(h.dir).catch(() => {});
              }}
            />
          ))}
        </div>
      )}
    </>
  );
}
