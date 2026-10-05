/**
 * Renderer Konva do motor parity (planta técnica). Passes: calçadas →
 * asfalto → meio-fio e sinalização (clipados nos cruzamentos) → rotatória
 * por cima → handles. Geometria em metros; `pxPerM` só na projeção.
 */

import { Fragment, useMemo } from "react";
import { Circle, Group, Line } from "react-konva";
import {
  buildRoadEdges,
  buildRoadRibbon,
  buildRoundaboutDiskPolygon,
  discretizeCircle,
  flattenVec2,
  projectWorldPoints,
  resolvePxPerM,
  sampleCubicBezier,
  type Vec2World,
} from "./geometry";
import { clipPolylineAgainstPolygons } from "./clipping";
import {
  PARITY_FAIXA_LARGURA_M,
  resolveParityEixo,
  type SicroParityObject,
  type SicroRoadObject_parity,
  type SicroRoundaboutObject_parity,
} from "./types";
import { parityTemaColors, resolveParityStyle, type ParityStyle } from "./style";
import { isParityRoad, isParityRoundabout } from "./guards";

const SELECTION = "#4A80FF";
const SELECTION_GUIDE = "#6080C0";
/** Metade do vão entre as duas linhas do eixo duplo (m). */
const EIXO_GAP_M = 0.15;
/** Dê a preferência (LDP): traço = espaço = 0,5 m. */
const LDP_DASH_M = 0.5;

// ---- Hachura da calçada (canvas 16×16 desenhado a 0,5 ⇒ 8 px na tela) ----

const hatchCache = new Map<string, HTMLCanvasElement>();

function hatchCanvas(line: string, bg: string): HTMLCanvasElement | null {
  if (typeof document === "undefined") return null;
  const key = `${line}|${bg}`;
  const hit = hatchCache.get(key);
  if (hit) return hit;
  const c = document.createElement("canvas");
  c.width = 16;
  c.height = 16;
  const g = c.getContext("2d");
  if (!g) return null;
  g.fillStyle = bg;
  g.fillRect(0, 0, 16, 16);
  g.strokeStyle = line;
  g.lineWidth = 1.6;
  g.beginPath();
  g.moveTo(-4, 20);
  g.lineTo(20, -4);
  g.moveTo(-4, 4);
  g.lineTo(4, -4);
  g.moveTo(12, 20);
  g.lineTo(20, 12);
  g.stroke();
  hatchCache.set(key, c);
  return c;
}

type FillProps = Record<string, unknown>;

function sidewalkFill(st: ParityStyle, colors: ReturnType<typeof parityTemaColors>): FillProps {
  if (st.calcada === "hachura") {
    const img = hatchCanvas(colors.hachuraLinha, colors.hachuraFundo);
    if (img) {
      return {
        fillPatternImage: img,
        fillPatternRepeat: "repeat",
        fillPatternScaleX: 0.5,
        fillPatternScaleY: 0.5,
        fillPriority: "pattern",
      };
    }
  }
  return { fill: colors.calcada };
}

function surfaceFill(
  superficie: SicroRoadObject_parity["superficie"],
  st: ParityStyle,
  colors: ReturnType<typeof parityTemaColors>,
): string {
  if (superficie === "calcada") return colors.calcada;
  if (superficie === "terra") return colors.terra;
  return st.asfalto;
}

// ---- Malha por via ----

interface RoadMesh {
  road: SicroRoadObject_parity;
  samplesWorld: Vec2World[];
  /** Meia-largura da pista de rolamento (m). */
  hw: number;
  /** Meia-largura pavimentada: pista + acostamento (m). */
  pavedHalf: number;
  sidewalkM: number;
  /** Também serve de obstáculo no clipping das outras vias. */
  pavedPoly: Vec2World[];
  sidewalkPoly: Vec2World[] | null;
}

