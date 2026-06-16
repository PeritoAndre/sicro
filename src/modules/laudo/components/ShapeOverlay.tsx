/**
 * Q — ShapeOverlay: handles + toolbar pra forma selecionada.
 *
 * Análogo simplificado do FigureOverlay. Renderiza sobre o `.editorRegion`
 * quando há uma shape selecionada (useSelectedShape). Oferece:
 *
 *   - 4 handles de resize (cantos) — sem aspect ratio fixo
 *   - DragZone pra mover a forma livremente
 *   - Toolbar com:
 *       · 3 modos de wrap (Alinhado / Em frente / Atrás)
 *       · Color picker pra cor da borda (stroke_color)
 *       · Input pra espessura da borda (stroke_width)
 *       · Color picker pra cor de preenchimento (fill_color, só rect/ellipse)
 *       · Botão excluir
 *
 * Drag-to-reposition usa mesma técnica P24 (dragZone com z-index 5 acima
 * do texto). Setas e Linhas não têm preenchimento, então o color picker
 * de fill é escondido pra esses kinds.
 */

import { useCallback, useEffect, useLayoutEffect, useState } from "react";
import type { Editor } from "@tiptap/react";
import { NodeSelection } from "@tiptap/pm/state";
import { BringToFront, SendToBack, Trash2, WrapText } from "lucide-react";
import type {
  ShapeKind,
  ShapeWrapMode,
} from "../document-engine/nodes/Shape";
import type { SelectedShape } from "../hooks/useSelectedShape";
import styles from "./ShapeOverlay.module.css";

export interface ShapeOverlayProps {
  editor: Editor | null;
  selected: SelectedShape | null;
  containerRef: React.RefObject<HTMLElement>;
}

type ResizeDir = "nw" | "ne" | "se" | "sw";

const MIN_WIDTH_CM = 0.5;
const MIN_HEIGHT_CM = 0.5;
const PX_PER_CM = 37.7952755906;

