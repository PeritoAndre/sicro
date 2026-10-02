/**
 * Renderer Konva do motor parity, em 4 passes (como o SICRO 1.0 Python):
 * calçadas → asfalto → marcações clipadas → handles do objeto selecionado.
 */

import { Fragment, useMemo } from "react";
import { Circle, Group, Line } from "react-konva";
import {
  buildRoadEdges,
  buildRoadRibbon,
  buildRoadSidewalk,
  buildRoundaboutDiskPolygon,
  buildRoundaboutRings,
  discretizeCircle,
  flattenVec2,
  projectWorldPoints,
  resolvePxPerM,
  sampleCubicBezier,
  type Vec2World,
} from "./geometry";
import { clipPolylineAgainstPolygons } from "./clipping";
import {
  type SicroParityObject,
  type SicroRoadObject_parity,
  type SicroRoundaboutObject_parity,
} from "./types";
import { isParityRoad, isParityRoundabout } from "./guards";

// Cores fixas, copiadas do SICRO 1.0 Python.
const PARITY_COLORS = {
  asphalt: "#1C1C1C",
  sidewalk: "#7C7460",
  earth: "#9C7A4E",
  islandDefault: "#3A6535",
  edge: "#FFFFFF",
  yellow: "#F5C518",
  white: "#FFFFFF",
  selection: "#4A80FF",
  selectionGuide: "#6080C0",
} as const;

/** Espessuras em px de tela. */
const PARITY_STROKE_WIDTHS = {
  edgeLine: 2,
  centerLine: 2,
} as const;

/** Dash do eixo central (px de tela). */
const PARITY_CENTER_LINE_DASH: readonly [number, number] = [12, 8];

/** Tensão do Konva.Line fechado — equivale ao `smooth=True` do Tkinter. */
const PARITY_LINE_TENSION = 0.5;

// ---- Helpers ----

function surfaceFillForRoad(road: SicroRoadObject_parity): string {
  switch (road.superficie) {
    case "asfalto":
      return PARITY_COLORS.asphalt;
    case "calcada":
      return PARITY_COLORS.sidewalk;
    case "terra":
      return PARITY_COLORS.earth;
    default:
      return PARITY_COLORS.asphalt;
  }
}

function centerLineColorForRoad(road: SicroRoadObject_parity): string {
  switch (road.marcacao) {
    case "amarela":
      return PARITY_COLORS.yellow;
    case "branca":
      return PARITY_COLORS.white;
    default:
      return PARITY_COLORS.edge;
  }
}

/** Geometria pré-computada por via (metros), compartilhada pelos 4 passes. */
interface RoadMesh {
  road: SicroRoadObject_parity;
  samplesWorld: Vec2World[];
  /** Também serve de obstáculo no clipping das outras vias. */
  asphaltPolyWorld: Vec2World[];
  sidewalkPolyWorld: Vec2World[];
}

function buildRoadMesh(road: SicroRoadObject_parity): RoadMesh {
  const samples = sampleCubicBezier(road, 32);
  const halfWidth = road.largura_m / 2;
  return {
    road,
    samplesWorld: samples,
    asphaltPolyWorld: buildRoadRibbon(samples, halfWidth),
    sidewalkPolyWorld: buildRoadSidewalk(samples, halfWidth),
  };
}

// ---- Componente ----

interface RoadParityRendererProps {
  objects: ReadonlyArray<SicroParityObject>;
  /** Escala do documento (px/m); null/undefined usa o default. */
  pxPerM?: number | null;
  /** Translação mundo → canvas (px). */
  offsetX?: number;
  offsetY?: number;
  selectedId?: string | null;
  onSelect?: (id: string | null) => void;
  /**
   * Chamado ao arrastar handles (via: ax/ay/bx/by/cx1/cy1/cx2/cy2; rotatória: cx/cy).
   * Ausente ⇒ handles estáticos (modo visualização).
   */
  onObjectChange?: (
    id: string,
    patch: Partial<SicroParityObject>,
  ) => void;
}