function buildRoadMesh(road: SicroRoadObject_parity, st: ParityStyle): RoadMesh {
  const samples = sampleCubicBezier(road, 32);
  const hw = road.largura_m / 2;
  const pavedHalf = hw + Math.max(0, road.acostamento_m ?? 0);
  const sidewalkM =
    st.calcada === "nenhuma" ? 0 : Math.max(0, road.calcada_m ?? st.calcada_m);
  return {
    road,
    samplesWorld: samples,
    hw,
    pavedHalf,
    sidewalkM,
    pavedPoly: buildRoadRibbon(samples, pavedHalf),
    sidewalkPoly:
      sidewalkM > 0 && road.superficie === "asfalto"
        ? buildRoadRibbon(samples, pavedHalf + sidewalkM)
        : null,
  };
}

/** Polilinha deslocada `d` m da linha central (d > 0 = lado +normal). */
function offsetLine(samples: Vec2World[], d: number): Vec2World[] {
  if (Math.abs(d) < 1e-6) return samples;
  const { left, right } = buildRoadEdges(samples, Math.abs(d));
  return d > 0 ? left : right;
}

/** Faixas por sentido (mão dupla) ou no total (mão única); 0 = não dividir. */
function laneCount(road: SicroRoadObject_parity, st: ParityStyle): number {
  if (road.faixas != null) return Math.max(0, Math.floor(road.faixas));
  if (!st.faixas_auto) return 0;
  const base = road.mao_dupla ? road.largura_m / 2 : road.largura_m;
  return Math.max(1, Math.round(base / PARITY_FAIXA_LARGURA_M));
}

/** Linha "dê a preferência" na boca de uma via que chega à rotatória; null se a via não é aproximação. */
function giveWaySegment(
  m: RoadMesh,
  rb: SicroRoundaboutObject_parity,
): Vec2World[] | null {
  const outerR = rb.r_m + rb.largura_m / 2;
  const dist = (p: Vec2World) => Math.hypot(p.x - rb.cx, p.y - rb.cy);
  const pts = m.samplesWorld;
  if (pts.length < 2) return null;
  const aIn = dist(pts[0]!) <= outerR + 1;
  const bIn = dist(pts[pts.length - 1]!) <= outerR + 1;
  if (aIn === bIn) return null;
  const seq = bIn ? pts : pts.slice().reverse();
  const idx = seq.findIndex((p) => dist(p) <= outerR + 0.6);
  if (idx <= 0) return null;
  const p0 = seq[idx - 1]!;
  const p1 = seq[idx]!;
  let tx = p1.x - p0.x;
  let ty = p1.y - p0.y;
  const len = Math.hypot(tx, ty) || 1;
  tx /= len;
  ty /= len;
  // Direita de quem chega (eixo y para baixo): só a metade de entrada na mão dupla.
  const nx = -ty;
  const ny = tx;
  const from = m.road.mao_dupla
    ? p0
    : { x: p0.x - nx * m.hw, y: p0.y - ny * m.hw };
  const to = { x: p0.x + nx * m.hw, y: p0.y + ny * m.hw };
  return [from, to];
}

// ---- Componente ----

interface RoadParityRendererProps {
  objects: ReadonlyArray<SicroParityObject>;
  /** Escala do documento (px/m); null/undefined usa o default. */
  pxPerM?: number | null;
  /** Translação mundo → canvas (px). */
  offsetX?: number;
  offsetY?: number;
  /** Estilo do croqui (`doc.style`); ausente ⇒ planta técnica. */
  style?: Partial<ParityStyle> | null;
  selectedId?: string | null;
  onSelect?: (id: string | null) => void;
  /** Seleção múltipla: corpos arrastáveis (o drag em grupo do CanvasStage acha os nós pelo id). */
  selectedIds?: string[];
  /** Ausente ⇒ handles estáticos (modo visualização). */
  onObjectChange?: (id: string, patch: Partial<SicroParityObject>) => void;
}

