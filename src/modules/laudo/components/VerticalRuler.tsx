/**
 * VerticalRuler — régua vertical estilo Word com SEGMENTOS POR PÁGINA.
 *
 * F7.3 — Em vez de uma régua contínua, renderiza N segmentos
 * INDEPENDENTES (uma régua por página). Cada segmento mostra ticks
 * 0–29.7 cm e exibe "Pág N" no centro. Entre segmentos, um gap
 * transparente acompanha o gap visual entre os page cards.
 *
 * Handles dragáveis:
 *   - TOP: triângulo no primeiro segmento, em y = topMarginCm.
 *   - BOTTOM: triângulo no último segmento, em y = pageHeightCm - bottomMarginCm.
 *
 * O cálculo do drag usa proporção `(yPx - segmentTop) / segmentHeight * pageHeightCm`,
 * independente de zoom.
 */

import { useEffect, useRef, useState } from "react";
import { createPortal } from "react-dom";
import styles from "./Ruler.module.css";
import { PX_PER_CM, RULER_THICKNESS, snapCmIf } from "./HorizontalRuler";

interface VerticalRulerProps {
  pageHeightCm: number;
  pageCount: number;
  pageGapCm: number;
  topMarginCm: number;
  bottomMarginCm: number;
  /** Piso da margem superior (cm). A margem do texto não pode subir acima dele —
   *  é a base do cabeçalho, senão o texto começaria em cima do timbre. Default 0. */
  minTopMarginCm?: number;
  onTopMarginChange?: (cm: number) => void;
  onBottomMarginChange?: (cm: number) => void;
}

