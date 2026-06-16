/**
 * HorizontalRuler — régua estilo Word desenhada em SVG.
 *
 * F7.1: handles agora DRAGÁVEIS. Quando o pai passa
 * `onLeftMarginChange` / `onRightMarginChange`, o usuário pode arrastar o
 * triângulo dourado pra alterar a margem em tempo real (mouse move +
 * mouse up commit). Sem callbacks, a régua se comporta como antes —
 * apenas indicação visual.
 */

import { useEffect, useRef, useState } from "react";
import { createPortal } from "react-dom";
import styles from "./Ruler.module.css";

export const PX_PER_CM = 96 / 2.54;
export const RULER_THICKNESS = 22;

/**
 * Passo de snap padrão das réguas. Bate com a densidade visual de ticks
 * (subdivisões a cada 0.25 cm). Quando o usuário arrasta com SÓ o botão
 * esquerdo, o valor é arredondado pra esse incremento. Segurando o botão
 * DIREITO em paralelo, o snap é desligado e o handle anda livre.
 */
export const SNAP_STEP_CM = 0.25;

/** Snap arredondando pra múltiplo mais próximo de `step`. */
export function snapCmIf(cm: number, snap: boolean, step = SNAP_STEP_CM): number {
  return snap ? Math.round(cm / step) * step : cm;
}

interface HorizontalRulerProps {
  widthCm: number;
  leftMarginCm: number;
  rightMarginCm: number;
  /** F7.1 — Drag em tempo real. cm já clamped (0..8) pelo handler. */
  onLeftMarginChange?: (cm: number) => void;
  onRightMarginChange?: (cm: number) => void;
  /** R — Recuo da primeira linha do parágrafo onde o cursor está. cm
   *  relativo à margem esquerda (positivo = recuado pra dentro). */
  firstLineIndentCm?: number;
  onFirstLineIndentChange?: (cm: number) => void;
}