export function RoadParityRenderer({
  objects,
  pxPerM,
  offsetX = 0,
  offsetY = 0,
  style,
  selectedId = null,
  onSelect,
  selectedIds,
  onObjectChange,
}: RoadParityRendererProps) {
  const ppm = resolvePxPerM(pxPerM);
  const st = useMemo(() => resolveParityStyle(style), [style]);
  const colors = useMemo(() => parityTemaColors(st), [st]);

  const isSel = (id: string) => selectedId === id || (selectedIds?.includes(id) ?? false);
  const canvasToWorldX = (px: number): number => (px - offsetX) / Math.max(ppm, 0.0001);
  const canvasToWorldY = (px: number): number => (px - offsetY) / Math.max(ppm, 0.0001);
  const P = (pts: ReadonlyArray<Vec2World>) => flattenVec2(projectWorldPoints(pts, ppm, offsetX, offsetY));
  const dash = [st.traco_m * ppm, st.espaco_m * ppm];
  const ldpDash = [LDP_DASH_M * ppm, LDP_DASH_M * ppm];

  const roads = useMemo(
    () => objects.filter((o): o is SicroRoadObject_parity => isParityRoad(o) && o.visible !== false),
    [objects],
  );
  const roundabouts = useMemo(
    () => objects.filter((o): o is SicroRoundaboutObject_parity => isParityRoundabout(o) && o.visible !== false),
    [objects],
  );
  const meshes = useMemo<RoadMesh[]>(() => roads.map((r) => buildRoadMesh(r, st)), [roads, st]);
  const pavedPolys = useMemo(() => meshes.map((m) => m.pavedPoly), [meshes]);
  const roundaboutDisks = useMemo(
    () => roundabouts.map((rb) => buildRoundaboutDiskPolygon(rb, 96, 0)),
    [roundabouts],
  );
  const sidewalkPolys = useMemo(
    () => meshes.map((m) => m.sidewalkPoly).filter((p): p is Vec2World[] => p !== null),
    [meshes],
  );
  const rbSidewalkDisks = useMemo(
    () =>
      st.calcada === "nenhuma"
        ? []
        : roundabouts.map((rb) => buildRoundaboutDiskPolygon(rb, 96, st.calcada_m)),
    [roundabouts, st],
  );

  const strokeSegs = (
    key: string,
    segs: ReadonlyArray<ReadonlyArray<Vec2World>>,
    stroke: string,
    width: number,
    dashArr?: number[],
  ) =>
    segs
      .filter((seg) => seg.length >= 2)
      .map((seg, i) => (
        <Line
          key={`${key}_${i}`}
          points={P(seg)}
          stroke={stroke}
          strokeWidth={width}
          dash={dashArr}
          lineCap="butt"
          lineJoin="round"
          listening={false}
        />
      ));

  const showSidewalkEdge =
    st.calcada === "hachura" || st.calcada === "linha" || (st.calcada === "cinza" && st.tema !== "escuro");
  const sidewalkEdgeW = Math.max(0.5, st.borda_px * 0.5);
  const swFill = sidewalkFill(st, colors);

  return (
    <Fragment>
      {/* ---------- 1. Calçadas ---------- */}
      <Group listening={false}>
        {st.calcada !== "nenhuma" && st.calcada !== "linha" && (
          <>
            {roundabouts.map((rb) => (
              <Circle
                key={`sw_rb_${rb.id}`} name={`obj_${rb.id}`}
                x={rb.cx * ppm + offsetX}
                y={rb.cy * ppm + offsetY}
                radius={(rb.r_m + rb.largura_m / 2 + st.calcada_m) * ppm}
                {...swFill}
              />
            ))}
            {meshes.map((m) =>
              m.sidewalkPoly ? (
                <Line key={`sw_${m.road.id}`} name={`obj_${m.road.id}`} points={P(m.sidewalkPoly)} closed {...swFill} />
              ) : null,
            )}
          </>
        )}
        {showSidewalkEdge &&
          meshes.map((m) => {
            if (!m.sidewalkPoly) return null;
            const obstacles = [
              ...sidewalkPolys.filter((p) => p !== m.sidewalkPoly),
              ...rbSidewalkDisks,
            ];
            const d = m.pavedHalf + m.sidewalkM;
            return (
              <Group key={`swe_${m.road.id}`} name={`obj_${m.road.id}`}>
                {strokeSegs(
                  `swe_l_${m.road.id}`,
                  clipPolylineAgainstPolygons(offsetLine(m.samplesWorld, d), obstacles).segments,
                  st.borda,
                  sidewalkEdgeW,
                )}
                {strokeSegs(
                  `swe_r_${m.road.id}`,
                  clipPolylineAgainstPolygons(offsetLine(m.samplesWorld, -d), obstacles).segments,
                  st.borda,
                  sidewalkEdgeW,
                )}
              </Group>
            );
          })}
        {showSidewalkEdge &&
          roundabouts.map((rb) => (
            <Group key={`swe_rb_${rb.id}`} name={`obj_${rb.id}`}>
              {strokeSegs(
                `swe_rb_${rb.id}`,
                clipPolylineAgainstPolygons(
                  discretizeCircle(rb.cx, rb.cy, rb.r_m + rb.largura_m / 2 + st.calcada_m, 0, Math.PI * 2, 180),
                  sidewalkPolys,
                ).segments,
                st.borda,
                sidewalkEdgeW,
              )}
            </Group>
          ))}
      </Group>

      {/* ---------- 2. Asfalto ---------- */}
      <Group>
        {meshes.map((m) => (
          <Line
            key={`as_${m.road.id}`}
            id={m.road.id}
            points={P(m.pavedPoly)}
            closed
            fill={surfaceFill(m.road.superficie, st, colors)}
            onClick={() => onSelect?.(m.road.id)}
            onTap={() => onSelect?.(m.road.id)}
            draggable={!!onObjectChange && isSel(m.road.id)}
            onDragEnd={(e) => {
              // Arrasto pelo corpo: translada os quatro pontos (metros).
              const dx = e.target.x() / Math.max(ppm, 0.0001);
              const dy = e.target.y() / Math.max(ppm, 0.0001);
              e.target.position({ x: 0, y: 0 });
              if (!onObjectChange || (dx === 0 && dy === 0)) return;
              const r = m.road;
              onObjectChange(r.id, {
                ax: r.ax + dx, ay: r.ay + dy, bx: r.bx + dx, by: r.by + dy,
                cx1: r.cx1 + dx, cy1: r.cy1 + dy, cx2: r.cx2 + dx, cy2: r.cy2 + dy,
              });
            }}
          />
        ))}
        {roundabouts.map((rb) => (
          <Circle
            key={`as_rb_${rb.id}`}
            id={rb.id}
            x={rb.cx * ppm + offsetX}
            y={rb.cy * ppm + offsetY}
            radius={(rb.r_m + rb.largura_m / 2) * ppm}
            fill={surfaceFill(rb.superficie, st, colors)}
            onClick={() => onSelect?.(rb.id)}
            onTap={() => onSelect?.(rb.id)}
            draggable={!!onObjectChange && isSel(rb.id)}
            onDragEnd={(e) => {
              onObjectChange?.(rb.id, { cx: canvasToWorldX(e.target.x()), cy: canvasToWorldY(e.target.y()) });
            }}
          />
        ))}
      </Group>

      {/* ---------- 3. Meio-fio e sinalização ---------- */}
      <Group listening={false}>
        {meshes.map((m) => {
          const obstacles: Vec2World[][] = [
            ...pavedPolys.filter((p) => p !== m.pavedPoly),
            ...roundaboutDisks,
          ];
          const clipped = (line: Vec2World[]) => clipPolylineAgainstPolygons(line, obstacles).segments;
          const els: JSX.Element[] = [];

          // meio-fio
          els.push(
            ...strokeSegs(`e_l_${m.road.id}`, clipped(offsetLine(m.samplesWorld, m.pavedHalf)), st.borda, st.borda_px),
            ...strokeSegs(`e_r_${m.road.id}`, clipped(offsetLine(m.samplesWorld, -m.pavedHalf)), st.borda, st.borda_px),
          );
          if (m.road.superficie !== "asfalto") return <Group key={`mk_${m.road.id}`} name={`obj_${m.road.id}`}>{els}</Group>;

          // eixo
          const eixo = resolveParityEixo(m.road);
          if (eixo === "amarela_dupla" || eixo === "amarela_mista") {
            els.push(
              ...strokeSegs(`c1_${m.road.id}`, clipped(offsetLine(m.samplesWorld, EIXO_GAP_M)), st.amarela, st.marcacao_px),
              ...strokeSegs(
                `c2_${m.road.id}`,
                clipped(offsetLine(m.samplesWorld, -EIXO_GAP_M)),
                st.amarela,
                st.marcacao_px,
                eixo === "amarela_mista" ? dash : undefined,
              ),
            );
          } else if (eixo === "amarela_trac" || eixo === "branca_trac") {
            els.push(
              ...strokeSegs(
                `c_${m.road.id}`,
                clipped(m.samplesWorld),
                eixo === "amarela_trac" ? st.amarela : st.branca,
                st.marcacao_px,
                dash,
              ),
            );
          }

          // faixas
          const lanes = laneCount(m.road, st);
          if (lanes > 1) {
            if (m.road.mao_dupla) {
              for (let k = 1; k < lanes; k++) {
                const d = k * (m.hw / lanes);
                els.push(
                  ...strokeSegs(`f_l${k}_${m.road.id}`, clipped(offsetLine(m.samplesWorld, d)), st.branca, st.marcacao_px, dash),
                  ...strokeSegs(`f_r${k}_${m.road.id}`, clipped(offsetLine(m.samplesWorld, -d)), st.branca, st.marcacao_px, dash),
                );
              }
            } else {
              for (let k = 1; k < lanes; k++) {
                const d = -m.hw + k * (m.road.largura_m / lanes);
                els.push(
                  ...strokeSegs(`f_${k}_${m.road.id}`, clipped(offsetLine(m.samplesWorld, d)), st.branca, st.marcacao_px, dash),
                );
              }
            }
          }

          // linha de bordo (só com acostamento)
          if (m.pavedHalf > m.hw + 1e-6) {
            els.push(
              ...strokeSegs(`b_l_${m.road.id}`, clipped(offsetLine(m.samplesWorld, m.hw)), st.branca, st.marcacao_px),
              ...strokeSegs(`b_r_${m.road.id}`, clipped(offsetLine(m.samplesWorld, -m.hw)), st.branca, st.marcacao_px),
            );
          }
          return <Group key={`mk_${m.road.id}`} name={`obj_${m.road.id}`}>{els}</Group>;
        })}

        {/* Rotatória por cima do que as vias deixaram no disco: anel, ilha,
            meio-fio externo só fora das entradas, ilha contínua, faixas do anel,
            dê a preferência nas bocas. */}
        {roundabouts.map((rb) => {
          const cx = rb.cx * ppm + offsetX;
          const cy = rb.cy * ppm + offsetY;
          const outerR = rb.r_m + rb.largura_m / 2;
          const innerR = Math.max(0, rb.r_m - rb.largura_m / 2);
          const els: JSX.Element[] = [];
          els.push(
            <Circle key="ring" x={cx} y={cy} radius={outerR * ppm + 0.5} fill={surfaceFill(rb.superficie, st, colors)} />,
          );
          if (innerR >= 0.5) {
            els.push(
              <Circle key="island" x={cx} y={cy} radius={innerR * ppm} fill={rb.inner_color ?? colors.ilha} />,
            );
          }
          els.push(
            ...strokeSegs(
              `rb_o_${rb.id}`,
              clipPolylineAgainstPolygons(discretizeCircle(rb.cx, rb.cy, outerR, 0, Math.PI * 2, 180), pavedPolys).segments,
              st.borda,
              st.borda_px,
            ),
          );
          if (innerR >= 0.5) {
            els.push(
              <Circle key="island_edge" x={cx} y={cy} radius={innerR * ppm} stroke={st.borda} strokeWidth={st.borda_px} fillEnabled={false} />,
            );
          }
          // faixas do anel (contínuas em volta) ou o eixo antigo do anel
          if (st.faixas_auto && rb.superficie === "asfalto") {
            const n = Math.max(1, Math.round(rb.largura_m / PARITY_FAIXA_LARGURA_M));
            for (let k = 1; k < n; k++) {
              const r = innerR + k * (rb.largura_m / n);
              els.push(
                <Circle key={`lane_${k}`} x={cx} y={cy} radius={r * ppm} stroke={st.branca} strokeWidth={st.marcacao_px} dash={dash} fillEnabled={false} />,
              );
            }
          } else if (rb.marcacao && rb.marcacao !== "nenhuma") {
            els.push(
              <Circle
                key="mid"
                x={cx}
                y={cy}
                radius={((outerR + innerR) / 2) * ppm}
                stroke={rb.marcacao === "amarela" ? st.amarela : st.branca}
                strokeWidth={st.marcacao_px}
                dash={dash}
                fillEnabled={false}
              />,
            );
          }
          for (const m of meshes) {
            if (m.road.superficie !== "asfalto") continue;
            const seg = giveWaySegment(m, rb);
            if (seg) els.push(...strokeSegs(`ldp_${rb.id}_${m.road.id}`, [seg], st.branca, st.marcacao_px * 1.5, ldpDash));
          }
          return <Group key={`rb_${rb.id}`} name={`obj_${rb.id}`}>{els}</Group>;
        })}
      </Group>

      {/* ---------- 4. Handles ---------- */}
      {selectedId && (
        <Group>
          {meshes
            .filter((m) => m.road.id === selectedId)
            .map((m) => {
              const pt = (x: number, y: number) => projectWorldPoints([{ x, y }], ppm, offsetX, offsetY)[0]!;
              const a = pt(m.road.ax, m.road.ay);
              const b = pt(m.road.bx, m.road.by);
              const c1 = pt(m.road.cx1, m.road.cy1);
              const c2 = pt(m.road.cx2, m.road.cy2);
              const canDrag = onObjectChange !== undefined;
              const roadId = m.road.id;
              return (
                <Group key={`h_${roadId}`} name={`obj_${roadId}`}>
                  <Line points={[a.x, a.y, c1.x, c1.y]} stroke={SELECTION_GUIDE} strokeWidth={1} dash={[3, 3]} listening={false} />
                  <Line points={[b.x, b.y, c2.x, c2.y]} stroke={SELECTION_GUIDE} strokeWidth={1} dash={[3, 3]} listening={false} />
                  {/* Âncoras arrastam o controle junto (preserva a curvatura). */}
                  <Circle
                    x={a.x}
                    y={a.y}
                    radius={7}
                    fill={SELECTION}
                    stroke="#1a1a1a"
                    strokeWidth={2}
                    draggable={canDrag}
                    onDragEnd={(e) => {
                      if (!onObjectChange) return;
                      const nx = canvasToWorldX(e.target.x());
                      const ny = canvasToWorldY(e.target.y());
                      onObjectChange(roadId, {
                        ax: nx,
                        ay: ny,
                        cx1: m.road.cx1 + (nx - m.road.ax),
                        cy1: m.road.cy1 + (ny - m.road.ay),
                      });
                    }}
                  />
                  <Circle
                    x={b.x}
                    y={b.y}
                    radius={7}
                    fill={SELECTION}
                    stroke="#1a1a1a"
                    strokeWidth={2}
                    draggable={canDrag}
                    onDragEnd={(e) => {
                      if (!onObjectChange) return;
                      const nx = canvasToWorldX(e.target.x());
                      const ny = canvasToWorldY(e.target.y());
                      onObjectChange(roadId, {
                        bx: nx,
                        by: ny,
                        cx2: m.road.cx2 + (nx - m.road.bx),
                        cy2: m.road.cy2 + (ny - m.road.by),
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
                      onObjectChange(roadId, { cx1: canvasToWorldX(e.target.x()), cy1: canvasToWorldY(e.target.y()) });
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
                      onObjectChange(roadId, { cx2: canvasToWorldX(e.target.x()), cy2: canvasToWorldY(e.target.y()) });
                    }}
                  />
                </Group>
              );
            })}
          {roundabouts
            .filter((rb) => rb.id === selectedId)
            .map((rb) => {
              const canDrag = onObjectChange !== undefined;
              const rbId = rb.id;
              const cx = rb.cx * ppm + offsetX;
              const cy = rb.cy * ppm + offsetY;
              return (
                <Group key={`h_rb_${rbId}`} name={`obj_${rbId}`}>
                  <Circle
                    x={cx}
                    y={cy}
                    radius={7}
                    fill={SELECTION}
                    stroke="#1a1a1a"
                    strokeWidth={2}
                    draggable={canDrag}
                    onDragEnd={(e) => {
                      if (!onObjectChange) return;
                      onObjectChange(rbId, { cx: canvasToWorldX(e.target.x()), cy: canvasToWorldY(e.target.y()) });
                    }}
                  />
                  <Circle
                    x={cx}
                    y={cy}
                    radius={(rb.r_m + rb.largura_m / 2) * ppm}
                    stroke={SELECTION}
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
