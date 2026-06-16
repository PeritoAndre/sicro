/**
 * Python Parity Engine — renderer Konva multipass.
 *
 * Implementa o pipeline em **4 passes** inspirado em
 * `_desenhar_vias_multipass` do SICRO 1.0 Python:
 *
 *   1. Calçadas (vias + rotatórias).
 *   2. Asfalto (vias + anel de rotatória + ilha verde).
 *   3. Marcações (bordas brancas + eixo tracejado, clipadas).
 *   4. Handles (apenas objeto selecionado).
 *
 * **Sem** junction patches, **sem** flares, **sem** roundabout
 * entries, **sem** smoothing modes, **sem** lane dividers, **sem**
 * road_style. Filosofia: poucos campos, poucos passes, visual limpo.
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

// ---------------------------------------------------------------------------
// Cores hardcoded (paridade SICRO 1.0 Python — `editor_croqui.py:2810`).

const PARITY_COLORS = {
  /** Asfalto (linha 2950 do Python). */
  asphalt: "#1C1C1C",
  /** Calçada cinza-amarelado (linha 2927). */
  sidewalk: "#7C7460",
  /** Terra (`superficies.py` linha 44). */
  earth: "#9C7A4E",
  /** Ilha central rotatória — verde canteiro (linha 2964). */
  islandDefault: "#3A6535",
  /** Bordas brancas (linhas 2982, 3013). */
  edge: "#FFFFFF",
  /** Eixo tracejado amarelo (linha 2991). */
  yellow: "#F5C518",
  /** Eixo tracejado branco. */
  white: "#FFFFFF",
  /** Stroke de seleção. */
  selection: "#4A80FF",
  /** Linha tracejada de seleção. */
  selectionGuide: "#6080C0",
} as const;

/** Espessuras em px de tela. `lw_b = 2`, `lw_mc = 2` (Python 2967-2968). */
const PARITY_STROKE_WIDTHS = {
  edgeLine: 2,
  centerLine: 2,
} as const;

/** Padrão do dash do eixo central (px de tela, Python 2969). */
const PARITY_CENTER_LINE_DASH: readonly [number, number] = [12, 8];

/** Tensão do Konva.Line(closed) — equivalente a Tkinter `smooth=True`. */
const PARITY_LINE_TENSION = 0.5;

// ---------------------------------------------------------------------------
// Helpers internos.

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

/**
 * Estrutura pré-computada por via — calculada uma vez em useMemo,
 * usada nos 4 passes para evitar re-amostragem.
 */
interface RoadMesh {
  road: SicroRoadObject_parity;
  /** Amostras da Bezier em mundo (m). */
  samplesWorld: Vec2World[];
  /** Polígono do asfalto em mundo (m) — usado para clipping cruzado. */
  asphaltPolyWorld: Vec2World[];
  /** Polígono da calçada em mundo (m). */
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

// ---------------------------------------------------------------------------
// Componente principal.

export interface RoadParityRendererProps {
  /** Objetos a renderizar. Tipicamente `doc.parity_objects`. */
  objects: ReadonlyArray<SicroParityObject>;
  /** Escala do documento (px/m). Quando null/undefined, usa default. */
  pxPerM?: number | null;
  /** Translação do mundo → canvas (px). Tipicamente 0,0 dentro de um Stage. */
  offsetX?: number;
  offsetY?: number;
  /** Id do objeto selecionado — desenha handles. */
  selectedId?: string | null;
  /** Click handler — usado pelo modo "select" do editor. */
  onSelect?: (id: string | null) => void;
  /**
   * Patch handler — recebe id + campos modificados. Usado quando o
   * perito arrasta handles. Quando ausente, handles ficam estáticos
   * (só visualização — útil no lab e em modo "view").
   *
   * Para vias: patches incluem ax, ay, bx, by, cx1, cy1, cx2, cy2.
   * Para rotatórias: patches incluem cx, cy.
   */
  onObjectChange?: (
    id: string,
    patch: Partial<SicroParityObject>,
  ) => void;
}

/**
 * Renderer Konva multipass — entrada do Python Parity Engine.
 *
 * Componente puro: lê `objects` + `pxPerM` + `offsetX/Y` e produz
 * `<Layer>`s do react-konva. Não tem estado próprio.
 */
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

