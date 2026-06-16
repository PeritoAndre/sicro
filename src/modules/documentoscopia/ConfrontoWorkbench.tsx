/**
 * Confronto sincronizado (Fase 5 — foco).
 *
 * Bancada de comparação QUESTIONADO × PADRÃO: dois visualizadores com zoom
 * compartilhado (trava de zoom), pan independente ou sincronizado, modo de
 * sobreposição com opacidade/diferença, e grade de referência. O perito
 * registra observações e salva o confronto (reabrível, reproduzível).
 *
 * §13: esta ferramenta APENAS alinha, escala e mede pixels. Ela NÃO calcula
 * índice de similaridade nem conclui autoria/autenticidade. A interpretação
 * e a conclusão são exclusivamente do perito. Nenhum "score" é exibido.
 */

import {
  useCallback,
  useEffect,
  useRef,
  useState,
  type CSSProperties,
} from "react";
import { convertFileSrc } from "@tauri-apps/api/core";
import {
  AlertTriangle,
  ArrowLeftRight,
  ChevronLeft,
  ChevronRight,
  Columns2,
  Grid3x3,
  ImageDown,
  Layers,
  Link2,
  Link2Off,
  MapPin,
  Maximize2,
  Pencil,
  Plus,
  Redo2,
  RotateCcw,
  Ruler,
  Save,
  Trash2,
  Triangle,
  Type,
  Undo2,
} from "lucide-react";

import { Button } from "@components/Button/Button";
import { commands } from "@core/commands";
import { toSicroError } from "@core/errors";
import { useShortcuts } from "@core/useShortcuts";
import {
  docTypeLabel,
  type ComparisonSession,
  type DocumentCaseFile,
} from "@domain/documentoscopia";
import { renderPdfPageToDataUrl } from "./pdf";
import {
  angleAtDeg,
  correspondingAnchor,
  distance,
  fitTransform,
  imageToScreen,
  pan,
  screenToImage,
  wheelFactor,
  zoomAt,
  zoomTo,
  type Size,
  type ViewTransform,
} from "./confrontoView";
import styles from "./DocumentoscopiaModule.module.css";

const CONFRONTO_TYPE = "confronto_visual";
const RESULTS_SCHEMA = "confronto.v1";

type Mode = "side" | "overlay";

interface Props {
  ws: string;
  documents: DocumentCaseFile[];
}

/** Ponto numerado de confronto. Coordenadas em px da IMAGEM (não da tela),
 * por lado. O mesmo número em Q e P indica pontos correspondentes. */
interface Marker {
  id: string;
  n: number;
  side: "q" | "r";
  x: number;
  y: number;
  note: string;
}

/** Medição sobre um lado. `dist` = 2 pontos; `angle` = 3 pontos (vértice no
 * meio). Coordenadas em px da IMAGEM. */
interface Measurement {
  id: string;
  side: "q" | "r";
  kind: "dist" | "angle";
  pts: { x: number; y: number }[];
}

/** Calibração de um lado: o comprimento real (mm) de uma medição-referência
 * define mm/px. Cada lado calibra separado (resoluções diferentes). */
interface Calib {
  refId: string;
  mm: number;
  mmPerPx: number;
}

type Tool = "pan" | "marker" | "ruler" | "angle" | "pencil" | "text";

/** Traço livre (lápis) sobre um lado. Pontos em px da IMAGEM. */
interface Stroke {
  id: string;
  side: "q" | "r";
  pts: { x: number; y: number }[];
}

/** Rótulo de texto livre sobre um lado. Âncora em px da IMAGEM. */
interface TextLabel {
  id: string;
  side: "q" | "r";
  x: number;
  y: number;
  text: string;
}

interface ConfrontoResults {
  schema: string;
  questioned: { documentId: string; page: number };
  reference: { documentId: string; page: number };
  mode: Mode;
  lockZoom: boolean;
  syncPan: boolean;
  sharpPixels: boolean;
  qView: ViewTransform;
  rView: ViewTransform;
  grid: { on: boolean; size: number };
  overlay: { opacity: number; blendDiff: boolean };
  markers: Marker[];
  currentN: number;
  measurements: Measurement[];
  strokes: Stroke[];
  texts: TextLabel[];
  qCalib: Calib | null;
  rCalib: Calib | null;
}

const ZERO: ViewTransform = { scale: 1, x: 0, y: 0 };

function newId(): string {
  try {
    return crypto.randomUUID();
  } catch {
    return `m${Date.now()}-${Math.round(performance.now())}`;
  }
}

