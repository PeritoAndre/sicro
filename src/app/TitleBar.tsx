/**
 * TitleBar — barra de título CUSTOM (substitui a nativa do Windows).
 *
 * A janela roda com `decorations: false` (sem a moldura do SO). Esta barra:
 *   - é escura, na cor do chrome (`--sicro-surface-1`) — combina com o app;
 *   - NÃO mostra nome nem ícone (a marca SICRO fica só na sidebar);
 *   - traz os botões minimizar / maximizar-restaurar / fechar (estilo Windows);
 *   - tem área de arrastar (`data-tauri-drag-region`) — duplo-clique maximiza;
 *   - reabilita o REDIMENSIONAR pelas bordas (janela frameless perde isso no
 *     Windows): 8 "alças" finas nas bordas/cantos chamam `startResizeDragging`.
 *
 * O `title` da janela (`tauri.conf.json`) continua "SICRO 2.0" — ele some da
 * janela (não há barra nativa), mas segue identificando o app na barra de
 * tarefas / alt-tab.
 */

import { useEffect, useState } from "react";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { Brand } from "./Brand";
import styles from "./TitleBar.module.css";

// Bordas (finas) + cantos (quadradinhos) com a direção de resize do Tauri.
// `as const` deixa cada `dir` como literal — casa com o union ResizeDirection.
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
        {/* Marca (logo + nome) no canto superior esquerdo, sobre a coluna da
            sidebar. data-tauri-drag-region: arrasta a janela a partir daqui
            também (os filhos têm pointer-events:none pra o clique cair aqui). */}
        <div className={styles.brandSlot} data-tauri-drag-region>
          <Brand />
        </div>
        {/* Área de arrastar do meio. Duplo-clique = maximizar (nativo do
            drag-region, via permissão internal-toggle-maximize). */}
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

      {/* Alças de redimensionamento — só quando NÃO maximizado. */}
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
