/** Desenho da planta no Konva (unidades do mundo: 1 m = 100). O mesmo desenho serve à tela e ao PNG. */
import { Arrow, Circle, Group, Line, Rect, Shape, Text } from "react-konva";
import type Konva from "konva";
import { add, fmtM, mul, norm, perp, sub, type Pt } from "../model/geom";
import { openingDrawing, type OpeningDrawing } from "../model/openings";
import type { WallGeom } from "../model/walls";
import type { RoomGeom } from "../model/rooms";
import type { DimLine } from "../model/dims";
import type { MeasureLine } from "../model/evidence";
import { drawPrims, PERSON_PRIMS, PERSON_SIZE, SYMBOL_BY_ID, type PrimStyle } from "../model/symbols";
import type { PDimension, PEvidence, PItem, POpening, PPerson, PSheet, PText, PTrajectory, PWall } from "../model/schema";
import { M } from "./store";

export interface PlanStyle {
  editor: boolean;
  wallFill: string;
  muroFill: string;
  ink: string;
  dimInk: string;
  prim: PrimStyle;
  tints: boolean;
}

export const EDITOR_STYLE: PlanStyle = {
  editor: true,
  wallFill: "#2b3240",
  muroFill: "#6b7280",
  ink: "#2b3240",
  dimInk: "#475569",
  prim: { ink: "#7c8697", paper: "#ffffff", tone: "#e8ebf0", body: "#fbe9e9", bodyInk: "#9f2a2a", lw: 0.015 },
  tints: true,
};

export const EXPORT_STYLE: PlanStyle = {
  editor: false,
  wallFill: "#111111",
  muroFill: "#555555",
  ink: "#111111",
  dimInk: "#374151",
  prim: { ink: "#4b5563", paper: "#ffffff", tone: "#e5e7eb", body: "#fde8e8", bodyInk: "#7f1d1d", lw: 0.015 },
  tints: false,
};

export const TINTS = ["#fbf3e2", "#e9f3f9", "#edf4e9", "#f3eef8", "#e8f4f3", "#f9ecec", "#eef0f9"];

const flat = (pts: Pt[]) => pts.flatMap((p) => [p.x * M, p.y * M]);
const W = (v: number) => v * M;

/** Grade da folha (linhas de 1 em 1 unidade de grade, mais fortes a cada 5). */
export function GridShape({ sheet }: { sheet: PSheet }) {
  return (
    <Shape
      listening={false}
      sceneFunc={(ctx) => {
        const c = ctx._context;
        const x0 = W(sheet.x);
        const y0 = W(sheet.y);
        const x1 = W(sheet.x + sheet.w);
        const y1 = W(sheet.y + sheet.h);
        const g = Math.max(0.1, sheet.grid);
        c.lineWidth = 1;
        let i = 0;
        for (let x = sheet.x; x <= sheet.x + sheet.w + 1e-6; x += g, i++) {
          c.strokeStyle = i % 5 === 0 ? "#d7dde5" : "#eef1f4";
          c.beginPath();
          c.moveTo(W(x), y0);
          c.lineTo(W(x), y1);
          c.stroke();
        }
        i = 0;
        for (let y = sheet.y; y <= sheet.y + sheet.h + 1e-6; y += g, i++) {
          c.strokeStyle = i % 5 === 0 ? "#d7dde5" : "#eef1f4";
          c.beginPath();
          c.moveTo(x0, W(y));
          c.lineTo(x1, W(y));
          c.stroke();
        }
      }}
    />
  );
}

export function WallShape({ wall, geom, style, selected }: { wall: PWall; geom: WallGeom; style: PlanStyle; selected: boolean }) {
  const fill = wall.kind === "muro" ? style.muroFill : wall.kind === "grade" ? "#ffffff" : style.wallFill;
  return (
    <Line
      name={`sel:wall:${wall.id}`}
      points={flat(geom.poly)}
      closed
      fill={fill}
      stroke={selected ? "#d7a84f" : wall.kind === "grade" ? style.ink : fill}
      strokeWidth={selected ? 3 : wall.kind === "grade" ? 1.2 : 0.6}
      dash={wall.kind === "grade" ? [6, 4] : undefined}
      perfectDrawEnabled={false}
    />
  );
}