/** Componente puro, sem estado próprio. */
export function RoadParityRenderer({
  objects,
  pxPerM,
  offsetX = 0,
  offsetY = 0,
  selectedId = null,
  onSelect,
  onObjectChange,
}: RoadParityRendererProps) {
  const effectivePxPerM = resolvePxPerM(pxPerM);

  // Inverso de `projectWorldPoints`.
  const canvasToWorldX = (px: number): number =>
    (px - offsetX) / Math.max(effectivePxPerM, 0.0001);
  const canvasToWorldY = (px: number): number =>
    (px - offsetY) / Math.max(effectivePxPerM, 0.0001);

  const roads = useMemo(
    () =>
      objects.filter(
        (o): o is SicroRoadObject_parity => isParityRoad(o),
      ),
    [objects],
  );
  const roundabouts = useMemo(
    () =>
      objects.filter(
        (o): o is SicroRoundaboutObject_parity => isParityRoundabout(o),
      ),
    [objects],
  );

  const meshes = useMemo<RoadMesh[]>(
    () => roads.map(buildRoadMesh),
    [roads],
  );

  const allRoadAsphaltPolys = useMemo(
    () => meshes.map((m) => m.asphaltPolyWorld),
    [meshes],
  );
  // Sem padding: as bordas das vias devem parar exatamente no raio externo do
  // anel; qualquer sobra é coberta pelo overlay do pass 3b.
  const allRoundaboutDisks = useMemo(
    () =>
      roundabouts.map((rb) => buildRoundaboutDiskPolygon(rb, 96, 0)),
    [roundabouts],
  );

  return (
    <Fragment>
      {/* ---------- PASS 1: Calçadas ---------- */}
      <Group listening={false}>
        {meshes
          .filter((m) => m.road.superficie === "asfalto" && m.road.visible !== false)
          .map((m) => {
            const projected = projectWorldPoints(
              m.sidewalkPolyWorld,
              effectivePxPerM,
              offsetX,
              offsetY,
            );
            return (
              <Line
                key={`pp1_sw_${m.road.id}`}
                points={flattenVec2(projected)}
                closed
                tension={PARITY_LINE_TENSION}
                fill={PARITY_COLORS.sidewalk}
              />
            );
          })}
        {roundabouts
          .filter((rb) => rb.visible !== false)
          .map((rb) => {
            const rings = buildRoundaboutRings(rb, effectivePxPerM);
            return (
              <Circle
                key={`pp1_rb_sw_${rb.id}`}
                x={rings.cx_px + offsetX}
                y={rings.cy_px + offsetY}
                radius={rings.sidewalk_r_px}
                fill={PARITY_COLORS.sidewalk}
              />
            );
          })}
      </Group>

      {/* ---------- PASS 2: Asfalto ---------- */}
      <Group>
        {meshes
          .filter((m) => m.road.visible !== false)
          .map((m) => {
            const projected = projectWorldPoints(
              m.asphaltPolyWorld,
              effectivePxPerM,
              offsetX,
              offsetY,
            );
            return (
              <Line
                key={`pp2_as_${m.road.id}`}
                points={flattenVec2(projected)}
                closed
                tension={PARITY_LINE_TENSION}
                fill={surfaceFillForRoad(m.road)}
                onClick={() => onSelect?.(m.road.id)}
                onTap={() => onSelect?.(m.road.id)}
              />
            );
          })}
        {roundabouts
          .filter((rb) => rb.visible !== false)
          .map((rb) => {
            const rings = buildRoundaboutRings(rb, effectivePxPerM);
            const islandColor = rb.inner_color ?? PARITY_COLORS.islandDefault;
            return (
              <Group key={`pp2_rb_${rb.id}`}>
                <Circle
                  x={rings.cx_px + offsetX}
                  y={rings.cy_px + offsetY}
                  radius={rings.outer_r_px}
                  fill={PARITY_COLORS.asphalt}
                  onClick={() => onSelect?.(rb.id)}
                  onTap={() => onSelect?.(rb.id)}
                />
                {rings.inner_r_px >= 1 && (
                  <Circle
                    x={rings.cx_px + offsetX}
                    y={rings.cy_px + offsetY}
                    radius={rings.inner_r_px}
                    fill={islandColor}
                    listening={false}
                  />
                )}
              </Group>
            );
          })}
      </Group>

      {/* ---------- PASS 3: Marcações (bordas + eixo central) ---------- */}
      <Group listening={false}>
        {meshes
          .filter((m) => m.road.visible !== false)
          .map((m) => {
            const obstacles: Vec2World[][] = [
              ...allRoadAsphaltPolys.filter(
                (_, idx) => meshes[idx]?.road.id !== m.road.id,
              ),
              ...allRoundaboutDisks,
            ];
            const halfWidthM = m.road.largura_m / 2;
            const { left, right } = buildRoadEdges(m.samplesWorld, halfWidthM);

            const leftClip = clipPolylineAgainstPolygons(left, obstacles);
            const rightClip = clipPolylineAgainstPolygons(right, obstacles);

            const elements: JSX.Element[] = [];

            for (let i = 0; i < leftClip.segments.length; i++) {
              const seg = leftClip.segments[i] as Vec2World[];
              const proj = projectWorldPoints(seg, effectivePxPerM, offsetX, offsetY);
              if (proj.length < 2) continue;
              elements.push(
                <Line
                  key={`pp3_el_${m.road.id}_${i}`}
                  points={flattenVec2(proj)}
                  stroke={PARITY_COLORS.edge}
                  strokeWidth={PARITY_STROKE_WIDTHS.edgeLine}
                  lineCap="butt"
                  lineJoin="round"
                  listening={false}
                />,
              );
            }
            for (let i = 0; i < rightClip.segments.length; i++) {
              const seg = rightClip.segments[i] as Vec2World[];
              const proj = projectWorldPoints(seg, effectivePxPerM, offsetX, offsetY);
              if (proj.length < 2) continue;
              elements.push(
                <Line
                  key={`pp3_er_${m.road.id}_${i}`}
                  points={flattenVec2(proj)}
                  stroke={PARITY_COLORS.edge}
                  strokeWidth={PARITY_STROKE_WIDTHS.edgeLine}
                  lineCap="butt"
                  lineJoin="round"
                  listening={false}
                />,
              );
            }

            if (m.road.mao_dupla && m.road.marcacao !== "nenhuma") {
              const centerClip = clipPolylineAgainstPolygons(
                m.samplesWorld,
                obstacles,
              );
              for (let i = 0; i < centerClip.segments.length; i++) {
                const seg = centerClip.segments[i] as Vec2World[];
                const proj = projectWorldPoints(
                  seg,
                  effectivePxPerM,
                  offsetX,
                  offsetY,
                );
                if (proj.length < 2) continue;
                elements.push(
                  <Line
                    key={`pp3_cc_${m.road.id}_${i}`}
                    points={flattenVec2(proj)}
                    stroke={centerLineColorForRoad(m.road)}
                    strokeWidth={PARITY_STROKE_WIDTHS.centerLine}
                    dash={[PARITY_CENTER_LINE_DASH[0], PARITY_CENTER_LINE_DASH[1]]}
                    lineCap="butt"
                    listening={false}
                  />,
                );
              }
            }

            return <Group key={`pp3_${m.road.id}`}>{elements}</Group>;
          })}

        {/* Pass 3b — repinta anel + ilha por cima das marcações das vias que
            cruzam o disco. Dois <Circle> em vez de donut: o z-order resolve. */}
        {roundabouts
          .filter((rb) => rb.visible !== false)
          .map((rb) => {
            const rings = buildRoundaboutRings(rb, effectivePxPerM);
            const cxPx = rings.cx_px + offsetX;
            const cyPx = rings.cy_px + offsetY;
            const islandColor = rb.inner_color ?? PARITY_COLORS.islandDefault;
            return (
              <Group key={`pp3_rb_overlay_${rb.id}`} listening={false}>
                <Circle
                  x={cxPx}
                  y={cyPx}
                  radius={rings.outer_r_px + 0.5}
                  fill={PARITY_COLORS.asphalt}
                />
                {rings.inner_r_px >= 1 && (
                  <Circle
                    x={cxPx}
                    y={cyPx}
                    radius={rings.inner_r_px}
                    fill={islandColor}
                  />
                )}
              </Group>
            );
          })}

        {/* Bordas + eixo das rotatórias, clipados geometricamente contra o
            asfalto das vias — junção sem heurística angular. */}
        {roundabouts
          .filter((rb) => rb.visible !== false)
          .map((rb) => {
            const marcacao = rb.marcacao ?? "nenhuma";
            const showCentralLine = marcacao !== "nenhuma";
            const centralColor =
              marcacao === "amarela"
                ? PARITY_COLORS.yellow
                : marcacao === "branca"
                  ? PARITY_COLORS.white
                  : PARITY_COLORS.edge;

            const halfLargM = rb.largura_m / 2;
            const outerRm = rb.r_m + halfLargM;
            const innerRm = Math.max(0, rb.r_m - halfLargM);
            const midRm = (outerRm + innerRm) / 2;

            const outerLoop = discretizeCircle(rb.cx, rb.cy, outerRm);
            const innerLoop = discretizeCircle(rb.cx, rb.cy, innerRm);
            const midLoop = discretizeCircle(rb.cx, rb.cy, midRm);

            const outerClipped = clipPolylineAgainstPolygons(
              outerLoop,
              allRoadAsphaltPolys,
            );
            const innerClipped = clipPolylineAgainstPolygons(
              innerLoop,
              allRoadAsphaltPolys,
            );
            const midClipped = showCentralLine
              ? clipPolylineAgainstPolygons(midLoop, allRoadAsphaltPolys)
              : { segments: [] as Vec2World[][] };

            return (
              <Group key={`pp3_rb_${rb.id}`}>
                {outerClipped.segments.map((seg, i) => {
                  const proj = projectWorldPoints(
                    seg,
                    effectivePxPerM,
                    offsetX,
                    offsetY,
                  );
                  if (proj.length < 2) return null;
                  return (
                    <Line
                      key={`pp3_rb_${rb.id}_outer_${i}`}
                      points={flattenVec2(proj)}
                      stroke={PARITY_COLORS.edge}
                      strokeWidth={PARITY_STROKE_WIDTHS.edgeLine}
                      lineCap="butt"
                      lineJoin="round"
                      listening={false}
                    />
                  );
                })}
                {innerRm >= 0.5 &&
                  innerClipped.segments.map((seg, i) => {
                    const proj = projectWorldPoints(
                      seg,
                      effectivePxPerM,
                      offsetX,
                      offsetY,
                    );
                    if (proj.length < 2) return null;
                    return (
                      <Line
                        key={`pp3_rb_${rb.id}_inner_${i}`}
                        points={flattenVec2(proj)}
                        stroke={PARITY_COLORS.edge}
                        strokeWidth={PARITY_STROKE_WIDTHS.edgeLine}
                        lineCap="butt"
                        lineJoin="round"
                        listening={false}
                      />
                    );
                  })}
                {showCentralLine &&
                  midRm >= 0.5 &&
                  midClipped.segments.map((seg, i) => {
                    const proj = projectWorldPoints(
                      seg,
                      effectivePxPerM,
                      offsetX,
                      offsetY,
                    );
                    if (proj.length < 2) return null;
                    return (
                      <Line
                        key={`pp3_rb_${rb.id}_center_${i}`}
                        points={flattenVec2(proj)}
                        stroke={centralColor}
                        strokeWidth={PARITY_STROKE_WIDTHS.centerLine}
                        dash={[
                          PARITY_CENTER_LINE_DASH[0],
                          PARITY_CENTER_LINE_DASH[1],
                        ]}
                        lineCap="butt"
                        listening={false}
                      />
                    );
                  })}
              </Group>
            );
          })}
      </Group>

      {/* ---------- PASS 4: Handles ---------- */}
      {selectedId && (
        <Group>
          {meshes
            .filter((m) => m.road.id === selectedId)
            .map((m) => {
              const a = projectWorldPoints(
                [{ x: m.road.ax, y: m.road.ay }],
                effectivePxPerM,
                offsetX,
                offsetY,
              )[0]!;
              const b = projectWorldPoints(
                [{ x: m.road.bx, y: m.road.by }],
                effectivePxPerM,
                offsetX,
                offsetY,
              )[0]!;
              const c1 = projectWorldPoints(
                [{ x: m.road.cx1, y: m.road.cy1 }],
                effectivePxPerM,
                offsetX,
                offsetY,
              )[0]!;
              const c2 = projectWorldPoints(
                [{ x: m.road.cx2, y: m.road.cy2 }],
                effectivePxPerM,
                offsetX,
                offsetY,
              )[0]!;
              const canDrag = onObjectChange !== undefined;
              const roadId = m.road.id;
              return (
                <Group key={`pp4_${roadId}`}>
                  <Line
                    points={[a.x, a.y, c1.x, c1.y]}
                    stroke={PARITY_COLORS.selectionGuide}
                    strokeWidth={1}
                    dash={[3, 3]}
                    listening={false}
                  />
                  <Line
                    points={[b.x, b.y, c2.x, c2.y]}
                    stroke={PARITY_COLORS.selectionGuide}
                    strokeWidth={1}
                    dash={[3, 3]}
                    listening={false}
                  />
                  {/* Âncoras arrastam o controle junto (preserva a curvatura). */}
                  <Circle
                    x={a.x}
                    y={a.y}
                    radius={7}
                    fill={PARITY_COLORS.selection}
                    stroke="#1a1a1a"
                    strokeWidth={2}
                    draggable={canDrag}
                    onDragEnd={(e) => {
                      if (!onObjectChange) return;
                      const newAx = canvasToWorldX(e.target.x());
                      const newAy = canvasToWorldY(e.target.y());
                      const dx = newAx - m.road.ax;
                      const dy = newAy - m.road.ay;
                      onObjectChange(roadId, {
                        ax: newAx,
                        ay: newAy,
                        cx1: m.road.cx1 + dx,
                        cy1: m.road.cy1 + dy,
                      });
                    }}
                  />
                  <Circle
                    x={b.x}
                    y={b.y}
                    radius={7}
                    fill={PARITY_COLORS.selection}
                    stroke="#1a1a1a"
                    strokeWidth={2}
                    draggable={canDrag}
                    onDragEnd={(e) => {
                      if (!onObjectChange) return;
                      const newBx = canvasToWorldX(e.target.x());
                      const newBy = canvasToWorldY(e.target.y());
                      const dx = newBx - m.road.bx;
                      const dy = newBy - m.road.by;
                      onObjectChange(roadId, {
                        bx: newBx,
                        by: newBy,
                        cx2: m.road.cx2 + dx,
                        cy2: m.road.cy2 + dy,
                      });
                    }}
                  />
                  <Circle
                    x={c1.x}
                    y={c1.y}
                    radius={5}
                    fill="#4F72E0"
                    draggable={canDrag}
                    onDragEnd={(e) => {
                      if (!onObjectChange) return;
                      onObjectChange(roadId, {
                        cx1: canvasToWorldX(e.target.x()),
                        cy1: canvasToWorldY(e.target.y()),
                      });
                    }}
                  />
                  <Circle
                    x={c2.x}
                    y={c2.y}
                    radius={5}
                    fill="#4F72E0"
                    draggable={canDrag}
                    onDragEnd={(e) => {
                      if (!onObjectChange) return;
                      onObjectChange(roadId, {
                        cx2: canvasToWorldX(e.target.x()),
                        cy2: canvasToWorldY(e.target.y()),
                      });
                    }}
                  />
                </Group>
              );
            })}
          {roundabouts
            .filter((rb) => rb.id === selectedId)
            .map((rb) => {
              const rings = buildRoundaboutRings(rb, effectivePxPerM);
              const canDrag = onObjectChange !== undefined;
              const rbId = rb.id;
              return (
                <Group key={`pp4_rb_${rbId}`}>
                  <Circle
                    x={rings.cx_px + offsetX}
                    y={rings.cy_px + offsetY}
                    radius={7}
                    fill={PARITY_COLORS.selection}
                    stroke="#1a1a1a"
                    strokeWidth={2}
                    draggable={canDrag}
                    onDragEnd={(e) => {
                      if (!onObjectChange) return;
                      onObjectChange(rbId, {
                        cx: canvasToWorldX(e.target.x()),
                        cy: canvasToWorldY(e.target.y()),
                      });
                    }}
                  />
                  <Circle
                    x={rings.cx_px + offsetX}
                    y={rings.cy_px + offsetY}
                    radius={rings.outer_r_px}
                    stroke={PARITY_COLORS.selection}
                    strokeWidth={1}
                    dash={[4, 4]}
                    fillEnabled={false}
                    listening={false}
                  />
                </Group>
              );
            })}
        </Group>
      )}
    </Fragment>
  );
}