export function ConfrontoWorkbench({ ws, documents }: Props) {
  const [qDocId, setQDocId] = useState<string | null>(null);
  const [rDocId, setRDocId] = useState<string | null>(null);
  const [qPage, setQPage] = useState(1);
  const [rPage, setRPage] = useState(1);
  const [mode, setMode] = useState<Mode>("side");
  const [vt, setVt] = useState<{ q: ViewTransform; r: ViewTransform }>({
    q: ZERO,
    r: ZERO,
  });
  const [lockZoom, setLockZoom] = useState(true);
  const [syncPan, setSyncPan] = useState(true);
  const [sharpPixels, setSharpPixels] = useState(true);
  const [grid, setGrid] = useState(false);
  const [gridSize, setGridSize] = useState(40);
  const [opacity, setOpacity] = useState(0.5);
  const [blendDiff, setBlendDiff] = useState(false);
  const [tool, setTool] = useState<Tool>("pan");
  const markerMode = tool === "marker";
  const [markers, setMarkers] = useState<Marker[]>([]);
  const [currentN, setCurrentN] = useState(1);
  const [selectedMarkerId, setSelectedMarkerId] = useState<string | null>(null);
  const [pending, setPending] = useState<{
    side: "q" | "r";
    kind: "dist" | "angle";
    pts: { x: number; y: number }[];
  } | null>(null);
  const [measurements, setMeasurements] = useState<Measurement[]>([]);
  const [strokes, setStrokes] = useState<Stroke[]>([]);
  const [texts, setTexts] = useState<TextLabel[]>([]);
  const [editingTextId, setEditingTextId] = useState<string | null>(null);
  const [draft, setDraft] = useState<Stroke | null>(null);
  const [qCalib, setQCalib] = useState<Calib | null>(null);
  const [rCalib, setRCalib] = useState<Calib | null>(null);
  const [qSrc, setQSrc] = useState<string | null>(null);
  const [rSrc, setRSrc] = useState<string | null>(null);
  const [observations, setObservations] = useState("");
  const [saved, setSaved] = useState<ComparisonSession[]>([]);
  const [loadedId, setLoadedId] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const [err, setErr] = useState<string | null>(null);
  const [dragging, setDragging] = useState(false);

  const qPanelRef = useRef<HTMLDivElement | null>(null);
  const rPanelRef = useRef<HTMLDivElement | null>(null);
  const dragRef = useRef<{
    which: "q" | "r" | "overlay";
    x: number;
    y: number;
    moved: number;
    downCursor: { x: number; y: number };
  } | null>(null);
  // Espelha o estado vivo para o listener nativo de wheel (evita stale closure).
  const liveRef = useRef({ mode, lockZoom });
  liveRef.current = { mode, lockZoom };
  // Quando carregamos um confronto salvo, guardamos as views para aplicar no
  // onLoad da imagem em vez de auto-enquadrar.
  const restoreRef = useRef<{ q?: ViewTransform; r?: ViewTransform }>({});

  // --- Histórico de anotações (Ctrl+Z / Ctrl+Y) ---
  // Snapshot de tudo que o perito INSERE ou MODIFICA: marcadores, medições,
  // traços (lápis), textos e calibração. A vista (zoom/pan) NÃO entra no
  // histórico — desfazer não bagunça o enquadramento.
  interface Snapshot {
    markers: Marker[];
    measurements: Measurement[];
    strokes: Stroke[];
    texts: TextLabel[];
    qCalib: Calib | null;
    rCalib: Calib | null;
    currentN: number;
  }
  const undoStack = useRef<Snapshot[]>([]);
  const redoStack = useRef<Snapshot[]>([]);
  const [, bumpHist] = useState(0);
  const snapshot = (): Snapshot => ({
    markers,
    measurements,
    strokes,
    texts,
    qCalib,
    rCalib,
    currentN,
  });
  const applySnapshot = (s: Snapshot) => {
    setMarkers(s.markers);
    setMeasurements(s.measurements);
    setStrokes(s.strokes);
    setTexts(s.texts);
    setQCalib(s.qCalib);
    setRCalib(s.rCalib);
    setCurrentN(s.currentN);
  };
  /** Empilha o estado ATUAL antes de uma mutação (chamar ANTES de mexer). */
  const pushUndo = () => {
    undoStack.current.push(snapshot());
    if (undoStack.current.length > 100) undoStack.current.shift();
    redoStack.current = [];
    bumpHist((t) => t + 1);
  };
  const undo = () => {
    const prev = undoStack.current.pop();
    if (!prev) return;
    redoStack.current.push(snapshot());
    applySnapshot(prev);
    setPending(null);
    setEditingTextId(null);
    bumpHist((t) => t + 1);
  };
  const redo = () => {
    const next = redoStack.current.pop();
    if (!next) return;
    undoStack.current.push(snapshot());
    applySnapshot(next);
    setPending(null);
    setEditingTextId(null);
    bumpHist((t) => t + 1);
  };
  const canUndo = undoStack.current.length > 0;
  const canRedo = redoStack.current.length > 0;

  const docById = useCallback(
    (id: string | null) => documents.find((d) => d.id === id) ?? null,
    [documents],
  );
  const qDoc = docById(qDocId);
  const rDoc = docById(rDocId);

  // Seleção padrão: primeiro × segundo documento.
  useEffect(() => {
    if (!qDocId && documents[0]) setQDocId(documents[0].id);
    if (!rDocId && documents[1]) setRDocId(documents[1].id);
    else if (!rDocId && documents[0]) setRDocId(documents[0].id);
  }, [documents, qDocId, rDocId]);

  // Lista de confrontos salvos.
  const reloadSaved = useCallback(async () => {
    try {
      const all = await commands.listComparisons(ws);
      setSaved(all.filter((c) => c.comparison_type === CONFRONTO_TYPE));
    } catch (e) {
      setErr(toSicroError(e).message);
    }
  }, [ws]);
  useEffect(() => {
    void reloadSaved();
  }, [reloadSaved]);

  // Resolve o raster de cada lado (imagem direto; PDF via pdf.js).
  const resolveSrc = useCallback(
    async (doc: DocumentCaseFile | null, page: number): Promise<string | null> => {
      if (!doc) return null;
      const fileUrl = convertFileSrc(`${ws}/${doc.relative_path}`);
      if (doc.file_type === "image") return fileUrl;
      return await renderPdfPageToDataUrl(fileUrl, page, 2.0);
    },
    [ws],
  );

  useEffect(() => {
    let alive = true;
    setQSrc(null);
    (async () => {
      try {
        const s = await resolveSrc(qDoc, qPage);
        if (alive) setQSrc(s);
      } catch (e) {
        if (alive) setErr(toSicroError(e).message);
      }
    })();
    return () => {
      alive = false;
    };
  }, [qDoc, qPage, resolveSrc]);

  useEffect(() => {
    let alive = true;
    setRSrc(null);
    (async () => {
      try {
        const s = await resolveSrc(rDoc, rPage);
        if (alive) setRSrc(s);
      } catch (e) {
        if (alive) setErr(toSicroError(e).message);
      }
    })();
    return () => {
      alive = false;
    };
  }, [rDoc, rPage, resolveSrc]);

  const fitSide = useCallback(
    (which: "q" | "r", nat: Size) => {
      const el =
        which === "q"
          ? qPanelRef.current
          : mode === "overlay"
            ? qPanelRef.current
            : rPanelRef.current;
      if (!el) return;
      const r = el.getBoundingClientRect();
      const t = fitTransform(nat, { w: r.width, h: r.height });
      setVt((prev) => (which === "q" ? { ...prev, q: t } : { ...prev, r: t }));
    },
    [mode],
  );

  const onImgLoad =
    (which: "q" | "r") => (e: React.SyntheticEvent<HTMLImageElement>) => {
      const nat: Size = {
        w: e.currentTarget.naturalWidth,
        h: e.currentTarget.naturalHeight,
      };
      const restore = restoreRef.current[which];
      if (restore) {
        setVt((prev) =>
          which === "q" ? { ...prev, q: restore } : { ...prev, r: restore },
        );
        restoreRef.current[which] = undefined;
      } else {
        fitSide(which, nat);
      }
    };

  // Zoom por scroll (listener nativo não-passivo — onWheel do React é passivo).
  useEffect(() => {
    const qEl = qPanelRef.current;
    const rEl = rPanelRef.current;
    const makeHandler = (which: "q" | "r") => (e: WheelEvent) => {
      e.preventDefault();
      const f = wheelFactor(e.deltaY);
      const el = which === "q" ? qPanelRef.current : rPanelRef.current;
      if (!el) return;
      const rect = el.getBoundingClientRect();
      const cursor = { x: e.clientX - rect.left, y: e.clientY - rect.top };
      const { mode: m, lockZoom: lock } = liveRef.current;
      setVt((prev) => {
        let q = prev.q;
        let r = prev.r;
        if (m === "overlay") {
          q = zoomAt(q, f, cursor);
          r = zoomAt(r, f, cursor);
        } else if (which === "q") {
          q = zoomAt(q, f, cursor);
          if (lock && rPanelRef.current) {
            const rr = rPanelRef.current.getBoundingClientRect();
            // Zoom travado: o lado padrão amplia no MESMO ponto relativo do
            // cursor (não no centro) — assim os dois lados ficam alinhados.
            r = zoomAt(
              r,
              f,
              correspondingAnchor(
                cursor,
                { w: rect.width, h: rect.height },
                { w: rr.width, h: rr.height },
              ),
            );
          }
        } else {
          r = zoomAt(r, f, cursor);
          if (lock && qPanelRef.current) {
            const qr = qPanelRef.current.getBoundingClientRect();
            q = zoomAt(
              q,
              f,
              correspondingAnchor(
                cursor,
                { w: rect.width, h: rect.height },
                { w: qr.width, h: qr.height },
              ),
            );
          }
        }
        return { q, r };
      });
    };
    const qh = makeHandler("q");
    const rh = makeHandler("r");
    qEl?.addEventListener("wheel", qh, { passive: false });
    rEl?.addEventListener("wheel", rh, { passive: false });
    return () => {
      qEl?.removeEventListener("wheel", qh);
      rEl?.removeEventListener("wheel", rh);
    };
  }, [mode, qSrc, rSrc]);

  const drawingRef = useRef<{
    side: "q" | "r";
    pts: { x: number; y: number }[];
  } | null>(null);

  // Pan por arraste; clique limpo dispara a ferramenta ativa (marcador/medição/
  // texto); em modo lápis o arraste desenha um traço livre.
  const onDown = (which: "q" | "r" | "overlay") => (e: React.PointerEvent) => {
    const el = e.currentTarget as HTMLElement;
    el.setPointerCapture(e.pointerId);
    const rect = el.getBoundingClientRect();
    const cursor = { x: e.clientX - rect.left, y: e.clientY - rect.top };
    if (tool === "pencil" && which !== "overlay") {
      const side = which as "q" | "r";
      const view = side === "q" ? vt.q : vt.r;
      const p = screenToImage(view, cursor);
      drawingRef.current = { side, pts: [p] };
      setDraft({ id: "draft", side, pts: [p] });
      setDragging(true);
      return;
    }
    dragRef.current = {
      which,
      x: e.clientX,
      y: e.clientY,
      moved: 0,
      downCursor: cursor,
    };
    setDragging(true);
  };
  const onMove = (e: React.PointerEvent) => {
    if (drawingRef.current) {
      const dr = drawingRef.current;
      const rect = (e.currentTarget as HTMLElement).getBoundingClientRect();
      const cursor = { x: e.clientX - rect.left, y: e.clientY - rect.top };
      const view = dr.side === "q" ? vt.q : vt.r;
      dr.pts.push(screenToImage(view, cursor));
      setDraft({ id: "draft", side: dr.side, pts: [...dr.pts] });
      return;
    }
    const d = dragRef.current;
    if (!d) return;
    const dx = e.clientX - d.x;
    const dy = e.clientY - d.y;
    d.x = e.clientX;
    d.y = e.clientY;
    d.moved += Math.abs(dx) + Math.abs(dy);
    setVt((prev) => {
      let q = prev.q;
      let r = prev.r;
      if (d.which === "overlay") {
        // Em sobreposição, arrastar alinha o PADRÃO sobre o questionado.
        r = pan(r, { x: dx, y: dy });
      } else if (d.which === "q") {
        q = pan(q, { x: dx, y: dy });
        if (syncPan) r = pan(r, { x: dx, y: dy });
      } else {
        r = pan(r, { x: dx, y: dy });
        if (syncPan) q = pan(q, { x: dx, y: dy });
      }
      return { q, r };
    });
  };
  const onUp = (e: React.PointerEvent) => {
    setDragging(false);
    try {
      (e.currentTarget as HTMLElement).releasePointerCapture(e.pointerId);
    } catch {
      /* noop */
    }
    // Fim de um traço de lápis → vira um Stroke (≥2 pontos), desfazível.
    if (drawingRef.current) {
      const dr = drawingRef.current;
      drawingRef.current = null;
      setDraft(null);
      if (dr.pts.length >= 2) {
        pushUndo();
        setStrokes((prev) => [...prev, { id: newId(), side: dr.side, pts: dr.pts }]);
      }
      return;
    }
    const d = dragRef.current;
    dragRef.current = null;
    // Clique limpo (sem arraste), num painel de lado a lado: dispara a
    // ferramenta ativa na coordenada de IMAGEM clicada.
    if (d && d.moved < 4 && d.which !== "overlay") {
      const side = d.which as "q" | "r";
      const view = side === "q" ? vt.q : vt.r;
      const p = screenToImage(view, d.downCursor);
      if (tool === "marker") {
        pushUndo();
        setMarkers((prev) => [
          ...prev,
          { id: newId(), n: currentN, side, x: p.x, y: p.y, note: "" },
        ]);
      } else if (tool === "ruler" || tool === "angle") {
        const kind: "dist" | "angle" = tool === "angle" ? "angle" : "dist";
        const need = kind === "angle" ? 3 : 2;
        const base =
          pending && pending.side === side && pending.kind === kind ? pending.pts : [];
        const pts = [...base, p];
        if (pts.length >= need) {
          pushUndo();
          setMeasurements((ms) => [...ms, { id: newId(), side, kind, pts }]);
          setPending(null);
        } else {
          setPending({ side, kind, pts });
        }
      } else if (tool === "text") {
        pushUndo();
        const id = newId();
        setTexts((prev) => [...prev, { id, side, x: p.x, y: p.y, text: "" }]);
        setEditingTextId(id);
      }
    }
  };

  const zoomBy = (factor: number) => {
    const qr = qPanelRef.current?.getBoundingClientRect();
    const rr = rPanelRef.current?.getBoundingClientRect();
    setVt((prev) => ({
      q: qr ? zoomAt(prev.q, factor, { x: qr.width / 2, y: qr.height / 2 }) : prev.q,
      r:
        mode === "overlay"
          ? qr
            ? zoomAt(prev.r, factor, { x: qr.width / 2, y: qr.height / 2 })
            : prev.r
          : rr
            ? zoomAt(prev.r, factor, { x: rr.width / 2, y: rr.height / 2 })
            : prev.r,
    }));
  };

  // 1:1 — escala exata 1 (pixel real). Centraliza cada lado no seu painel.
  const setOneToOne = () => {
    const qr = qPanelRef.current?.getBoundingClientRect();
    const rEl = (mode === "overlay" ? qPanelRef : rPanelRef).current;
    const rr = rEl?.getBoundingClientRect();
    setVt((prev) => ({
      q: qr ? zoomTo(prev.q, 1, { x: qr.width / 2, y: qr.height / 2 }) : prev.q,
      r: rr ? zoomTo(prev.r, 1, { x: rr.width / 2, y: rr.height / 2 }) : prev.r,
    }));
  };

  const deleteMarker = (id: string) => {
    pushUndo();
    setMarkers((prev) => prev.filter((m) => m.id !== id));
  };
  const setMarkerNote = (id: string, note: string) =>
    setMarkers((prev) => prev.map((m) => (m.id === id ? { ...m, note } : m)));
  const clearMarkers = () => {
    pushUndo();
    setMarkers([]);
    setCurrentN(1);
    setSelectedMarkerId(null);
  };

  // --- Lápis / texto ---
  const setTextContent = (id: string, text: string) =>
    setTexts((prev) => prev.map((t) => (t.id === id ? { ...t, text } : t)));
  const commitText = (id: string) => {
    setEditingTextId((cur) => (cur === id ? null : cur));
    // Rótulo em branco não vira anotação.
    setTexts((prev) => prev.filter((t) => t.id !== id || t.text.trim() !== ""));
  };
  const deleteStroke = (id: string) => {
    pushUndo();
    setStrokes((prev) => prev.filter((s) => s.id !== id));
  };
  const deleteText = (id: string) => {
    pushUndo();
    setTexts((prev) => prev.filter((t) => t.id !== id));
    setEditingTextId((cur) => (cur === id ? null : cur));
  };
  const clearStrokes = () => {
    pushUndo();
    setStrokes([]);
    setTexts([]);
    setEditingTextId(null);
  };

  // --- Ferramentas de anotação (mutuamente exclusivas) ---
  const toggleTool = (t: Tool) => {
    setPending(null);
    setEditingTextId(null);
    setTool((cur) => (cur === t ? "pan" : t));
  };

  // --- Medição / calibração ---
  const calibFor = (side: "q" | "r") => (side === "q" ? qCalib : rCalib);
  /** Define a calibração de um lado a partir de uma medição de distância e
   * seu comprimento real em mm (mm/px = mm / pixels da linha). */
  const setCalibFromMeasurement = (m: Measurement, mmText: string) => {
    const mm = parseFloat(mmText.replace(",", "."));
    const [a, b] = m.pts;
    if (!(mm > 0) || m.kind !== "dist" || !a || !b) return;
    const px = distance(a, b);
    if (px <= 0) return;
    pushUndo();
    const calib: Calib = { refId: m.id, mm, mmPerPx: mm / px };
    if (m.side === "q") setQCalib(calib);
    else setRCalib(calib);
  };
  const deleteMeasurement = (id: string) => {
    pushUndo();
    setMeasurements((prev) => prev.filter((m) => m.id !== id));
    setQCalib((c) => (c && c.refId === id ? null : c));
    setRCalib((c) => (c && c.refId === id ? null : c));
  };
  const clearMeasurements = () => {
    pushUndo();
    setMeasurements([]);
    setQCalib(null);
    setRCalib(null);
    setPending(null);
  };
  /** Rótulo de uma medição: ângulo em graus; distância em mm se o lado estiver
   * calibrado, senão em px. */
  const measureLabel = (m: Measurement): string => {
    const [a, b, c] = m.pts;
    if (m.kind === "angle" && a && b && c) {
      return `${angleAtDeg(a, b, c).toFixed(1)}°`;
    }
    if (a && b) {
      const px = distance(a, b);
      const calib = calibFor(m.side);
      return calib ? `${(px * calib.mmPerPx).toFixed(2)} mm` : `${px.toFixed(0)} px`;
    }
    return "";
  };

  // Atalhos (ferramentas, vista, edição) — ligados ao keymap customizável mais
  // abaixo, depois que todos os handlers estiverem definidos.

  // Enquadrar = reenquadrar a partir do tamanho natural real das <img>.
  const fitBoth = () => {
    restoreRef.current = {};
    refitFromImages();
  };

  // Reenquadra a partir do tamanho natural real das <img> montadas.
  const refitFromImages = () => {
    const qImg = qPanelRef.current?.querySelector("img[data-side='q']") as HTMLImageElement | null;
    const rImg = (mode === "overlay" ? qPanelRef : rPanelRef).current?.querySelector(
      "img[data-side='r']",
    ) as HTMLImageElement | null;
    if (qImg && qImg.naturalWidth) fitSide("q", { w: qImg.naturalWidth, h: qImg.naturalHeight });
    if (rImg && rImg.naturalWidth) fitSide("r", { w: rImg.naturalWidth, h: rImg.naturalHeight });
  };

  const resetView = () => {
    setVt({ q: ZERO, r: ZERO });
    refitFromImages();
  };

  const swap = () => {
    setQDocId(rDocId);
    setRDocId(qDocId);
    setQPage(rPage);
    setRPage(qPage);
    setVt((prev) => ({ q: prev.r, r: prev.q }));
  };

  const pageCount = (doc: DocumentCaseFile | null) =>
    doc && doc.file_type === "pdf" ? Math.max(1, doc.page_count) : 1;

  const canSave = !!qDocId && !!rDocId;

  const saveConfronto = async () => {
    if (!canSave) return;
    setBusy(true);
    setErr(null);
    try {
      const results: ConfrontoResults = {
        schema: RESULTS_SCHEMA,
        questioned: { documentId: qDocId!, page: qPage },
        reference: { documentId: rDocId!, page: rPage },
        mode,
        lockZoom,
        syncPan,
        sharpPixels,
        qView: vt.q,
        rView: vt.r,
        grid: { on: grid, size: gridSize },
        overlay: { opacity, blendDiff },
        markers,
        currentN,
        measurements,
        strokes,
        texts,
        qCalib,
        rCalib,
      };
      const session = await commands.saveComparison(
        ws,
        qDocId!,
        rDocId!,
        CONFRONTO_TYPE,
        JSON.stringify(results),
        observations,
      );
      setLoadedId(session.id);
      await reloadSaved();
    } catch (e) {
      setErr(toSicroError(e).message);
    } finally {
      setBusy(false);
    }
  };

  const loadConfronto = (session: ComparisonSession) => {
    try {
      const r = JSON.parse(session.results_json || "{}") as Partial<ConfrontoResults>;
      // `restoreRef` cobre o caso em que as imagens recarregam (onImgLoad
      // aplica a vista salva em vez de reenquadrar). Aplicar `vt` direto
      // cobre o caso em que os documentos já são os atuais (sem recarga).
      restoreRef.current = { q: r.qView, r: r.rView };
      if (r.qView && r.rView) setVt({ q: r.qView, r: r.rView });
      setQDocId(session.questioned_document_id);
      setRDocId(session.reference_document_id);
      setQPage(r.questioned?.page ?? 1);
      setRPage(r.reference?.page ?? 1);
      setMode(r.mode ?? "side");
      setLockZoom(r.lockZoom ?? true);
      setSyncPan(r.syncPan ?? true);
      setGrid(r.grid?.on ?? false);
      setGridSize(r.grid?.size ?? 40);
      setOpacity(r.overlay?.opacity ?? 0.5);
      setBlendDiff(r.overlay?.blendDiff ?? false);
      setSharpPixels(r.sharpPixels ?? true);
      setMarkers(Array.isArray(r.markers) ? r.markers : []);
      setCurrentN(r.currentN ?? 1);
      setMeasurements(Array.isArray(r.measurements) ? r.measurements : []);
      setStrokes(Array.isArray(r.strokes) ? r.strokes : []);
      setTexts(Array.isArray(r.texts) ? r.texts : []);
      setQCalib(r.qCalib ?? null);
      setRCalib(r.rCalib ?? null);
      setTool("pan");
      setEditingTextId(null);
      undoStack.current = [];
      redoStack.current = [];
      setPending(null);
      setObservations(session.summary ?? "");
      setLoadedId(session.id);
    } catch (e) {
      setErr(toSicroError(e).message);
    }
  };

  const loadImageEl = (src: string) =>
    new Promise<HTMLImageElement>((resolve, reject) => {
      const im = new Image();
      // Sem isto, desenhar a imagem (protocolo asset, cross-origin) "contamina"
      // o canvas e o toDataURL falha ("Tainted canvases may not be exported").
      // O protocolo asset do Tauri responde com CORS — mesmo padrão do croqui.
      im.crossOrigin = "anonymous";
      im.onload = () => resolve(im);
      im.onerror = () => reject(new Error("falha ao carregar imagem"));
      im.src = src;
    });

  /**
   * Compõe o confronto atual (enquadramento + marcadores + medições + cabeçalho
   * + rodapé §13) num PNG e salva no workspace (documentoscopia/confrontos).
   * A inserção direta no corpo do laudo chega com a reformulação da aba
   * Evidências; por ora o perito arrasta o PNG para o laudo.
   */
  const exportConfronto = async () => {
    if (!canSave) return;
    const qEl = qPanelRef.current;
    if (!qEl) return;
    setBusy(true);
    setErr(null);
    try {
      const EF = 2;
      const pad = 16 * EF;
      const headerH = 56 * EF;
      const footerH = 50 * EF;
      const gap = 12 * EF;
      const qr = qEl.getBoundingClientRect();
      const pw = Math.max(1, Math.round(qr.width * EF));
      const ph = Math.max(1, Math.round(qr.height * EF));
      const isSide = mode === "side";
      const cw = (isSide ? pw * 2 + gap : pw) + pad * 2;
      const ch = headerH + ph + footerH;
      const canvas = document.createElement("canvas");
      canvas.width = cw;
      canvas.height = ch;
      const ctx = canvas.getContext("2d");
      if (!ctx) throw new Error("canvas indisponível");

      ctx.fillStyle = "#ffffff";
      ctx.fillRect(0, 0, cw, ch);
      ctx.textBaseline = "top";
      ctx.fillStyle = "#111827";
      ctx.font = `bold ${15 * EF}px sans-serif`;
      ctx.fillText("Confronto documentoscópico", pad, 10 * EF);
      ctx.font = `${11 * EF}px sans-serif`;
      ctx.fillStyle = "#374151";
      ctx.fillText(
        `Questionado: ${qDoc?.title ?? "—"}    ×    Padrão: ${rDoc?.title ?? "—"}`,
        pad,
        30 * EF,
      );
      ctx.fillStyle = "#6b7280";
      ctx.fillText(new Date().toLocaleString("pt-BR"), pad, 42 * EF);

      const qImg = qSrc ? await loadImageEl(qSrc) : null;
      const rImg = rSrc ? await loadImageEl(rSrc) : null;

      const drawImg = (
        img: HTMLImageElement,
        view: ViewTransform,
        ox: number,
        alpha = 1,
        blend: GlobalCompositeOperation = "source-over",
      ) => {
        ctx.save();
        ctx.globalAlpha = alpha;
        ctx.globalCompositeOperation = blend;
        ctx.imageSmoothingEnabled = !(sharpPixels && view.scale >= 1);
        ctx.drawImage(
          img,
          ox + view.x * EF,
          headerH + view.y * EF,
          img.naturalWidth * view.scale * EF,
          img.naturalHeight * view.scale * EF,
        );
        ctx.restore();
      };
      const drawAnnotations = (side: "q" | "r", view: ViewTransform, ox: number) => {
        const col = side === "q" ? "#d98a00" : "#1d6fd8";
        const tx = (p: { x: number; y: number }) => {
          const s = imageToScreen(view, p);
          return { x: ox + s.x * EF, y: headerH + s.y * EF };
        };
        const label = (text: string, x: number, y: number) => {
          ctx.save();
          ctx.font = `bold ${11 * EF}px sans-serif`;
          ctx.lineWidth = 3 * EF;
          ctx.strokeStyle = "rgba(0,0,0,.75)";
          ctx.fillStyle = "#fff";
          ctx.strokeText(text, x, y);
          ctx.fillText(text, x, y);
          ctx.restore();
        };
        ctx.save();
        ctx.lineWidth = 1.5 * EF;
        ctx.strokeStyle = col;
        ctx.fillStyle = col;
        measurements
          .filter((m) => m.side === side)
          .forEach((m) => {
            const [a, b, c] = m.pts.map(tx);
            if (m.kind === "dist" && a && b) {
              ctx.beginPath();
              ctx.moveTo(a.x, a.y);
              ctx.lineTo(b.x, b.y);
              ctx.stroke();
              [a, b].forEach((p) => {
                ctx.beginPath();
                ctx.arc(p.x, p.y, 3 * EF, 0, Math.PI * 2);
                ctx.fill();
              });
              label(measureLabel(m), (a.x + b.x) / 2, (a.y + b.y) / 2 - 6 * EF);
            } else if (m.kind === "angle" && a && b && c) {
              ctx.beginPath();
              ctx.moveTo(a.x, a.y);
              ctx.lineTo(b.x, b.y);
              ctx.lineTo(c.x, c.y);
              ctx.stroke();
              [a, b, c].forEach((p) => {
                ctx.beginPath();
                ctx.arc(p.x, p.y, 3 * EF, 0, Math.PI * 2);
                ctx.fill();
              });
              label(measureLabel(m), b.x + 6 * EF, b.y - 6 * EF);
            }
          });
        markers
          .filter((m) => m.side === side)
          .forEach((m) => {
            const p = tx({ x: m.x, y: m.y });
            ctx.beginPath();
            ctx.arc(p.x, p.y, 9 * EF, 0, Math.PI * 2);
            ctx.fill();
            ctx.save();
            ctx.fillStyle = "#fff";
            ctx.textAlign = "center";
            ctx.textBaseline = "middle";
            ctx.font = `bold ${11 * EF}px sans-serif`;
            ctx.fillText(String(m.n), p.x, p.y);
            ctx.restore();
          });
        // Traços de lápis (linhas livres).
        strokes
          .filter((s) => s.side === side)
          .forEach((s) => {
            const pts = s.pts.map(tx);
            const [first, ...rest] = pts;
            if (!first) return;
            ctx.save();
            ctx.lineWidth = 2 * EF;
            ctx.lineJoin = "round";
            ctx.lineCap = "round";
            ctx.beginPath();
            ctx.moveTo(first.x, first.y);
            rest.forEach((q) => ctx.lineTo(q.x, q.y));
            ctx.stroke();
            ctx.restore();
          });
        // Textos livres.
        texts
          .filter((t) => t.side === side && t.text.trim())
          .forEach((t) => {
            const p = tx({ x: t.x, y: t.y });
            label(t.text, p.x, p.y);
          });
        ctx.restore();
      };
      const clipPanel = (ox: number, fn: () => void) => {
        ctx.save();
        ctx.beginPath();
        ctx.rect(ox, headerH, pw, ph);
        ctx.clip();
        ctx.fillStyle = "#f3f4f6";
        ctx.fillRect(ox, headerH, pw, ph);
        fn();
        ctx.restore();
      };

      if (isSide) {
        const ox2 = pad + pw + gap;
        clipPanel(pad, () => {
          if (qImg) drawImg(qImg, vt.q, pad);
          drawAnnotations("q", vt.q, pad);
        });
        clipPanel(ox2, () => {
          if (rImg) drawImg(rImg, vt.r, ox2);
          drawAnnotations("r", vt.r, ox2);
        });
        ctx.lineWidth = 2 * EF;
        ctx.strokeStyle = "#d98a00";
        ctx.strokeRect(pad, headerH, pw, ph);
        ctx.strokeStyle = "#1d6fd8";
        ctx.strokeRect(ox2, headerH, pw, ph);
      } else {
        clipPanel(pad, () => {
          if (qImg) drawImg(qImg, vt.q, pad);
          if (rImg)
            drawImg(rImg, vt.r, pad, opacity, blendDiff ? "difference" : "source-over");
          drawAnnotations("q", vt.q, pad);
          drawAnnotations("r", vt.r, pad);
        });
        ctx.lineWidth = 2 * EF;
        ctx.strokeStyle = "#9ca3af";
        ctx.strokeRect(pad, headerH, pw, ph);
      }

      ctx.textBaseline = "top";
      ctx.textAlign = "left";
      ctx.fillStyle = "#374151";
      ctx.font = `${10 * EF}px sans-serif`;
      ctx.fillText(
        `Calibração — Questionado: ${qCalib ? `${qCalib.mmPerPx.toFixed(4)} mm/px` : "sem escala"} · Padrão: ${rCalib ? `${rCalib.mmPerPx.toFixed(4)} mm/px` : "sem escala"}`,
        pad,
        headerH + ph + 6 * EF,
      );
      ctx.fillStyle = "#6b7280";
      ctx.font = `${9.5 * EF}px sans-serif`;
      ctx.fillText(
        "Ferramenta de alinhamento e medição visual — não calcula similaridade nem conclui autoria/autenticidade. A conclusão é exclusiva do perito (§13).",
        pad,
        headerH + ph + 22 * EF,
      );

      const dataUrl = canvas.toDataURL("image/png");
      const name = `confronto_${(qDoc?.title ?? "q").slice(0, 18)}_x_${(rDoc?.title ?? "p").slice(0, 18)}.png`;
      const rel = await commands.saveConfrontoImage(ws, dataUrl, name);
      await commands.revealEvidenceInFolder(ws, rel);
    } catch (e) {
      setErr(toSicroError(e).message);
    } finally {
      setBusy(false);
    }
  };

  const layerStyle = (view: ViewTransform, extra?: CSSProperties): CSSProperties => ({
    position: "absolute",
    left: 0,
    top: 0,
    transformOrigin: "0 0",
    transform: `translate(${view.x}px, ${view.y}px) scale(${view.scale})`,
    // Acima de 1× sem interpolação = pixels nítidos (pixel peeping forense).
    // Abaixo de 1× (redução p/ enquadrar) deixamos suavizar para evitar aliasing.
    imageRendering: sharpPixels && view.scale >= 1 ? "pixelated" : "auto",
    userSelect: "none",
    pointerEvents: "none",
    maxWidth: "none",
    ...extra,
  });

  const gridStyle = (view: ViewTransform): CSSProperties => {
    const step = Math.max(3, gridSize * view.scale);
    return {
      position: "absolute",
      inset: 0,
      pointerEvents: "none",
      backgroundImage:
        "linear-gradient(to right, rgba(56,150,255,.5) 0 1px, transparent 1px), linear-gradient(to bottom, rgba(56,150,255,.5) 0 1px, transparent 1px)",
      backgroundSize: `${step}px ${step}px`,
      backgroundPosition: `${view.x}px ${view.y}px`,
    };
  };

  // Dots dos marcadores de um lado, posicionados via imageToScreen (seguem
  // zoom/pan). pointer-events:none — não bloqueiam o arraste/clique do painel.
  const markerDots = (side: "q" | "r") => {
    const view = side === "q" ? vt.q : vt.r;
    return markers
      .filter((m) => m.side === side)
      .map((m) => {
        const s = imageToScreen(view, { x: m.x, y: m.y });
        return (
          <div
            key={m.id}
            className={styles.cfMarker}
            data-side={side}
            data-selected={m.id === selectedMarkerId}
            style={{ left: `${s.x}px`, top: `${s.y}px` }}
            title={m.note || `Ponto ${m.n}`}
          >
            {m.n}
          </div>
        );
      });
  };

  // Overlay SVG das medições de um lado (linhas/ângulos + pontos pendentes),
  // posicionado via imageToScreen (segue zoom/pan). pointer-events:none.
  const measureSvg = (side: "q" | "r") => {
    const view = side === "q" ? vt.q : vt.r;
    const items = measurements.filter((m) => m.side === side);
    const isPending = pending?.side === side;
    if (items.length === 0 && !isPending) return null;
    const stroke = side === "q" ? "var(--sicro-warning)" : "var(--sicro-info)";
    const toScr = (p: { x: number; y: number }) => imageToScreen(view, p);
    return (
      <svg className={styles.cfMeasure} aria-hidden>
        {items.map((m) => {
          const [p0, p1, p2] = m.pts.map(toScr);
          const label = measureLabel(m);
          if (m.kind === "dist" && p0 && p1) {
            const mid = { x: (p0.x + p1.x) / 2, y: (p0.y + p1.y) / 2 };
            return (
              <g key={m.id}>
                <line x1={p0.x} y1={p0.y} x2={p1.x} y2={p1.y} stroke={stroke} strokeWidth={1.5} />
                <circle cx={p0.x} cy={p0.y} r={3} fill={stroke} />
                <circle cx={p1.x} cy={p1.y} r={3} fill={stroke} />
                <text x={mid.x} y={mid.y - 5} className={styles.cfMeasureText}>
                  {label}
                </text>
              </g>
            );
          }
          if (m.kind === "angle" && p0 && p1 && p2) {
            return (
              <g key={m.id}>
                <polyline
                  points={`${p0.x},${p0.y} ${p1.x},${p1.y} ${p2.x},${p2.y}`}
                  fill="none"
                  stroke={stroke}
                  strokeWidth={1.5}
                />
                <circle cx={p0.x} cy={p0.y} r={3} fill={stroke} />
                <circle cx={p1.x} cy={p1.y} r={3} fill={stroke} />
                <circle cx={p2.x} cy={p2.y} r={3} fill={stroke} />
                <text x={p1.x + 7} y={p1.y - 7} className={styles.cfMeasureText}>
                  {label}
                </text>
              </g>
            );
          }
          return null;
        })}
        {isPending &&
          pending!.pts.map((p, i) => {
            const s = toScr(p);
            return (
              <circle key={i} cx={s.x} cy={s.y} r={4} className={styles.cfPendingDot} />
            );
          })}
        {isPending && pending!.pts.length >= 2 && (
          <polyline
            points={pending!.pts.map((p) => { const s = toScr(p); return `${s.x},${s.y}`; }).join(" ")}
            fill="none"
            stroke={stroke}
            strokeWidth={1}
            strokeDasharray="4 3"
          />
        )}
      </svg>
    );
  };

  // Traços de lápis (e o traço em desenho) de um lado, em SVG (seguem zoom/pan).
  const strokesSvg = (side: "q" | "r") => {
    const view = side === "q" ? vt.q : vt.r;
    const col = side === "q" ? "var(--sicro-warning)" : "var(--sicro-info)";
    const list = strokes.filter((s) => s.side === side);
    const drawingHere = draft && draft.side === side ? draft : null;
    if (list.length === 0 && !drawingHere) return null;
    const path = (pts: { x: number; y: number }[]) =>
      pts
        .map((p, i) => {
          const s = imageToScreen(view, p);
          return `${i === 0 ? "M" : "L"}${s.x},${s.y}`;
        })
        .join(" ");
    return (
      <svg className={styles.cfMeasure} aria-hidden>
        {list.map((s) => (
          <path
            key={s.id}
            d={path(s.pts)}
            fill="none"
            stroke={col}
            strokeWidth={2}
            strokeLinejoin="round"
            strokeLinecap="round"
          />
        ))}
        {drawingHere && drawingHere.pts.length >= 2 && (
          <path
            d={path(drawingHere.pts)}
            fill="none"
            stroke={col}
            strokeWidth={2}
            strokeLinejoin="round"
            strokeLinecap="round"
            opacity={0.85}
          />
        )}
      </svg>
    );
  };

  // Rótulos de texto de um lado (HTML, seguem zoom/pan). O que está em edição
  // vira um <input>.
  const textLabels = (side: "q" | "r") => {
    const view = side === "q" ? vt.q : vt.r;
    return texts
      .filter((t) => t.side === side)
      .map((t) => {
        const s = imageToScreen(view, { x: t.x, y: t.y });
        if (t.id === editingTextId) {
          return (
            <input
              key={t.id}
              className={styles.cfTextInput}
              data-side={side}
              autoFocus
              value={t.text}
              placeholder="texto…"
              style={{ left: `${s.x}px`, top: `${s.y}px` }}
              onPointerDown={(e) => e.stopPropagation()}
              onChange={(e) => setTextContent(t.id, e.target.value)}
              onBlur={() => commitText(t.id)}
              onKeyDown={(e) => {
                if (e.key === "Enter" || e.key === "Escape") commitText(t.id);
              }}
            />
          );
        }
        return (
          <div
            key={t.id}
            className={styles.cfText}
            data-side={side}
            style={{ left: `${s.x}px`, top: `${s.y}px` }}
            onPointerDown={(e) => {
              e.stopPropagation();
              setEditingTextId(t.id);
            }}
            title="Clique para editar"
          >
            {t.text || "texto…"}
          </div>
        );
      });
  };

  const zoomPct = Math.round(vt.q.scale * 100);
  const activeTool = tool;

  // --- Atalhos de teclado (customizáveis em Configurações → Atalhos) ---
  const pickTool = (t: Tool) => {
    setPending(null);
    setEditingTextId(null);
    setTool(t);
  };
  useShortcuts({
    "confronto.tool.pan": () => pickTool("pan"),
    "confronto.tool.marker": () => pickTool("marker"),
    "confronto.tool.ruler": () => pickTool("ruler"),
    "confronto.tool.angle": () => pickTool("angle"),
    "confronto.tool.pencil": () => pickTool("pencil"),
    "confronto.tool.text": () => pickTool("text"),
    "confronto.nextMarker": () => setCurrentN((n) => n + 1),
    "confronto.cancel": () => pickTool("pan"),
    "confronto.mode.side": () => setMode("side"),
    "confronto.mode.overlay": () => setMode("overlay"),
    "confronto.lockZoom": () => setLockZoom((v) => !v),
    "confronto.syncPan": () => setSyncPan((v) => !v),
    "confronto.grid": () => setGrid((v) => !v),
    "confronto.sharp": () => setSharpPixels((v) => !v),
    "confronto.fit": () => fitBoth(),
    "confronto.oneToOne": () => setOneToOne(),
    "confronto.zoomIn": () => zoomBy(1.25),
    "confronto.zoomOut": () => zoomBy(1 / 1.25),
    "confronto.resetView": () => resetView(),
    "confronto.swap": () => {
      if (canSave) swap();
    },
    "confronto.undo": () => undo(),
    "confronto.redo": () => redo(),
    "confronto.save": () => {
      if (canSave && !busy) void saveConfronto();
    },
    "confronto.export": () => {
      if (canSave && !busy) void exportConfronto();
    },
  });

  if (documents.length === 0) {
    return (
      <div className={styles.cfEmpty}>
        <Columns2 size={40} strokeWidth={1.2} />
        <p>Importe ao menos um documento para iniciar um confronto.</p>
      </div>
    );
  }

  return (
    <div className={styles.cfWrap}>
      {/* Seletores + ações */}
      <div className={styles.cfPickers}>
        <DocPicker
          tone="questioned"
          label="Questionado"
          docs={documents}
          value={qDocId}
          onChange={(id) => {
            setQDocId(id);
            setQPage(1);
          }}
          page={qPage}
          pageCount={pageCount(qDoc)}
          onPage={setQPage}
        />
        <button
          className={styles.cfSwap}
          title="Trocar lados"
          onClick={swap}
          disabled={!canSave}
        >
          <ArrowLeftRight size={15} />
        </button>
        <DocPicker
          tone="reference"
          label="Padrão"
          docs={documents}
          value={rDocId}
          onChange={(id) => {
            setRDocId(id);
            setRPage(1);
          }}
          page={rPage}
          pageCount={pageCount(rDoc)}
          onPage={setRPage}
        />
      </div>

      {/* Barra de ferramentas */}
      <div className={styles.cfToolbar}>
        <div className={styles.cfModeSwitch}>
          <button
            data-active={mode === "side"}
            onClick={() => setMode("side")}
            title="Lado a lado"
          >
            <Columns2 size={14} /> Lado a lado
          </button>
          <button
            data-active={mode === "overlay"}
            onClick={() => setMode("overlay")}
            title="Sobreposição"
          >
            <Layers size={14} /> Sobreposição
          </button>
        </div>

        <span className={styles.cfDivider} />

        <button className={styles.cfToolBtn} onClick={() => zoomBy(1 / 1.25)} title="Reduzir">
          −
        </button>
        <span className={styles.cfZoom}>{zoomPct}%</span>
        <button className={styles.cfToolBtn} onClick={() => zoomBy(1.25)} title="Ampliar">
          +
        </button>
        <button className={styles.cfToolBtn} onClick={fitBoth} title="Enquadrar">
          <Maximize2 size={14} />
        </button>
        <button
          className={styles.cfToolBtn}
          onClick={setOneToOne}
          title="1:1 — pixel real (escala 100%)"
        >
          1:1
        </button>
        <button className={styles.cfToolBtn} onClick={resetView} title="Redefinir vista">
          <RotateCcw size={14} />
        </button>
        <button
          className={styles.cfToggle}
          data-on={sharpPixels}
          onClick={() => setSharpPixels((v) => !v)}
          title="Pixels nítidos sem interpolação (ampliar não cria detalhe além da resolução original)"
        >
          Pixels nítidos
        </button>

        <span className={styles.cfDivider} />

        <button
          className={styles.cfToggle}
          data-on={lockZoom}
          onClick={() => setLockZoom((v) => !v)}
          title="Travar zoom dos dois lados juntos"
        >
          {lockZoom ? <Link2 size={14} /> : <Link2Off size={14} />} Zoom travado
        </button>
        {mode === "side" && (
          <button
            className={styles.cfToggle}
            data-on={syncPan}
            onClick={() => setSyncPan((v) => !v)}
            title="Mover os dois lados juntos ao arrastar"
          >
            {syncPan ? <Link2 size={14} /> : <Link2Off size={14} />} Pan sincronizado
          </button>
        )}
        <button
          className={styles.cfToggle}
          data-on={grid}
          onClick={() => setGrid((v) => !v)}
          title="Grade de referência"
        >
          <Grid3x3 size={14} /> Grade
        </button>
        {grid && (
          <input
            className={styles.cfGridSize}
            type="number"
            min={8}
            max={400}
            value={gridSize}
            onChange={(e) => setGridSize(Math.max(8, Number(e.target.value) || 40))}
            title="Espaçamento da grade (px da imagem)"
          />
        )}

        {mode === "overlay" && (
          <>
            <span className={styles.cfDivider} />
            <label className={styles.cfSlider} title="Opacidade do padrão">
              Opacidade
              <input
                type="range"
                min={0}
                max={100}
                value={Math.round(opacity * 100)}
                onChange={(e) => setOpacity(Number(e.target.value) / 100)}
              />
            </label>
            <button
              className={styles.cfToggle}
              data-on={blendDiff}
              onClick={() => setBlendDiff((v) => !v)}
              title="Realça desalinhamento de traço (modo diferença)"
            >
              Diferença
            </button>
          </>
        )}

        {mode === "side" && (
          <>
            <span className={styles.cfDivider} />
            <button
              className={styles.cfToggle}
              data-on={markerMode}
              onClick={() => toggleTool("marker")}
              title="Soltar pontos numerados — clique no documento (mesmo nº nos dois lados = ponto correspondente)"
            >
              <MapPin size={14} /> Marcadores
            </button>
            {markerMode && (
              <>
                <span className={styles.cfMarkerN}>Ponto nº {currentN}</span>
                <button
                  className={styles.cfToolBtn}
                  onClick={() => setCurrentN((n) => n + 1)}
                  title="Avançar para o próximo número"
                >
                  <Plus size={14} />
                </button>
              </>
            )}
            <button
              className={styles.cfToggle}
              data-on={tool === "ruler"}
              onClick={() => toggleTool("ruler")}
              title="Medir distância — clique em 2 pontos (vira mm após calibrar o lado)"
            >
              <Ruler size={14} /> Medir
            </button>
            <button
              className={styles.cfToggle}
              data-on={tool === "angle"}
              onClick={() => toggleTool("angle")}
              title="Medir ângulo — clique em 3 pontos (vértice no meio)"
            >
              <Triangle size={14} /> Ângulo
            </button>
            <button
              className={styles.cfToggle}
              data-on={tool === "pencil"}
              onClick={() => toggleTool("pencil")}
              title="Lápis — arraste para desenhar um traço livre sobre o documento"
            >
              <Pencil size={14} /> Lápis
            </button>
            <button
              className={styles.cfToggle}
              data-on={tool === "text"}
              onClick={() => toggleTool("text")}
              title="Texto — clique no documento para inserir um rótulo"
            >
              <Type size={14} /> Texto
            </button>
            {pending && (
              <span className={styles.cfMarkerN}>
                {pending.kind === "angle"
                  ? `ângulo ${pending.pts.length}/3`
                  : `medir ${pending.pts.length}/2`}
              </span>
            )}
          </>
        )}

        <span className={styles.cfDivider} />
        <button
          className={styles.cfToolBtn}
          onClick={undo}
          disabled={!canUndo}
          title="Desfazer (Ctrl+Z)"
        >
          <Undo2 size={14} />
        </button>
        <button
          className={styles.cfToolBtn}
          onClick={redo}
          disabled={!canRedo}
          title="Refazer (Ctrl+Shift+Z / Ctrl+Y)"
        >
          <Redo2 size={14} />
        </button>
      </div>

      {/* Visualizadores */}
      <div className={styles.cfStage} data-mode={mode} data-dragging={dragging}>
        {mode === "side" ? (
          <>
            <div
              className={styles.cfPanel}
              data-tone="questioned"
              data-marker={activeTool !== "pan"}
              ref={qPanelRef}
              onPointerDown={onDown("q")}
              onPointerMove={onMove}
              onPointerUp={onUp}
            >
              <span className={styles.cfPanelTag} data-tone="questioned">
                Questionado
              </span>
              {qSrc ? (
                <img
                  data-side="q"
                  src={qSrc}
                  alt="Questionado"
                  style={layerStyle(vt.q)}
                  onLoad={onImgLoad("q")}
                  draggable={false}
                />
              ) : (
                <span className={styles.cfPanelEmpty}>Selecione o documento questionado</span>
              )}
              {grid && qSrc && <div style={gridStyle(vt.q)} />}
              {qSrc && strokesSvg("q")}
              {qSrc && measureSvg("q")}
              {qSrc && markerDots("q")}
              {qSrc && textLabels("q")}
            </div>
            <div
              className={styles.cfPanel}
              data-tone="reference"
              data-marker={activeTool !== "pan"}
              ref={rPanelRef}
              onPointerDown={onDown("r")}
              onPointerMove={onMove}
              onPointerUp={onUp}
            >
              <span className={styles.cfPanelTag} data-tone="reference">
                Padrão
              </span>
              {rSrc ? (
                <img
                  data-side="r"
                  src={rSrc}
                  alt="Padrão"
                  style={layerStyle(vt.r)}
                  onLoad={onImgLoad("r")}
                  draggable={false}
                />
              ) : (
                <span className={styles.cfPanelEmpty}>Selecione o documento padrão</span>
              )}
              {grid && rSrc && <div style={gridStyle(vt.r)} />}
              {rSrc && strokesSvg("r")}
              {rSrc && measureSvg("r")}
              {rSrc && markerDots("r")}
              {rSrc && textLabels("r")}
            </div>
          </>
        ) : (
          <div
            className={styles.cfPanel}
            data-tone="overlay"
            data-marker={activeTool !== "pan"}
            ref={qPanelRef}
            onPointerDown={onDown("overlay")}
            onPointerMove={onMove}
            onPointerUp={onUp}
          >
            <span className={styles.cfPanelTag}>Sobreposição — arraste para alinhar o padrão</span>
            {qSrc && (
              <img
                data-side="q"
                src={qSrc}
                alt="Questionado"
                style={layerStyle(vt.q)}
                onLoad={onImgLoad("q")}
                draggable={false}
              />
            )}
            {rSrc && (
              <img
                data-side="r"
                src={rSrc}
                alt="Padrão"
                style={layerStyle(vt.r, {
                  opacity,
                  mixBlendMode: blendDiff ? "difference" : "normal",
                })}
                onLoad={onImgLoad("r")}
                draggable={false}
              />
            )}
            {grid && qSrc && <div style={gridStyle(vt.q)} />}
            {qSrc && strokesSvg("q")}
            {rSrc && strokesSvg("r")}
            {qSrc && measureSvg("q")}
            {rSrc && measureSvg("r")}
            {qSrc && markerDots("q")}
            {rSrc && markerDots("r")}
            {qSrc && textLabels("q")}
            {rSrc && textLabels("r")}
          </div>
        )}
      </div>

      {/* Observações + marcadores + salvar */}
      <div className={styles.cfBottom}>
        <div className={styles.cfObs}>
          <label htmlFor="cf-obs">Observações do perito</label>
          <textarea
            id="cf-obs"
            value={observations}
            placeholder="Registre aqui o que observou (convergências, divergências, pontos a examinar). A conclusão é sua."
            onChange={(e) => setObservations(e.target.value)}
          />
        </div>

        <div className={styles.cfMarkersCol}>
          <div className={styles.cfMarkersHead}>
            <span>
              Pontos de confronto{markers.length > 0 ? ` (${markers.length})` : ""}
            </span>
            {markers.length > 0 && (
              <button onClick={clearMarkers} title="Limpar todos os pontos">
                Limpar
              </button>
            )}
          </div>
          {markers.length === 0 ? (
            <p className={styles.cfMarkersHint}>
              Ative <strong>Marcadores</strong> e clique no documento para apontar
              pontos. O mesmo nº em Questionado e Padrão indica correspondência.
            </p>
          ) : (
            <div className={styles.cfMarkersList}>
              {[...markers]
                .sort((a, b) => a.n - b.n || a.side.localeCompare(b.side))
                .map((m) => (
                  <div
                    key={m.id}
                    className={styles.cfMarkerRow}
                    data-selected={m.id === selectedMarkerId}
                    onMouseEnter={() => setSelectedMarkerId(m.id)}
                    onMouseLeave={() => setSelectedMarkerId(null)}
                  >
                    <span className={styles.cfMarkerBadge} data-side={m.side}>
                      {m.n}
                    </span>
                    <span className={styles.cfMarkerSide}>
                      {m.side === "q" ? "Quest." : "Padrão"}
                    </span>
                    <input
                      className={styles.cfMarkerNote}
                      value={m.note}
                      placeholder="nota…"
                      onChange={(e) => setMarkerNote(m.id, e.target.value)}
                    />
                    <button
                      className={styles.cfMarkerDel}
                      onClick={() => deleteMarker(m.id)}
                      title="Remover ponto"
                    >
                      <Trash2 size={12} />
                    </button>
                  </div>
                ))}
            </div>
          )}

          <div className={styles.cfMeasHead}>
            <span>
              Medições{measurements.length > 0 ? ` (${measurements.length})` : ""}
            </span>
            {measurements.length > 0 && (
              <button onClick={clearMeasurements} title="Limpar medições e calibração">
                Limpar
              </button>
            )}
          </div>
          <div className={styles.cfCalibRow}>
            <span data-cal={!!qCalib}>
              Q: {qCalib ? `${qCalib.mmPerPx.toFixed(4)} mm/px` : "sem escala"}
            </span>
            <span data-cal={!!rCalib}>
              P: {rCalib ? `${rCalib.mmPerPx.toFixed(4)} mm/px` : "sem escala"}
            </span>
          </div>
          {measurements.length === 0 ? (
            <p className={styles.cfMarkersHint}>
              <strong>Medir</strong> (2 cliques) ou <strong>Ângulo</strong> (3
              cliques). Para mm, informe o comprimento real de uma linha em “= mm”
              (calibra aquele lado).
            </p>
          ) : (
            <div className={styles.cfMarkersList}>
              {measurements.map((m) => {
                const calib = m.side === "q" ? qCalib : rCalib;
                const isRef = calib?.refId === m.id;
                return (
                  <div key={m.id} className={styles.cfMarkerRow}>
                    <span
                      className={styles.cfMeasBadge}
                      data-side={m.side}
                      title={m.kind === "angle" ? "ângulo" : "distância"}
                    >
                      {m.kind === "angle" ? "∠" : "↔"}
                    </span>
                    <span className={styles.cfMarkerSide}>
                      {m.side === "q" ? "Quest." : "Padrão"}
                    </span>
                    <span className={styles.cfMeasVal}>
                      {measureLabel(m)}
                      {isRef && <span className={styles.cfRefTag}>régua</span>}
                    </span>
                    {m.kind === "dist" && (
                      <input
                        className={styles.cfCalibInput}
                        type="text"
                        inputMode="decimal"
                        placeholder="= mm"
                        defaultValue={isRef ? String(calib?.mm ?? "") : ""}
                        title="Comprimento real desta linha em mm → calibra este lado"
                        onKeyDown={(e) => {
                          if (e.key === "Enter")
                            setCalibFromMeasurement(m, (e.target as HTMLInputElement).value);
                        }}
                        onBlur={(e) => {
                          if (e.target.value.trim())
                            setCalibFromMeasurement(m, e.target.value);
                        }}
                      />
                    )}
                    <button
                      className={styles.cfMarkerDel}
                      onClick={() => deleteMeasurement(m.id)}
                      title="Remover medição"
                    >
                      <Trash2 size={12} />
                    </button>
                  </div>
                );
              })}
            </div>
          )}

          <div className={styles.cfMeasHead}>
            <span>
              Traços &amp; textos
              {strokes.length + texts.length > 0
                ? ` (${strokes.length + texts.length})`
                : ""}
            </span>
            {strokes.length + texts.length > 0 && (
              <button onClick={clearStrokes} title="Limpar traços e textos">
                Limpar
              </button>
            )}
          </div>
          {strokes.length + texts.length === 0 ? (
            <p className={styles.cfMarkersHint}>
              <strong>Lápis</strong> desenha traços livres; <strong>Texto</strong>{" "}
              insere rótulos. Tudo é desfazível (Ctrl+Z).
            </p>
          ) : (
            <div className={styles.cfMarkersList}>
              {strokes.map((s) => (
                <div key={s.id} className={styles.cfMarkerRow}>
                  <span
                    className={styles.cfMeasBadge}
                    data-side={s.side}
                    title="traço de lápis"
                  >
                    ✎
                  </span>
                  <span className={styles.cfMarkerSide}>
                    {s.side === "q" ? "Quest." : "Padrão"}
                  </span>
                  <span className={styles.cfMeasVal}>traço livre</span>
                  <button
                    className={styles.cfMarkerDel}
                    onClick={() => deleteStroke(s.id)}
                    title="Remover traço"
                  >
                    <Trash2 size={12} />
                  </button>
                </div>
              ))}
              {texts.map((t) => (
                <div key={t.id} className={styles.cfMarkerRow}>
                  <span
                    className={styles.cfMeasBadge}
                    data-side={t.side}
                    title="texto"
                  >
                    T
                  </span>
                  <span className={styles.cfMarkerSide}>
                    {t.side === "q" ? "Quest." : "Padrão"}
                  </span>
                  <span className={styles.cfMeasVal}>{t.text || "(vazio)"}</span>
                  <button
                    className={styles.cfMarkerDel}
                    onClick={() => deleteText(t.id)}
                    title="Remover texto"
                  >
                    <Trash2 size={12} />
                  </button>
                </div>
              ))}
            </div>
          )}
        </div>

        <div className={styles.cfSavePane}>
          <Button
            variant="primary"
            size="sm"
            leftIcon={<Save size={14} />}
            onClick={() => void saveConfronto()}
            disabled={!canSave || busy}
          >
            Salvar confronto
          </Button>
          <Button
            variant="secondary"
            size="sm"
            leftIcon={<ImageDown size={14} />}
            onClick={() => void exportConfronto()}
            disabled={!canSave || busy}
            title="Compõe a vista atual (com pontos e medições) num PNG no workspace para anexar ao laudo"
          >
            Exportar imagem
          </Button>
          {saved.length > 0 && (
            <div className={styles.cfSavedList}>
              <span className={styles.cfSavedTitle}>Confrontos salvos</span>
              {saved.map((s) => (
                <div
                  key={s.id}
                  className={styles.cfSavedItem}
                  data-active={s.id === loadedId}
                >
                  <button
                    className={styles.cfSavedOpen}
                    onClick={() => loadConfronto(s)}
                    title="Abrir este confronto"
                  >
                    {docById(s.questioned_document_id)?.title ?? "?"} ×{" "}
                    {docById(s.reference_document_id)?.title ?? "?"}
                    <span className={styles.cfSavedDate}>
                      {new Date(s.created_at).toLocaleString("pt-BR")}
                    </span>
                  </button>
                </div>
              ))}
            </div>
          )}
        </div>
      </div>

      {err && (
        <div className={styles.cfError}>
          <AlertTriangle size={14} /> {err}
          <button onClick={() => setErr(null)}>×</button>
        </div>
      )}

      <p className={styles.cfNote}>
        Ferramenta de <strong>alinhamento e medição visual</strong>. Não calcula
        similaridade nem conclui autoria/autenticidade — a interpretação e a
        conclusão são exclusivamente do perito (§13).
      </p>
    </div>
  );
}