export function VerticalRuler({
  pageHeightCm,
  pageCount,
  pageGapCm,
  topMarginCm,
  bottomMarginCm,
  minTopMarginCm = 0,
  onTopMarginChange,
  onBottomMarginChange,
}: VerticalRulerProps) {
  // F7.4 — Mantemos a régua em UNITS CM (consistente com o pageStack)
  // para garantir alinhamento perfeito. O SVG interno usa viewBox em px
  // (necessário pra renderizar marks/handles em coords precisas), mas o
  // container outer é dimensionado em cm exatamente como os page cards.
  // M7 — pageHeightPx/pageGapPx removidos depois da refatoração do
  // startDrag pra cm puro. As medidas em px só são usadas dentro do
  // PageRulerSegment (recalculadas lá via PX_PER_CM).
  const totalHeightCm = pageCount * pageHeightCm + (pageCount - 1) * pageGapCm;

  const containerRef = useRef<HTMLDivElement>(null);
  // Overlay visual (linha tracejada HORIZONTAL) durante o drag dos handles
  // top/bottom. Mesmo padrão dos handles da régua horizontal:
  //   - Os triângulos ficam parados no lugar durante o drag (sempre lêem
  //     direto da prop topMarginCm/bottomMarginCm).
  //   - Uma linha tracejada amber via portal mostra onde a margem vai cair.
  //   - Linha some no mouseup, prop atualiza, triângulos "saltam" pra
  //     posição final.
  const [marginOverlay, setMarginOverlay] = useState<{
    side: "top" | "bottom";
    screenY: number;
    leftPx: number;
  } | null>(null);

  // Top y (in stack) onde cada segmento começa, EM CM (consistente com
  // o pageStack que posiciona os pageCards em cm).
  const segmentTopsCm: number[] = [];
  for (let i = 0; i < pageCount; i++) {
    segmentTopsCm.push(i * (pageHeightCm + pageGapCm));
  }

  // Os handles sempre lêem da prop — não há mais preview-movível. O
  // feedback visual durante o drag vem da linha tracejada (marginOverlay).
  const topMarginEffectiveCm = topMarginCm;
  const bottomMarginEffectiveCm = bottomMarginCm;

  const startDrag = (
    e: React.MouseEvent<SVGElement>,
    side: "top" | "bottom",
    pageIndex: number,
  ) => {
    if (side === "top" && !onTopMarginChange) return;
    if (side === "bottom" && !onBottomMarginChange) return;
    e.preventDefault();
    e.stopPropagation();

    const container = containerRef.current;
    if (!container) return;
    const rect = container.getBoundingClientRect();
    const totalVisualPx = rect.height;
    if (totalVisualPx <= 0) return;

    // M7 — Trabalha em CM puro (mesmo padrão do HorizontalRuler pós-fix).
    // O cálculo anterior misturava visual-px (cursor) com internal-px
    // (handle position) escalando manualmente entre os dois. Em zoom != 1
    // ou com pequenos erros de floating-point isso fazia o handle dar
    // micro-saltos. Normalizando tudo em cm o cálculo fica unit-consistent
    // independente de zoom.
    const cursorCm = (cursorY: number): number =>
      ((cursorY - rect.top) / totalVisualPx) *
      (pageCount * pageHeightCm + (pageCount - 1) * pageGapCm);

    const pageTopInStackCm = pageIndex * (pageHeightCm + pageGapCm);
    const handleCmInStack =
      side === "top"
        ? pageTopInStackCm + topMarginCm
        : pageTopInStackCm + pageHeightCm - bottomMarginCm;

    const cursorCmAtStart = cursorCm(e.clientY);
    const offsetCm = cursorCmAtStart - handleCmInStack;

    const compute = (clientY: number): number => {
      const newHandleCmInStack = cursorCm(clientY) - offsetCm;
      if (side === "top") {
        const cm = newHandleCmInStack - pageTopInStackCm;
        // Piso = base do cabeçalho (minTopMarginCm). O teto sobe junto se o
        // cabeçalho passar de 8cm (até 10cm), senão o handle ficaria preso
        // abaixo do próprio piso.
        return clamp(cm, minTopMarginCm, Math.max(8, minTopMarginCm));
      } else {
        const cm = pageTopInStackCm + pageHeightCm - newHandleCmInStack;
        return clamp(cm, 0, 8);
      }
    };

    // cm (margem) → screen Y absoluto. Usa a altura visual do container
    // pra respeitar zoom da página (CSS transform).
    const totalStackCm = pageCount * pageHeightCm + (pageCount - 1) * pageGapCm;
    const marginToScreenY = (cm: number, s: "top" | "bottom"): number => {
      const posCmInStack =
        s === "top"
          ? pageTopInStackCm + cm
          : pageTopInStackCm + pageHeightCm - cm;
      return rect.top + (posCmInStack / totalStackCm) * rect.height;
    };

    // Linha tracejada horizontal já visível no mousedown.
    const currentCm = side === "top" ? topMarginCm : bottomMarginCm;
    setMarginOverlay({
      side,
      screenY: marginToScreenY(currentCm, side),
      leftPx: rect.right, // começa logo à direita da régua vertical
    });

    // Snap padrão (left only) com toggle livre via botão direito —
    // mesmo padrão dos handles horizontais.
    let didMove = false;
    let lastButtons = e.buttons;

    const onMove = (ev: MouseEvent) => {
      didMove = true;
      lastButtons = ev.buttons;
      const raw = compute(ev.clientY);
      const next = snapCmIf(raw, (lastButtons & 2) === 0);
      setMarginOverlay({
        side,
        screenY: marginToScreenY(next, side),
        leftPx: rect.right,
      });
    };
    const onUp = (ev: MouseEvent) => {
      if (ev.button !== 0) return;
      if (didMove) {
        const raw = compute(ev.clientY);
        const final = snapCmIf(raw, (lastButtons & 2) === 0);
        if (side === "top") onTopMarginChange?.(final);
        else onBottomMarginChange?.(final);
      }
      // Linha some sempre no release — mesmo padrão dos outros handles.
      setMarginOverlay(null);
      window.removeEventListener("mousemove", onMove);
      window.removeEventListener("mouseup", onUp);
      window.removeEventListener("contextmenu", onContextMenu);
    };
    const onContextMenu = (ev: MouseEvent) => ev.preventDefault();
    window.addEventListener("mousemove", onMove);
    window.addEventListener("mouseup", onUp);
    window.addEventListener("contextmenu", onContextMenu);
  };

  useEffect(() => {
    if (!marginOverlay) return;
    const prev = document.body.style.cursor;
    document.body.style.cursor = "ns-resize";
    return () => {
      document.body.style.cursor = prev;
    };
  }, [marginOverlay]);

  // Nota: o useEffect M7 antigo (que sincronizava dragPreview com a prop
  // pós-commit pra evitar flicker) foi removido — o handle sempre lê
  // direto da prop, então flicker é impossível.

  const draggableTop = !!onTopMarginChange;
  const draggableBottom = !!onBottomMarginChange;

  return (
    <div
      ref={containerRef}
      className={styles.verticalContainer}
      style={{
        width: `${RULER_THICKNESS}px`,
        // F7.4 — Altura em CM (mesmo unit do pageStack) garante alinhamento
        // pixel-perfect com os page cards. Antes usávamos px calculado via
        // PX_PER_CM, que somava arredondamento sub-pixel.
        height: `${totalHeightCm}cm`,
        flexShrink: 0,
      }}
    >
      {/* Segmento por página — handles TOP e BOTTOM em CADA página */}
      {segmentTopsCm.map((segTopCm, i) => (
        <PageRulerSegment
          key={i}
          topCm={segTopCm}
          pageHeightCm={pageHeightCm}
          pageIndex={i + 1}
          pageIndex0={i}
          showTopHandle={draggableTop}
          topHandleYInSegmentPx={topMarginEffectiveCm * PX_PER_CM}
          showBottomHandle={draggableBottom}
          bottomHandleYInSegmentPx={
            (pageHeightCm - bottomMarginEffectiveCm) * PX_PER_CM
          }
          onStartDrag={startDrag}
          topActive={marginOverlay?.side === "top"}
          bottomActive={marginOverlay?.side === "bottom"}
        />
      ))}
      {/* Linha tracejada HORIZONTAL durante drag dos handles top/bottom.
       *  Mesma técnica das outras réguas: portal em `document.body` com
       *  `position: fixed`, some no mouseup. Cor amber (#fbbf24). */}
      {marginOverlay !== null &&
        createPortal(
          <div
            aria-hidden
            style={{
              position: "fixed",
              top: `${marginOverlay.screenY}px`,
              left: `${marginOverlay.leftPx}px`,
              right: 0,
              height: 0,
              borderTop: "1px dashed #fbbf24",
              pointerEvents: "none",
              zIndex: 9999,
            }}
          />,
          document.body,
        )}
    </div>
  );
}