export function ShapeOverlay({
  editor,
  selected,
  containerRef,
}: ShapeOverlayProps) {
  const [rect, setRect] = useState<DOMRect | null>(null);
  const [dragTooltip, setDragTooltip] = useState<{
    x: number;
    y: number;
    label: string;
  } | null>(null);

  const recompute = useCallback(() => {
    if (!selected || !containerRef.current || !editor) {
      setRect(null);
      return;
    }
    let domEl: HTMLElement | null = selected.domEl;
    try {
      const dom = editor.view.nodeDOM(selected.pos);
      if (dom instanceof HTMLElement) domEl = dom;
    } catch {
      // ignore
    }
    if (!domEl || !domEl.isConnected) {
      requestAnimationFrame(() => {
        try {
          const dom2 = editor.view.nodeDOM(selected.pos);
          if (dom2 instanceof HTMLElement && dom2.isConnected && containerRef.current) {
            const r = dom2.getBoundingClientRect();
            const c = containerRef.current.getBoundingClientRect();
            setRect(new DOMRect(r.left - c.left, r.top - c.top, r.width, r.height));
          }
        } catch {
          // ignore
        }
      });
      return;
    }
    const r = domEl.getBoundingClientRect();
    const c = containerRef.current.getBoundingClientRect();
    setRect(new DOMRect(r.left - c.left, r.top - c.top, r.width, r.height));
  }, [selected, containerRef, editor]);

  useLayoutEffect(() => {
    recompute();
  }, [recompute]);

  // Scroll listeners — coleta scrollers ancestrais (igual FigureOverlay P12)
  useEffect(() => {
    if (!selected || !containerRef.current) return undefined;
    const el = containerRef.current;
    const onScroll = () => recompute();
    const scrollers: HTMLElement[] = [];
    let cur: HTMLElement | null = selected.domEl.parentElement;
    while (cur && cur !== document.body) {
      const cs = window.getComputedStyle(cur);
      if (
        cs.overflowY === "auto" ||
        cs.overflowY === "scroll" ||
        cs.overflowX === "auto" ||
        cs.overflowX === "scroll"
      ) {
        scrollers.push(cur);
      }
      cur = cur.parentElement;
    }
    for (const sc of scrollers) {
      sc.addEventListener("scroll", onScroll, { passive: true });
    }
    el.addEventListener("scroll", onScroll, { passive: true });
    window.addEventListener("resize", onScroll);
    window.addEventListener("scroll", onScroll, { passive: true });
    const ro = new ResizeObserver(onScroll);
    ro.observe(selected.domEl);
    ro.observe(el);
    return () => {
      for (const sc of scrollers) {
        sc.removeEventListener("scroll", onScroll);
      }
      el.removeEventListener("scroll", onScroll);
      window.removeEventListener("resize", onScroll);
      window.removeEventListener("scroll", onScroll);
      ro.disconnect();
    };
  }, [selected, containerRef, recompute]);

  // Update atômico de attrs + re-afirma seleção (mesma técnica P20).
  const updateShapeAttrs = useCallback(
    (patch: Record<string, unknown>) => {
      if (!editor || !selected) return;
      const { state } = editor;
      const sel = state.selection;
      if (!(sel instanceof NodeSelection)) return;
      const pos = sel.from;
      const node = sel.node;
      if (node.type.name !== "shape") return;
      const tr = state.tr.setNodeMarkup(pos, undefined, {
        ...node.attrs,
        ...patch,
      });
      tr.setSelection(NodeSelection.create(tr.doc, pos));
      editor.view.dispatch(tr);
    },
    [editor, selected],
  );

  const deleteShape = useCallback(() => {
    if (!editor) return;
    editor.chain().focus().deleteSelection().run();
  }, [editor]);

  // ---- Drag-to-reposition ----
  const handleDragMouseDown = useCallback(
    (e: React.MouseEvent) => {
      if (!editor || !selected) return;
      if (e.button !== 0) return;
      const wrapMode =
        (selected.attrs.wrap_mode as ShapeWrapMode | null) ?? "in_front";
      if (wrapMode === "inline") return;
      e.preventDefault();
      e.stopPropagation();

      const startX = e.clientX;
      const startY = e.clientY;
      const startWrapX = Number(selected.attrs.wrap_x_cm) || 0;
      const startWrapY = Number(selected.attrs.wrap_y_cm) || 0;
      const shapeEl = selected.domEl;
      const shapeRect = shapeEl.getBoundingClientRect();
      const layoutH = shapeEl.offsetHeight || 1;
      const zoom = shapeRect.height / Math.max(1, layoutH);
      let hasMoved = false;
      let latestX = startWrapX;
      let latestY = startWrapY;

      const onMove = (ev: MouseEvent) => {
        const dx = ev.clientX - startX;
        const dy = ev.clientY - startY;
        if (!hasMoved && Math.abs(dx) + Math.abs(dy) < 5) return;
        hasMoved = true;
        ev.preventDefault();
        const dxLayout = dx / Math.max(0.0001, zoom);
        const dyLayout = dy / Math.max(0.0001, zoom);
        const newX = startWrapX + dxLayout / PX_PER_CM;
        const newY = startWrapY + dyLayout / PX_PER_CM;
        shapeEl.style.left = `${newX.toFixed(2)}cm`;
        shapeEl.style.top = `${newY.toFixed(2)}cm`;
        latestX = newX;
        latestY = newY;
        setDragTooltip({
          x: ev.clientX,
          y: ev.clientY,
          label: `${newX.toFixed(2)}cm, ${newY.toFixed(2)}cm`,
        });
        recompute();
      };

      const onUp = () => {
        window.removeEventListener("mousemove", onMove);
        window.removeEventListener("mouseup", onUp);
        if (hasMoved) {
          setDragTooltip(null);
          updateShapeAttrs({
            wrap_x_cm: Number(latestX.toFixed(2)),
            wrap_y_cm: Number(latestY.toFixed(2)),
          });
        }
      };

      window.addEventListener("mousemove", onMove);
      window.addEventListener("mouseup", onUp);
    },
    [editor, selected, updateShapeAttrs, recompute],
  );

  // ---- Resize via cantos ----
  const handleResizeMouseDown = useCallback(
    (dir: ResizeDir) => (e: React.MouseEvent) => {
      if (!editor || !selected) return;
      if (e.button !== 0) return;
      e.preventDefault();
      e.stopPropagation();

      const startX = e.clientX;
      const startY = e.clientY;
      const startW = Number(selected.attrs.width_cm) || 1;
      const startH = Number(selected.attrs.height_cm) || 1;
      const startWrapX = Number(selected.attrs.wrap_x_cm) || 0;
      const startWrapY = Number(selected.attrs.wrap_y_cm) || 0;
      const shapeEl = selected.domEl;
      const shapeRect = shapeEl.getBoundingClientRect();
      const layoutH = shapeEl.offsetHeight || 1;
      const zoom = shapeRect.height / Math.max(1, layoutH);

      let latestW = startW;
      let latestH = startH;
      let latestX = startWrapX;
      let latestY = startWrapY;

      const onMove = (ev: MouseEvent) => {
        const dx = (ev.clientX - startX) / Math.max(0.0001, zoom);
        const dy = (ev.clientY - startY) / Math.max(0.0001, zoom);
        const dxCm = dx / PX_PER_CM;
        const dyCm = dy / PX_PER_CM;
        let newW = startW;
        let newH = startH;
        let newX = startWrapX;
        let newY = startWrapY;
        if (dir === "se") {
          newW = Math.max(MIN_WIDTH_CM, startW + dxCm);
          newH = Math.max(MIN_HEIGHT_CM, startH + dyCm);
        } else if (dir === "sw") {
          newW = Math.max(MIN_WIDTH_CM, startW - dxCm);
          newH = Math.max(MIN_HEIGHT_CM, startH + dyCm);
          newX = startWrapX + (startW - newW);
        } else if (dir === "ne") {
          newW = Math.max(MIN_WIDTH_CM, startW + dxCm);
          newH = Math.max(MIN_HEIGHT_CM, startH - dyCm);
          newY = startWrapY + (startH - newH);
        } else if (dir === "nw") {
          newW = Math.max(MIN_WIDTH_CM, startW - dxCm);
          newH = Math.max(MIN_HEIGHT_CM, startH - dyCm);
          newX = startWrapX + (startW - newW);
          newY = startWrapY + (startH - newH);
        }
        latestW = newW;
        latestH = newH;
        latestX = newX;
        latestY = newY;
        shapeEl.style.width = `${newW.toFixed(2)}cm`;
        shapeEl.style.height = `${newH.toFixed(2)}cm`;
        shapeEl.style.left = `${newX.toFixed(2)}cm`;
        shapeEl.style.top = `${newY.toFixed(2)}cm`;
        setDragTooltip({
          x: ev.clientX,
          y: ev.clientY,
          label: `${newW.toFixed(1)} × ${newH.toFixed(1)} cm`,
        });
        recompute();
      };

      const onUp = () => {
        window.removeEventListener("mousemove", onMove);
        window.removeEventListener("mouseup", onUp);
        setDragTooltip(null);
        updateShapeAttrs({
          width_cm: Number(latestW.toFixed(2)),
          height_cm: Number(latestH.toFixed(2)),
          wrap_x_cm: Number(latestX.toFixed(2)),
          wrap_y_cm: Number(latestY.toFixed(2)),
        });
      };

      window.addEventListener("mousemove", onMove);
      window.addEventListener("mouseup", onUp);
    },
    [editor, selected, updateShapeAttrs, recompute],
  );

  // ---- Cálculos derivados pra UI ----
  const effectiveKind =
    ((selected?.attrs.kind as ShapeKind | null) ?? "rectangle");
  const effectiveWrapMode =
    ((selected?.attrs.wrap_mode as ShapeWrapMode | null) ?? "in_front");
  const effectiveStrokeColor =
    ((selected?.attrs.stroke_color as string | null) ?? "#d92626");
  const effectiveStrokeWidth = Number(selected?.attrs.stroke_width ?? 3);
  const effectiveFillColor =
    ((selected?.attrs.fill_color as string | null) ?? "rgba(255,255,255,0)");
  const hasFill = effectiveKind === "rectangle" || effectiveKind === "ellipse";

  if (!selected || !rect) return null;

  const HSIZE = 10;
  const half = HSIZE / 2;
  const handlePos: Record<ResizeDir, { left: number; top: number }> = {
    nw: { left: -half, top: -half },
    ne: { left: rect.width - half, top: -half },
    se: { left: rect.width - half, top: rect.height - half },
    sw: { left: -half, top: rect.height - half },
  };
  const cursorOf: Record<ResizeDir, string> = {
    nw: "nwse-resize",
    ne: "nesw-resize",
    se: "nwse-resize",
    sw: "nesw-resize",
  };

  return (
    <>
      <div
        className={styles.overlay}
        style={{
          left: `${rect.left}px`,
          top: `${rect.top}px`,
          width: `${rect.width}px`,
          height: `${rect.height}px`,
        }}
        onMouseDown={(e) => e.stopPropagation()}
      >
        {/* Toolbar acima */}
        <div
          className={styles.toolbar}
          style={{ top: -42, left: rect.width / 2, transform: "translateX(-50%)" }}
          onMouseDown={(e) => e.stopPropagation()}
        >
          <button
            type="button"
            className={`${styles.toolBtn} ${effectiveWrapMode === "inline" ? styles.toolBtnActive : ""}`}
            onClick={() => updateShapeAttrs({ wrap_mode: "inline" })}
            title="Alinhado ao texto"
          >
            <WrapText size={14} />
          </button>
          <button
            type="button"
            className={`${styles.toolBtn} ${effectiveWrapMode === "in_front" ? styles.toolBtnActive : ""}`}
            onClick={() => updateShapeAttrs({ wrap_mode: "in_front" })}
            title="Em frente ao texto"
          >
            <BringToFront size={14} />
          </button>
          <button
            type="button"
            className={`${styles.toolBtn} ${effectiveWrapMode === "behind" ? styles.toolBtnActive : ""}`}
            onClick={() => updateShapeAttrs({ wrap_mode: "behind" })}
            title="Atrás do texto"
          >
            <SendToBack size={14} />
          </button>
          <div className={styles.toolDivider} />
          {/* Cor da borda */}
          <span className={styles.labelSmall}>Borda</span>
          <label
            className={styles.colorSwatch}
            style={{ background: effectiveStrokeColor }}
            title="Cor da borda"
          >
            <input
              type="color"
              value={normalizeColor(effectiveStrokeColor)}
              onChange={(e) =>
                updateShapeAttrs({ stroke_color: e.target.value })
              }
            />
          </label>
          {/* Espessura */}
          <input
            type="number"
            min="0.5"
            max="20"
            step="0.5"
            className={styles.strokeWidthInput}
            value={effectiveStrokeWidth}
            onChange={(e) =>
              updateShapeAttrs({
                stroke_width: Number(e.target.value) || 1,
              })
            }
            title="Espessura da borda (mm)"
          />
          {hasFill && (
            <>
              <div className={styles.toolDivider} />
              <span className={styles.labelSmall}>Preench.</span>
              <label
                className={styles.colorSwatch}
                style={{ background: effectiveFillColor }}
                title="Cor de preenchimento (clique pra escolher)"
              >
                <input
                  type="color"
                  value={normalizeColor(effectiveFillColor)}
                  onChange={(e) =>
                    updateShapeAttrs({ fill_color: e.target.value })
                  }
                />
              </label>
              <button
                type="button"
                className={styles.toolBtn}
                onClick={() =>
                  updateShapeAttrs({
                    fill_color: "rgba(255,255,255,0)",
                  })
                }
                title="Sem preenchimento (transparente)"
                style={{ fontSize: 9 }}
              >
                ∅
              </button>
            </>
          )}
          <div className={styles.toolDivider} />
          <button
            type="button"
            className={`${styles.toolBtn} ${styles.toolBtnDanger}`}
            onClick={deleteShape}
            title="Excluir (Delete)"
          >
            <Trash2 size={14} />
          </button>
        </div>

        {/* Borda de seleção */}
        <div className={styles.selectionFrame} aria-hidden />

        {/* DragZone (modo flutuante) */}
        {effectiveWrapMode !== "inline" && (
          <div
            className={styles.dragZone}
            onMouseDown={handleDragMouseDown}
            aria-hidden
          />
        )}

        {/* 4 handles de canto pra resize */}
        {(Object.keys(handlePos) as ResizeDir[]).map((dir) => (
          <div
            key={dir}
            className={styles.handle}
            style={{
              left: `${handlePos[dir].left}px`,
              top: `${handlePos[dir].top}px`,
              width: `${HSIZE}px`,
              height: `${HSIZE}px`,
              cursor: cursorOf[dir],
            }}
            onMouseDown={handleResizeMouseDown(dir)}
          />
        ))}
      </div>

      {dragTooltip && (
        <div
          className={styles.dragTooltip}
          style={{
            left: `${dragTooltip.x + 14}px`,
            top: `${dragTooltip.y + 14}px`,
          }}
        >
          {dragTooltip.label}
        </div>
      )}
    </>
  );
}

/** Normaliza cor pra formato aceito por `<input type="color">` (#RRGGBB).
 *  Cores transparentes/rgba ficam como "#ffffff" (branco placeholder). */
function normalizeColor(color: string): string {
  if (!color) return "#000000";
  if (color.startsWith("#") && color.length === 7) return color;
  if (color.startsWith("#") && color.length === 4) {
    // #RGB → #RRGGBB
    const r = color[1];
    const g = color[2];
    const b = color[3];
    return `#${r}${r}${g}${g}${b}${b}`;
  }
  // rgba/rgb/named — fallback simples
  return "#000000";
}
