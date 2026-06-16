/**
 * CroquiEditor — orquestra Toolbar + Canvas + InspectorPanel + StatusBar.
 *
 * MVP 6 (Croqui Pericial):
 *   - Toolbar agrupada por domínio (Seleção / Referencial / Via /
 *     Veículos / Pessoas / Vestígios / Anotação / Imagem / Export).
 *   - Mais ferramentas (subtipos de veículo, vestígios, pessoas,
 *     R1/R2 dedicados, setas, faixas, divisões tracejadas, calçada).
 *   - Templates de via inseríveis em um clique.
 *   - Layer panel com agrupamento por categoria + ações por objeto
 *     (visibilidade, lock, renomear, mover, excluir).
 *   - Atalhos: Esc, V, H, Delete, Ctrl+Z, Ctrl+Y/Ctrl+Shift+Z,
 *     Ctrl+D, Ctrl+S.
 *   - Exportação PNG com carimbo técnico (título + escala + timestamp).
 *
 * O `.sicrocroqui` continua sendo a fonte da verdade do croqui; PNG é
 * exportação. Compatibilidade total com docs do Spike E (v0.1) —
 * `coerceCroquiDoc` aplica defaults para campos novos.
 */

import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { open as openFileDialog } from "@tauri-apps/plugin-dialog";
import { convertFileSrc } from "@tauri-apps/api/core";
import {
  selectActiveOccurrence,
  selectActiveWorkspacePath,
  useWorkspaceStore,
} from "@stores/workspaceStore";
import { toSicroError } from "@core/errors";
import { commands } from "@core/commands";
import type { MediaAsset } from "@domain/import";
import { useCroquiStore } from "../store/croquiStore";
import {
  cloneObject,
  computePxPerMeter,
  distancePx,
  fitImageToCanvas,
  formatMeasurement,
  inferCategory,
  makeLine,
  makeMarker,
  makeMeasurement,
  makeText,
  makeVehicle,
  type LineSubtype,
  type MarkerSubtype,
  type SicroCroquiBackgroundImage,
  type SicroCroquiDoc,
  type SicroObject,
  type SicroPoint,
  type VehicleBodyType,
} from "../engine";
import {
  makeParityRoad,
  makeParityRoundabout,
  type ParityMarcacao,
  type ParitySuperficie,
} from "../engine/road-parity";
import { useNavigate } from "react-router-dom";
import { useNavGuard } from "@app/navGuard";
import { useShortcuts } from "@core/useShortcuts";
import { Toolbar } from "./Toolbar";
import { InspectorPanel } from "./InspectorPanel";
import {
  BACKGROUND_SELECTION_ID,
  CanvasStage,
  CROQUI_ZOOM_MAX,
  CROQUI_ZOOM_MIN,
  type CanvasStageHandle,
} from "./CanvasStage";
import { DEFAULT_VIEWPORT, useEditorState, type Tool } from "./useEditorState";
import { UnsavedChangesModal } from "./UnsavedChangesModal";
import { DroneImportModal } from "./DroneImportModal";
import { OsmImportModal, type OsmImportResult } from "./OsmImportModal";
import styles from "./CroquiEditor.module.css";

