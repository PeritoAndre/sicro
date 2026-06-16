/**
 * W13 — Réguas do canvas (estilo GIMP/Photoshop): barras no TOPO e na
 * ESQUERDA com marcação numérica + um **marcador ao vivo** que segue o mouse
 * e indica exatamente onde ele está NA IMAGEM. Traz precisão para anotações,
 * medições e recortes.
 *
 * - Coordenadas em **px da imagem** (0 = canto superior-esquerdo da imagem) ou,
 *   quando há escala calibrada, na **unidade real** (cm/m/mm).
 * - Overlay SVG absoluto, `pointer-events: none` → não atrapalha o canvas.
 * - O marcador (triângulo na régua + crosshair fino) usa o MESMO `pointer`
 *   (coords-mundo) e `viewport` que a barra de status — então é exato.
 *
 * Substitui o antigo `GridOverlay` (que só tinha rótulos soltos, nunca foi
 * ligado e não seguia o mouse).
 */

import { useMemo } from "react";
import type { SicroImageScale } from "../engine/schema";

export const RULER_SIZE = 18; // px da faixa da régua

interface Props {
  /** Dimensões da imagem original (px). */
  imageWidth: number;
  imageHeight: number;
  /** Transform do Konva: scale + translate (x, y) em px de tela. */
  viewport: { scale: number; x: number; y: number };
  /** Posição do cursor em coordenadas-MUNDO (px da imagem). */
  pointer: { x: number; y: number };
  /** Tamanho da área de canvas (px de tela). */
  width: number;
  height: number;
  /** Escala calibrada (px → unidade real), opcional. */
  scale?: SicroImageScale | null;
  /** Mostra ou não. */
  visible?: boolean;
}

const RULER_BG = "rgba(17, 24, 39, 0.92)";
const TICK = "rgba(148, 163, 184, 0.55)";
const TICK_MAJOR = "rgba(203, 213, 225, 0.85)";
const LABEL = "rgba(203, 213, 225, 0.9)";
const MARKER = "#38bdf8";
const CROSSHAIR = "rgba(56, 189, 248, 0.35)";

/** Passo "redondo" (1/2/5 × 10^k) a partir de um passo aproximado. */
function niceStep(rough: number): number {
  if (!(rough > 0) || !Number.isFinite(rough)) return 1;
  const exp = Math.floor(Math.log10(rough));
  const base = Math.pow(10, exp);
  const f = rough / base;
  const nf = f < 1.5 ? 1 : f < 3 ? 2 : f < 7 ? 5 : 10;
  return nf * base;
}

