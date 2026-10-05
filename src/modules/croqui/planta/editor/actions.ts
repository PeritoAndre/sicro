/** Ações sobre a seleção e a folha. */
import { bbox, type Pt } from "../model/geom";
import { deleteWalls } from "../model/walls";
import { SCALES, sheetSize, type SicroPlantaDoc } from "../model/schema";
import { SYMBOL_BY_ID } from "../model/symbols";
import type { Sel } from "./store";

export function deleteSelection(d: SicroPlantaDoc, sel: Sel[]): SicroPlantaDoc {
  const ids = (k: Sel["kind"]) => new Set(sel.filter((s) => s.kind === k).map((s) => s.id));
  const walls = [...ids("wall")];
  let out = walls.length ? deleteWalls(d, walls) : d;
  const op = ids("opening");
  const rm = ids("room");
  const it = ids("item");
  const ev = ids("evidence");
  const pp = ids("person");
  const tj = ids("traj");
  const tx = ids("text");
  const dm = ids("dim");
  out = {
    ...out,
    openings: out.openings.filter((x) => !op.has(x.id)),
    rooms: out.rooms.filter((x) => !rm.has(x.id)),
    items: out.items.filter((x) => !it.has(x.id)),
    evidences: out.evidences.filter((x) => !ev.has(x.id)),
    people: out.people.filter((x) => !pp.has(x.id)),
    trajectories: out.trajectories.filter((x) => !tj.has(x.id)),
    texts: out.texts.filter((x) => !tx.has(x.id)),
    dims: out.dims.filter((x) => !dm.has(x.id)),
  };
  if (sel.some((s) => s.kind === "compass")) out = { ...out, options: { ...out.options, compass: { ...out.options.compass, show: false } } };
  if (sel.some((s) => s.kind === "bg")) out = { ...out, background: null };
  return out;
}

/** Tudo que foi desenhado (para enquadrar a folha). */
export function contentPoints(d: SicroPlantaDoc): Pt[] {
  const pts: Pt[] = [...d.nodes];
  for (const i of d.items) {
    const r = Math.hypot(i.w, i.d) / 2;
    pts.push({ x: i.x - r, y: i.y - r }, { x: i.x + r, y: i.y + r });
    void SYMBOL_BY_ID;
  }
  for (const e of d.evidences) pts.push(e);
  for (const p of d.people) pts.push({ x: p.x - 1, y: p.y - 1 }, { x: p.x + 1, y: p.y + 1 });
  for (const t of d.trajectories) pts.push(t.a, t.b);
  for (const t of d.texts) pts.push(t);
  for (const x of d.dims) pts.push(x.a, x.b);
  return pts;
}

/** Folha em volta da planta: menor escala que cabe (A4/A3) ou tamanho livre, centrada no desenho. */
export function fitSheetToContent(d: SicroPlantaDoc): SicroPlantaDoc {
  const box = bbox(contentPoints(d));
  if (!box) return d;
  const margin = 1.6;
  const needW = box.x1 - box.x0 + margin * 2;
  const needH = box.y1 - box.y0 + margin * 2;
  let { w, h } = d.sheet;
  let scale = d.sheet.scale;
  if (d.sheet.paper === "custom") {
    w = needW;
    h = needH;
  } else {
    const fits = SCALES.find((s) => {
      const z = sheetSize(d.sheet.paper as "A4" | "A3", d.sheet.orientation, s);
      return z.w >= needW && z.h >= needH;
    });
    scale = fits ?? SCALES[SCALES.length - 1]!;
    ({ w, h } = sheetSize(d.sheet.paper as "A4" | "A3", d.sheet.orientation, scale));
  }
  const cx = (box.x0 + box.x1) / 2;
  const cy = (box.y0 + box.y1) / 2;
  const sheet = { ...d.sheet, scale, w, h, x: cx - w / 2, y: cy - h / 2 };
  return { ...d, sheet, options: { ...d.options, compass: { ...d.options.compass, x: sheet.x + w - 1.1, y: sheet.y + 1.3 } } };
}

export function sheetLabel(d: SicroPlantaDoc): string {
  const s = d.sheet;
  const size = `${s.w.toFixed(1).replace(".", ",")} × ${s.h.toFixed(1).replace(".", ",")} m`;
  return s.paper === "custom" ? `Folha ${size}` : `Folha ${s.paper} ${s.orientation} · 1:${s.scale} · ${size}`;
}
