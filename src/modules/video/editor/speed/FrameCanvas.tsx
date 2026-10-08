/**
 * Konva Stage para marcar pontos num frame coletado (PNG). O "mundo" do Stage
 * é o pixel nativo do frame (a mesma base da homografia); o Stage só aplica o
 * viewport {scale,x,y}, logo toWorld(pointer) = (pointer − stage.xy) / scale.
 * Roda: Ctrl/⌘ = zoom no cursor; senão = pan.
 */

import { useEffect, useMemo, useRef, useState } from "react";
import {
  Circle,
  Group,
  Image as KonvaImage,
  Layer,
  Line,
  Stage,
  Text as KonvaText,
} from "react-konva";
import type Konva from "konva";

export interface FramePoint {
  x: number;
  y: number;
}
export interface FrameMarker extends FramePoint {
  label?: string;
  color?: string;
  /** Marca apagada (outros momentos). */
  faint?: boolean;
}

/** Linha de guia em pixel nativo (grade de 1 m, régua, trajetória). */
export interface FrameGuide {
  points: number[];
  color?: string;
  /** Espessura em px de tela. */
  width?: number;
  dash?: boolean;
}

interface Props {
  /** PNG servível (convertFileSrc). Null = sem frame selecionado. */
  src: string | null;
  /** Dimensões nativas (dica/fallback; o natural da imagem prevalece). */
  naturalWidth?: number | null;
  naturalHeight?: number | null;
  /** Marcadores (crosshair) em pixel nativo. */
  markers?: FrameMarker[];
  /** Polilinha opcional (segmento da calibração 'line' ou quad 'plane'). */
  polyline?: FramePoint[];
  /** Fecha a polilinha (quadrilátero do 'plane'). */
  closed?: boolean;
  /** Adiciona um ponto (px,py em pixel nativo). Ausente = somente leitura. */
  onAddPoint?: (x: number, y: number) => void;
  /** Altura de fallback (px) — só usada se o contêiner não tiver altura medível. */
  height?: number;
  /** Desabilita a captura de cliques (sem travar zoom/pan). */
  disabled?: boolean;
  /** Linhas de guia por cima do quadro (recortadas na imagem). */
  guides?: FrameGuide[];
  /** O que clicar agora, num selo sobre o quadro. */
  hint?: string | null;
  /** Lupa 3× ao lado do cursor. */
  lupa?: boolean;
}

const ZOOM_MIN = 0.1;
const ZOOM_MAX = 16;