function OpeningPaint({ dr, ink }: { dr: OpeningDrawing; ink: string }) {
  return (
    <Shape
      listening={false}
      sceneFunc={(ctx) => {
        const c = ctx._context;
        c.save();
        c.scale(M, M);
        c.lineCap = "round";
        c.strokeStyle = ink;
        for (const l of dr.lines) {
          c.lineWidth = 0.012;
          c.setLineDash([]);
          c.beginPath();
          l.forEach((p, i) => (i ? c.lineTo(p.x, p.y) : c.moveTo(p.x, p.y)));
          c.stroke();
        }
        for (const l of dr.dashed) {
          c.lineWidth = 0.01;
          c.setLineDash([0.06, 0.045]);
          c.beginPath();
          l.forEach((p, i) => (i ? c.lineTo(p.x, p.y) : c.moveTo(p.x, p.y)));
          c.stroke();
        }
        for (const a of dr.arcs) {
          c.lineWidth = 0.008;
          c.setLineDash([0.05, 0.035]);
          c.beginPath();
          c.arc(a.c.x, a.c.y, a.r, a.a0, a.a1, a.ccw);
          c.stroke();
        }
        c.restore();
      }}
    />
  );
}

export function OpeningShape({ o, geom, thickness, style, selected }: { o: POpening; geom: WallGeom; thickness: number; style: PlanStyle; selected: boolean }) {
  const dr = openingDrawing(geom, o, thickness);
  return (
    <Group>
      <Line name={`sel:opening:${o.id}`} points={flat(dr.gap)} closed fill="#ffffff" stroke={selected ? "#d7a84f" : undefined} strokeWidth={selected ? 2.5 : 0} />
      <OpeningPaint dr={dr} ink={selected ? "#b7801f" : style.ink} />
    </Group>
  );
}

export function RoomTint({ rg, index, selected }: { rg: RoomGeom; index: number; selected: boolean }) {
  return (
    <Line
      name={`sel:roomfloor:${rg.room.id}`}
      points={flat(rg.inner)}
      closed
      fill={TINTS[index % TINTS.length]}
      stroke={selected ? "#d7a84f" : undefined}
      strokeWidth={selected ? 3 : 0}
      perfectDrawEnabled={false}
    />
  );
}

export function RoomLabel({ rg, at, style, showArea, selected }: { rg: RoomGeom; at: Pt; style: PlanStyle; showArea: boolean; selected: boolean }) {
  const name = rg.room.name.toUpperCase();
  return (
    <Group name={`sel:room:${rg.room.id}`} x={W(at.x)} y={W(at.y)}>
      <Text text={name} fontSize={22} fontStyle="bold" fontFamily="'Barlow Semi Condensed', 'Arial Narrow', sans-serif" fill={selected ? "#b7801f" : style.ink} align="center" width={600} offsetX={300} offsetY={showArea ? 20 : 11} letterSpacing={1.2} />
      {showArea && <Text text={`${fmtM(rg.area, 1)} m²`} fontSize={17} fontFamily="'JetBrains Mono', monospace" fill={style.dimInk} align="center" width={400} offsetX={200} offsetY={-6} />}
    </Group>
  );
}

export function ItemShape({ item, style, selected }: { item: PItem; style: PlanStyle; selected: boolean }) {
  const def = SYMBOL_BY_ID.get(item.symbol);
  if (!def) return null;
  return (
    <Group name={`sel:item:${item.id}`} x={W(item.x)} y={W(item.y)} rotation={item.rot}>
      <Shape
        sceneFunc={(ctx, shape) => {
          const c = ctx._context;
          c.save();
          c.scale(M, M);
          drawPrims(c, def.prims, item.w, item.d, style.prim);
          c.restore();
          void shape;
        }}
        hitFunc={(ctx, shape) => {
          ctx.beginPath();
          ctx.rect(W(-item.w / 2), W(-item.d / 2), W(item.w), W(item.d));
          ctx.closePath();
          ctx.fillStrokeShape(shape);
        }}
      />
      {selected && <Rect x={W(-item.w / 2)} y={W(-item.d / 2)} width={W(item.w)} height={W(item.d)} stroke="#d7a84f" strokeWidth={2.5} dash={[8, 5]} listening={false} />}
      {item.label && <Text text={item.label} fontSize={16} fontFamily="'Source Sans 3', sans-serif" fill={style.ink} y={W(item.d / 2) + 4} width={400} offsetX={200} align="center" rotation={0} listening={false} />}
    </Group>
  );
}

