/**
 * W20 — Camada do "marquee" da seleção (estilo Photoshop). Konva Layer não-
 * interativa (listening=false): desenha a seleção confirmada (`doc.selection`)
 * e o rascunho em andamento (`draft`) com contorno tracejado animado
 * (marching ants). Coordenadas em px da imagem (mesma convenção das
 * anotações). Stroke/traço invariantes ao zoom (÷viewportScale).
 */
import { useEffect, useState } from "react";
import { Layer, Rect, Ellipse, Line, Circle } from "react-konva";
import type { SicroImagePoint, SicroImageSelection } from "../engine/schema";

export type SelDraft =
  | { mode: "rect" | "ellipse"; x0: number; y0: number; x1: number; y1: number }
  | { mode: "lasso"; points: SicroImagePoint[] }
  | { mode: "polygon"; points: SicroImagePoint[]; live: SicroImagePoint | null };

interface Props {
  selection: SicroImageSelection | null;
  draft: SelDraft | null;
  imageWidth: number;
  imageHeight: number;
  viewportScale: number;
}

const ANTS = "#22d3ee"; // ciano — alto contraste sobre foto
const ANTS_UNDER = "rgba(8,12,18,0.85)";

function flat(points: SicroImagePoint[]): number[] {
  const out: number[] = [];
  for (const p of points) out.push(p.x, p.y);
  return out;
}

export function SelectionMarqueeLayer({
  selection,
  draft,
  imageWidth,
  imageHeight,
  viewportScale,
}: Props) {
  const active = !!selection || !!draft;
  // Marching ants: anima o dashOffset enquanto há algo a mostrar.
  const [offset, setOffset] = useState(0);
  useEffect(() => {
    if (!active) return;
    const id = window.setInterval(() => setOffset((o) => (o + 1) % 1000), 70);
    return () => window.clearInterval(id);
  }, [active]);

  const s = viewportScale > 0 ? viewportScale : 1;
  const stroke = 1.5 / s;
  const under = 2.5 / s;
  const dash = [6 / s, 4 / s];
  const dashOffset = offset * (2 / s);
  const vtx = 4 / s; // raio dos vértices do polígono em construção

  // Desenha contorno "marching ants" (linha branca sólida embaixo + ciano
  // tracejado animado em cima) para uma forma genérica via render-prop.
  const antsProps = {
    stroke: ANTS,
    strokeWidth: stroke,
    dash,
    dashOffset,
    listening: false,
    perfectDrawEnabled: false,
  } as const;
  const underProps = {
    stroke: ANTS_UNDER,
    strokeWidth: under,
    listening: false,
    perfectDrawEnabled: false,
  } as const;

  return (
    <Layer listening={false}>
      {/* ----- Seleção confirmada ----- */}
      {selection?.kind === "rect" &&
        selection.x !== undefined &&
        selection.y !== undefined &&
        selection.width !== undefined &&
        selection.height !== undefined && (
          <>
            {selection.inverted && (
              <Rect
                x={selection.x}
                y={selection.y}
                width={selection.width}
                height={selection.height}
                fill="rgba(56,189,248,0.10)"
                listening={false}
              />
            )}
            <Rect
              x={selection.x}
              y={selection.y}
              width={selection.width}
              height={selection.height}
              {...underProps}
            />
            <Rect
              x={selection.x}
              y={selection.y}
              width={selection.width}
              height={selection.height}
              {...antsProps}
            />
          </>
        )}
      {selection?.kind === "ellipse" &&
        selection.x !== undefined &&
        selection.y !== undefined &&
        selection.width !== undefined &&
        selection.height !== undefined && (
          <>
            {selection.inverted && (
              <Ellipse
                x={selection.x + selection.width / 2}
                y={selection.y + selection.height / 2}
                radiusX={selection.width / 2}
                radiusY={selection.height / 2}
                fill="rgba(56,189,248,0.10)"
                listening={false}
              />
            )}
            <Ellipse
              x={selection.x + selection.width / 2}
              y={selection.y + selection.height / 2}
              radiusX={selection.width / 2}
              radiusY={selection.height / 2}
              {...underProps}
            />
            <Ellipse
              x={selection.x + selection.width / 2}
              y={selection.y + selection.height / 2}
              radiusX={selection.width / 2}
              radiusY={selection.height / 2}
              {...antsProps}
            />
          </>
        )}
      {selection?.kind === "polygon" &&
        (selection.points?.length ?? 0) >= 3 && (
          <>
            {selection.inverted && (
              <Line
                points={flat(selection.points ?? [])}
                closed
                fill="rgba(56,189,248,0.10)"
                listening={false}
              />
            )}
            <Line points={flat(selection.points ?? [])} closed {...underProps} />
            <Line points={flat(selection.points ?? [])} closed {...antsProps} />
          </>
        )}

      {/* ----- Rascunho em andamento ----- */}
      {draft?.mode === "rect" && (
        <Rect
          x={Math.min(draft.x0, draft.x1)}
          y={Math.min(draft.y0, draft.y1)}
          width={Math.abs(draft.x1 - draft.x0)}
          height={Math.abs(draft.y1 - draft.y0)}
          {...antsProps}
        />
      )}
      {draft?.mode === "ellipse" && (
        <Ellipse
          x={(draft.x0 + draft.x1) / 2}
          y={(draft.y0 + draft.y1) / 2}
          radiusX={Math.abs(draft.x1 - draft.x0) / 2}
          radiusY={Math.abs(draft.y1 - draft.y0) / 2}
          {...antsProps}
        />
      )}
      {draft?.mode === "lasso" && draft.points.length >= 2 && (
        <Line points={flat(draft.points)} {...antsProps} />
      )}
      {draft?.mode === "polygon" && (
        <>
          <Line
            points={flat(
              draft.live ? [...draft.points, draft.live] : draft.points,
            )}
            {...antsProps}
          />
          {draft.points.map((p, i) => (
            <Circle
              key={i}
              x={p.x}
              y={p.y}
              radius={vtx}
              fill={i === 0 ? "#22d3ee" : "#fff"}
              stroke={ANTS_UNDER}
              strokeWidth={1 / s}
              listening={false}
            />
          ))}
        </>
      )}
      {/* imageWidth/imageHeight reservados p/ futura visualização do "fora"
          quando invertida; mantidos na assinatura para S2. */}
      {false && <Rect width={imageWidth} height={imageHeight} listening={false} />}
    </Layer>
  );
}
