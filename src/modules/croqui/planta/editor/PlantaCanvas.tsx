/** Tela da planta: Konva com o zoom e a vista do SICRO, ferramentas, ímã, seleção e captura do PNG. */
import { forwardRef, useCallback, useEffect, useImperativeHandle, useMemo, useRef, useState } from "react";
import { flushSync } from "react-dom";
import { Circle, Group, Image as KonvaImage, Label, Layer, Line, Rect, Stage, Tag, Text } from "react-konva";
import type Konva from "konva";
import { convertFileSrc } from "@tauri-apps/api/core";
import { add, dist, dot, fmtM, interiorPoint, mul, norm, parseM, perp, sub, type Pt } from "../model/geom";
import { addWall, moveNodes, moveWallsBy, nodeMap, settleNode, wallGeometry, wallGeometryFull } from "../model/walls";
import { createRoomAt, nextRoomName, roomGeometry } from "../model/rooms";
import { autoDims } from "../model/dims";
import { clampT0, newOpening, openingGaps, snapToWall } from "../model/openings";
import { evidenceMeasures, nextEvidenceLabel } from "../model/evidence";
import { PERSON_SIZE, SYMBOL_BY_ID } from "../model/symbols";
import { OPENING_DEFAULTS, uid, type OpeningKind, type PItem, type SicroPlantaDoc } from "../model/schema";
import { snapPoint, type SnapGuide } from "./snap";
import { usePlanta, M, type Sel, type SelKind, type Tool } from "./store";
import {
  AutoDims,
  Compass,
  EDITOR_STYLE,
  EvidenceMarker,
  EXPORT_STYLE,
  GridShape,
  ItemShape,
  ManualDim,
  MeasureLines,
  OpeningShape,
  PersonShape,
  RoomLabel,
  RoomTint,
  TextShape,
  TrajShape,
  WallMeasure,
  WallShape,
} from "./PlanShapes";
import { askText } from "@components/Dialog/ask";
import styles from "./planta.module.css";

const W = (v: number) => v * M;
const ZMIN = 0.03;
const ZMAX = 40;

export interface PlantaCanvasHandle {
  capture(variant: "tecnico" | "limpo"): string | null;
  fit(): void;
  dropTool(tool: Tool, clientX: number, clientY: number): void;
}

interface Props {
  workspacePath: string;
  newWall: { thickness: number; kind: "parede" | "muro" | "grade" };
  onHint?: (hint: string) => void;
  /** Escala do fundo: devolve os dois pontos clicados. */
  onBgScalePoints?: (a: Pt, b: Pt) => void;
  title: string;
  message?: string | null;
}

type Gesture =
  | { t: "pan"; cx: number; cy: number; vx: number; vy: number }
  | { t: "move"; start: Pt; base: SicroPlantaDoc; sel: Sel[]; moved: boolean }
  | { t: "node"; nodeId: string; base: SicroPlantaDoc }
  | { t: "end"; kind: "traj" | "dim"; id: string; which: "a" | "b" }
  | { t: "dimOff"; id: string }
  | { t: "rot"; kind: "item" | "person"; id: string; c: Pt }
  | { t: "marquee"; a: Pt; b: Pt; add: boolean }
  | { t: "room"; a: Pt; b: Pt }
  | { t: "line"; kind: "dim" | "traj"; a: Pt; b: Pt };

const isTyping = (e: KeyboardEvent) => {
  const t = e.target as HTMLElement | null;
  return !!t && (t.tagName === "INPUT" || t.tagName === "TEXTAREA" || t.tagName === "SELECT" || t.isContentEditable);
};

/** Sobe da forma clicada até o grupo com nome "sel:tipo:id" ou "h:...". */
function pickName(node: Konva.Node | null): string | null {
  let n: Konva.Node | null = node;
  while (n && n.getType() !== "Stage") {
    const nm = n.name();
    if (nm && (nm.startsWith("sel:") || nm.startsWith("h:"))) return nm;
    n = n.getParent();
  }
  return null;
}

/** Encosta a peça na parede mais próxima (fundo na face) e gira junto, se estiver perto. */
function alignToWall(doc: SicroPlantaDoc, p: Pt, w: number, d: number): { x: number; y: number; rot: number } | null {
  const geo = wallGeometry(doc);
  let best: { foot: Pt; n: Pt; dir: Pt; dd: number; a: Pt; L: number; t: number } | null = null;
  for (const g of geo.values()) {
    const [p0, p1, p2, p3] = g.poly as [Pt, Pt, Pt, Pt];
    for (const [a, b] of [
      [p0, p1],
      [p3, p2],
    ] as const) {
      const ab = sub(b, a);
      const L = Math.hypot(ab.x, ab.y);
      if (L < 0.3) continue;
      const dir = norm(ab);
      const t = dot(sub(p, a), dir);
      if (t < 0 || t > L) continue;
      const foot = add(a, mul(dir, t));
      const dd = Math.hypot(p.x - foot.x, p.y - foot.y);
      // Normal para o lado do ponteiro.
      let n = perp(dir);
      if (dot(sub(p, foot), n) < 0) n = mul(n, -1);
      if (dd < d / 2 + 0.35 && (!best || dd < best.dd)) best = { foot, n, dir, dd, a, L, t };
    }
  }
  if (!best) return null;
  // Não passa das quinas daquele trecho de parede.
  const tc = best.L >= w ? Math.max(w / 2, Math.min(best.L - w / 2, best.t)) : best.L / 2;
  const foot = add(best.a, mul(best.dir, tc));
  const c = add(foot, mul(best.n, d / 2 + 0.005));
  // v (profundidade) aponta para fora da parede: rot faz o eixo y local virar n.
  const rot = (Math.atan2(best.n.y, best.n.x) * 180) / Math.PI - 90;
  return { x: c.x, y: c.y, rot: ((rot % 360) + 360) % 360 };
}

