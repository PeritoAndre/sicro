/**
 * PageControls — controles compactos de zoom + modo que ficam acoplados
 * à direita da `LaudoStatusBar`.
 *
 * Layout (da esquerda para a direita):
 *   - Toggle de modo (edição / leitura / foco / revisão).
 *   - Botão Zoom Out (−).
 *   - Slider 50–200%.
 *   - Botão Zoom In (+).
 *   - Botão "100%" (reset).
 *   - Botão "Ajustar" → dropdown com fitWidth / fitPage.
 *
 * Os ícones são propositalmente discretos — F3 visa entregar a função
 * sem competir com a toolbar principal. O slider usa CSS nativo para
 * evitar dependências.
 */

import { useState, useEffect, useRef } from "react";
import {
  BookOpen,
  Eye,
  Pencil,
  MinusCircle,
  PlusCircle,
  Maximize2,
  MessageSquare,
  RectangleHorizontal,
  RectangleVertical,
} from "lucide-react";
import type { LaudoMode } from "../hooks/useEditorMode";
import { ZOOM_MAX, ZOOM_MIN, ZOOM_STEP } from "../hooks/useZoom";
import styles from "./PageControls.module.css";

interface PageControlsProps {
  mode: LaudoMode;
  onModeChange: (mode: LaudoMode) => void;
  zoom: number;
  onZoomChange: (zoom: number) => void;
  onZoomIn: () => void;
  onZoomOut: () => void;
  onReset: () => void;
  onFitWidth: () => void;
  onFitPage: () => void;
  /**
   * F3.1 — Orientação atual e toggle rápido na status bar (paralelo ao
   * toggle do Inspector "Página"). Quando ambos handlers ausentes, o
   * controle some — útil para clientes que querem manter o toggle só
   * no Inspector.
   */
  orientation?: "portrait" | "landscape";
  onOrientationChange?: (next: "portrait" | "landscape") => void;
}

export function PageControls({
  mode,
  onModeChange,
  zoom,
  onZoomChange,
  onZoomIn,
  onZoomOut,
  onReset,
  onFitWidth,
  onFitPage,
  orientation = "portrait",
  onOrientationChange,
}: PageControlsProps) {
  const [fitOpen, setFitOpen] = useState(false);
  const fitWrapRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    if (!fitOpen) return undefined;
    const handler = (e: MouseEvent) => {
      if (fitWrapRef.current && !fitWrapRef.current.contains(e.target as Node)) {
        setFitOpen(false);
      }
    };
    window.addEventListener("mousedown", handler);
    return () => window.removeEventListener("mousedown", handler);
  }, [fitOpen]);

  const zoomPct = Math.round(zoom * 100);

  return (
    <div className={styles.root}>
      {/* Modo */}
      <div
        className={styles.modeGroup}
        role="radiogroup"
        aria-label="Modo do editor"
      >
        <ModeBtn
          active={mode === "edicao"}
          icon={<Pencil size={11} />}
          label="Edição"
          onClick={() => onModeChange("edicao")}
        />
        <ModeBtn
          active={mode === "leitura"}
          icon={<BookOpen size={11} />}
          label="Leitura"
          onClick={() => onModeChange("leitura")}
        />
        <ModeBtn
          active={mode === "foco"}
          icon={<Eye size={11} />}
          label="Foco"
          onClick={() => onModeChange("foco")}
        />
        <ModeBtn
          active={mode === "revisao"}
          icon={<MessageSquare size={11} />}
          label="Revisão"
          onClick={() => onModeChange("revisao")}
        />
      </div>

      <div className={styles.divider} />

      {/* Zoom */}
      <div className={styles.zoomGroup}>
        <button
          type="button"
          className={styles.iconBtn}
          onClick={onZoomOut}
          title="Reduzir (Ctrl+−)"
          disabled={zoom <= ZOOM_MIN + 0.001}
          aria-label="Reduzir zoom"
        >
          <MinusCircle size={13} />
        </button>
        <input
          type="range"
          min={ZOOM_MIN}
          max={ZOOM_MAX}
          step={ZOOM_STEP}
          value={zoom}
          onChange={(e) => onZoomChange(Number(e.target.value))}
          className={styles.slider}
          aria-label="Zoom"
          title={`${zoomPct}%`}
        />
        <button
          type="button"
          className={styles.iconBtn}
          onClick={onZoomIn}
          title="Aumentar (Ctrl+=)"
          disabled={zoom >= ZOOM_MAX - 0.001}
          aria-label="Aumentar zoom"
        >
          <PlusCircle size={13} />
        </button>
        <button
          type="button"
          className={styles.zoomLabel}
          onClick={onReset}
          title="Restaurar para 100% (Ctrl+0)"
        >
          {zoomPct}%
        </button>
        {onOrientationChange && (
          <>
            <div className={styles.divider} />
            <button
              type="button"
              className={`${styles.iconBtn} ${orientation === "portrait" ? styles.orientActive : ""}`}
              onClick={() => onOrientationChange("portrait")}
              title="Orientação retrato"
              aria-pressed={orientation === "portrait"}
              aria-label="Retrato"
            >
              <RectangleVertical size={13} />
            </button>
            <button
              type="button"
              className={`${styles.iconBtn} ${orientation === "landscape" ? styles.orientActive : ""}`}
              onClick={() => onOrientationChange("landscape")}
              title="Orientação paisagem"
              aria-pressed={orientation === "landscape"}
              aria-label="Paisagem"
            >
              <RectangleHorizontal size={13} />
            </button>
          </>
        )}
        <div className={styles.fitWrap} ref={fitWrapRef}>
          <button
            type="button"
            className={styles.iconBtn}
            onClick={() => setFitOpen((v) => !v)}
            title="Ajustar"
            aria-expanded={fitOpen}
            aria-haspopup="menu"
          >
            <Maximize2 size={12} />
          </button>
          {fitOpen && (
            <div className={styles.fitMenu} role="menu">
              <button
                type="button"
                className={styles.fitItem}
                onClick={() => {
                  onFitWidth();
                  setFitOpen(false);
                }}
              >
                Ajustar à largura
              </button>
              <button
                type="button"
                className={styles.fitItem}
                onClick={() => {
                  onFitPage();
                  setFitOpen(false);
                }}
              >
                Ajustar à página
              </button>
              <button
                type="button"
                className={styles.fitItem}
                onClick={() => {
                  onReset();
                  setFitOpen(false);
                }}
              >
                100%
              </button>
            </div>
          )}
        </div>
      </div>
    </div>
  );
}

function ModeBtn({
  active,
  icon,
  label,
  onClick,
}: {
  active: boolean;
  icon: React.ReactNode;
  label: string;
  onClick: () => void;
}) {
  return (
    <button
      type="button"
      role="radio"
      aria-checked={active}
      className={`${styles.modeBtn} ${active ? styles.modeBtnActive : ""}`}
      onClick={onClick}
      title={label}
    >
      {icon} <span className={styles.modeLabel}>{label}</span>
    </button>
  );
}