export function CanvasRulers({
  imageWidth,
  imageHeight,
  viewport,
  pointer,
  width,
  height,
  scale = null,
  visible = true,
}: Props) {
  const s = viewport.scale > 0 ? viewport.scale : 1;
  const calibrated = !!scale && scale.px_per_unit > 0;

  const screenX = (px: number) => viewport.x + px * s;
  const screenY = (py: number) => viewport.y + py * s;
  const worldXAt = (sx: number) => (sx - viewport.x) / s;
  const worldYAt = (sy: number) => (sy - viewport.y) / s;

  // Passo dos ticks: alvo ~64px de tela entre rótulos.
  const stepWorld = useMemo(() => niceStep(64 / s), [s]);

  const fmt = (px: number): string => {
    if (calibrated && scale) {
      const v = px / scale.px_per_unit;
      const dec = Math.abs(v) < 10 ? 2 : Math.abs(v) < 100 ? 1 : 0;
      return v.toFixed(dec);
    }
    return `${Math.round(px)}`;
  };

  // Ticks horizontais (régua do topo) cobrindo a faixa visível.
  const xticks = useMemo(() => {
    const out: { px: number; sx: number }[] = [];
    if (stepWorld <= 0) return out;
    const wStart = worldXAt(RULER_SIZE);
    const wEnd = worldXAt(width);
    let t = Math.ceil(wStart / stepWorld) * stepWorld;
    let guard = 0;
    while (t <= wEnd && guard++ < 4000) {
      out.push({ px: t, sx: screenX(t) });
      t += stepWorld;
    }
    return out;
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [stepWorld, width, viewport.x, viewport.scale]);

  const yticks = useMemo(() => {
    const out: { px: number; sy: number }[] = [];
    if (stepWorld <= 0) return out;
    const wStart = worldYAt(RULER_SIZE);
    const wEnd = worldYAt(height);
    let t = Math.ceil(wStart / stepWorld) * stepWorld;
    let guard = 0;
    while (t <= wEnd && guard++ < 4000) {
      out.push({ px: t, sy: screenY(t) });
      t += stepWorld;
    }
    return out;
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [stepWorld, height, viewport.y, viewport.scale]);

  if (!visible) return null;

  const mx = screenX(pointer.x);
  const my = screenY(pointer.y);
  const overImage =
    pointer.x >= 0 &&
    pointer.x <= imageWidth &&
    pointer.y >= 0 &&
    pointer.y <= imageHeight;
  const markerXOn = mx >= RULER_SIZE && mx <= width;
  const markerYOn = my >= RULER_SIZE && my <= height;

  return (
    <svg
      style={{ position: "absolute", inset: 0, width: "100%", height: "100%", pointerEvents: "none" }}
      aria-hidden="true"
    >
      {/* Crosshair fino seguindo o cursor (precisão visual) */}
      {markerXOn && (
        <line x1={mx} y1={RULER_SIZE} x2={mx} y2={height} stroke={CROSSHAIR} strokeWidth={1} />
      )}
      {markerYOn && (
        <line x1={RULER_SIZE} y1={my} x2={width} y2={my} stroke={CROSSHAIR} strokeWidth={1} />
      )}

      {/* ---- Régua do TOPO ---- */}
      <rect x={0} y={0} width={width} height={RULER_SIZE} fill={RULER_BG} />
      {xticks.map(({ px, sx }) => (
        <g key={`xt${px}`}>
          <line x1={sx} y1={RULER_SIZE - 6} x2={sx} y2={RULER_SIZE} stroke={TICK_MAJOR} strokeWidth={1} />
          <text x={sx + 2} y={9} fontSize={8.5} fill={LABEL} fontFamily="var(--font-mono, monospace)">
            {fmt(px)}
          </text>
          {/* tick menor no meio do passo */}
          <line
            x1={sx + (stepWorld / 2) * s}
            y1={RULER_SIZE - 3}
            x2={sx + (stepWorld / 2) * s}
            y2={RULER_SIZE}
            stroke={TICK}
            strokeWidth={1}
          />
        </g>
      ))}

      {/* ---- Régua da ESQUERDA ---- */}
      <rect x={0} y={0} width={RULER_SIZE} height={height} fill={RULER_BG} />
      {yticks.map(({ px, sy }) => (
        <g key={`yt${px}`}>
          <line x1={RULER_SIZE - 6} y1={sy} x2={RULER_SIZE} y2={sy} stroke={TICK_MAJOR} strokeWidth={1} />
          <text
            x={9}
            y={sy - 2}
            fontSize={8.5}
            fill={LABEL}
            fontFamily="var(--font-mono, monospace)"
            transform={`rotate(-90, 9, ${sy - 2})`}
            textAnchor="end"
          >
            {fmt(px)}
          </text>
          <line
            x1={RULER_SIZE - 3}
            y1={sy + (stepWorld / 2) * s}
            x2={RULER_SIZE}
            y2={sy + (stepWorld / 2) * s}
            stroke={TICK}
            strokeWidth={1}
          />
        </g>
      ))}

      {/* Canto */}
      <rect x={0} y={0} width={RULER_SIZE} height={RULER_SIZE} fill={RULER_BG} />
      <text x={RULER_SIZE / 2} y={12} fontSize={8} fill={LABEL} textAnchor="middle">
        {calibrated && scale ? scale.unit : "px"}
      </text>

      {/* ---- Marcadores ao vivo (triângulos) ---- */}
      {markerXOn && (
        <>
          <path
            d={`M${mx - 4},0 L${mx + 4},0 L${mx},6 Z`}
            fill={MARKER}
          />
          <line x1={mx} y1={0} x2={mx} y2={RULER_SIZE} stroke={MARKER} strokeWidth={1} />
        </>
      )}
      {markerYOn && (
        <>
          <path d={`M0,${my - 4} L0,${my + 4} L6,${my} Z`} fill={MARKER} />
          <line x1={0} y1={my} x2={RULER_SIZE} y2={my} stroke={MARKER} strokeWidth={1} />
        </>
      )}

      {/* Leitura numérica ao vivo perto do canto (precisão exata) */}
      {overImage && (
        <g>
          <rect x={RULER_SIZE + 2} y={RULER_SIZE + 2} width={118} height={16} rx={3} fill="rgba(17,24,39,0.85)" />
          <text x={RULER_SIZE + 8} y={RULER_SIZE + 13} fontSize={9.5} fill={MARKER} fontFamily="var(--font-mono, monospace)">
            {fmt(pointer.x)} , {fmt(pointer.y)} {calibrated && scale ? scale.unit : "px"}
          </text>
        </g>
      )}
    </svg>
  );
}