export function PersonShape({ p, style, selected }: { p: PPerson; style: PlanStyle; selected: boolean }) {
  const sz = PERSON_SIZE[p.pose];
  return (
    <Group name={`sel:person:${p.id}`} x={W(p.x)} y={W(p.y)} rotation={p.rot}>
      <Shape
        sceneFunc={(ctx) => {
          const c = ctx._context;
          c.save();
          c.scale(M, M);
          drawPrims(c, PERSON_PRIMS[p.pose], sz.w, sz.d, { ...style.prim, lw: 0.012 }, true);
          c.restore();
        }}
        hitFunc={(ctx, shape) => {
          ctx.beginPath();
          ctx.rect(W(-sz.w / 2), W(-sz.d / 2), W(sz.w), W(sz.d));
          ctx.closePath();
          ctx.fillStrokeShape(shape);
        }}
      />
      {selected && <Rect x={W(-sz.w / 2) - 4} y={W(-sz.d / 2) - 4} width={W(sz.w) + 8} height={W(sz.d) + 8} stroke="#d7a84f" strokeWidth={2.5} dash={[8, 5]} listening={false} />}
    </Group>
  );
}

export function EvidenceMarker({ e, selected }: { e: PEvidence; selected: boolean }) {
  return (
    <Group name={`sel:evidence:${e.id}`} x={W(e.x)} y={W(e.y)}>
      <Circle radius={17} fill="#facc15" stroke={selected ? "#b7801f" : "#111111"} strokeWidth={selected ? 3.5 : 2} />
      <Text text={e.label} fontSize={e.label.length > 1 ? 15 : 19} fontStyle="bold" fontFamily="'Source Sans 3', sans-serif" fill="#111111" width={40} height={34} offsetX={20} offsetY={17} align="center" verticalAlign="middle" listening={false} />
    </Group>
  );
}

export function MeasureLines({ lines }: { lines: MeasureLine[] }) {
  return (
    <Group listening={false}>
      {lines.map((l, i) => {
        const mid = add(l.from, mul(sub(l.to, l.from), 0.5));
        const d = norm(sub(l.to, l.from));
        const n = perp(d);
        const vertical = Math.abs(d.y) > Math.abs(d.x);
        return (
          <Group key={i}>
            <Line points={flat([l.from, l.to])} stroke="#1d4ed8" strokeWidth={1.4} dash={[4, 4]} />
            <Text
              text={fmtM(l.value)}
              x={W(mid.x + n.x * 0.12)}
              y={W(mid.y + n.y * 0.12)}
              fontSize={15}
              fontFamily="'JetBrains Mono', monospace"
              fill="#1d4ed8"
              offsetX={vertical ? 0 : 20}
              offsetY={8}
            />
          </Group>
        );
      })}
    </Group>
  );
}

export function TrajShape({ t, selected }: { t: PTrajectory; selected: boolean }) {
  const d = norm(sub(t.b, t.a));
  return (
    <Group name={`sel:traj:${t.id}`}>
      <Arrow points={flat([t.a, t.b])} stroke={selected ? "#d7a84f" : t.color} fill={selected ? "#d7a84f" : t.color} strokeWidth={2.4} dash={[12, 7]} pointerLength={14} pointerWidth={12} hitStrokeWidth={14} />
      <Text text={t.label} x={W(t.a.x) - d.x * 22} y={W(t.a.y) - d.y * 22} fontSize={18} fontStyle="bold" fontFamily="'Source Sans 3', sans-serif" fill={t.color} offsetX={10} offsetY={9} listening={false} />
    </Group>
  );
}

export function TextShape({ t, selected }: { t: PText; selected: boolean }) {
  return (
    <Text
      name={`sel:text:${t.id}`}
      text={t.text}
      x={W(t.x)}
      y={W(t.y)}
      rotation={t.rot}
      fontSize={W(t.size)}
      fontStyle={t.bold ? "bold" : "normal"}
      fontFamily="'Source Sans 3', sans-serif"
      fill={t.color}
      stroke={selected ? "#d7a84f" : undefined}
      strokeWidth={selected ? 0.6 : 0}
    />
  );
}