export const PlantaCanvas = forwardRef<PlantaCanvasHandle, Props>(function PlantaCanvas({ workspacePath, newWall, onHint, onBgScalePoints, title, message }, ref) {
  const wallThickness = newWall.thickness;
  const doc = usePlanta((s) => s.doc)!;
  const sel = usePlanta((s) => s.sel);
  const tool = usePlanta((s) => s.tool);
  const vp = usePlanta((s) => s.viewport);
  const st = usePlanta.getState;
  const hostRef = useRef<HTMLDivElement | null>(null);
  const stageRef = useRef<Konva.Stage | null>(null);
  const uiRef = useRef<Konva.Layer | null>(null);
  const [size, setSize] = useState({ w: 800, h: 600 });
  const [exportMode, setExportMode] = useState<null | "tecnico" | "limpo">(null);
  const [pointer, setPointer] = useState<Pt | null>(null);
  const [snapKind, setSnapKind] = useState<string>("free");
  const [guides, setGuides] = useState<SnapGuide[]>([]);
  const [chain, setChain] = useState<Pt | null>(null);
  const [gesture, setGesture] = useState<Gesture | null>(null);
  const gestureRef = useRef<Gesture | null>(null);
  gestureRef.current = gesture;
  const [space, setSpace] = useState(false);
  const altRef = useRef(false);
  const shiftRef = useRef(false);
  const [lengthInput, setLengthInput] = useState<string | null>(null);
  const [placeSide, setPlaceSide] = useState<1 | -1>(1);
  const [bgImg, setBgImg] = useState<HTMLImageElement | null>(null);
  const [bgFirst, setBgFirst] = useState<Pt | null>(null);

  // ---------- tamanho e enquadramento ----------
  useEffect(() => {
    const el = hostRef.current;
    if (!el) return;
    const ro = new ResizeObserver(() => setSize({ w: Math.max(200, el.clientWidth), h: Math.max(200, el.clientHeight) }));
    ro.observe(el);
    return () => ro.disconnect();
  }, []);
  const fit = useCallback(() => {
    const s = doc.sheet;
    const pad = 40;
    const scale = Math.min((size.w - pad * 2) / W(s.w), (size.h - pad * 2) / W(s.h));
    st().setViewport({ scale, x: (size.w - W(s.w) * scale) / 2 - W(s.x) * scale, y: (size.h - W(s.h) * scale) / 2 - W(s.y) * scale });
  }, [doc.sheet, size.w, size.h, st]);
  useEffect(() => {
    if (st().autoFit) fit();
  }, [fit, st]);

  // ---------- fundo ----------
  useEffect(() => {
    const bg = doc.background;
    if (!bg) {
      setBgImg(null);
      return;
    }
    const img = new window.Image();
    img.crossOrigin = "anonymous";
    img.onload = () => setBgImg(img);
    const abs = /^([a-zA-Z]:)?[\\/]/.test(bg.asset) ? bg.asset : `${workspacePath.replace(/[\\/]+$/, "")}/${bg.asset}`;
    img.src = convertFileSrc(abs);
  }, [doc.background?.asset, workspacePath]); // eslint-disable-line react-hooks/exhaustive-deps

  // ---------- geometria derivada ----------
  const geoFull = useMemo(() => wallGeometryFull(doc), [doc]);
  const geo = geoFull.walls;
  const rooms = useMemo(() => roomGeometry(doc), [doc]);
  const dims = useMemo(() => (doc.options.auto_dims ? autoDims(doc) : []), [doc]);
  const wallById = useMemo(() => new Map(doc.walls.map((w) => [w.id, w])), [doc.walls]);
  const selected = useCallback((kind: SelKind, id: string) => sel.some((s) => s.kind === kind && s.id === id), [sel]);
  const pxPerM = vp.scale * M;

  const toWorld = useCallback((sx: number, sy: number): Pt => {
    const v = st().viewport;
    return { x: (sx - v.x) / v.scale / M, y: (sy - v.y) / v.scale / M };
  }, [st]);
  const pointerWorld = (): Pt | null => {
    const p = stageRef.current?.getPointerPosition();
    return p ? toWorld(p.x, p.y) : null;
  };
  const snap = (raw: Pt, from?: Pt | null, exceptNode?: string) =>
    snapPoint(st().doc!, raw, { pxPerM: st().viewport.scale * M, magnet: !altRef.current, from, ortho: shiftRef.current && !!from, exceptNode });

  // ---------- colocar coisas ----------
  const openingPreview = useMemo(() => {
    if (!tool.startsWith("opening:") || !pointer) return null;
    const kind = tool.slice(8) as OpeningKind;
    const s = snapToWall(doc, pointer, 0.6);
    if (!s) return null;
    const g = geo.get(s.wall);
    const w = wallById.get(s.wall);
    if (!g || !w) return null;
    const width = OPENING_DEFAULTS[kind].width;
    const t0 = clampT0(g, Math.round((s.along - width / 2) / 0.05) * 0.05, width);
    return { o: { ...newOpening(kind, s.wall, t0, (s.side * placeSide) as 1 | -1), id: "_preview" }, g, w };
  }, [tool, pointer, doc, geo, wallById, placeSide]);

  const itemPreview = useMemo(() => {
    if (!pointer || !tool.startsWith("item:")) return null;
    const def = SYMBOL_BY_ID.get(tool.slice(5));
    if (!def) return null;
    const al = altRef.current ? null : alignToWall(doc, pointer, def.w, def.d);
    return { id: "_preview", symbol: def.id, x: al?.x ?? pointer.x, y: al?.y ?? pointer.y, rot: al?.rot ?? 0, w: def.w, d: def.d, label: null } as PItem;
  }, [pointer, tool, doc]);

  const placeAt = useCallback(
    async (t: Tool, p: Pt) => {
      const S = st();
      if (t.startsWith("opening:")) {
        const kind = t.slice(8) as OpeningKind;
        const s = snapToWall(S.doc!, p, 0.6);
        if (!s) return false;
        const g = wallGeometry(S.doc!).get(s.wall);
        if (!g) return false;
        const width = OPENING_DEFAULTS[kind].width;
        const o = newOpening(kind, s.wall, clampT0(g, Math.round((s.along - width / 2) / 0.05) * 0.05, width), (s.side * placeSide) as 1 | -1);
        S.apply((d) => ({ ...d, openings: [...d.openings, o] }));
        S.setSel([{ kind: "opening", id: o.id }]);
        return true;
      }
      if (t.startsWith("item:")) {
        const def = SYMBOL_BY_ID.get(t.slice(5));
        if (!def) return false;
        const al = altRef.current ? null : alignToWall(S.doc!, p, def.w, def.d);
        const it: PItem = { id: uid("i"), symbol: def.id, x: al?.x ?? p.x, y: al?.y ?? p.y, rot: al?.rot ?? 0, w: def.w, d: def.d, label: null };
        S.apply((d) => ({ ...d, items: [...d.items, it] }));
        S.setSel([{ kind: "item", id: it.id }]);
        return true;
      }
      if (t.startsWith("person:")) {
        const pose = t.slice(7) as "em_pe" | "caido";
        const id = uid("p");
        S.apply((d) => ({ ...d, people: [...d.people, { id, pose, x: p.x, y: p.y, rot: 0, label: pose === "caido" ? "Cadáver" : "Pessoa", descricao: "" }] }));
        S.setSel([{ kind: "person", id }]);
        return true;
      }
      if (t === "evidence") {
        const id = uid("e");
        S.apply((d) => ({
          ...d,
          evidences: [...d.evidences, { id, label: nextEvidenceLabel(d), tipo: "estojo", descricao: "", x: p.x, y: p.y, measure: "paredes", show_measure: false, points: [] }],
        }));
        S.setSel([{ kind: "evidence", id }]);
        return true;
      }
      if (t === "text") {
        const text = await askText({ title: "Texto na planta", confirmLabel: "Colocar" });
        if (!text?.trim()) return false;
        const id = uid("t");
        S.apply((d) => ({ ...d, texts: [...d.texts, { id, x: p.x, y: p.y, text: text.trim(), size: 0.25, color: "#111111", bold: false, rot: 0 }] }));
        S.setSel([{ kind: "text", id }]);
        return true;
      }
      return false;
    },
    [st, placeSide],
  );

  // ---------- paredes ----------
  const commitWall = useCallback(
    (from: Pt, to: Pt) => {
      const S = st();
      const tol = 10 / (S.viewport.scale * M);
      let endPt = to;
      S.apply((d) => {
        const r = addWall(d, from, to, { thickness: wallThickness, kind: newWall.kind }, tol);
        const n = r.doc.nodes.find((x) => x.id === r.end);
        if (n) endPt = { x: n.x, y: n.y };
        return r.doc;
      });
      return endPt;
    },
    [st, wallThickness, newWall.kind],
  );

  const finishRoom = useCallback(
    (a: Pt, b: Pt) => {
      const S = st();
      const x0 = Math.min(a.x, b.x);
      const x1 = Math.max(a.x, b.x);
      const y0 = Math.min(a.y, b.y);
      const y1 = Math.max(a.y, b.y);
      if (x1 - x0 < 0.4 || y1 - y0 < 0.4) return;
      const tol = 10 / (S.viewport.scale * M);
      let roomId: string | null = null;
      S.apply((d) => {
        const pts = [
          { x: x0, y: y0 },
          { x: x1, y: y0 },
          { x: x1, y: y1 },
          { x: x0, y: y1 },
        ];
        for (let i = 0; i < 4; i++) d = addWall(d, pts[i]!, pts[(i + 1) % 4]!, { thickness: wallThickness, kind: newWall.kind }, tol).doc;
        const r = createRoomAt(d, { x: (x0 + x1) / 2, y: (y0 + y1) / 2 }, nextRoomName(d));
        roomId = r.room;
        return r.doc;
      });
      if (roomId) S.setSel([{ kind: "room", id: roomId }]);
    },
    [st, wallThickness, newWall.kind],
  );

  // ---------- gestos ----------
  const applyMove = (g: Extract<Gesture, { t: "move" }>, cur: Pt) => {
    const S = st();
    let delta = sub(cur, g.start);
    if (!altRef.current) delta = { x: Math.round(delta.x / 0.01) * 0.01, y: Math.round(delta.y / 0.01) * 0.01 };
    const base = g.base;
    const ids = (k: SelKind) => new Set(g.sel.filter((s) => s.kind === k).map((s) => s.id));
    const wallIds = [...ids("wall")];
    const tr = (p: Pt) => add(p, delta);
    S.live(() => {
      let d = wallIds.length ? moveWallsBy(base, wallIds, delta) : base;
      const items = ids("item");
      const evid = ids("evidence");
      const ppl = ids("person");
      const txt = ids("text");
      const tj = ids("traj");
      const dm = ids("dim");
      const rm = ids("room");
      const op = ids("opening");
      const solo = g.sel.length === 1 && items.size === 1 && !altRef.current;
      d = {
        ...d,
        items: d.items.map((x) => {
          if (!items.has(x.id)) return x;
          const p = tr(base.items.find((b) => b.id === x.id) ?? x);
          const al = solo ? alignToWall(base, p, x.w, x.d) : null;
          return al ? { ...x, ...al } : { ...x, ...p };
        }),
        evidences: d.evidences.map((x) => (evid.has(x.id) ? { ...x, ...tr(x) } : x)),
        people: d.people.map((x) => (ppl.has(x.id) ? { ...x, ...tr(x) } : x)),
        texts: d.texts.map((x) => (txt.has(x.id) ? { ...x, ...tr(x) } : x)),
        trajectories: d.trajectories.map((x) => (tj.has(x.id) ? { ...x, a: tr(x.a), b: tr(x.b) } : x)),
        dims: d.dims.map((x) => (dm.has(x.id) ? { ...x, a: tr(x.a), b: tr(x.b) } : x)),
      };
      if (rm.size) {
        const rg = roomGeometry(base);
        d = {
          ...d,
          rooms: d.rooms.map((r) => {
            if (!rm.has(r.id)) return r;
            const g0 = rg.find((x) => x.room.id === r.id);
            const cur0 = r.label ?? (g0 ? labelSpot(g0) : null);
            return cur0 ? { ...r, label: tr(cur0) } : r;
          }),
        };
      }
      if (g.sel.some((s) => s.kind === "compass")) d = { ...d, options: { ...d.options, compass: { ...d.options.compass, ...tr(d.options.compass) } } };
      if (g.sel.some((s) => s.kind === "bg") && d.background) d = { ...d, background: { ...d.background, ...tr(base.background ?? d.background) } };
      if (op.size === 1 && g.sel.length === 1) {
        const o = base.openings.find((x) => op.has(x.id));
        const s = o ? snapToWall(base, cur, 0.6) : null;
        if (o && s) {
          const gg = wallGeometry(base).get(s.wall);
          if (gg) d = { ...d, openings: d.openings.map((x) => (x.id === o.id ? { ...x, wall: s.wall, t0: clampT0(gg, Math.round((s.along - o.width / 2) / 0.05) * 0.05, o.width) } : x)) };
        }
      }
      return d;
    });
  };

  const settleMoved = (base: SicroPlantaDoc, wallIds: string[]) => {
    const S = st();
    const tol = 8 / (S.viewport.scale * M);
    const nodes = new Set(base.walls.filter((w) => wallIds.includes(w.id)).flatMap((w) => [w.a, w.b]));
    return (d: SicroPlantaDoc) => {
      for (const n of nodes) d = settleNode(d, n, tol);
      return d;
    };
  };

  // ---------- eventos ----------
  const onWheel = (e: Konva.KonvaEventObject<WheelEvent>) => {
    e.evt.preventDefault();
    const S = st();
    const v = S.viewport;
    if (e.evt.ctrlKey || e.evt.metaKey) {
      const p = stageRef.current?.getPointerPosition();
      if (!p) return;
      const k = e.evt.deltaY > 0 ? 0.92 : 1.08;
      const scale = Math.max(ZMIN, Math.min(ZMAX, v.scale * k));
      const wx = (p.x - v.x) / v.scale;
      const wy = (p.y - v.y) / v.scale;
      S.setViewport({ scale, x: p.x - wx * scale, y: p.y - wy * scale }, true);
    } else {
      const dx = e.evt.shiftKey && !e.evt.deltaX ? e.evt.deltaY : e.evt.deltaX;
      const dy = e.evt.shiftKey && !e.evt.deltaX ? 0 : e.evt.deltaY;
      S.setViewport({ ...v, x: v.x - dx, y: v.y - dy }, true);
    }
  };

  const onDown = (e: Konva.KonvaEventObject<MouseEvent>) => {
    const S = st();
    altRef.current = e.evt.altKey;
    shiftRef.current = e.evt.shiftKey;
    const sp = stageRef.current?.getPointerPosition();
    if (!sp) return;
    if (e.evt.button === 1 || space || (e.evt.button === 0 && S.tool === "pan")) {
      setGesture({ t: "pan", cx: e.evt.clientX, cy: e.evt.clientY, vx: S.viewport.x, vy: S.viewport.y });
      return;
    }
    if (e.evt.button !== 0) return;
    const raw = toWorld(sp.x, sp.y);
    const t = S.tool;
    if (t === "select") {
      const nm = pickName(e.target);
      if (nm?.startsWith("h:")) {
        const [, kind, id, which] = nm.split(":");
        S.beginGesture();
        if (kind === "node") setGesture({ t: "node", nodeId: id!, base: S.doc! });
        else if (kind === "traj" || kind === "dim") setGesture(which === "off" ? { t: "dimOff", id: id! } : { t: "end", kind, id: id!, which: which as "a" | "b" });
        else if (kind === "rot") {
          const [, , k2, id2] = nm.split(":");
          const obj = k2 === "item" ? S.doc!.items.find((x) => x.id === id2) : S.doc!.people.find((x) => x.id === id2);
          if (obj) setGesture({ t: "rot", kind: k2 as "item" | "person", id: id2!, c: { x: obj.x, y: obj.y } });
        } else if (kind === "wall-len") {
          void (async () => {
            const g = wallGeometry(S.doc!).get(id!);
            const v = await askText({ title: "Medida interna da parede (m)", defaultValue: g ? fmtM(g.inner) : "", numeric: true, confirmLabel: "Aplicar" });
            const n = v ? parseM(v) : null;
            if (n && n > 0.05) {
              const { setWallInner } = await import("../model/walls");
              S.apply((d) => setWallInner(d, id!, n));
            }
          })();
          S.endGesture();
        }
        return;
      }
      if (nm?.startsWith("sel:roomfloor:")) {
        // Piso do cômodo: clique seleciona o cômodo; arrasto faz seleção por caixa.
        const id = nm.slice("sel:roomfloor:".length);
        if (!e.evt.shiftKey) S.setSel([{ kind: "room", id }]);
        setGesture({ t: "marquee", a: raw, b: raw, add: e.evt.shiftKey });
        return;
      }
      if (nm?.startsWith("sel:")) {
        const [, kind, id] = nm.split(":") as [string, SelKind, string];
        const item = { kind, id };
        let next = S.sel;
        const has = S.sel.some((s) => s.kind === kind && s.id === id);
        if (e.evt.shiftKey) next = has ? S.sel.filter((s) => !(s.kind === kind && s.id === id)) : [...S.sel, item];
        else if (!has) next = [item];
        S.setSel(next);
        if (!e.evt.shiftKey) {
          S.beginGesture();
          setGesture({ t: "move", start: raw, base: S.doc!, sel: next, moved: false });
        }
        return;
      }
      if (!e.evt.shiftKey) S.setSel([]);
      setGesture({ t: "marquee", a: raw, b: raw, add: e.evt.shiftKey });
      return;
    }
    if (t === "bgscale") {
      if (!bgFirst) setBgFirst(raw);
      else {
        const a = bgFirst;
        setBgFirst(null);
        S.setTool("select");
        if (dist(a, raw) > 0.01) onBgScalePoints?.(a, raw);
      }
      return;
    }
    if (t === "wall") {
      const p = snap(raw, chain).p;
      if (!chain) {
        setChain(p);
        return;
      }
      if (dist(p, chain) < 1e-6) return;
      const end = commitWall(chain, p);
      setChain(end);
      return;
    }
    if (t === "room") {
      const p = snap(raw).p;
      setGesture({ t: "room", a: p, b: p });
      return;
    }
    if (t === "dim" || t === "traj") {
      const p = t === "dim" ? snap(raw).p : raw;
      setGesture({ t: "line", kind: t, a: p, b: p });
      return;
    }
    void placeAt(t, t === "evidence" || t === "text" ? raw : raw).then((ok) => {
      if (ok && t !== "evidence") S.setTool("select");
    });
  };

  const onMove = () => {
    const sp = stageRef.current?.getPointerPosition();
    if (!sp) return;
    const raw = toWorld(sp.x, sp.y);
    const S = st();
    const g = gestureRef.current;
    if (g?.t === "pan") return;
    if (g?.t === "move") {
      if (!g.moved && dist(raw, g.start) * S.viewport.scale * M < 3) return;
      if (!g.moved) setGesture({ ...g, moved: true });
      applyMove(g, raw);
      setPointer(raw);
      return;
    }
    if (g?.t === "node") {
      const s = snap(raw, null, g.nodeId);
      setGuides(s.guides);
      S.live((d) => moveNodes(d, new Map([[g.nodeId, s.p]])));
      setPointer(s.p);
      return;
    }
    if (g?.t === "end") {
      const p = g.kind === "dim" ? snap(raw).p : raw;
      S.live((d) =>
        g.kind === "traj"
          ? { ...d, trajectories: d.trajectories.map((x) => (x.id === g.id ? { ...x, [g.which]: p } : x)) }
          : { ...d, dims: d.dims.map((x) => (x.id === g.id ? { ...x, [g.which]: p } : x)) },
      );
      setPointer(p);
      return;
    }
    if (g?.t === "dimOff") {
      S.live((d) => ({
        ...d,
        dims: d.dims.map((x) => {
          if (x.id !== g.id) return x;
          const n = perp(norm(sub(x.b, x.a)));
          return { ...x, offset: Math.round(dot(sub(raw, x.a), n) / 0.05) * 0.05 };
        }),
      }));
      return;
    }
    if (g?.t === "rot") {
      let deg = (Math.atan2(raw.y - g.c.y, raw.x - g.c.x) * 180) / Math.PI + 90;
      if (!altRef.current) deg = Math.round(deg / 15) * 15;
      deg = ((deg % 360) + 360) % 360;
      S.live((d) =>
        g.kind === "item" ? { ...d, items: d.items.map((x) => (x.id === g.id ? { ...x, rot: deg } : x)) } : { ...d, people: d.people.map((x) => (x.id === g.id ? { ...x, rot: deg } : x)) },
      );
      return;
    }
    if (g?.t === "marquee") {
      setGesture({ ...g, b: raw });
      return;
    }
    if (g?.t === "room") {
      const s = snap(raw);
      setGuides(s.guides);
      setGesture({ ...g, b: s.p });
      return;
    }
    if (g?.t === "line") {
      setGesture({ ...g, b: g.kind === "dim" ? snap(raw).p : raw });
      return;
    }
    if (S.tool === "wall" || S.tool === "room" || S.tool === "dim") {
      const s = snap(raw, S.tool === "wall" ? chain : null);
      setGuides(s.guides);
      setSnapKind(s.kind);
      setPointer(s.p);
      return;
    }
    setGuides([]);
    setPointer(raw);
  };

  const onUp = (e: Konva.KonvaEventObject<MouseEvent>) => {
    const g = gestureRef.current;
    const S = st();
    setGuides([]);
    if (!g) return;
    setGesture(null);
    if (g.t === "pan") return;
    if (g.t === "move") {
      const walls = g.sel.filter((s) => s.kind === "wall").map((s) => s.id);
      S.endGesture(g.moved && walls.length ? settleMoved(g.base, walls) : undefined);
      return;
    }
    if (g.t === "node") {
      const tol = 8 / (S.viewport.scale * M);
      S.endGesture((d) => settleNode(d, g.nodeId, tol));
      return;
    }
    if (g.t === "end" || g.t === "dimOff" || g.t === "rot") {
      S.endGesture();
      return;
    }
    if (g.t === "marquee") {
      const x0 = Math.min(g.a.x, g.b.x);
      const x1 = Math.max(g.a.x, g.b.x);
      const y0 = Math.min(g.a.y, g.b.y);
      const y1 = Math.max(g.a.y, g.b.y);
      if (x1 - x0 < 0.05 && y1 - y0 < 0.05) return;
      const inside = (p: Pt) => p.x >= x0 && p.x <= x1 && p.y >= y0 && p.y <= y1;
      const d = S.doc!;
      const nodes = nodeMap(d);
      const out: Sel[] = [];
      for (const w of d.walls) if (inside(nodes.get(w.a)!) && inside(nodes.get(w.b)!)) out.push({ kind: "wall", id: w.id });
      for (const x of d.items) if (inside(x)) out.push({ kind: "item", id: x.id });
      for (const x of d.evidences) if (inside(x)) out.push({ kind: "evidence", id: x.id });
      for (const x of d.people) if (inside(x)) out.push({ kind: "person", id: x.id });
      for (const x of d.texts) if (inside(x)) out.push({ kind: "text", id: x.id });
      for (const x of d.trajectories) if (inside(x.a) && inside(x.b)) out.push({ kind: "traj", id: x.id });
      for (const x of d.dims) if (inside(x.a) && inside(x.b)) out.push({ kind: "dim", id: x.id });
      S.setSel(g.add ? [...S.sel, ...out] : out);
      return;
    }
    if (g.t === "room") {
      finishRoom(g.a, g.b);
      return;
    }
    if (g.t === "line") {
      if (dist(g.a, g.b) < 0.1) return;
      const id = uid(g.kind === "dim" ? "d" : "j");
      if (g.kind === "dim") S.apply((d) => ({ ...d, dims: [...d.dims, { id, a: g.a, b: g.b, offset: 0.3 }] }));
      else
        S.apply((d) => ({
          ...d,
          trajectories: [...d.trajectories, { id, a: g.a, b: g.b, label: `T${d.trajectories.length + 1}`, descricao: "", color: "#b91c1c" }],
        }));
      S.setSel([{ kind: g.kind === "dim" ? "dim" : "traj", id }]);
      S.setTool("select");
    }
    void e;
  };

  const onDbl = () => {
    const S = st();
    if (S.tool === "wall") {
      setChain(null);
      return;
    }
    const p = pointerWorld();
    if (!p || (S.tool !== "select" && S.tool !== "room")) return;
    const has = roomGeometry(S.doc!).find((r) => r.face && r.inner.length && pointInsideFace(r.face.poly, p));
    if (has) {
      void askText({ title: "Nome do cômodo", defaultValue: has.room.name, confirmLabel: "Renomear" }).then((v) => {
        if (v?.trim()) S.apply((d) => ({ ...d, rooms: d.rooms.map((r) => (r.id === has.room.id ? { ...r, name: v.trim() } : r)) }));
      });
      return;
    }
    let id: string | null = null;
    S.apply((d) => {
      const r = createRoomAt(d, p, nextRoomName(d));
      id = r.room;
      return r.doc;
    });
    if (id) S.setSel([{ kind: "room", id }]);
  };

  // Arrasto da vista (botão do meio / espaço) pelo documento inteiro, para não perder o ponteiro.
  useEffect(() => {
    if (gesture?.t !== "pan") return;
    const g = gesture;
    const move = (ev: MouseEvent) => {
      const v = st().viewport;
      st().setViewport({ ...v, x: g.vx + ev.clientX - g.cx, y: g.vy + ev.clientY - g.cy }, true);
    };
    const up = () => setGesture(null);
    window.addEventListener("mousemove", move);
    window.addEventListener("mouseup", up);
    return () => {
      window.removeEventListener("mousemove", move);
      window.removeEventListener("mouseup", up);
    };
  }, [gesture, st]);

  // ---------- teclado da ferramenta ----------
  useEffect(() => {
    const down = (e: KeyboardEvent) => {
      altRef.current = e.altKey;
      shiftRef.current = e.shiftKey;
      if (isTyping(e)) return;
      const S = st();
      if (e.key === " " && !e.repeat) {
        const op = S.sel.find((s) => s.kind === "opening");
        if (S.tool.startsWith("opening:")) {
          e.preventDefault();
          setPlaceSide((v) => (v === 1 ? -1 : 1));
          return;
        }
        if (op && S.tool === "select") {
          e.preventDefault();
          S.apply((d) => ({ ...d, openings: d.openings.map((o) => (o.id === op.id ? { ...o, side: (o.side === 1 ? -1 : 1) as 1 | -1 } : o)) }));
          return;
        }
        setSpace(true);
        return;
      }
      if (e.key === "Escape") {
        if (lengthInput !== null) setLengthInput(null);
        else if (chain) setChain(null);
        else if (S.tool !== "select") S.setTool("select");
        else S.setSel([]);
        return;
      }
      if ((e.key === "r" || e.key === "R") && !e.ctrlKey && !e.metaKey) {
        const ids = new Set(S.sel.filter((s) => s.kind === "item" || s.kind === "person").map((s) => s.id));
        if (ids.size)
          S.apply((d) => ({
            ...d,
            items: d.items.map((x) => (ids.has(x.id) ? { ...x, rot: (x.rot + 90) % 360 } : x)),
            people: d.people.map((x) => (ids.has(x.id) ? { ...x, rot: (x.rot + 90) % 360 } : x)),
          }));
        return;
      }
      if (S.tool === "wall" && chain && lengthInput === null && (/^[0-9]$/.test(e.key) || e.key === "Enter")) {
        e.preventDefault();
        setLengthInput(e.key === "Enter" ? "" : e.key);
      }
    };
    const up = (e: KeyboardEvent) => {
      altRef.current = e.altKey;
      shiftRef.current = e.shiftKey;
      if (e.key === " ") setSpace(false);
    };
    window.addEventListener("keydown", down);
    window.addEventListener("keyup", up);
    return () => {
      window.removeEventListener("keydown", down);
      window.removeEventListener("keyup", up);
    };
  }, [chain, lengthInput, st]);

  useEffect(() => {
    if (tool !== "wall") setChain(null);
    if (tool !== "bgscale") setBgFirst(null);
    setPlaceSide(1);
  }, [tool]);

  const commitTypedLength = () => {
    const v = lengthInput ? parseM(lengthInput) : null;
    setLengthInput(null);
    if (!chain || !v || v <= 0) return;
    const p = pointer ?? add(chain, { x: 1, y: 0 });
    let d = norm(sub(p, chain));
    if (Math.hypot(d.x, d.y) < 0.5) d = { x: 1, y: 0 };
    // Medida digitada = face interna; o eixo leva a espessura das quinas.
    const end = commitWall(chain, add(chain, mul(d, v + wallThickness)));
    setChain(end);
  };

  // ---------- captura do PNG ----------
  useImperativeHandle(ref, () => ({
    capture(variant) {
      const stage = stageRef.current;
      if (!stage) return null;
      flushSync(() => setExportMode(variant));
      const s = st().doc!.sheet;
      const prev = { x: stage.x(), y: stage.y(), k: stage.scaleX() };
      stage.scale({ x: 1, y: 1 });
      stage.position({ x: 0, y: 0 });
      const ui = uiRef.current;
      ui?.visible(false);
      try {
        const pixelRatio = Math.min(s.png_width, 8000) / W(s.w);
        return stage.toDataURL({ x: W(s.x), y: W(s.y), width: W(s.w), height: W(s.h), pixelRatio, mimeType: "image/png" });
      } finally {
        ui?.visible(true);
        stage.scale({ x: prev.k, y: prev.k });
        stage.position({ x: prev.x, y: prev.y });
        flushSync(() => setExportMode(null));
        stage.batchDraw();
      }
    },
    fit,
    dropTool(t, clientX, clientY) {
      const r = hostRef.current?.getBoundingClientRect();
      if (!r || clientX < r.left || clientX > r.right || clientY < r.top || clientY > r.bottom) {
        st().setTool(t);
        return;
      }
      const p = toWorld(clientX - r.left, clientY - r.top);
      void placeAt(t, p).then((ok) => {
        if (!ok) st().setTool(t);
      });
    },
  }));

  // ---------- dica ----------
  const hint = useMemo(() => {
    if (tool === "wall") return chain ? "Clique o próximo ponto · digite a medida e Enter · Shift trava o ângulo · duplo clique ou Esc termina" : "Clique onde a parede começa";
    if (tool === "room") return "Arraste o retângulo do cômodo (medidas internas aparecem na hora)";
    if (tool.startsWith("opening:")) return "Leve até a parede e clique · Espaço inverte o lado da abertura";
    if (tool.startsWith("item:")) return "Clique para colocar · perto da parede ela encosta sozinha (Alt solta) · R gira";
    if (tool === "evidence") return "Clique para marcar vestígios em sequência · Esc termina";
    if (tool === "dim") return "Arraste de um ponto a outro";
    if (tool === "traj") return "Arraste da origem ao destino";
    if (tool === "text") return "Clique onde o texto fica";
    if (tool.startsWith("person:")) return "Clique para colocar · R gira";
    if (tool === "bgscale") return bgFirst ? "Clique o segundo ponto de distância conhecida" : "Clique o primeiro ponto de uma distância conhecida no fundo";
    return "Clique para selecionar · arraste para mover · duplo clique num espaço fechado cria o cômodo";
  }, [tool, chain, bgFirst]);
  useEffect(() => onHint?.(hint), [hint, onHint]);

  const style = exportMode ? EXPORT_STYLE : EDITOR_STYLE;
  const sheet = doc.sheet;
  const sk = 1 / vp.scale; // 1 px de tela em unidades do mundo
  const selWall = sel.length === 1 && sel[0]!.kind === "wall" ? doc.walls.find((w) => w.id === sel[0]!.id) : undefined;
  const selOpening = sel.length === 1 && sel[0]!.kind === "opening" ? doc.openings.find((o) => o.id === sel[0]!.id) : undefined;
  const nodes = nodeMap(doc);
  const showMeasureFor = doc.evidences.filter((e) => e.show_measure || (!exportMode && selected("evidence", e.id)));

  const pill = (p: Pt, text: string, key: string, color = "#0e7490", name?: string) => (
    <Label key={key} x={W(p.x)} y={W(p.y)} name={name}>
      <Tag fill={color} cornerRadius={8 * sk} pointerDirection="none" />
      <Text text={text} fontSize={11.5 * sk} padding={4 * sk} fill="#ffffff" fontFamily="'JetBrains Mono', monospace" />
    </Label>
  );

  return (
    <div className={styles.canvasWrap}>
    <div className={styles.canvasHost} ref={hostRef} style={{ cursor: gesture?.t === "pan" || space ? "grabbing" : tool === "select" ? "default" : "crosshair" }}>
      <Stage
        ref={stageRef}
        width={size.w}
        height={size.h}
        scaleX={vp.scale}
        scaleY={vp.scale}
        x={vp.x}
        y={vp.y}
        onWheel={onWheel}
        onMouseDown={onDown}
        onMouseMove={onMove}
        onMouseUp={onUp}
        onDblClick={onDbl}
        onMouseLeave={() => setPointer(null)}
        onContextMenu={(e) => e.evt.preventDefault()}
      >
        <Layer listening={false}>
          <Rect x={W(sheet.x)} y={W(sheet.y)} width={W(sheet.w)} height={W(sheet.h)} fill="#ffffff" shadowColor="#0f172a" shadowBlur={exportMode ? 0 : 24 * sk} shadowOpacity={exportMode ? 0 : 0.35} />
          {sheet.show_grid && !exportMode && <GridShape sheet={sheet} />}
        </Layer>
        {doc.background && bgImg && (
          <Layer listening={!doc.background.locked && tool === "select"} opacity={doc.background.opacity}>
            <KonvaImage
              name="sel:bg:bg"
              stroke={selected("bg", "bg") ? "#d7a84f" : undefined}
              strokeWidth={selected("bg", "bg") ? 3 * sk : 0}
              image={bgImg} x={W(doc.background.x)} y={W(doc.background.y)} width={W(doc.background.width)} height={W((doc.background.width * bgImg.height) / bgImg.width)} rotation={doc.background.rot} />
          </Layer>
        )}
        {style.tints && (
          <Layer>
            {rooms.map((rg, i) => (
              <RoomTint key={rg.room.id} rg={rg} index={i} selected={selected("room", rg.room.id)} />
            ))}
          </Layer>
        )}
        <Layer>
          {doc.items.map((it) => (
            <ItemShape key={it.id} item={it} style={style} selected={!exportMode && selected("item", it.id)} />
          ))}
          {doc.walls.map((w) => {
            const g = geo.get(w.id);
            return g ? <WallShape key={w.id} wall={w} geom={g} style={style} selected={!exportMode && selected("wall", w.id)} /> : null;
          })}
          {geoFull.hubs.map((h) => (
            <Line key={`hub${h.node}`} points={h.poly.flatMap((p) => [W(p.x), W(p.y)])} closed fill={h.wall.kind === "muro" ? style.muroFill : h.wall.kind === "grade" ? "#ffffff" : style.wallFill} stroke={h.wall.kind === "muro" ? style.muroFill : h.wall.kind === "grade" ? "#ffffff" : style.wallFill} strokeWidth={0.6} listening={false} />
          ))}
          {doc.openings.map((o) => {
            const g = geo.get(o.wall);
            const w = wallById.get(o.wall);
            return g && w ? <OpeningShape key={o.id} o={o} geom={g} thickness={w.thickness} style={style} selected={!exportMode && selected("opening", o.id)} /> : null;
          })}
          {rooms.map((rg) => (
            <RoomLabel key={rg.room.id} rg={rg} at={rg.room.label ?? labelSpot(rg)} style={style} showArea={doc.options.room_areas && rg.room.show_area} selected={!exportMode && selected("room", rg.room.id)} />
          ))}
          {exportMode !== "limpo" && dims.length > 0 && <AutoDims dims={dims} ink={style.dimInk} />}
          {(exportMode ? doc.options.wall_measures : false) && doc.walls.map((w) => (geo.get(w.id) ? <WallMeasure key={w.id} geom={geo.get(w.id)!} ink={style.dimInk} /> : null))}
          {doc.dims.map((d) => (
            <ManualDim key={d.id} dim={d} ink={style.dimInk} selected={!exportMode && selected("dim", d.id)} />
          ))}
          {showMeasureFor.map((e) => (
            <MeasureLines key={e.id} lines={evidenceMeasures(doc, e)} />
          ))}
          {doc.trajectories.map((t) => (
            <TrajShape key={t.id} t={t} selected={!exportMode && selected("traj", t.id)} />
          ))}
          {doc.people.map((p) => (
            <PersonShape key={p.id} p={p} style={style} selected={!exportMode && selected("person", p.id)} />
          ))}
          {doc.evidences.map((e) => (
            <EvidenceMarker key={e.id} e={e} selected={!exportMode && selected("evidence", e.id)} />
          ))}
          {doc.texts.map((t) => (
            <TextShape key={t.id} t={t} selected={!exportMode && selected("text", t.id)} />
          ))}
          {doc.options.compass.show && <Compass x={doc.options.compass.x} y={doc.options.compass.y} deg={doc.options.compass.deg} selected={!exportMode && selected("compass", "compass")} />}
        </Layer>
        <Layer ref={uiRef}>
          {tool === "bgscale" && bgFirst && pointer && (
            <Line points={[W(bgFirst.x), W(bgFirst.y), W(pointer.x), W(pointer.y)]} stroke="#0e7490" strokeWidth={2 * sk} dash={[6 * sk, 4 * sk]} listening={false} />
          )}
          {guides.map((g, i) => (
            <Line key={`g${i}`} points={[W(g.a.x), W(g.a.y), W(g.b.x), W(g.b.y)]} stroke="#06b6d4" strokeWidth={1 * sk} dash={[6 * sk, 4 * sk]} listening={false} />
          ))}
          {/* Parede em desenho */}
          {tool === "wall" && chain && pointer && (
            <Group listening={false}>
              <Line points={[W(chain.x), W(chain.y), W(pointer.x), W(pointer.y)]} stroke="#d7a84f" strokeWidth={W(wallThickness)} opacity={0.45} lineCap="butt" />
              <Line points={[W(chain.x), W(chain.y), W(pointer.x), W(pointer.y)]} stroke="#b7801f" strokeWidth={1.5 * sk} dash={[6 * sk, 4 * sk]} />
              {dist(chain, pointer) > 0.05 && pill(add(chain, mul(sub(pointer, chain), 0.5)), `${fmtM(Math.max(0, dist(chain, pointer) - wallThickness))} m`, "len")}
            </Group>
          )}
          {(tool === "wall" || tool === "room" || tool === "dim") && pointer && (
            <Rect
              x={W(pointer.x) - 5 * sk}
              y={W(pointer.y) - 5 * sk}
              width={10 * sk}
              height={10 * sk}
              cornerRadius={snapKind === "node" ? 0 : 5 * sk}
              stroke={snapKind === "free" ? "#94a3b8" : "#0e7490"}
              strokeWidth={1.5 * sk}
              listening={false}
            />
          )}
          {gesture?.t === "room" && (
            <Group listening={false}>
              <Rect
                x={W(Math.min(gesture.a.x, gesture.b.x))}
                y={W(Math.min(gesture.a.y, gesture.b.y))}
                width={W(Math.abs(gesture.b.x - gesture.a.x))}
                height={W(Math.abs(gesture.b.y - gesture.a.y))}
                stroke="#b7801f"
                strokeWidth={W(wallThickness)}
                opacity={0.45}
              />
              {pill(
                { x: (gesture.a.x + gesture.b.x) / 2, y: (gesture.a.y + gesture.b.y) / 2 },
                `${fmtM(Math.max(0, Math.abs(gesture.b.x - gesture.a.x) - wallThickness))} × ${fmtM(Math.max(0, Math.abs(gesture.b.y - gesture.a.y) - wallThickness))} m`,
                "roomdim",
              )}
            </Group>
          )}
          {gesture?.t === "line" && (
            <Line points={[W(gesture.a.x), W(gesture.a.y), W(gesture.b.x), W(gesture.b.y)]} stroke={gesture.kind === "traj" ? "#b91c1c" : "#475569"} strokeWidth={2 * sk} dash={[8 * sk, 5 * sk]} listening={false} />
          )}
          {gesture?.t === "line" && gesture.kind === "dim" && pill(add(gesture.a, mul(sub(gesture.b, gesture.a), 0.5)), `${fmtM(dist(gesture.a, gesture.b))} m`, "dimlen")}
          {gesture?.t === "marquee" && (
            <Rect
              x={W(Math.min(gesture.a.x, gesture.b.x))}
              y={W(Math.min(gesture.a.y, gesture.b.y))}
              width={W(Math.abs(gesture.b.x - gesture.a.x))}
              height={W(Math.abs(gesture.b.y - gesture.a.y))}
              fill="rgba(215,168,79,0.08)"
              stroke="#d7a84f"
              strokeWidth={1 * sk}
              dash={[5 * sk, 4 * sk]}
              listening={false}
            />
          )}
          {/* Prévia de abertura encaixada */}
          {openingPreview && (
            <Group listening={false} opacity={0.9}>
              <OpeningShape o={openingPreview.o} geom={openingPreview.g} thickness={openingPreview.w.thickness} style={EDITOR_STYLE} selected />
              {(() => {
                const gp = openingGaps(openingPreview.g, openingPreview.o);
                const f = openingPreview.g.frame;
                const before = add(f.A, mul(f.dir, (openingPreview.g.marginA + openingPreview.o.t0) / 2));
                const after = add(f.A, mul(f.dir, (openingPreview.o.t0 + openingPreview.o.width + f.L - openingPreview.g.marginB) / 2));
                return [pill(before, `${fmtM(gp.before)} m`, "gb"), pill(after, `${fmtM(gp.after)} m`, "ga")];
              })()}
            </Group>
          )}
          {itemPreview && (
            <Group listening={false} opacity={0.65}>
              <ItemShape item={itemPreview} style={EDITOR_STYLE} selected={false} />
            </Group>
          )}
          {pointer && tool.startsWith("person:") && (
            <Group listening={false} opacity={0.6}>
              <PersonShape p={{ id: "_p", pose: tool.slice(7) as "em_pe" | "caido", x: pointer.x, y: pointer.y, rot: 0, label: "", descricao: "" }} style={EDITOR_STYLE} selected={false} />
            </Group>
          )}
          {/* Alças da seleção */}
          {selWall &&
            [selWall.a, selWall.b].map((nid) => {
              const n = nodes.get(nid);
              return n ? <Circle key={nid} name={`h:node:${nid}`} x={W(n.x)} y={W(n.y)} radius={7 * sk} fill="#ffffff" stroke="#b7801f" strokeWidth={2 * sk} /> : null;
            })}
          {selWall &&
            (() => {
              const g = geo.get(selWall.id);
              if (!g) return null;
              const mid = add(g.frame.A, mul(g.frame.dir, g.frame.L / 2));
              const side = g.faceLeft <= g.faceRight ? 1 : -1;
              return pill(add(mid, mul(g.frame.nrm, side * (selWall.thickness / 2 + 0.25))), `${fmtM(g.inner)} m ✎`, "wl", "#b7801f", `h:wall-len:${selWall.id}`);
            })()}
          {selOpening &&
            (() => {
              const g = geo.get(selOpening.wall);
              if (!g) return null;
              const gp = openingGaps(g, selOpening);
              const f = g.frame;
              return [
                pill(add(f.A, mul(f.dir, (g.marginA + selOpening.t0) / 2)), `${fmtM(gp.before)} m`, "sb"),
                pill(add(f.A, mul(f.dir, (selOpening.t0 + selOpening.width + f.L - g.marginB) / 2)), `${fmtM(gp.after)} m`, "sa"),
              ];
            })()}
          {sel.length === 1 &&
            (sel[0]!.kind === "item" || sel[0]!.kind === "person") &&
            (() => {
              const s0 = sel[0]!;
              const o = s0.kind === "item" ? doc.items.find((x) => x.id === s0.id) : doc.people.find((x) => x.id === s0.id);
              if (!o) return null;
              const dd = s0.kind === "item" ? (o as PItem).d : PERSON_SIZE[(o as { pose: "em_pe" | "caido" }).pose].d;
              const r = (o.rot * Math.PI) / 180;
              const h = { x: o.x + Math.sin(r) * (dd / 2 + 0.35), y: o.y - Math.cos(r) * (dd / 2 + 0.35) };
              return <Circle name={`h:rot:${s0.kind}:${s0.id}`} x={W(h.x)} y={W(h.y)} radius={7 * sk} fill="#d7a84f" stroke="#ffffff" strokeWidth={2 * sk} />;
            })()}
          {sel.length === 1 &&
            sel[0]!.kind === "traj" &&
            (() => {
              const t = doc.trajectories.find((x) => x.id === sel[0]!.id);
              return t
                ? (["a", "b"] as const).map((k) => <Circle key={k} name={`h:traj:${t.id}:${k}`} x={W(t[k].x)} y={W(t[k].y)} radius={7 * sk} fill="#ffffff" stroke="#b91c1c" strokeWidth={2 * sk} />)
                : null;
            })()}
          {sel.length === 1 &&
            sel[0]!.kind === "dim" &&
            (() => {
              const d = doc.dims.find((x) => x.id === sel[0]!.id);
              if (!d) return null;
              const n = perp(norm(sub(d.b, d.a)));
              const mid = add(add(d.a, mul(sub(d.b, d.a), 0.5)), mul(n, d.offset));
              return [
                ...(["a", "b"] as const).map((k) => <Circle key={k} name={`h:dim:${d.id}:${k}`} x={W(d[k].x)} y={W(d[k].y)} radius={7 * sk} fill="#ffffff" stroke="#475569" strokeWidth={2 * sk} />),
                <Rect key="off" name={`h:dim:${d.id}:off`} x={W(mid.x) - 6 * sk} y={W(mid.y) - 6 * sk} width={12 * sk} height={12 * sk} fill="#d7a84f" stroke="#ffffff" strokeWidth={1.5 * sk} />,
              ];
            })()}
        </Layer>
      </Stage>
      {lengthInput !== null && pointer && (
        <div className={styles.lengthBox} style={{ left: W(pointer.x) * vp.scale + vp.x + 14, top: W(pointer.y) * vp.scale + vp.y + 14 }}>
          <label htmlFor="planta-len">Medida interna (m)</label>
          <input
            id="planta-len"
            autoFocus
            value={lengthInput}
            onChange={(e) => setLengthInput(e.target.value)}
            onKeyDown={(e) => {
              if (e.key === "Enter") commitTypedLength();
              if (e.key === "Escape") setLengthInput(null);
              e.stopPropagation();
            }}
          />
        </div>
      )}
    </div>
      <div className={styles.status}>
        <span className={styles.statusTitle}>{title}</span>
        {message ? <span className={styles.statusMsg}>{message}</span> : <span>{hint}</span>}
        <span className={styles.statusRight}>
          {pointer ? `${fmtM(pointer.x)} · ${fmtM(pointer.y)} m` : ""} · ímã {altRef.current ? "desligado" : "ligado, Alt desliga"} · 1 m = {pxPerM.toFixed(0)} px
        </span>
      </div>
    </div>
  );
});

function pointInsideFace(poly: Pt[], p: Pt): boolean {
  let inside = false;
  for (let i = 0, j = poly.length - 1; i < poly.length; j = i++) {
    const a = poly[i]!;
    const b = poly[j]!;
    if (a.y > p.y !== b.y > p.y && p.x < ((b.x - a.x) * (p.y - a.y)) / (b.y - a.y) + a.x) inside = !inside;
  }
  return inside;
}

export function labelSpot(rg: { inner: Pt[] }): Pt {
  return interiorPoint(rg.inner);
}