interface PageRulerSegmentProps {
  topCm: number;
  pageHeightCm: number;
  pageIndex: number;
  pageIndex0: number;
  showTopHandle: boolean;
  topHandleYInSegmentPx: number;
  showBottomHandle: boolean;
  bottomHandleYInSegmentPx: number;
  onStartDrag: (
    e: React.MouseEvent<SVGElement>,
    side: "top" | "bottom",
    pageIndex: number,
  ) => void;
  topActive: boolean;
  bottomActive: boolean;
}

function PageRulerSegment({
  topCm,
  pageHeightCm,
  pageIndex,
  pageIndex0,
  showTopHandle,
  topHandleYInSegmentPx,
  showBottomHandle,
  bottomHandleYInSegmentPx,
  onStartDrag,
  topActive,
  bottomActive,
}: PageRulerSegmentProps) {
  const heightPx = pageHeightCm * PX_PER_CM;
  // Estilo Word: ticks finos a cada 0.25 cm.
  //   - inteiro     → tick maior (12px) + label
  //   - meio        → tick médio (7px)
  //   - quarto      → tick pequeno (3px)
  const ticks: number[] = [];
  for (let q = 0; q <= pageHeightCm * 4; q++) {
    ticks.push(q / 4);
  }
  // Labels em todo cm inteiro (excluindo 0 pra não colidir com a margem).
  const stops: number[] = [];
  for (let n = 1; n <= Math.floor(pageHeightCm); n++) stops.push(n);

  // Posições do triângulo "área útil" — entre topMargin e (pH - bottomMargin)
  const usableTopPx = topHandleYInSegmentPx;
  const usableBottomPx = bottomHandleYInSegmentPx;

  // F7.4 — width/height do container em CM; SVG viewBox em px (escalado
  // automaticamente pelo browser). Garante alinhamento pixel-perfect com
  // os page cards que também usam cm.
  return (
    <svg
      className={styles.segment}
      style={{
        position: "absolute",
        left: 0,
        top: `${topCm}cm`,
        width: `${RULER_THICKNESS}px`,
        height: `${pageHeightCm}cm`,
      }}
      preserveAspectRatio="none"
      viewBox={`0 0 ${RULER_THICKNESS} ${heightPx}`}
    >
      <rect
        x={0}
        y={0}
        width={RULER_THICKNESS}
        height={heightPx}
        className={styles.bg}
      />
      <rect
        x={0}
        y={usableTopPx}
        width={RULER_THICKNESS}
        height={Math.max(0, usableBottomPx - usableTopPx)}
        className={styles.usable}
      />
      {ticks.map((t) => {
        const y = t * PX_PER_CM;
        const isInt = Number.isInteger(t);
        const isHalf = !isInt && (t * 2) % 1 === 0;
        const tickW = isInt ? 12 : isHalf ? 7 : 3;
        return (
          <line
            key={t}
            x1={RULER_THICKNESS - tickW}
            x2={RULER_THICKNESS}
            y1={y}
            y2={y}
            className={styles.tick}
          />
        );
      })}
      {stops.map((n) => (
        <text
          key={n}
          x={9}
          y={n * PX_PER_CM}
          className={styles.label}
          textAnchor="middle"
          transform={`rotate(-90 9 ${n * PX_PER_CM})`}
        >
          {n}
        </text>
      ))}
      {/* Page label no centro do segmento */}
      <text
        x={RULER_THICKNESS / 2}
        y={heightPx / 2}
        className={styles.pageLabel}
        transform={`rotate(-90 ${RULER_THICKNESS / 2} ${heightPx / 2})`}
      >
        Pág {pageIndex}
      </text>
      {/* Top handle — em cada página (marca margem TOP global) */}
      {showTopHandle && (
        <MarginHandle
          y={topHandleYInSegmentPx}
          active={topActive}
          onMouseDown={(e) => onStartDrag(e, "top", pageIndex0)}
          tooltip={`Margem superior (pg ${pageIndex}): ${(topHandleYInSegmentPx / PX_PER_CM).toFixed(2)} cm — arraste`}
        />
      )}
      {/* Bottom handle — em cada página (marca margem BOTTOM global) */}
      {showBottomHandle && (
        <MarginHandle
          y={bottomHandleYInSegmentPx}
          active={bottomActive}
          onMouseDown={(e) => onStartDrag(e, "bottom", pageIndex0)}
          tooltip={`Margem inferior (pg ${pageIndex}): ${(pageHeightCm - bottomHandleYInSegmentPx / PX_PER_CM).toFixed(2)} cm — arraste`}
        />
      )}
    </svg>
  );
}

function MarginHandle({
  y,
  active,
  onMouseDown,
  tooltip,
}: {
  y: number;
  active: boolean;
  onMouseDown: (e: React.MouseEvent<SVGElement>) => void;
  tooltip: string;
}) {
  const size = 6;
  const points = [
    `${RULER_THICKNESS},${y}`,
    `${RULER_THICKNESS - size - 1},${y - size}`,
    `${RULER_THICKNESS - size - 1},${y + size}`,
  ].join(" ");
  return (
    <g>
      <rect
        x={RULER_THICKNESS - 12}
        y={y - 8}
        width={12}
        height={16}
        fill="transparent"
        style={{ cursor: "ns-resize" }}
        onMouseDown={onMouseDown}
      >
        <title>{tooltip}</title>
      </rect>
      <polygon
        points={points}
        className={`${styles.handle} ${active ? styles.handleActive : ""}`}
        style={{ cursor: "ns-resize" }}
        onMouseDown={onMouseDown}
      >
        <title>{tooltip}</title>
      </polygon>
    </g>
  );
}

function clamp(v: number, lo: number, hi: number): number {
  if (v < lo) return lo;
  if (v > hi) return hi;
  return v;
}