export function HorizontalRuler({
  widthCm,
  leftMarginCm,
  rightMarginCm,
  onLeftMarginChange,
  onRightMarginChange,
  firstLineIndentCm = 0,
  onFirstLineIndentChange,
}: HorizontalRulerProps) {
  const widthPx = widthCm * PX_PER_CM;
  const svgRef = useRef<SVGSVGElement>(null);

  // Overlay visual (linha tracejada VERTICAL) durante o drag das margens
  // esquerda/direita. Mesma estratégia do indent: o triângulo fica parado
  // no lugar durante o drag, e uma linha tracejada via portal segue a
  // posição projetada onde a margem vai cair. Some no mouseup.
  // Cor amber (#fbbf24) pra combinar com a cor do MarginHandle.
  const [marginOverlay, setMarginOverlay] = useState<{
    side: "left" | "right";
    screenX: number;
    topPx: number;
  } | null>(null);
  // R — Drag state separado pro handle do recuo da primeira linha.
  const [indentDragPreview, setIndentDragPreview] = useState<number | null>(
    null,
  );
  // Overlay visual (linha tracejada vertical) durante o drag do recuo.
  // O triângulo do handle por design fica parado no lugar até o
  // mouseup (anti-flicker via useEffect M7). Pra dar feedback de
  // "onde vai cair", desenhamos uma linha tracejada ao longo da página
  // que segue a posição projetada do indent. Coordenadas em screen-px
  // pra que `position: fixed` funcione corretamente sob CSS transform
  // (zoom da página). Renderizada via portal pra não ser cortada pelo
  // SVG da régua.
  const [indentOverlay, setIndentOverlay] = useState<{
    screenX: number;
    topPx: number;
  } | null>(null);

  // Estilo Word: subdivisões a cada 0.25 cm com 3 tamanhos
  //   - tick maior (12px)   → todo número inteiro (1, 2, 3, …)  e leva label
  //   - tick médio (7px)    → metades (.5)
  //   - tick pequeno (3px)  → quartos (.25 e .75)
  const ticks: number[] = [];
  for (let q = 0; q <= widthCm * 4; q++) {
    ticks.push(q / 4);
  }

  // Posições (em px) — sempre usam as props. O handle fica parado durante
  // o drag (o feedback visual vem pela linha tracejada do `marginOverlay`).
  // Após o mouseup, a prop atualiza via store e o handle "salta" pra
  // posição final — mesmo padrão do handle do recuo (indent).
  const leftPx = leftMarginCm * PX_PER_CM;
  const rightPxFromRight = rightMarginCm * PX_PER_CM;
  const rightMarginPx = widthPx - rightPxFromRight;
  const usableWidthPx = Math.max(0, rightMarginPx - leftPx);

  const startDrag = (
    e: React.MouseEvent<SVGElement>,
    side: "left" | "right",
  ) => {
    if (side === "left" && !onLeftMarginChange) return;
    if (side === "right" && !onRightMarginChange) return;
    e.preventDefault();
    e.stopPropagation();

    const svg = svgRef.current;
    if (!svg) return;
    const rect = svg.getBoundingClientRect();
    const widthVisualPx = rect.width;
    if (widthVisualPx <= 0) return;

    // M5 — Trabalha INTEIRAMENTE em centímetros para evitar a mistura de
    // sistemas de coordenadas. Antes o cálculo usava:
    //   - cursorXAtStart = clientX - rect.left  (px VISUAL, com zoom)
    //   - handleXAtStart = leftMarginCm * PX_PER_CM  (px INTERNO, sem zoom)
    //   - offsetPx = visual - internal  (mistura!)
    // Em zoom != 1 isso fazia o handle pular no mousedown porque
    // o offset capturava o erro de proporção (visual/internal). A correção
    // F11.4 minimizava o salto mas não eliminava a raiz. Agora normalizo
    // tudo em cm, que é unit-consistent independente de zoom.
    const cursorCm = (cursorX: number): number =>
      ((cursorX - rect.left) / widthVisualPx) * widthCm;

    // cm (na convenção lateral correspondente: distância da borda esquerda
    // pra "left", ou da borda direita pra "right") → screen X. Usa as
    // dimensões visuais do svgRect, então respeita CSS transform (zoom).
    const marginToScreenX = (cm: number, s: "left" | "right"): number => {
      const absCmFromLeft = s === "left" ? cm : widthCm - cm;
      return rect.left + (absCmFromLeft / widthCm) * rect.width;
    };

    const cursorCmAtStart = cursorCm(e.clientX);
    const handleCmAtStart =
      side === "left" ? leftMarginCm : widthCm - rightMarginCm;
    const offsetCm = cursorCmAtStart - handleCmAtStart;

    const compute = (clientX: number): number => {
      const newHandlePosCm = cursorCm(clientX) - offsetCm;
      if (side === "left") {
        // Distância em cm do canto esquerdo da página.
        return clamp(newHandlePosCm, 0, 8);
      } else {
        // Distância em cm do canto DIREITO da página.
        return clamp(widthCm - newHandlePosCm, 0, 8);
      }
    };

    // Linha tracejada já visível no mousedown (na posição atual do handle).
    const currentCm = side === "left" ? leftMarginCm : rightMarginCm;
    setMarginOverlay({
      side,
      screenX: marginToScreenX(currentCm, side),
      topPx: rect.bottom,
    });

    // Snap: ativo por padrão. Desliga quando o botão DIREITO está
    // simultaneamente pressionado (mousedown left + right = drag livre).
    // `lastButtons` guarda o bitmask de botões da última posição do mouse
    // pra usar consistentemente entre onMove e onUp.
    let didMove = false;
    let lastButtons = e.buttons;

    const onMove = (ev: MouseEvent) => {
      didMove = true;
      lastButtons = ev.buttons;
      const raw = compute(ev.clientX);
      // bit 2 = botão direito pressionado → snap OFF
      const next = snapCmIf(raw, (lastButtons & 2) === 0);
      // Atualiza a linha tracejada pra refletir onde a margem vai cair.
      setMarginOverlay({
        side,
        screenX: marginToScreenX(next, side),
        topPx: rect.bottom,
      });
    };
    const onUp = (ev: MouseEvent) => {
      // Só responde ao release do botão ESQUERDO (button=0). Release do
      // direito enquanto o esquerdo segue pressionado não encerra o drag —
      // só altera o estado de snap.
      if (ev.button !== 0) return;
      // Só commita se o user realmente moveu o mouse (não foi click puro).
      if (didMove) {
        const raw = compute(ev.clientX);
        const final = snapCmIf(raw, (lastButtons & 2) === 0);
        if (side === "left") onLeftMarginChange?.(final);
        else onRightMarginChange?.(final);
      }
      // A linha tracejada some sempre no release — mesmo padrão do indent.
      setMarginOverlay(null);
      window.removeEventListener("mousemove", onMove);
      window.removeEventListener("mouseup", onUp);
      window.removeEventListener("contextmenu", onContextMenu);
    };
    // Suprime o context-menu do sistema durante o drag pra que o botão
    // direito sirva só como modificador "snap off".
    const onContextMenu = (ev: MouseEvent) => ev.preventDefault();
    window.addEventListener("mousemove", onMove);
    window.addEventListener("mouseup", onUp);
    window.addEventListener("contextmenu", onContextMenu);
  };

  // Cursor + cleanup
  useEffect(() => {
    if (!marginOverlay && indentDragPreview === null) return;
    const prev = document.body.style.cursor;
    document.body.style.cursor = "ew-resize";
    return () => {
      document.body.style.cursor = prev;
    };
  }, [marginOverlay, indentDragPreview]);

  // R — Sincroniza indentDragPreview com a prop (igual margens M7).
  useEffect(() => {
    if (indentDragPreview === null) return;
    if (Math.abs(firstLineIndentCm - indentDragPreview) < 0.005) {
      setIndentDragPreview(null);
    } else {
      setIndentDragPreview(firstLineIndentCm);
    }
  }, [firstLineIndentCm, indentDragPreview]);

  // R — Drag handler do recuo da primeira linha. Diferente das margens:
  // a posição é relativa à margem esquerda (não absoluta na régua), e
  // permite valores negativos (recuo deslocado pra esquerda, "hanging").
  //
  // Aplica o mesmo padrão dos margin handles (M5+M7+F11.4):
  //   - Trabalha em CM, não em px (evita mistura visual/internal em zoom != 1)
  //   - Captura o offset entre cursor e handle no mousedown (preserva grab point)
  //   - didMove flag — clique sem arrastar não altera o valor
  //   - Commita SÓ no mouseup (não a cada mousemove)
  //   - Não seta dragPreview no mousedown
  //   - Mantém preview até prop alinhar (anti-flicker, useEffect cuida disso)
  const startIndentDrag = (e: React.MouseEvent<SVGElement>) => {
    if (!onFirstLineIndentChange) return;
    e.preventDefault();
    e.stopPropagation();

    const svg = svgRef.current;
    if (!svg) return;
    const rect = svg.getBoundingClientRect();
    const widthVisualPx = rect.width;
    if (widthVisualPx <= 0) return;

    // Cursor em cm (consistente com leftMarginCm/firstLineIndentCm) —
    // converte clientX bruto pra cm na escala da página, independente de zoom.
    const cursorCm = (cursorX: number): number =>
      ((cursorX - rect.left) / widthVisualPx) * widthCm;

    // indent (cm) → screen X (px absolutos) — usa as mesmas dimensões
    // visuais do svgRect, então funciona corretamente sob CSS transform
    // (zoom da página).
    const indentToScreenX = (indentCm: number): number =>
      rect.left + ((leftMarginCm + indentCm) / widthCm) * rect.width;

    // Posição absoluta do handle na régua em cm = margem esquerda + indent atual.
    const cursorCmAtStart = cursorCm(e.clientX);
    const handleAbsCmAtStart = leftMarginCm + firstLineIndentCm;
    const offsetCm = cursorCmAtStart - handleAbsCmAtStart;

    const compute = (clientX: number): number => {
      const newHandleAbsCm = cursorCm(clientX) - offsetCm;
      // Convert back to indent (relativo à margem esquerda). Permite hanging
      // até -2cm e recuo até +8cm — mesma faixa do código original.
      return clamp(newHandleAbsCm - leftMarginCm, -2, 8);
    };

    // Linha tracejada já visível no primeiro clique, na posição atual
    // do handle (ainda não moveu, mas já dá feedback de "drag iniciado").
    setIndentOverlay({
      screenX: indentToScreenX(firstLineIndentCm),
      topPx: rect.bottom,
    });

    // Snap: ON por padrão (left-only), OFF se right button também
    // pressionado. Mesmo padrão das margens.
    let didMove = false;
    let lastButtons = e.buttons;

    const onMove = (ev: MouseEvent) => {
      didMove = true;
      lastButtons = ev.buttons;
      const raw = compute(ev.clientX);
      const next = snapCmIf(raw, (lastButtons & 2) === 0);
      setIndentDragPreview(next);
      // Atualiza a linha tracejada pra refletir onde o indent vai cair
      // (já considerando clamping da faixa [-2, 8] cm).
      setIndentOverlay({
        screenX: indentToScreenX(next),
        topPx: rect.bottom,
      });
    };
    const onUp = (ev: MouseEvent) => {
      if (ev.button !== 0) return;
      if (didMove) {
        const raw = compute(ev.clientX);
        const final = snapCmIf(raw, (lastButtons & 2) === 0);
        onFirstLineIndentChange(final);
        // M7 — segura o preview no valor final até a prop chegar do store.
        // O useEffect abaixo limpa quando prop e preview convergirem.
        setIndentDragPreview(final);
      } else {
        // Clique puro: nenhuma mudança, limpa imediato.
        setIndentDragPreview(null);
      }
      // A linha tracejada some sempre no release — feature pedida.
      setIndentOverlay(null);
      window.removeEventListener("mousemove", onMove);
      window.removeEventListener("mouseup", onUp);
      window.removeEventListener("contextmenu", onContextMenu);
    };
    const onContextMenu = (ev: MouseEvent) => ev.preventDefault();
    window.addEventListener("mousemove", onMove);
    window.addEventListener("mouseup", onUp);
    window.addEventListener("contextmenu", onContextMenu);
  };

  // Nota: o antigo useEffect M7 (que sincronizava `dragPreview` com a
  // prop pós-commit pra evitar flicker) foi removido — não há mais
  // `dragPreview` afetando a posição do handle. O handle sempre lê
  // direto da prop, então flicker é impossível.

  const draggableLeft = !!onLeftMarginChange;
  const draggableRight = !!onRightMarginChange;

  return (
    <>
    <svg
      ref={svgRef}
      role="presentation"
      className={styles.horizontal}
      width={widthPx}
      height={RULER_THICKNESS}
      viewBox={`0 0 ${widthPx} ${RULER_THICKNESS}`}
    >
      <rect
        x={0}
        y={0}
        width={widthPx}
        height={RULER_THICKNESS}
        className={styles.bg}
      />
      <rect
        x={leftPx}
        y={0}
        width={usableWidthPx}
        height={RULER_THICKNESS}
        className={styles.usable}
      />
      {ticks.map((t) => {
        const x = t * PX_PER_CM;
        const isInt = Number.isInteger(t);
        const isHalf = !isInt && (t * 2) % 1 === 0;
        // 12 / 7 / 3 px — matches Word's ruler density
        const tickH = isInt ? 12 : isHalf ? 7 : 3;
        return (
          <line
            key={t}
            x1={x}
            x2={x}
            y1={RULER_THICKNESS - tickH}
            y2={RULER_THICKNESS}
            className={styles.tick}
          />
        );
      })}
      {Array.from({ length: Math.floor(widthCm) }, (_, i) => i + 1).map((n) => (
        <text
          key={n}
          x={n * PX_PER_CM}
          y={9}
          className={styles.label}
          textAnchor="middle"
        >
          {n}
        </text>
      ))}
      {/* Left margin handle (top + bottom triangles) */}
      <MarginHandle
        x={leftPx}
        draggable={draggableLeft}
        active={marginOverlay?.side === "left"}
        onMouseDown={(e) => startDrag(e, "left")}
        tooltip={
          draggableLeft
            ? `Margem esquerda: ${(leftPx / PX_PER_CM).toFixed(2)} cm — arraste para ajustar`
            : `Margem esquerda: ${leftMarginCm.toFixed(2)} cm`
        }
      />
      {/* Right margin handle */}
      <MarginHandle
        x={rightMarginPx}
        draggable={draggableRight}
        active={marginOverlay?.side === "right"}
        onMouseDown={(e) => startDrag(e, "right")}
        tooltip={
          draggableRight
            ? `Margem direita: ${(rightPxFromRight / PX_PER_CM).toFixed(2)} cm — arraste para ajustar`
            : `Margem direita: ${rightMarginCm.toFixed(2)} cm`
        }
      />
      {/* R — Handle do recuo da primeira linha (triângulo SUPERIOR
       *  apontando pra baixo). Posição = margem esquerda + indent. */}
      {onFirstLineIndentChange && (() => {
        const effectiveIndent =
          indentDragPreview ?? firstLineIndentCm;
        const indentX = leftPx + effectiveIndent * PX_PER_CM;
        return (
          <IndentHandle
            x={indentX}
            active={indentDragPreview !== null}
            onMouseDown={startIndentDrag}
            tooltip={`Recuo da primeira linha: ${effectiveIndent.toFixed(2)} cm — arraste para ajustar`}
          />
        );
      })()}
    </svg>
    {/* Linha tracejada VERTICAL durante drag de margem esquerda/direita.
     *  Cor amber (#fbbf24) pra combinar com o MarginHandle. Mesma técnica
     *  do indent: portal em `document.body` com `position: fixed`, some
     *  no mouseup. */}
    {marginOverlay !== null &&
      createPortal(
        <div
          aria-hidden
          style={{
            position: "fixed",
            left: `${marginOverlay.screenX}px`,
            top: `${marginOverlay.topPx}px`,
            bottom: 0,
            width: 0,
            borderLeft: "1px dashed #fbbf24",
            pointerEvents: "none",
            zIndex: 9999,
          }}
        />,
        document.body,
      )}
    {/* R — Linha tracejada vertical durante drag do recuo da primeira
     *  linha. O triângulo do handle, por design, fica parado até o
     *  release (anti-flicker via M7), então essa linha é o feedback
     *  visual de "onde vai cair". Renderizada via portal em
     *  `document.body` com `position: fixed` pra não ser cortada pelo
     *  SVG da régua e pra ignorar transforms (zoom) do container do
     *  editor. Some no mouseup. */}
    {indentOverlay !== null &&
      createPortal(
        <div
          aria-hidden
          style={{
            position: "fixed",
            left: `${indentOverlay.screenX}px`,
            top: `${indentOverlay.topPx}px`,
            bottom: 0,
            width: 0,
            borderLeft: "1px dashed #1f6feb",
            pointerEvents: "none",
            zIndex: 9999,
          }}
        />,
        document.body,
      )}
    </>
  );
}