/** Cota: linha afastada de ab, linhas de chamada, tiques a 45° e valor. */
export function DimPaint({ a, b, offsetDir, offset, value, ink, name, selected }: { a: Pt; b: Pt; offsetDir: Pt; offset: number; value: number; ink: string; name?: string; selected?: boolean }) {
  const o = mul(offsetDir, offset);
  const A = add(a, o);
  const B = add(b, o);
  const d = norm(sub(B, A));
  const ang = (Math.atan2(d.y, d.x) * 180) / Math.PI;
  const upright = ang > 90 || ang < -90 ? ang + 180 : ang;
  const mid = add(A, mul(sub(B, A), 0.5));
  const tick = (p: Pt) => {
    const t1 = add(p, mul({ x: d.x + -d.y, y: d.y + d.x }, 0.06));
    const t2 = add(p, mul({ x: d.x + -d.y, y: d.y + d.x }, -0.06));
    return flat([t1, t2]);
  };
  const ext = (p: Pt, q: Pt) => flat([add(p, mul(offsetDir, Math.sign(offset) * 0.08)), add(q, mul(offsetDir, Math.sign(offset) * 0.08))]);
  const c = selected ? "#b7801f" : ink;
  const textOff = mul(offsetDir, Math.sign(offset || 1) * 0.13);
  return (
    <Group name={name}>
      <Line points={flat([A, B])} stroke={c} strokeWidth={1.2} hitStrokeWidth={12} />
      <Line points={ext(a, A)} stroke={c} strokeWidth={0.8} listening={false} />
      <Line points={ext(b, B)} stroke={c} strokeWidth={0.8} listening={false} />
      <Line points={tick(A)} stroke={c} strokeWidth={1.8} listening={false} />
      <Line points={tick(B)} stroke={c} strokeWidth={1.8} listening={false} />
      <Text
        text={fmtM(value)}
        x={W(mid.x + textOff.x)}
        y={W(mid.y + textOff.y)}
        rotation={upright}
        fontSize={16}
        fontFamily="'JetBrains Mono', monospace"
        fill={c}
        width={120}
        offsetX={60}
        offsetY={8}
        align="center"
        listening={false}
      />
    </Group>
  );
}

export function AutoDims({ dims, ink }: { dims: DimLine[]; ink: string }) {
  return (
    <Group listening={false}>
      {dims.map((d, i) => (
        <DimPaint key={i} a={d.a} b={d.b} offsetDir={d.out} offset={d.offset} value={d.value} ink={ink} />
      ))}
    </Group>
  );
}

export function ManualDim({ dim, ink, selected }: { dim: PDimension; ink: string; selected: boolean }) {
  const n = perp(norm(sub(dim.b, dim.a)));
  return <DimPaint name={`sel:dim:${dim.id}`} a={dim.a} b={dim.b} offsetDir={n} offset={dim.offset} value={Math.hypot(dim.b.x - dim.a.x, dim.b.y - dim.a.y)} ink={ink} selected={selected} />;
}

/** Medida interna escrita junto da face menor de cada parede. */
export function WallMeasure({ geom, ink }: { geom: WallGeom; ink: string }) {
  const { A, dir, nrm, L } = geom.frame;
  const side = geom.faceLeft <= geom.faceRight ? 1 : -1;
  const t = geom.poly.length ? Math.abs((geom.poly[0]!.x - A.x) * nrm.x + (geom.poly[0]!.y - A.y) * nrm.y) : 0.075;
  const mid = add(add(A, mul(dir, L / 2)), mul(nrm, side * (t + 0.14)));
  const ang = (Math.atan2(dir.y, dir.x) * 180) / Math.PI;
  const upright = ang > 90 || ang < -90 ? ang + 180 : ang;
  if (geom.inner < 0.3) return null;
  return (
    <Text text={fmtM(geom.inner)} x={W(mid.x)} y={W(mid.y)} rotation={upright} fontSize={14} fontFamily="'JetBrains Mono', monospace" fill={ink} width={100} offsetX={50} offsetY={7} align="center" listening={false} />
  );
}

export function Compass({ x, y, deg, selected }: { x: number; y: number; deg: number; selected: boolean }) {
  const r = 32;
  return (
    <Group name="sel:compass:compass" x={W(x)} y={W(y)} rotation={deg}>
      <Circle radius={r} stroke={selected ? "#d7a84f" : "#111111"} strokeWidth={selected ? 2.5 : 1.4} fill="#ffffff" />
      <Line points={[0, -r - 8, 10, 0, 0, -6]} closed fill="#111111" />
      <Line points={[0, -r - 8, -10, 0, 0, -6]} closed fill="#ffffff" stroke="#111111" strokeWidth={1} />
      <Line points={[0, r + 8, 10, 0, 0, 6]} closed fill="#ffffff" stroke="#111111" strokeWidth={1} />
      <Line points={[0, r + 8, -10, 0, 0, 6]} closed fill="#111111" />
      <Text text="N" fontSize={20} fontStyle="bold" fontFamily="'Barlow Semi Condensed', sans-serif" fill="#111111" y={-r - 32} width={40} offsetX={20} align="center" listening={false} />
    </Group>
  );
}

export type { Konva };