  /**
   * Converte um ponto canvas (px) de volta para mundo (m).
   * Inverte o `projectWorldPoints`.
   */
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

  // Pré-computa meshes (compartilhados entre todos os passes).
  const meshes = useMemo<RoadMesh[]>(
    () => roads.map(buildRoadMesh),
    [roads],
  );

  // Obstáculos para clipping de marcações: polígonos do asfalto das
  // outras vias + discos das rotatórias.
  const allRoadAsphaltPolys = useMemo(
    () => meshes.map((m) => m.asphaltPolyWorld),
    [meshes],
  );
  // Discos das rotatórias usados como obstáculos para clipping das
  // bordas das vias. Sem padding — as bordas das vias devem parar
  // EXATAMENTE no raio externo do anel, encontrando a borda circular
  // branca da rotatória. Quaisquer pontos tangentes que escapem do
  // clipping são cobertos pelo PASS 3b (overlay de asfalto da
  // rotatória), portanto não há risco de vazamento visual.
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
        {/* Calçadas externas das rotatórias. */}
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
        {/* Rotatórias: asfalto + ilha. */}
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

            // Bordas brancas.
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

            // Eixo central — apenas se mão dupla E marcação não é "nenhuma".
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

        {/* Pass 3b — Repinta o anel ASFALTO + ilha verde POR CIMA das
            marcações das vias. Garante que bordas / eixos centrais das
            vias que cruzaram dentro do disco da rotatória sejam
            cobertos. Em vez de desenhar um donut (Shape com sceneFunc),
            usamos dois <Circle> simples: o externo cobre tudo, o
            interno restaura a ilha por cima. Z-order natural do
            react-konva resolve. */}
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

        {/* Bordas + eixo central das rotatórias — clipping GEOMÉTRICO
            REAL contra os polígonos de asfalto das vias. Onde o
            asfalto da via cruza o anel, a borda do anel é cortada
            EXATAMENTE na borda lateral da via — junções seamless
            sem heurística angular. */}
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

            // Raios em metros (mundo) — clipping é geométrico em
            // mundo, depois projetamos pra canvas.
            const halfLargM = rb.largura_m / 2;
            const outerRm = rb.r_m + halfLargM;
            const innerRm = Math.max(0, rb.r_m - halfLargM);
            const midRm = (outerRm + innerRm) / 2;

            // Discretiza os 3 anéis em polylines fechadas (96 vértices).
            const outerLoop = discretizeCircle(rb.cx, rb.cy, outerRm);
            const innerLoop = discretizeCircle(rb.cx, rb.cy, innerRm);
            const midLoop = discretizeCircle(rb.cx, rb.cy, midRm);

            // Clipa contra TODOS os polígonos de asfalto das vias —
            // os pedaços DENTRO do asfalto da via são descartados.
            // Resultado: arcos visíveis APENAS onde não há via.
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
                {/* Borda externa — arcos sobreviventes do clipping. */}
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
                {/* Borda interna — arcos sobreviventes. Em rotatórias
                    pequenas / largas, vias podem cruzar até esse anel;
                    o clip cuida. */}
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
                {/* Eixo central tracejado — também clipado. */}
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
                  {/* Handle A — arrasta A junto com C1 (preserva curvatura). */}
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
                  {/* Handle B — arrasta B junto com C2. */}
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
                  {/* Handle C1 — move só o controle (curvatura muda). */}
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
                  {/* Handle C2. */}
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
                  {/* Centro — arrasta a rotatória inteira. */}
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