function MarginHandle({
  x,
  draggable,
  active,
  onMouseDown,
  tooltip,
}: {
  x: number;
  draggable: boolean;
  active: boolean;
  onMouseDown: (e: React.MouseEvent<SVGElement>) => void;
  tooltip: string;
}) {
  const size = 6;
  const points = [
    `${x},${RULER_THICKNESS}`,
    `${x - size},${RULER_THICKNESS - size - 1}`,
    `${x + size},${RULER_THICKNESS - size - 1}`,
  ].join(" ");
  return (
    <g>
      {/* Hit area maior para drag confortável */}
      {draggable && (
        <rect
          x={x - 8}
          y={RULER_THICKNESS - 12}
          width={16}
          height={12}
          fill="transparent"
          style={{ cursor: "ew-resize" }}
          onMouseDown={onMouseDown}
        >
          <title>{tooltip}</title>
        </rect>
      )}
      <polygon
        points={points}
        className={`${styles.handle} ${active ? styles.handleActive : ""}`}
        style={draggable ? { cursor: "ew-resize" } : undefined}
        onMouseDown={draggable ? onMouseDown : undefined}
      >
        <title>{tooltip}</title>
      </polygon>
    </g>
  );
}

/**
 * R — IndentHandle: triângulo SUPERIOR (apontando pra baixo) que controla
 * o recuo da primeira linha do parágrafo onde o cursor está. Estilo Word.
 */
function IndentHandle({
  x,
  active,
  onMouseDown,
  tooltip,
}: {
  x: number;
  active: boolean;
  onMouseDown: (e: React.MouseEvent<SVGElement>) => void;
  tooltip: string;
}) {
  const size = 5;
  // Triângulo apontando pra BAIXO, no topo da régua.
  const points = [
    `${x},${size + 1}`, // ponta inferior
    `${x - size},0`, // topo esquerda
    `${x + size},0`, // topo direita
  ].join(" ");
  return (
    <g>
      {/* Hit area maior pra drag confortável */}
      <rect
        x={x - 7}
        y={0}
        width={14}
        height={size + 4}
        fill="transparent"
        style={{ cursor: "ew-resize" }}
        onMouseDown={onMouseDown}
      >
        <title>{tooltip}</title>
      </rect>
      <polygon
        points={points}
        className={`${styles.indentHandle} ${active ? styles.indentHandleActive : ""}`}
        style={{ cursor: "ew-resize" }}
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