export function FrameCanvas({
  src,
  naturalWidth,
  naturalHeight,
  markers = [],
  polyline,
  closed = false,
  onAddPoint,
  height = 460,
  disabled = false,
  guides = [],
  hint = null,
  lupa = false,
}: Props) {
  const lupaRef = useRef<HTMLCanvasElement>(null);
  const [hover, setHover] = useState<{ sx: number; sy: number; x: number; y: number } | null>(null);
  const wrapRef = useRef<HTMLDivElement>(null);
  const stageRef = useRef<Konva.Stage | null>(null);
  const [size, setSize] = useState({ w: 640, h: height });
  const [image, setImage] = useState<HTMLImageElement | null>(null);
  const [viewport, setViewport] = useState({ scale: 1, x: 0, y: 0 });

  // Mede o contêiner nos dois eixos; `height` é só fallback para o 1º paint.
  useEffect(() => {
    const el = wrapRef.current;
    if (!el) return;
    const update = () =>
      setSize({
        w: el.clientWidth || 640,
        h: el.clientHeight || height || 460,
      });
    update();
    const ro = new ResizeObserver(update);
    ro.observe(el);
    return () => ro.disconnect();
  }, [height]);

  useEffect(() => {
    if (!src) {
      setImage(null);
      return;
    }
    const img = new window.Image();
    img.crossOrigin = "anonymous";
    img.onload = () => setImage(img);
    img.onerror = () => setImage(null);
    img.src = src;
    return () => {
      img.onload = null;
      img.onerror = null;
    };
  }, [src]);

  const natW = image?.naturalWidth || naturalWidth || 1280;
  const natH = image?.naturalHeight || naturalHeight || 720;

  // Ajusta ao contêiner a cada imagem/tamanho novo: cada frame começa na vista inteira.
  useEffect(() => {
    if (!image) return;
    const raw = Math.min(size.w / natW, size.h / natH);
    const scale = Number.isFinite(raw) && raw > 0 ? raw : 1;
    setViewport({
      scale,
      x: (size.w - natW * scale) / 2,
      y: (size.h - natH * scale) / 2,
    });
  }, [image, size.w, size.h, natW, natH]);

  const handleWheel = (e: Konva.KonvaEventObject<WheelEvent>) => {
    e.evt.preventDefault();
    const stage = stageRef.current;
    if (!stage) return;
    const pointer = stage.getPointerPosition();
    if (!pointer) return;
    const isPinch = e.evt.ctrlKey || e.evt.metaKey;
    if (isPinch) {
      const old = viewport.scale;
      const factor = e.evt.deltaY > 0 ? 0.9 : 1.1;
      const next = clamp(old * factor, ZOOM_MIN, ZOOM_MAX);
      const wx = (pointer.x - viewport.x) / old;
      const wy = (pointer.y - viewport.y) / old;
      setViewport({ scale: next, x: pointer.x - wx * next, y: pointer.y - wy * next });
    } else {
      setViewport((v) => ({ ...v, x: v.x - e.evt.deltaX, y: v.y - e.evt.deltaY }));
    }
  };

  const handleClick = () => {
    if (disabled || !onAddPoint) return;
    const stage = stageRef.current;
    if (!stage) return;
    const pos = stage.getPointerPosition();
    if (!pos) return;
    const x = (pos.x - viewport.x) / viewport.scale;
    const y = (pos.y - viewport.y) / viewport.scale;
    // Ignora cliques fora da imagem (margem cinza do canvas).
    if (x < 0 || y < 0 || x > natW || y > natH) return;
    onAddPoint(round2(x), round2(y));
  };

  const inv = 1 / Math.max(viewport.scale, 0.0001);
  const flatPolyline = useMemo(() => {
    if (!polyline || polyline.length === 0) return null;
    const f: number[] = [];
    for (const p of polyline) f.push(p.x, p.y);
    return f;
  }, [polyline]);

  const interactive = !!onAddPoint && !disabled;

  // Lupa: recorte do quadro original, ampliado, desenhado num canvas HTML ao lado do cursor.
  useEffect(() => {
    const c = lupaRef.current;
    if (!c || !hover || !image) return;
    const g = c.getContext("2d");
    if (!g) return;
    const R = 70;
    const Z = 3;
    g.clearRect(0, 0, 2 * R, 2 * R);
    g.save();
    g.beginPath();
    g.arc(R, R, R - 1, 0, Math.PI * 2);
    g.clip();
    g.imageSmoothingEnabled = false;
    g.drawImage(image, hover.x - R / Z, hover.y - R / Z, (2 * R) / Z, (2 * R) / Z, 0, 0, 2 * R, 2 * R);
    g.strokeStyle = "rgba(255,255,255,0.9)";
    g.lineWidth = 1;
    g.beginPath();
    g.moveTo(R - 9, R);
    g.lineTo(R + 9, R);
    g.moveTo(R, R - 9);
    g.lineTo(R, R + 9);
    g.stroke();
    g.restore();
    g.strokeStyle = "#d7a84f";
    g.lineWidth = 2;
    g.beginPath();
    g.arc(R, R, R - 1, 0, Math.PI * 2);
    g.stroke();
  }, [hover, image]);

  const handleMove = () => {
    if (!lupa) return;
    const pos = stageRef.current?.getPointerPosition();
    if (!pos) return;
    const x = (pos.x - viewport.x) / viewport.scale;
    const y = (pos.y - viewport.y) / viewport.scale;
    setHover(x < 0 || y < 0 || x > natW || y > natH ? null : { sx: pos.x, sy: pos.y, x, y });
  };

  return (
    <div ref={wrapRef} style={{ width: "100%", height: "100%", position: "relative" }}>
      <Stage
        ref={stageRef}
        width={size.w}
        height={size.h}
        x={viewport.x}
        y={viewport.y}
        scaleX={viewport.scale}
        scaleY={viewport.scale}
        onClick={handleClick}
        onTap={handleClick}
        onWheel={handleWheel}
        onMouseMove={handleMove}
        onMouseLeave={() => setHover(null)}
        style={{
          background: "#0f172a",
          cursor: interactive ? "crosshair" : "default",
          borderRadius: 6,
        }}
      >
        <Layer listening={false}>
          {image && (
            <KonvaImage image={image} x={0} y={0} width={natW} height={natH} />
          )}
        </Layer>
        <Layer listening={false}>
          <Group clipX={0} clipY={0} clipWidth={natW} clipHeight={natH}>
            {guides.map((g, i) => (
              <Line
                key={i}
                points={g.points}
                stroke={g.color ?? "rgba(215,168,79,0.55)"}
                strokeWidth={(g.width ?? 1) * inv}
                dash={g.dash ? [6 * inv, 5 * inv] : undefined}
                lineCap="round"
                lineJoin="round"
              />
            ))}
          </Group>
          {flatPolyline && flatPolyline.length >= 4 && (
            <Line
              points={flatPolyline}
              stroke="#f59e0b"
              strokeWidth={1.5 * inv}
              dash={[6 * inv, 4 * inv]}
              closed={closed}
              fill={closed ? "rgba(245,158,11,0.12)" : undefined}
            />
          )}
          {markers.map((m, i) => (
            <CrosshairMarker
              key={`${m.x},${m.y},${i}`}
              x={m.x}
              y={m.y}
              inv={inv}
              label={m.label}
              color={m.color ?? "#22d3ee"}
              faint={m.faint}
            />
          ))}
        </Layer>
      </Stage>
      {hint && (
        <div
          style={{
            position: "absolute",
            left: 10,
            bottom: 10,
            padding: "5px 10px",
            borderRadius: 6,
            background: "rgba(13,21,32,0.82)",
            color: "#f6e2b5",
            font: "500 13px var(--font-ui)",
            pointerEvents: "none",
          }}
        >
          {hint}
        </div>
      )}
      {lupa && hover && (
        <canvas
          ref={lupaRef}
          width={140}
          height={140}
          style={{
            position: "absolute",
            left: Math.min(size.w - 150, hover.sx + 24),
            top: Math.max(6, hover.sy - 164),
            pointerEvents: "none",
          }}
        />
      )}
    </div>
  );
}

function CrosshairMarker({
  x,
  y,
  inv,
  label,
  color,
  faint,
}: {
  x: number;
  y: number;
  inv: number;
  label?: string;
  color: string;
  faint?: boolean;
}) {
  const r = (faint ? 6 : 10) * inv;
  if (faint) return <Circle x={x} y={y} radius={r} stroke={color} strokeWidth={1.5 * inv} opacity={0.6} />;
  return (
    <>
      <Line points={[x - r, y, x + r, y]} stroke={color} strokeWidth={1.5 * inv} />
      <Line points={[x, y - r, x, y + r]} stroke={color} strokeWidth={1.5 * inv} />
      <Circle x={x} y={y} radius={3 * inv} fill={color} />
      {label && (
        <KonvaText
          x={x + r + 2 * inv}
          y={y - r}
          text={label}
          fontSize={13 * inv}
          fontStyle="bold"
          fill={color}
        />
      )}
    </>
  );
}

function clamp(v: number, min: number, max: number): number {
  return Math.max(min, Math.min(max, v));
}

function round2(v: number): number {
  return Math.round(v * 100) / 100;
}