function DocPicker({
  tone,
  label,
  docs,
  value,
  onChange,
  page,
  pageCount,
  onPage,
}: {
  tone: "questioned" | "reference";
  label: string;
  docs: DocumentCaseFile[];
  value: string | null;
  onChange: (id: string | null) => void;
  page: number;
  pageCount: number;
  onPage: (p: number) => void;
}) {
  const doc = docs.find((d) => d.id === value) ?? null;
  return (
    <div className={styles.cfPicker} data-tone={tone}>
      <span className={styles.cfPickerLabel} data-tone={tone}>
        {label}
      </span>
      <select
        className={styles.cfSelect}
        value={value ?? ""}
        onChange={(e) => onChange(e.target.value || null)}
      >
        <option value="">— escolher —</option>
        {docs.map((d) => (
          <option key={d.id} value={d.id}>
            {d.title} · {docTypeLabel(d.doc_type)}
          </option>
        ))}
      </select>
      {doc && pageCount > 1 && (
        <div className={styles.cfPageNav}>
          <button onClick={() => onPage(Math.max(1, page - 1))} disabled={page <= 1}>
            <ChevronLeft size={14} />
          </button>
          <span>
            {page}/{pageCount}
          </span>
          <button
            onClick={() => onPage(Math.min(pageCount, page + 1))}
            disabled={page >= pageCount}
          >
            <ChevronRight size={14} />
          </button>
        </div>
      )}
    </div>
  );
}
