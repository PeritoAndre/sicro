/**
 * Editor do croqui viário: orquestra Toolbar + CanvasStage + InspectorPanel
 * + StatusBar. O `.sicrocroqui` é a fonte da verdade; o PNG é só exportação.
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

  // Dirty = JSON do doc difere do snapshot salvo; evita instrumentar
  // cada caminho de mutação.
  const [lastSavedJson, setLastSavedJson] = useState<string | null>(null);
  const dirty = useMemo(() => {
    if (!doc || !lastSavedJson) return false;
    return JSON.stringify(doc) !== lastSavedJson;
  }, [doc, lastSavedJson]);

  // Navegação pendente enquanto o modal "salvar antes de sair?" está aberto.
  const [pendingNav, setPendingNav] = useState<null | {
    proceed: () => void;
    /** Destino, para o texto do modal. */
    label?: string;
    /** Presente quando veio do nav guard global (ActivityRail). */
    resolve?: (proceed: boolean) => void;
  }>(null);

  useEffect(() => {
    if (activeDoc) {
      setDoc(activeDoc);
      setLastSavedJson(JSON.stringify(activeDoc));
    }
  }, [activeDoc]);

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

  // ----- Mutações (com undo/redo) -----

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

  /** Patch de objeto parity preservando o `kind` discriminante. */
  const handleParityObjectChange = useCallback(
    (
      id: string,
      patch: Partial<import("../engine/road-parity").SicroParityObject>,
    ) => {
      setDoc((prev) => {
        if (!prev) return prev;
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
    // O sentinela do fundo nunca entra no marquee, então vem sempre sozinho.
    if (ids.length === 1 && ids[0] === BACKGROUND_SELECTION_ID) {
      setDoc((prev) =>
        prev ? { ...prev, background_image: null } : prev,
      );
      editor.setSelectedId(null);
      return;
    }
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

  // ----- Zoom por teclado (o da roda fica no CanvasStage) -----
  // Ancorado no centro do canvas visível para o enquadramento não pular.

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

  // ----- Imagem de fundo -----

  /** Caminho relativo ao workspace ou absoluto → URL via asset protocol do Tauri. */
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
   * Define o fundo: mede a imagem, encaixa na área útil (10 % de margem) e
   * deixa destravado para ajuste. `preMeasured` (drone) pula a medição.
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
        // Já seleciona para os handles do Transformer aparecerem.
        editor.setSelectedId(BACKGROUND_SELECTION_ID);
        setFeedback(
          `Fundo aplicado (${Math.round(rect.width)}×${Math.round(rect.height)}px), centralizado e desbloqueado para ajuste.`,
        );
      };
      if (extra?.preMeasured) {
        apply(extra.preMeasured.width, extra.preMeasured.height);
        return;
      }
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
        // Sem medida, insere no tamanho do canvas para o usuário reenquadrar.
        apply(doc.canvas.width_px, doc.canvas.height_px);
      };
    },
    [doc, resolveBackgroundUrl],
  );

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

  const handleCenterBackground = useCallback(() => {
    if (!doc?.background_image) return;
    const bg = doc.background_image;
    handleBackgroundChange({
      x: (doc.canvas.width_px - bg.width) / 2,
      y: (doc.canvas.height_px - bg.height) / 2,
    });
    setFeedback("Fundo centralizado.");
  }, [doc, handleBackgroundChange]);

  const handleFitBackground = useCallback(() => {
    if (!doc?.background_image) return;
    const bg = doc.background_image;
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

  const handleResetBackgroundRotation = useCallback(() => {
    handleBackgroundChange({ rotation: 0 });
    setFeedback("Rotação do fundo reiniciada.");
  }, [handleBackgroundChange]);

  /** Import OSM confirmado: troca as vias OSM anteriores e registra a sessão. */
  const handleOsmImportConfirm = useCallback(
    (result: OsmImportResult) => {
      if (!doc) return;

      setDoc((prev) => {
        if (!prev) return prev;
        editor.pushHistory(prev.objects);
        const nextImports = [
          ...(prev.osm_imports ?? []),
          result.session,
        ];
        // Só substitui parity vindo do OSM; o criado à mão fica.
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
        // O adapter assume `pxPerM = suggested_px_per_m`; com outra escala
        // os objetos saem do canvas. Por isso o import sobrescreve a escala.
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

  // ----- Cliques no canvas -----

  const handleCanvasDblClick = useCallback(() => {
    // No-op por ora.
  }, []);

  const handleCanvasClick = (p: SicroPoint) => {
    if (!doc) return;
    const tool = editor.tool;

    // Parity guarda metros; o renderer multiplica por pxPerM.
    const pxToM = (pt: SicroPoint): SicroPoint => {
      const pxPerM = doc.scale?.px_per_m ?? 10;
      return { x: pt.x / pxPerM, y: pt.y / pxPerM };
    };

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

    // Via em dois cliques; o preset da ferramenta é ajustado depois no Inspector.
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
      // Com escala, o veículo entra em tamanho real (arte desenhada em
      // 1 mm = 1 m); sem escala, presets em px.
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

  // ----- Atalhos (escopo `croqui`) -----
  // handleSave/handleExportPng são declarados mais abaixo: só são lidos
  // quando a tecla dispara, nunca no render, então a ordem não importa.
  useShortcuts({
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

  // Único ponto de saída do editor: sem dirty executa direto, com dirty
  // guarda o destino e abre o modal.
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

  // Guard global para o ActivityRail: a Promise resolve com a escolha do modal.
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
      // "limpo" = sem carimbo, para o corpo do laudo onde o cabeçalho já existe.
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

  // ----- Modal de alterações não salvas -----
  const handleModalSaveAndLeave = async () => {
    if (!pendingNav) return;
    const ok = await handleSave();
    if (!ok) return; // fica no editor para tentar de novo
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
            // O sidecar (parâmetros de lente/crop) vai junto no doc para
            // manter a cadeia de custódia.
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
// Ferramenta → subtipo

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
    // Viaturas/especiais
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
    // Pedestres em decúbito
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
    // Vestígios
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
    // Mobiliário urbano
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

/** Próximo rótulo "V1", "V2", … livre. */
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

/** Preset de via parity por ferramenta `road_*`; o perito refina no Inspector. */
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
      return {
        largura_m: 7,
        superficie: "asfalto",
        mao_dupla: true,
        marcacao: "amarela",
      };
    case "road_avenue":
      return {
        largura_m: 14,
        superficie: "asfalto",
        mao_dupla: true,
        marcacao: "amarela",
      };
    case "road_highway":
      return {
        largura_m: 12,
        superficie: "asfalto",
        mao_dupla: true,
        marcacao: "amarela",
      };
    case "road_dirt":
      return {
        largura_m: 5,
        superficie: "terra",
        mao_dupla: false,
        marcacao: "nenhuma",
      };
    case "road_parking":
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
// Barra de status

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
      {/* Indicador estático do motor de via; não é clicável. */}
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
// Carimbo técnico do PNG

/** Cabeçalho (título · escala · data · BO) e rodapé em canvas 2D off-screen. */
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

  ctx.fillStyle = "#ffffff";
  ctx.fillRect(0, 0, canvas.width, canvas.height);

  // Cabeçalho
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

  ctx.drawImage(img, 0, headerH);

  // Rodapé
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