export function CroquiEditor() {
  const workspacePath = useWorkspaceStore(selectActiveWorkspacePath);
  const occurrence = useWorkspaceStore(selectActiveOccurrence);
  const activeCroqui = useCroquiStore((s) => s.activeCroqui);
  const activeDoc = useCroquiStore((s) => s.activeDoc);
  const saveCurrent = useCroquiStore((s) => s.saveCurrent);
  const exportPng = useCroquiStore((s) => s.exportPng);
  const clearCurrent = useCroquiStore((s) => s.clearCurrent);
  const isExportStale = useCroquiStore((s) => s.isExportStale);
  const lastExportedAt = useCroquiStore((s) => s.lastExportedAt);

  const navigate = useNavigate();
  const registerNavGuard = useNavGuard((s) => s.register);
  const unregisterNavGuard = useNavGuard((s) => s.unregister);

  const [doc, setDoc] = useState<SicroCroquiDoc | null>(activeDoc);
  const [saving, setSaving] = useState(false);
  const [exporting, setExporting] = useState(false);
  const [feedback, setFeedback] = useState<string | null>(null);
  const [showPhotoPicker, setShowPhotoPicker] = useState(false);
  const [showDroneImport, setShowDroneImport] = useState(false);
  const [showOsmImport, setShowOsmImport] = useState(false);
  const editor = useEditorState();
  const canvasRef = useRef<HTMLDivElement | null>(null);
  const stageRef = useRef<CanvasStageHandle | null>(null);
  const [canvasSize, setCanvasSize] = useState({ width: 800, height: 600 });

  // MVP 9 Round 3 — dirty tracking.
  //
  // `lastSavedJson` is a JSON snapshot of the doc at the moment it was
  // last saved. The doc is considered "dirty" whenever the local doc's
  // serialized form differs from `lastSavedJson` — that's the cheapest
  // way to detect any structural change (object added / moved / edited
  // / removed) without instrumenting every mutation path.
  const [lastSavedJson, setLastSavedJson] = useState<string | null>(null);
  const dirty = useMemo(() => {
    if (!doc || !lastSavedJson) return false;
    return JSON.stringify(doc) !== lastSavedJson;
  }, [doc, lastSavedJson]);

  // MVP 9 Round 3 — unsaved-changes modal. When the user tries to
  // navigate away while `dirty`, we stash the intended target here and
  // present the modal; the user picks Save+leave, Discard, or Cancel.
  const [pendingNav, setPendingNav] = useState<null | {
    /** Run after the user confirms (save+leave) or discards. */
    proceed: () => void;
    /** Human-readable target — shown in the modal copy. */
    label?: string;
    /**
     * `resolve` is set when the guard was triggered via the global nav
     * guard (ActivityRail). Resolving it tells the guard whether to
     * proceed or stay.
     */
    resolve?: (proceed: boolean) => void;
  }>(null);

  // Sync the local doc with the store when the active croqui changes
  // — also reset `lastSavedJson` so dirty derivation starts fresh.
  useEffect(() => {
    if (activeDoc) {
      setDoc(activeDoc);
      setLastSavedJson(JSON.stringify(activeDoc));
    }
  }, [activeDoc]);

  // Resize observer for the central canvas column.
  useEffect(() => {
    const el = canvasRef.current;
    if (!el) return;
    const ro = new ResizeObserver(() => {
      const rect = el.getBoundingClientRect();
      setCanvasSize({
        width: Math.max(200, rect.width),
        height: Math.max(200, rect.height),
      });
    });
    ro.observe(el);
    return () => ro.disconnect();
  }, []);

  // ----- Mutation helpers (with undo/redo bookkeeping) -----

  const mutateObjects = useCallback(
    (mutator: (objs: SicroObject[]) => SicroObject[]) => {
      setDoc((prev) => {
        if (!prev) return prev;
        editor.pushHistory(prev.objects);
        return { ...prev, objects: mutator(prev.objects) };
      });
    },
    [editor],
  );

  const addObject = (obj: SicroObject) => {
    mutateObjects((objs) => [...objs, obj]);
    editor.setSelectedId(obj.id);
  };

  const handleObjectChange = (id: string, patch: Partial<SicroObject>) => {
    mutateObjects((objs) =>
      objs.map((o) => (o.id === id ? ({ ...o, ...patch } as SicroObject) : o)),
    );
  };

  /**
   * Fase S — patch handler para objetos parity. Imutável e preserva a
   * discriminação `kind` do objeto. Parity objects coexistem com vehicles/
   * lines/markers/text/measurement no array `doc.objects`.
   */
  const handleParityObjectChange = useCallback(
    (
      id: string,
      patch: Partial<import("../engine/road-parity").SicroParityObject>,
    ) => {
      setDoc((prev) => {
        if (!prev) return prev;
        // Empurra snapshot no history ANTES de mutar — assim edits de
        // parity (largura de via, cor da marcação, raio de rotatória, etc.)
        // ficam undoable via Ctrl+Z, mesmo padrão de `mutateObjects`.
        editor.pushHistory(prev.objects);
        const next = prev.objects.map((o) => {
          if (o.id !== id) return o;
          if (o.kind === "road_parity") {
            return { ...o, ...patch, kind: "road_parity" as const };
          }
          if (o.kind === "roundabout_parity") {
            return { ...o, ...patch, kind: "roundabout_parity" as const };
          }
          return o;
        });
        return { ...prev, objects: next };
      });
    },
    [editor],
  );

  const handleDelete = useCallback(() => {
    const ids = editor.selectedIds;
    if (ids.length === 0) return;
    // MVP 9 Round 5 — Del with the background selected removes it.
    // Background sentinel sempre vem sozinho na seleção (não dá pra
    // marquee-selecionar background), então tratamento legado fica
    // intacto.
    if (ids.length === 1 && ids[0] === BACKGROUND_SELECTION_ID) {
      setDoc((prev) =>
        prev ? { ...prev, background_image: null } : prev,
      );
      editor.setSelectedId(null);
      return;
    }
    // Apaga TODOS os objetos cujos IDs estão na seleção. Mantém ordem
    // dos demais.
    const idSet = new Set(ids);
    mutateObjects((objs) => objs.filter((o) => !idSet.has(o.id)));
    editor.setSelectedId(null);
  }, [editor, mutateObjects]);

  const handleDuplicate = useCallback(() => {
    const id = editor.selectedId;
    if (!id || !doc) return;
    const src = doc.objects.find((o) => o.id === id);
    if (!src) return;
    const dup = cloneObject(src);
    mutateObjects((cur) => [...cur, dup]);
    editor.setSelectedId(dup.id);
  }, [doc, editor, mutateObjects]);

  const handleUndo = useCallback(() => {
    const prev = editor.popHistory();
    setDoc((d) => {
      if (!d) return d;
      // Push current onto redo before swapping in the undone snapshot.
      editor.pushRedo(d.objects);
      return prev ? { ...d, objects: prev } : d;
    });
  }, [editor]);

  const handleRedo = useCallback(() => {
    const next = editor.popRedo();
    setDoc((d) => {
      if (!d || !next) return d;
      editor.pushHistory(d.objects);
      return { ...d, objects: next };
    });
  }, [editor]);

  const handleMoveObject = (id: string, direction: "up" | "down") => {
    mutateObjects((objs) => {
      const idx = objs.findIndex((o) => o.id === id);
      if (idx < 0) return objs;
      const swapWith = direction === "up" ? idx + 1 : idx - 1;
      if (swapWith < 0 || swapWith >= objs.length) return objs;
      const next = [...objs];
      [next[idx], next[swapWith]] = [next[swapWith]!, next[idx]!];
      return next;
    });
  };

  const handleSelectTool = useCallback(
    (t: Tool) => {
      editor.setTool(t);
      editor.setPending(null);
    },
    [editor],
  );

  // ----- View / zoom helpers (keyboard-driven; wheel zoom lives in the
  // CanvasStage). Zoom in/out are anchored to the CENTRE of the visible
  // canvas so the framing stays stable. Fit computes the scale + offset
  // that centres the whole logical canvas in the available viewport. -----

  const zoomAroundCenter = useCallback(
    (factor: number) => {
      editor.setViewport((vp) => {
        const cx = canvasSize.width / 2;
        const cy = canvasSize.height / 2;
        const newScale = Math.max(
          CROQUI_ZOOM_MIN,
          Math.min(CROQUI_ZOOM_MAX, vp.scale * factor),
        );
        const worldX = (cx - vp.x) / vp.scale;
        const worldY = (cy - vp.y) / vp.scale;
        return {
          scale: newScale,
          x: cx - worldX * newScale,
          y: cy - worldY * newScale,
        };
      });
    },
    [editor, canvasSize.width, canvasSize.height],
  );

  const handleZoomIn = useCallback(() => zoomAroundCenter(1.2), [zoomAroundCenter]);
  const handleZoomOut = useCallback(() => zoomAroundCenter(1 / 1.2), [zoomAroundCenter]);
  const handleZoomReset = useCallback(() => {
    editor.setViewport(DEFAULT_VIEWPORT);
  }, [editor]);

  const handleFitView = useCallback(() => {
    if (!doc) return;
    const margin = 0.92; // deixa uma folga de ~8% nas bordas
    const sx = (canvasSize.width / doc.canvas.width_px) * margin;
    const sy = (canvasSize.height / doc.canvas.height_px) * margin;
    const scale = Math.max(
      CROQUI_ZOOM_MIN,
      Math.min(CROQUI_ZOOM_MAX, Math.min(sx, sy)),
    );
    editor.setViewport({
      scale,
      x: (canvasSize.width - doc.canvas.width_px * scale) / 2,
      y: (canvasSize.height - doc.canvas.height_px * scale) / 2,
    });
  }, [doc, editor, canvasSize.width, canvasSize.height]);

  const handleToggleGrid = useCallback(() => {
    setDoc((prev) => {
      if (!prev) return prev;
      const grid = prev.canvas.grid ?? { enabled: true, size_px: 50 };
      return {
        ...prev,
        canvas: { ...prev.canvas, grid: { ...grid, enabled: !grid.enabled } },
      };
    });
  }, []);

  // ----- Background image helpers -----

  /**
   * Resolve a workspace-relative-or-absolute background path into a URL
   * the browser can fetch. Mirrors what `CanvasStage.resolveAssetPath`
   * does — duplicated here to avoid leaking that helper into the editor.
   */
  const resolveBackgroundUrl = useCallback(
    (sourcePath: string): string => {
      const looksAbsolute =
        /^([a-zA-Z]:)?[\\/]/.test(sourcePath) ||
        sourcePath.startsWith("file://");
      if (!looksAbsolute && workspacePath) {
        const sep = workspacePath.includes("\\") ? "\\" : "/";
        const abs = `${workspacePath}${sep}${sourcePath.replace(/\//g, sep)}`;
        return convertFileSrc(abs.replace(/^file:\/\//, ""));
      }
      return convertFileSrc(sourcePath.replace(/^file:\/\//, ""));
    },
    [workspacePath],
  );

  /**
   * Set (or replace) the croqui background. MVP 9 Round 5:
   *   - measures the image in the browser so we know its natural size;
   *   - fits the image into the canvas useful area (10% margin);
   *   - centres it;
   *   - starts the background UNLOCKED so the user can immediately drag
   *     / resize it. The toolbar's lock toggle locks it whenever the
   *     perito is happy with the framing.
   *
   * When `extra.preMeasured` is passed (drone import path), skips the
   * browser-side measure and uses the dimensions Rust already produced.
   */
  const setBackgroundFromPath = useCallback(
    (
      sourcePath: string,
      extra?: {
        preMeasured?: { width: number; height: number };
        sidecar_path?: string;
        original_path?: string;
        opacity?: number;
      },
    ) => {
      if (!doc) return;
      const apply = (imgW: number, imgH: number) => {
        const rect = fitImageToCanvas(
          imgW,
          imgH,
          doc.canvas.width_px,
          doc.canvas.height_px,
          0.1,
        );
        setDoc((prev) =>
          prev
            ? {
                ...prev,
                background_image: {
                  source_path: sourcePath,
                  x: rect.x,
                  y: rect.y,
                  width: rect.width,
                  height: rect.height,
                  opacity: extra?.opacity ?? 0.6,
                  locked: false,
                  rotation: 0,
                  ...(extra?.sidecar_path
                    ? { sidecar_path: extra.sidecar_path }
                    : {}),
                  ...(extra?.original_path
                    ? { original_path: extra.original_path }
                    : {}),
                },
              }
            : prev,
        );
        // Auto-select the new background so the Transformer handles
        // appear straight away — the user can drag/resize without an
        // extra click.
        editor.setSelectedId(BACKGROUND_SELECTION_ID);
        setFeedback(
          `Fundo aplicado (${Math.round(rect.width)}×${Math.round(rect.height)}px), centralizado e desbloqueado para ajuste.`,
        );
      };
      if (extra?.preMeasured) {
        apply(extra.preMeasured.width, extra.preMeasured.height);
        return;
      }
      // Pre-measure via an off-screen Image so fit-to-canvas has real
      // numbers. Fall back to canvas dimensions if measurement fails.
      const url = resolveBackgroundUrl(sourcePath);
      const probe = new window.Image();
      probe.crossOrigin = "anonymous";
      probe.src = url;
      probe.onload = () => {
        const w = probe.naturalWidth || doc.canvas.width_px;
        const h = probe.naturalHeight || doc.canvas.height_px;
        apply(w, h);
      };
      probe.onerror = () => {
        // Could not pre-measure — apply with canvas-sized placeholder so
        // the image at least gets inserted and the user can re-frame.
        apply(doc.canvas.width_px, doc.canvas.height_px);
      };
    },
    [doc, resolveBackgroundUrl],
  );

  /**
   * Patch the current background — used by both the Konva drag/transform
   * pipeline and the toolbar action buttons (Center, Fit, Reset…).
   */
  const handleBackgroundChange = useCallback(
    (patch: Partial<SicroCroquiBackgroundImage>) => {
      setDoc((prev) =>
        prev && prev.background_image
          ? {
              ...prev,
              background_image: { ...prev.background_image, ...patch },
            }
          : prev,
      );
    },
    [],
  );

  /** Centre the background on the canvas, keeping its current size. */
  const handleCenterBackground = useCallback(() => {
    if (!doc?.background_image) return;
    const bg = doc.background_image;
    handleBackgroundChange({
      x: (doc.canvas.width_px - bg.width) / 2,
      y: (doc.canvas.height_px - bg.height) / 2,
    });
    setFeedback("Fundo centralizado.");
  }, [doc, handleBackgroundChange]);

  /** Re-fit the background into the canvas useful area (10% margin). */
  const handleFitBackground = useCallback(() => {
    if (!doc?.background_image) return;
    const bg = doc.background_image;
    // Use current aspect ratio (which already matches the image's), then
    // fit into the canvas with the standard margin.
    const rect = fitImageToCanvas(
      bg.width || 1,
      bg.height || 1,
      doc.canvas.width_px,
      doc.canvas.height_px,
      0.1,
    );
    handleBackgroundChange(rect);
    setFeedback("Fundo ajustado à área útil.");
  }, [doc, handleBackgroundChange]);

  /** Reset the background's rotation back to 0°. */
  const handleResetBackgroundRotation = useCallback(() => {
    handleBackgroundChange({ rotation: 0 });
    setFeedback("Rotação do fundo reiniciada.");
  }, [handleBackgroundChange]);

  /**
   * MVP 10 — OSM import done. Append the new RoadObjects, stamp the
   * session into `doc.osm_imports`, mark the doc dirty. Suggested
   * scale is recorded but NOT auto-applied: the perito has to confirm
   * via the existing "Definir escala" tool.
   */
  const handleOsmImportConfirm = useCallback(
    (result: OsmImportResult) => {
      if (!doc) return;

      // Fase S — único motor de importação OSM é o parity.
      // O modal devolve apenas `parity_roads` + `parity_roundabouts`.
      setDoc((prev) => {
        if (!prev) return prev;
        editor.pushHistory(prev.objects);
        const nextImports = [
          ...(prev.osm_imports ?? []),
          result.session,
        ];
        // Substituir objetos parity OSM antigos. Não toca em objetos
        // parity criados manualmente (sem source === "osm") nem em
        // outros kinds (vehicle/line/marker/text/measurement).
        const isOsmParity = (o: SicroObject): boolean => {
          if (o.kind !== "road_parity" && o.kind !== "roundabout_parity") {
            return false;
          }
          if (!o.metadata_json) return false;
          try {
            const m = JSON.parse(o.metadata_json) as { source?: unknown };
            return m.source === "osm";
          } catch {
            return false;
          }
        };
        const keptObjects = prev.objects.filter((o) => !isOsmParity(o));
        // O adapter parity calcula coords MUNDO assumindo
        // `pxPerM = suggested_px_per_m`. Se o doc.scale tiver outro
        // valor, o renderer projeta com offset errado e os objetos
        // saem do canvas. Por isso o import OSM SOBRESCREVE a escala
        // do doc (sai um aviso na barra de feedback).
        const importedScale: SicroCroquiDoc["scale"] = result.session
          .suggested_px_per_m
          ? {
              px_per_m: Math.max(result.session.suggested_px_per_m, 1),
            }
          : { px_per_m: 10 };
        return {
          ...prev,
          objects: [
            ...keptObjects,
            ...result.parity_roads,
            ...result.parity_roundabouts,
          ],
          osm_imports: nextImports,
          scale: importedScale,
        };
      });

      const firstParitySelectable =
        result.parity_roads[0] ?? result.parity_roundabouts[0];
      if (firstParitySelectable)
        editor.setSelectedId(firstParitySelectable.id);
      setShowOsmImport(false);
      const parityScaleMsg = result.session.suggested_px_per_m
        ? ` Escala sugerida: ${result.session.suggested_px_per_m.toFixed(2)} px/m.`
        : "";
      const parityRbMsg =
        result.parity_roundabouts.length > 0
          ? ` · ${result.parity_roundabouts.length} rotatória(s)`
          : "";
      const parityWarnMsg =
        result.warnings.length > 0
          ? ` · ${result.warnings.length} aviso(s) — veja o console`
          : "";
      if (result.warnings.length > 0) {
        for (const w of result.warnings) console.warn("[OSM-parity]", w);
      }
      setFeedback(
        `Importadas ${result.parity_roads.length} via(s)${parityRbMsg} do OSM (centro ${result.session.center_lat.toFixed(5)}, ${result.session.center_lon.toFixed(5)} · raio ${result.session.radius_m} m). Vias OSM anteriores foram substituídas.${parityScaleMsg}${parityWarnMsg}`,
      );
    },
    [doc, editor],
  );

  /** Remove the background entirely. */
  const handleRemoveBackground = useCallback(() => {
    setDoc((prev) =>
      prev ? { ...prev, background_image: null } : prev,
    );
    setFeedback("Fundo removido.");
  }, []);

  const handleImportBackground = async () => {
    try {
      const selected = await openFileDialog({
        multiple: false,
        title: "Imagem de fundo do croqui",
        filters: [
          { name: "Imagens", extensions: ["png", "jpg", "jpeg", "webp"] },
        ],
      });
      if (typeof selected !== "string") return;
      setBackgroundFromPath(selected);
    } catch (e) {
      setFeedback(`Falha ao importar imagem: ${toSicroError(e).message}`);
    }
  };

  const handleToggleBackgroundLock = () => {
    setDoc((prev) =>
      prev && prev.background_image
        ? {
            ...prev,
            background_image: {
              ...prev.background_image,
              locked: !prev.background_image.locked,
            },
          }
        : prev,
    );
  };
  const handleChangeBackgroundOpacity = (v: number) => {
    setDoc((prev) =>
      prev && prev.background_image
        ? {
            ...prev,
            background_image: { ...prev.background_image, opacity: v },
          }
        : prev,
    );
  };

  // ----- Canvas click dispatcher (after `doc` and helpers are defined) -----

  // Fase S — Road engine legacy v1/v2 removido. As ferramentas road_*
  // / roundabout deixaram de existir; a única forma de criar via
  // hoje é via OSM import ou o demo parity.
  const handleCanvasDblClick = useCallback(() => {
    // Reservado para futuras gestures (ex.: finalizar polilinha
    // parity). Hoje é no-op.
  }, []);

  const handleCanvasClick = (p: SicroPoint) => {
    if (!doc) return;
    const tool = editor.tool;

    // Helper: converte ponto canvas (px) → mundo (m). Coords parity
    // são em metros — o renderer multiplica por pxPerM ao desenhar.
    const pxToM = (pt: SicroPoint): SicroPoint => {
      const pxPerM = doc.scale?.px_per_m ?? 10;
      return { x: pt.x / pxPerM, y: pt.y / pxPerM };
    };

    // Rotatória parity — clique único insere centro com raio default.
    if (tool === "roundabout") {
      const pm = pxToM(p);
      const rb = makeParityRoundabout(pm.x, pm.y, 15, {
        largura_m: 7,
        label: nextRoundaboutLabel(doc.objects),
      });
      addObject(rb);
      editor.setTool("select");
      return;
    }

    // Vias parity — 2 cliques (p1 → p2). Cada tool tem preset de
    // largura/superfície/marcação. O perito ajusta no Inspector
    // depois.
    const roadPreset = roadToolToParityPreset(tool);
    if (roadPreset) {
      if (!editor.pending) {
        editor.setPending({ tool, first: p });
        return;
      }
      const p1 = pxToM(editor.pending.first);
      const p2 = pxToM(p);
      editor.setPending(null);
      const road = makeParityRoad(p1.x, p1.y, p2.x, p2.y, roadPreset);
      addObject(road);
      editor.setTool("select");
      return;
    }

    const vehicleType = toolToVehicleBody(tool);
    if (vehicleType) {
      const nextLabel = nextVehicleLabel(doc.objects);
      // Com escala definida, o veículo entra no TAMANHO REAL (arte do designer
      // foi desenhada em 1mm=1m); sem escala, caem os presets em px.
      addObject(
        makeVehicle(p, nextLabel, vehicleType, doc.scale?.px_per_m ?? null),
      );
      editor.setTool("select");
      return;
    }
    const markerSubtype = toolToMarkerSubtype(tool);
    if (markerSubtype) {
      // Pedestres com arte entram na altura humana real se houver escala.
      addObject(
        makeMarker(p, markerSubtype, undefined, doc.scale?.px_per_m ?? null),
      );
      editor.setTool("select");
      return;
    }
    if (tool === "text") {
      const text = window.prompt("Texto:", "Anotação");
      if (text == null || text.trim() === "") return;
      addObject(makeText(p, text.trim()));
      editor.setTool("select");
      return;
    }
    const lineSubtype = toolToLineSubtype(tool);
    if (lineSubtype) {
      if (!editor.pending) {
        editor.setPending({ tool, first: p });
        return;
      }
      const p1 = editor.pending.first;
      const p2 = p;
      editor.setPending(null);
      addObject(makeLine(p1, p2, lineSubtype));
      editor.setTool("select");
      return;
    }
    if (tool === "measurement") {
      if (!editor.pending) {
        editor.setPending({ tool, first: p });
        return;
      }
      const p1 = editor.pending.first;
      const p2 = p;
      editor.setPending(null);
      addObject(makeMeasurement(p1, p2));
      editor.setTool("select");
      const label = formatMeasurement(distancePx(p1, p2), doc.scale?.px_per_m);
      setFeedback(
        doc.scale
          ? `Medida: ${label}`
          : `Medida: ${label} · aviso: escala ainda não foi definida.`,
      );
      return;
    }
    if (tool === "set_scale") {
      if (!editor.pending) {
        editor.setPending({ tool, first: p });
        return;
      }
      const p1 = editor.pending.first;
      const p2 = p;
      editor.setPending(null);
      const px = distancePx(p1, p2);
      const declared = window.prompt(
        `Distância real entre os dois pontos (em metros)?\n\nPixels medidos: ${px.toFixed(1)}`,
        "10",
      );
      if (!declared) return;
      const real = Number(declared.replace(",", "."));
      if (!Number.isFinite(real) || real <= 0) {
        setFeedback("Valor inválido — escala não atualizada.");
        return;
      }
      try {
        const pxPerM = computePxPerMeter(p1, p2, real);
        setDoc((prev) =>
          prev
            ? {
                ...prev,
                scale: {
                  px_per_m: pxPerM,
                  definition: { p1, p2, real_distance_m: real },
                },
              }
            : prev,
        );
        setFeedback(`Escala definida: ${pxPerM.toFixed(2)} px/m (1 m = ${(pxPerM).toFixed(2)} px).`);
        editor.setTool("select");
      } catch (e) {
        setFeedback(`Falha ao calibrar: ${(e as Error).message}`);
      }
    }
  };

  // ----- Keyboard shortcuts (customizáveis, escopo `croqui`) -----
  //
  // `useShortcuts` já ignora foco em INPUT/TEXTAREA/SELECT (mesmo guard do
  // antigo listener manual) e só dispara as ações cujo handler é fornecido.
  // Cada combinação é resolvida do override do usuário ?? padrão do catálogo.
  //
  // Os handlers referenciados que são definidos mais abaixo no corpo do
  // componente (handleSave, handleExportPng, handleInsertInLaudo) só são
  // LIDOS quando uma tecla dispara — nunca durante o render — então a ordem
  // de declaração não importa (mesmo padrão do listener anterior).
  useShortcuts({
    // Esc — limpa pending/rascunho, deseleciona e volta pra ferramenta de
    // seleção (comportamento idêntico ao listener anterior).
    "croqui.cancel": () => {
      editor.setPending(null);
      editor.setRoadDraft(null);
      editor.setSelectedId(null);
      editor.setTool("select");
    },
    // Ferramentas.
    "croqui.tool.select": () => handleSelectTool("select"),
    "croqui.tool.pan": () => handleSelectTool("pan"),
    "croqui.tool.measure": () => handleSelectTool("measurement"),
    "croqui.tool.scale": () => handleSelectTool("set_scale"),
    "croqui.tool.text": () => handleSelectTool("text"),
    "croqui.tool.r1": () => handleSelectTool("line_r1"),
    "croqui.tool.r2": () => handleSelectTool("line_r2"),
    "croqui.tool.roadUrban": () => handleSelectTool("road_urban"),
    "croqui.tool.roadAvenue": () => handleSelectTool("road_avenue"),
    "croqui.tool.roadHighway": () => handleSelectTool("road_highway"),
    "croqui.tool.roadDirt": () => handleSelectTool("road_dirt"),
    "croqui.tool.roadParking": () => handleSelectTool("road_parking"),
    "croqui.tool.roundabout": () => handleSelectTool("roundabout"),
    "croqui.tool.vehicle": () => handleSelectTool("vehicle_sedan"),
    "croqui.tool.vestigio": () => handleSelectTool("marker_x"),
    "croqui.tool.mobiliario": () => handleSelectTool("marker_semaforo"),
    "croqui.tool.pessoa": () => handleSelectTool("marker_pedestre_m_dorsal"),
    "croqui.tool.arrow": () => handleSelectTool("line_arrow"),
    "croqui.tool.callout": () => handleSelectTool("line_callout"),
    "croqui.tool.trajectory": () => handleSelectTool("line_trajetoria"),
    // Edição.
    "croqui.delete": () => {
      if (editor.selectedIds.length > 0) handleDelete();
    },
    "croqui.duplicate": handleDuplicate,
    "croqui.undo": handleUndo,
    "croqui.redo": handleRedo,
    "croqui.save": () => void handleSave(),
    // Vista.
    "croqui.zoomIn": handleZoomIn,
    "croqui.zoomOut": handleZoomOut,
    "croqui.zoomReset": handleZoomReset,
    "croqui.fit": handleFitView,
    "croqui.toggleGrid": handleToggleGrid,
    // Imagem de fundo.
    "croqui.bg.import": () => void handleImportBackground(),
    "croqui.bg.toggleLock": () => {
      if (doc?.background_image) handleToggleBackgroundLock();
    },
    "croqui.bg.fit": () => {
      if (doc?.background_image) handleFitBackground();
    },
    "croqui.importDrone": () => setShowDroneImport(true),
    "croqui.importOsm": () => setShowOsmImport(true),
    // Exportação.
    "croqui.exportPng": () => void handleExportPng("tecnico"),
    "croqui.exportPngClean": () => void handleExportPng("limpo"),
    "croqui.openLaudo": () => handleInsertInLaudo(),
  });

  if (!workspacePath || !activeCroqui || !doc) {
    return <div className={styles.empty}>Sem croqui aberto.</div>;
  }

  const handleSave = async (): Promise<boolean> => {
    if (!workspacePath || !doc) return false;
    setSaving(true);
    setFeedback(null);
    try {
      await saveCurrent(workspacePath, doc);
      // The store stamps the doc with a new updated_at — capture the
      // saved-snapshot JSON so dirty derivation starts fresh.
      setLastSavedJson(JSON.stringify(doc));
      setFeedback("Croqui salvo.");
      setTimeout(() => setFeedback(null), 2500);
      return true;
    } catch (err) {
      setFeedback(`Falha ao salvar: ${toSicroError(err).message}`);
      return false;
    } finally {
      setSaving(false);
    }
  };

  // MVP 9 Round 3 — unsaved-changes guard glue.
  //
  // `tryNavigateAway` is the single entry point used by every UI affordance
  // that takes the user out of the editor (Voltar button, Abrir Laudo,
  // ActivityRail link). When the doc isn't dirty we just run the target
  // immediately; when it is, we stash the target and show the modal so the
  // user can decide.
  const tryNavigateAway = useCallback(
    (target: () => void, label?: string) => {
      if (!dirty) {
        target();
        return;
      }
      setPendingNav({ proceed: target, label });
    },
    [dirty],
  );

  // Register a guard with the global nav-guard store so the ActivityRail
  // can ask permission before taking the user to another module. The
  // guard returns a Promise that resolves true (proceed) or false (stay)
  // depending on what the user picks in the modal.
  useEffect(() => {
    if (!dirty) {
      unregisterNavGuard();
      return;
    }
    registerNavGuard(
      () =>
        new Promise<boolean>((resolve) => {
          setPendingNav({
            proceed: () => resolve(true),
            label: "outro módulo",
            resolve,
          });
        }),
    );
    return () => unregisterNavGuard();
  }, [dirty, registerNavGuard, unregisterNavGuard]);

  const handleExportPng = async (variant: "tecnico" | "limpo" = "tecnico") => {
    if (!workspacePath || !doc || !stageRef.current) return;
    setExporting(true);
    setFeedback(null);
    try {
      await saveCurrent(workspacePath, doc);
      const rawDataUrl = stageRef.current.toPng(2);
      if (!rawDataUrl) throw new Error("toDataURL retornou null");
      // MVP 6 + MVP 9: dois modos de PNG.
      //   - "tecnico" → carimbo institucional (BO, escala, timestamp);
      //   - "limpo"   → PNG cru sem carimbo, ideal para inserir no
      //                 corpo de um laudo onde o cabeçalho já existe.
      const final =
        variant === "limpo"
          ? rawDataUrl
          : await stampPng(rawDataUrl, {
              title: activeCroqui.title,
              occurrence,
              scaleLabel: doc.scale
                ? `Escala 1 m = ${doc.scale.px_per_m.toFixed(2)} px`
                : "Escala não definida",
              timestamp: new Date(),
            });
      const path = await exportPng(workspacePath, final);
      setFeedback(
        variant === "limpo"
          ? `PNG limpo salvo em ${path}`
          : `PNG técnico salvo em ${path}`,
      );
    } catch (err) {
      setFeedback(`Falha ao exportar: ${toSicroError(err).message}`);
    } finally {
      setExporting(false);
    }
  };

  const handleToggleLayerVisibility = (layerId: string) => {
    setDoc((prev) =>
      prev
        ? {
            ...prev,
            layers: prev.layers.map((l) =>
              l.id === layerId ? { ...l, visible: !l.visible } : l,
            ),
          }
        : prev,
    );
  };

  const visibleObjects = useMemo(() => {
    const hiddenLayers = new Set(
      doc.layers.filter((l) => !l.visible).map((l) => l.id),
    );
    return doc.objects.filter(
      (o) => o.visible !== false && !hiddenLayers.has(o.layer_id),
    );
  }, [doc.layers, doc.objects]);

  const docForStage: SicroCroquiDoc = useMemo(
    () => ({ ...doc, objects: visibleObjects }),
    [doc, visibleObjects],
  );

  const totalsByCategory = useMemo(() => {
    const counts: Record<string, number> = {};
    for (const o of doc.objects) {
      const cat = o.category ?? inferCategory(o);
      counts[cat] = (counts[cat] ?? 0) + 1;
    }
    return counts;
  }, [doc.objects]);

  const handleBackToList = () =>
    tryNavigateAway(() => clearCurrent(), "a lista de croquis");

  // MVP 9 Round 3 — `ensureCroquiExportFresh`:
  //
  //   - if the croqui is dirty → save it first;
  //   - if there's no recorded export or the export is older than the
  //     doc (`isExportStale`), generate a fresh PNG (technical variant);
  //   - return true on success, false on failure.
  //
  // The Laudo flow ("Abrir Laudo") calls this before navigating so the
  // panel on the other side always sees the most recent PNG. Avoids the
  // "salvei o croqui mas o laudo ainda mostra o PNG antigo" bug the
  // user reported.
  const ensureExportFresh = useCallback(async (): Promise<boolean> => {
    if (!workspacePath || !doc || !activeCroqui || !stageRef.current) {
      return false;
    }
    // Step 1 — flush dirty state.
    if (dirty) {
      const ok = await handleSave();
      if (!ok) return false;
    }
    // Step 2 — re-export only when the recorded PNG is older than the
    // last save (or doesn't exist at all).
    if (!isExportStale(activeCroqui.id) && activeCroqui.last_export_relative_path) {
      return true;
    }
    setExporting(true);
    try {
      const rawDataUrl = stageRef.current.toPng(2);
      if (!rawDataUrl) throw new Error("toDataURL retornou null");
      const final = await stampPng(rawDataUrl, {
        title: activeCroqui.title,
        occurrence,
        scaleLabel: doc.scale
          ? `Escala 1 m = ${doc.scale.px_per_m.toFixed(2)} px`
          : "Escala não definida",
        timestamp: new Date(),
      });
      await exportPng(workspacePath, final);
      return true;
    } catch (err) {
      setFeedback(
        `Falha ao gerar PNG atualizado: ${toSicroError(err).message}`,
      );
      return false;
    } finally {
      setExporting(false);
    }
  }, [
    workspacePath,
    doc,
    activeCroqui,
    occurrence,
    dirty,
    isExportStale,
    handleSave,
    exportPng,
  ]);

  const handleInsertInLaudo = () => {
    // Run the freshness pipeline first; only navigate if it succeeded
    // so the Laudo always sees a PNG matching the current .sicrocroqui.
    void (async () => {
      const ok = await ensureExportFresh();
      if (!ok) return;
      navigate("/laudo");
    })();
  };

  // Modal handlers ---------------------------------------------------------
  const handleModalSaveAndLeave = async () => {
    if (!pendingNav) return;
    const ok = await handleSave();
    if (!ok) return; // stay on the editor so the user can retry
    const { proceed, resolve } = pendingNav;
    setPendingNav(null);
    proceed();
    resolve?.(true);
  };

  const handleModalDiscardAndLeave = () => {
    if (!pendingNav) return;
    const { proceed, resolve } = pendingNav;
    setPendingNav(null);
    proceed();
    resolve?.(true);
  };

  const handleModalCancel = () => {
    if (!pendingNav) return;
    const { resolve } = pendingNav;
    setPendingNav(null);
    resolve?.(false);
  };

  return (
    <div className={styles.wrap}>
      <Toolbar
        activeTool={editor.tool}
        onSelectTool={handleSelectTool}
        canDelete={editor.selectedIds.length > 0}
        onDelete={handleDelete}
        canUndo={editor.history.length > 0}
        onUndo={handleUndo}
        canRedo={editor.redoStack.length > 0}
        onRedo={handleRedo}
        canDuplicate={!!editor.selectedId}
        onDuplicate={handleDuplicate}
        onImportBackground={() => void handleImportBackground()}
        onPickFromDossie={() => setShowPhotoPicker(true)}
        onImportDrone={() => setShowDroneImport(true)}
        onImportOsm={() => setShowOsmImport(true)}
        onCenterBackground={handleCenterBackground}
        onFitBackground={handleFitBackground}
        onResetBackgroundRotation={handleResetBackgroundRotation}
        onRemoveBackground={handleRemoveBackground}
        hasBackground={!!doc.background_image}
        bgLocked={doc.background_image?.locked ?? true}
        onToggleBackgroundLock={handleToggleBackgroundLock}
        bgOpacity={doc.background_image?.opacity ?? 0.6}
        onChangeBackgroundOpacity={handleChangeBackgroundOpacity}
        onInsertInLaudo={handleInsertInLaudo}
        onSave={() => void handleSave()}
        onExportPng={() => void handleExportPng("tecnico")}
        onExportPngClean={() => void handleExportPng("limpo")}
        onBackToList={handleBackToList}
        saving={saving}
        exporting={exporting}
      />
      <div className={styles.canvasArea} ref={canvasRef}>
        <CanvasStage
          ref={stageRef}
          doc={docForStage}
          editor={editor}
          containerWidth={canvasSize.width}
          containerHeight={canvasSize.height}
          onCanvasClick={handleCanvasClick}
          onCanvasDblClick={handleCanvasDblClick}
          onObjectChange={handleObjectChange}
          onParityObjectChange={handleParityObjectChange}
          onBackgroundChange={handleBackgroundChange}
          onSelect={(id) => editor.setSelectedId(id)}
          workspacePath={workspacePath}
        />
        <StatusBar
          tool={editor.tool}
          pointer={editor.pointerWorld}
          viewport={editor.viewport}
          scale={doc.scale}
          objectCount={doc.objects.length}
          totalsByCategory={totalsByCategory}
          saving={saving}
          exporting={exporting}
          dirty={dirty}
          exportStale={
            activeCroqui ? isExportStale(activeCroqui.id) : true
          }
          hasAnyExport={
            (activeCroqui?.last_export_relative_path != null) ||
            (activeCroqui ? Boolean(lastExportedAt[activeCroqui.id]) : false)
          }
          feedback={feedback}
          croquiTitle={activeCroqui.title}
        />
      </div>
      <InspectorPanel
        layers={doc.layers}
        objects={doc.objects}
        selectedId={editor.selectedId}
        scale={doc.scale}
        onSelectObject={(id) => editor.setSelectedId(id)}
        onToggleLayerVisibility={handleToggleLayerVisibility}
        onUpdateObject={handleObjectChange}
        onDeleteObject={(id) => {
          editor.setSelectedId(id);
          handleDelete();
        }}
        onMoveObject={handleMoveObject}
      />

      {showPhotoPicker && (
        <DossiePhotoPicker
          workspacePath={workspacePath}
          onPick={(rel) => {
            setBackgroundFromPath(rel);
            setShowPhotoPicker(false);
          }}
          onClose={() => setShowPhotoPicker(false)}
        />
      )}

      {showDroneImport && (
        <DroneImportModal
          workspacePath={workspacePath}
          croquiId={activeCroqui.id}
          occurrenceId={activeCroqui.occurrence_id}
          onConfirm={(result) => {
            // The corrected + cropped PNG becomes the croqui background.
            // We pass the Rust-side dimensions as `preMeasured` so
            // `setBackgroundFromPath` skips the off-screen probe and
            // fits the image into the canvas useful area immediately.
            // The sidecar path is carried into `background_image` so
            // the chain of custody (lens correction + crop parameters)
            // stays attached to the doc.
            setBackgroundFromPath(result.output_relative_path, {
              preMeasured: {
                width: result.output_width,
                height: result.output_height,
              },
              sidecar_path: result.sidecar_relative_path,
            });
            setShowDroneImport(false);
            setFeedback(
              `Drone: imagem corrigida (${result.output_width}×${result.output_height}px) ` +
                `centralizada e dimensionada para a área útil. Ajuste a posição/tamanho ` +
                `arrastando a imagem — depois bloqueie e defina a escala.`,
            );
          }}
          onCancel={() => setShowDroneImport(false)}
        />
      )}

      {showOsmImport && (
        <OsmImportModal
          canvasWidth={doc.canvas.width_px}
          canvasHeight={doc.canvas.height_px}
          dossieCoords={
            occurrence?.latitude != null && occurrence?.longitude != null
              ? { lat: occurrence.latitude, lon: occurrence.longitude }
              : null
          }
          // Fase S — `engine` prop removida. O modal agora sempre usa o
          // Python Parity Engine; Road v2 OSM adapter foi descontinuado.
          onConfirm={handleOsmImportConfirm}
          onCancel={() => setShowOsmImport(false)}
        />
      )}

      {pendingNav && (
        <UnsavedChangesModal
          saving={saving}
          exporting={exporting}
          destinationLabel={pendingNav.label}
          onSaveAndLeave={() => void handleModalSaveAndLeave()}
          onDiscardAndLeave={handleModalDiscardAndLeave}
          onCancel={handleModalCancel}
        />
      )}
    </div>
  );
}

// ---------------------------------------------------------------------------
// Tool dispatching helpers

function toolToVehicleBody(tool: Tool): VehicleBodyType | null {
  switch (tool) {
    case "vehicle":
    case "vehicle_car":
      return "car";
    case "vehicle_sedan":
      return "sedan";
    case "vehicle_suv":
      return "suv";
    case "vehicle_hatch":
      return "hatch";
    case "vehicle_truck":
      return "truck";
    case "vehicle_moto":
      return "moto";
    case "vehicle_bike":
      return "bike";
    // MVP 9
    case "vehicle_pickup":
      return "pickup";
    case "vehicle_van":
      return "van";
    case "vehicle_onibus":
      return "onibus";
    case "vehicle_moto_esportiva":
      return "moto_esportiva";
    case "vehicle_moto_carga":
      return "moto_carga";
    case "vehicle_caminhao_pesado":
      return "caminhao_pesado";
    case "vehicle_carreta":
      return "carreta";
    // Frota SVG do designer — civis
    case "vehicle_van_furgao":
      return "van_furgao";
    case "vehicle_micro_onibus":
      return "micro_onibus";
    case "vehicle_onibus_leito":
      return "onibus_leito";
    case "vehicle_reboque_guincho":
      return "reboque_guincho";
    case "vehicle_trator":
      return "trator";
    case "vehicle_bike_estrada":
      return "bike_estrada";
    case "vehicle_bike_cargueira":
      return "bike_cargueira";
    // Frota SVG do designer — viaturas/especiais
    case "vehicle_ambulancia":
      return "ambulancia";
    case "vehicle_taxi":
      return "taxi";
    case "vehicle_vtr_pm":
      return "vtr_pm";
    case "vehicle_vtr_pc":
      return "vtr_pc";
    case "vehicle_vtr_pci":
      return "vtr_pci";
    case "vehicle_vtr_bm":
      return "vtr_bm";
    case "vehicle_vtr_pp":
      return "vtr_pp";
    default:
      return null;
  }
}

function toolToMarkerSubtype(tool: Tool): MarkerSubtype | null {
  switch (tool) {
    case "marker_x":
      return "collision_x";
    case "marker_brake":
      return "brake_mark";
    case "marker_drag":
      return "drag_mark";
    case "marker_fluid":
      return "fluid";
    case "marker_blood":
      return "blood";
    case "marker_debris":
      return "debris";
    case "marker_pedestrian":
      return "pedestrian";
    case "marker_body":
      return "body";
    // Pedestres em decúbito (frota SVG do designer)
    case "marker_pedestre_m_dorsal":
      return "pedestre_m_dorsal";
    case "marker_pedestre_m_lateral":
      return "pedestre_m_lateral";
    case "marker_pedestre_m_ventral":
      return "pedestre_m_ventral";
    case "marker_pedestre_f_dorsal":
      return "pedestre_f_dorsal";
    case "marker_pedestre_f_lateral":
      return "pedestre_f_lateral";
    case "marker_pedestre_f_ventral":
      return "pedestre_f_ventral";
    // MVP 9 — vestígios extras
    case "marker_skid_curve":
      return "skid_curve";
    case "marker_sulcagem":
      return "sulcagem";
    case "marker_ranhura":
      return "ranhura";
    case "marker_impact_area":
      return "impact_area";
    case "marker_rest_position":
      return "rest_position";
    // MVP 9 — mobiliário urbano
    case "marker_semaforo":
      return "semaforo";
    case "marker_placa_pare":
      return "placa_pare";
    case "marker_placa_preferencia":
      return "placa_preferencia";
    case "marker_poste":
      return "poste";
    case "marker_arvore":
      return "arvore";
    case "marker_guia":
      return "guia";
    case "marker_faixa_pedestre":
      return "faixa_pedestre";
    default:
      return null;
  }
}

function toolToLineSubtype(tool: Tool): LineSubtype | null {
  switch (tool) {
    case "line_road":
      return "road";
    case "line_r1":
      return "r1";
    case "line_r2":
      return "r2";
    case "line_lane":
      return "lane";
    case "line_lane_separator":
      return "lane_separator";
    case "line_sidewalk":
      return "sidewalk";
    case "line_arrow":
      return "arrow";
    // MVP 9
    case "line_canteiro":
      return "canteiro";
    case "line_acostamento":
      return "acostamento";
    case "line_trajetoria":
      return "trajetoria";
    case "line_callout":
      return "callout";
    default:
      return null;
  }
}

/** Find the next "V1", "V2", … label that isn't taken yet. */
function nextVehicleLabel(objs: SicroObject[]): string {
  const taken = new Set<number>();
  for (const o of objs) {
    if (o.kind !== "vehicle" || !o.label) continue;
    const m = /^V(\d+)$/.exec(o.label);
    if (m) taken.add(Number(m[1]));
  }
  for (let i = 1; i < 100; i++) {
    if (!taken.has(i)) return `V${i}`;
  }
  return "V";
}

function nextRoundaboutLabel(objs: SicroObject[]): string {
  const taken = new Set<number>();
  for (const o of objs) {
    if (o.kind !== "roundabout_parity" || !o.label) continue;
    const m = /^Rotatória (\d+)$/.exec(o.label);
    if (m) taken.add(Number(m[1]));
  }
  for (let i = 1; i < 100; i++) {
    if (!taken.has(i)) return `Rotatória ${i}`;
  }
  return "Rotatória";
}

/**
 * Fase S — mapeia tool `road_*` da Toolbar para um preset de via parity
 * (largura, superfície, mão dupla, marcação). O perito refina depois
 * pelo Inspector.
 */
function roadToolToParityPreset(
  tool: Tool,
):
  | {
      largura_m: number;
      superficie: ParitySuperficie;
      mao_dupla: boolean;
      marcacao: ParityMarcacao;
    }
  | null {
  switch (tool) {
    case "road_urban":
      // Via urbana — 7m, mão dupla, eixo amarelo (default arterial).
      return {
        largura_m: 7,
        superficie: "asfalto",
        mao_dupla: true,
        marcacao: "amarela",
      };
    case "road_avenue":
      // Avenida — 14m (duas pistas largas), mão dupla, eixo amarelo.
      return {
        largura_m: 14,
        superficie: "asfalto",
        mao_dupla: true,
        marcacao: "amarela",
      };
    case "road_highway":
      // Rodovia — 12m, mão dupla, eixo amarelo.
      return {
        largura_m: 12,
        superficie: "asfalto",
        mao_dupla: true,
        marcacao: "amarela",
      };
    case "road_dirt":
      // Estrada de terra — 5m, sem marcação central.
      return {
        largura_m: 5,
        superficie: "terra",
        mao_dupla: false,
        marcacao: "nenhuma",
      };
    case "road_parking":
      // Acesso de estacionamento — 6m, sem marcação central.
      return {
        largura_m: 6,
        superficie: "asfalto",
        mao_dupla: false,
        marcacao: "nenhuma",
      };
    default:
      return null;
  }
}

// ---------------------------------------------------------------------------
// Status bar (bottom)

function StatusBar({
  tool,
  pointer,
  viewport,
  scale,
  objectCount,
  totalsByCategory,
  saving,
  exporting,
  dirty,
  exportStale,
  hasAnyExport,
  feedback,
  croquiTitle,
}: {
  tool: Tool;
  pointer: SicroPoint;
  viewport: { scale: number; x: number; y: number };
  scale: SicroCroquiDoc["scale"];
  objectCount: number;
  totalsByCategory: Record<string, number>;
  saving: boolean;
  exporting: boolean;
  dirty: boolean;
  exportStale: boolean;
  hasAnyExport: boolean;
  feedback: string | null;
  croquiTitle: string;
}) {
  // MVP 9 Round 3 — show two new chips so the perito knows at a glance
  // whether their work is safe.
  const saveLabel = saving
    ? "salvando…"
    : dirty
      ? "alterações não salvas"
      : "salvo";
  const saveColor = saving
    ? "#f59e0b"
    : dirty
      ? "#dc2626"
      : "#16a34a";

  const exportLabel = exporting
    ? "exportando…"
    : !hasAnyExport
      ? "sem exportação"
      : exportStale
        ? "exportação desatualizada"
        : "exportação atualizada";
  const exportColor = exporting
    ? "#f59e0b"
    : exportStale || !hasAnyExport
      ? "#dc2626"
      : "#16a34a";

  return (
    <div className={styles.statusBar}>
      <span className={styles.statusTitle}>{croquiTitle}</span>
      <span>
        Ferramenta: <code>{tool}</code>
      </span>
      <span>
        x={pointer.x.toFixed(0)} · y={pointer.y.toFixed(0)}
      </span>
      <span>zoom {(viewport.scale * 100).toFixed(0)}%</span>
      <span>
        {scale ? `escala ${scale.px_per_m.toFixed(2)} px/m` : "escala indefinida"}
      </span>
      <span>
        {objectCount} obj
        {Object.entries(totalsByCategory).length > 0 && (
          <span className={styles.statusCounts}>
            {" "}
            ({Object.entries(totalsByCategory)
              .map(([cat, n]) => `${cat}:${n}`)
              .join(" · ")})
          </span>
        )}
      </span>
      <span style={{ color: saveColor, fontWeight: 600 }}>
        ● {saveLabel}
      </span>
      <span style={{ color: exportColor, fontWeight: 600 }}>
        ● {exportLabel}
      </span>
      {/* Fase S — indicador estático "Road Parity". Antes era um toggle
          que ciclava v1 → v2 → parity → v1; v1 e v2 foram removidos (viraram
          stubs no-op) e o único motor real agora é o Road Engine.
          Mantemos o pill apenas para o perito ver de relance qual motor
          está ativo. Não é clicável. */}
      <span
        title="Road Engine — motor de via compatível com o estilo visual do SICRO 1.0"
        style={{
          marginLeft: "auto",
          padding: "2px 8px",
          fontSize: "11px",
          border: "1px solid #7c3aed",
          background: "#ede9fe",
          color: "#6d28d9",
          borderRadius: "4px",
          fontWeight: 600,
        }}
      >
        Road Parity
      </span>
      <span className={styles.statusFeedback}>{feedback}</span>
    </div>
  );
}

// ---------------------------------------------------------------------------
// Picker para usar foto do Dossiê como fundo

function DossiePhotoPicker({
  workspacePath,
  onPick,
  onClose,
}: {
  workspacePath: string;
  onPick: (relativePath: string) => void;
  onClose: () => void;
}) {
  const [photos, setPhotos] = useState<MediaAsset[] | null>(null);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    let cancelled = false;
    commands
      .listDossiePhotos(workspacePath)
      .then((data) => {
        if (!cancelled) setPhotos(data);
      })
      .catch((err) => {
        if (!cancelled) setError(toSicroError(err).message);
      });
    return () => {
      cancelled = true;
    };
  }, [workspacePath]);

  return (
    <div
      className={styles.overlay}
      role="dialog"
      aria-modal="true"
      aria-label="Selecionar foto do Dossiê como fundo"
      onClick={(e) => {
        if (e.target === e.currentTarget) onClose();
      }}
    >
      <div className={styles.dialog}>
        <header className={styles.dialogHeader}>
          <strong>Foto do Dossiê como fundo</strong>
          <button type="button" onClick={onClose} className={styles.dialogClose}>
            Fechar
          </button>
        </header>
        {photos === null && !error && (
          <p className={styles.dim}>Carregando fotos…</p>
        )}
        {error && <p className={styles.danger}>{error}</p>}
        {photos && photos.length === 0 && (
          <p className={styles.dim}>Nenhuma foto importada no Dossiê.</p>
        )}
        {photos && photos.length > 0 && (
          <ul className={styles.dialogList}>
            {photos.map((p) => (
              <li key={p.id}>
                <button
                  type="button"
                  className={styles.dialogItem}
                  onClick={() => onPick(p.relative_path)}
                >
                  <span>{p.original_id ?? p.id.slice(0, 8)}</span>
                  <code>{p.relative_path}</code>
                </button>
              </li>
            ))}
          </ul>
        )}
      </div>
    </div>
  );
}

// ---------------------------------------------------------------------------
// PNG stamping (MVP 6 — exportação técnica)

/**
 * Take the raw PNG data URL produced by Konva and prepend a thin
 * technical header (title · escala · timestamp · BO opcional). Runs
 * entirely off-screen via a 2D canvas; no Konva involved here. Returns
 * a new data URL.
 */
async function stampPng(
  rawDataUrl: string,
  meta: {
    title: string;
    occurrence:
      | { numero_bo?: string | null; tipo_pericia?: string | null; municipio?: string | null }
      | null;
    scaleLabel: string;
    timestamp: Date;
  },
): Promise<string> {
  const img = await loadImage(rawDataUrl);
  const headerH = 64;
  const footerH = 28;
  const canvas = document.createElement("canvas");
  canvas.width = img.width;
  canvas.height = img.height + headerH + footerH;
  const ctx = canvas.getContext("2d");
  if (!ctx) return rawDataUrl;

  // White background
  ctx.fillStyle = "#ffffff";
  ctx.fillRect(0, 0, canvas.width, canvas.height);

  // Header band
  ctx.fillStyle = "#0f172a";
  ctx.fillRect(0, 0, canvas.width, headerH);
  ctx.fillStyle = "#f8fafc";
  ctx.textBaseline = "middle";
  ctx.font = "bold 18px Inter, system-ui, sans-serif";
  ctx.fillText(meta.title, 18, headerH / 2 - 8);
  ctx.font = "12px Inter, system-ui, sans-serif";
  const subtitleParts: string[] = [];
  const occ = meta.occurrence;
  if (occ?.numero_bo) subtitleParts.push(`BO ${occ.numero_bo}`);
  if (occ?.tipo_pericia) subtitleParts.push(occ.tipo_pericia);
  if (occ?.municipio) subtitleParts.push(occ.municipio);
  if (subtitleParts.length > 0) {
    ctx.fillText(subtitleParts.join(" · "), 18, headerH / 2 + 12);
  }
  ctx.textAlign = "right";
  ctx.fillText(meta.scaleLabel, canvas.width - 18, headerH / 2 - 8);
  ctx.fillText(
    `Exportado em ${formatStampDate(meta.timestamp)}`,
    canvas.width - 18,
    headerH / 2 + 12,
  );
  ctx.textAlign = "left";

  // Sketch body
  ctx.drawImage(img, 0, headerH);

  // Footer band
  ctx.fillStyle = "#1f2937";
  ctx.fillRect(0, headerH + img.height, canvas.width, footerH);
  ctx.fillStyle = "#94a3b8";
  ctx.font = "11px Inter, system-ui, sans-serif";
  ctx.textBaseline = "middle";
  ctx.fillText(
    "SICRO Desktop — Croqui Pericial · documento técnico, sujeito a revisão pelo perito.",
    18,
    headerH + img.height + footerH / 2,
  );

  return canvas.toDataURL("image/png");
}

function loadImage(src: string): Promise<HTMLImageElement> {
  return new Promise((resolve, reject) => {
    const img = new Image();
    img.onload = () => resolve(img);
    img.onerror = (e) => reject(e);
    img.src = src;
  });
}

function formatStampDate(d: Date): string {
  const pad = (n: number) => String(n).padStart(2, "0");
  return `${pad(d.getDate())}/${pad(d.getMonth() + 1)}/${d.getFullYear()} ${pad(
    d.getHours(),
  )}:${pad(d.getMinutes())}`;
}
