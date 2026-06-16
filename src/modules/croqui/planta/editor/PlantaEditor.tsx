/**
 * PlantaEditor — orquestra o croqui de planta baixa (motor Pixi forkado do
 * arcada). Monta a Application + viewport Main, expõe a toolbar no design system
 * do SICRO e persiste o floorplan no `.sicroplanta` via commands (read/save).
 *
 * O motor (vendored, @ts-nocheck) é acessado por um helper tipado-frouxo
 * (engine/mount.ts). A camada PERICIAL (vestígios + legenda + export) entra na
 * fase seguinte (P-D).
 */

import { useCallback, useEffect, useRef, useState } from "react";
import {
  ArrowLeft,
  Save,
  Hand,
  MousePointer2,
  PencilLine,
  DoorOpen,
  AppWindow,
  Ruler,
  Eraser,
  Magnet,
  Plus,
  Minus,
  ZoomIn,
  ZoomOut,
  Maximize,
  Undo2,
  Redo2,
  Sofa,
  MapPin,
  X,
  FileImage,
  Printer,
  Users,
  Crosshair,
  Type,
  Fence,
} from "lucide-react";
import {
  selectActiveWorkspacePath,
  selectActiveOccurrence,
  useWorkspaceStore,
} from "@stores/workspaceStore";
import { commands } from "@core/commands";
import { toSicroError } from "@core/errors";
import { useCroquiStore } from "../../store/croquiStore";
import { coercePlantaDoc, type SicroPlantaDoc } from "../schema";
import {
  mountPlanta,
  loadFloorplan,
  saveFloorplan,
  destroyPlanta,
  changeFloor,
  zoomIn,
  zoomOut,
  zoomReset,
  floorplanSnapshot,
  restoreFloorplan,
  capturePlantaDataUrl,
  wallChainPending,
  cancelWallChain,
  pickWallAt,
  setWallStyle,
  loadWallStyles,
  setNodesVisible,
  showOnlyNodes,
  loadLabelOffsets,
  setLabelMovedHandler,
  deleteWallByNodes,
} from "../engine/mount";
import {
  stampPlantaPng,
  buildLegendRows,
  openPlantaPrintView,
} from "./exportPlanta";
import { useStore } from "../engine/stores/EditorStore";
import { Tool } from "../engine/editor/editor/constants";
import {
  getCategoriesRequest,
  getCategoryInfo,
  getPeople,
  type CategoryDef,
  type FurnitureDef,
} from "../engine/api/api-client";
import {
  renderEvidenceMarkers,
  renderTrajectories,
  renderStructures,
  renderTexts,
  screenToWorldPoint,
  worldToScreenPoint,
} from "../engine/evidenceLayer";
import {
  EVIDENCE_TIPOS,
  evidenceMeta,
  evidenceLabelFor,
  type EvidenceTipo,
  type EvidenceLabelKind,
} from "../evidence";
import type {
  PlantaEvidenceMarker,
  PlantaTrajectory,
  PlantaText,
  PlantaStructure,
  PlantaStructureKind,
} from "../schema";

interface ToolItem {
  tool: Tool;
  icon: typeof Hand;
  label: string;
}

const TOOLS: ToolItem[] = [
  { tool: Tool.View, icon: Hand, label: "Navegar" },
  { tool: Tool.Edit, icon: MousePointer2, label: "Selecionar" },
  { tool: Tool.WallAdd, icon: PencilLine, label: "Parede" },
  { tool: Tool.FurnitureAddDoor, icon: DoorOpen, label: "Porta" },
  { tool: Tool.FurnitureAddWindow, icon: AppWindow, label: "Janela" },
  { tool: Tool.Measure, icon: Ruler, label: "Medir" },
  { tool: Tool.Remove, icon: Eraser, label: "Remover" },
];

export function PlantaEditor() {
  const workspacePath = useWorkspaceStore(selectActiveWorkspacePath);
  const occurrence = useWorkspaceStore(selectActiveOccurrence);
  const activeCroqui = useCroquiStore((s) => s.activeCroqui);
  const clearCurrent = useCroquiStore((s) => s.clearCurrent);
  const exportPngStore = useCroquiStore((s) => s.exportPng);

  const activeTool = useStore((s) => s.activeTool);
  const setTool = useStore((s) => s.setTool);
  const setPendingFurniture = useStore((s) => s.setPendingFurniture);
  const snap = useStore((s) => s.snap);
  const setSnap = useStore((s) => s.setSnap);
  const floor = useStore((s) => s.floor);

  // Catálogo de mobília (placeholders locais; o perito troca a arte depois).
  const [furnitureOpen, setFurnitureOpen] = useState(false);
  const [catalog, setCatalog] = useState<
    { cat: CategoryDef; items: FurnitureDef[] }[]
  >([]);

  // Vestígios (camada pericial) — vivem em doc.evidences; rótulo derivado da ordem.
  const [evidences, setEvidences] = useState<PlantaEvidenceMarker[]>([]);
  const [labelKind, setLabelKind] = useState<EvidenceLabelKind>("letra");
  const [vestigiosOpen, setVestigiosOpen] = useState(false);
  const [evidenceType, setEvidenceType] = useState<EvidenceTipo | null>(null);
  const evidenceTypeRef = useRef<EvidenceTipo | null>(null);
  useEffect(() => {
    evidenceTypeRef.current = evidenceType;
  }, [evidenceType]);

  // Pessoas (poses, colocadas como mobília) + Trajetória balística (camada SICRO).
  const [peopleOpen, setPeopleOpen] = useState(false);
  const [trajectories, setTrajectories] = useState<PlantaTrajectory[]>([]);
  const [trajetoriaActive, setTrajetoriaActive] = useState(false);
  const trajetoriaActiveRef = useRef(false);
  const trajStartRef = useRef<{ x: number; y: number } | null>(null);
  useEffect(() => {
    trajetoriaActiveRef.current = trajetoriaActive;
    if (!trajetoriaActive) trajStartRef.current = null;
  }, [trajetoriaActive]);

  // Texto livre + Estruturas (muro/cerca/calçada) — camadas SICRO em escala.
  const [texts, setTexts] = useState<PlantaText[]>([]);
  const [structures, setStructures] = useState<PlantaStructure[]>([]);
  // Skin por parede (muro/cerca/calçada): mapa par-de-nós → tipo. A parede é
  // desenhada com o tool Parede (preview ao vivo) e ganha aparência aqui.
  const [wallStyles, setWallStyles] = useState<Record<string, PlantaStructureKind>>(
    {},
  );
  const wallStylesRef = useRef<Record<string, PlantaStructureKind>>({});
  useEffect(() => {
    wallStylesRef.current = wallStyles;
  }, [wallStyles]);
  // Offsets das cotas arrastadas (par de nós → {x,y} locais).
  const [labelOffsets, setLabelOffsets] = useState<
    Record<string, { x: number; y: number }>
  >({});
  const labelOffsetsRef = useRef<Record<string, { x: number; y: number }>>({});
  useEffect(() => {
    labelOffsetsRef.current = labelOffsets;
  }, [labelOffsets]);
  // Registra o callback que o motor chama quando uma cota é arrastada.
  useEffect(() => {
    setLabelMovedHandler((key, x, y) => {
      setLabelOffsets((prev) => ({ ...prev, [key]: { x, y } }));
    });
    return () => setLabelMovedHandler(null);
  }, []);
  // Parede selecionada (modo Selecionar) → painel Propriedades.
  const [selectedWall, setSelectedWall] = useState<{
    key: string;
    kind: PlantaStructureKind | "parede";
    lengthM: number;
    nodeIds: number[];
  } | null>(null);
  const selectedWallRef = useRef<{
    key: string;
    kind: PlantaStructureKind | "parede";
    lengthM: number;
    nodeIds: number[];
  } | null>(null);
  useEffect(() => {
    selectedWallRef.current = selectedWall;
  }, [selectedWall]);
  const [textMode, setTextMode] = useState(false);
  const textModeRef = useRef(false);
  const [structKind, setStructKind] = useState<PlantaStructureKind | null>(null);
  const structKindRef = useRef<PlantaStructureKind | null>(null);
  const structStartRef = useRef<{ x: number; y: number } | null>(null);
  const [structOpen, setStructOpen] = useState(false);
  // Estrutura selecionada (clique no modo Selecionar) → highlight + Delete remove.
  const [selectedStructureId, setSelectedStructureId] = useState<string | null>(
    null,
  );
  const selectedStructIdRef = useRef<string | null>(null);
  useEffect(() => {
    selectedStructIdRef.current = selectedStructureId;
  }, [selectedStructureId]);
  // Editor de texto inline (input flutuante sobre o canvas).
  const [textEditor, setTextEditor] = useState<{
    sx: number;
    sy: number;
    wx: number;
    wy: number;
    value: string;
    editId: string | null;
  } | null>(null);
  useEffect(() => {
    textModeRef.current = textMode;
  }, [textMode]);
  useEffect(() => {
    structKindRef.current = structKind;
    if (!structKind) structStartRef.current = null;
  }, [structKind]);
  // Esc no editor de texto cancela em vez de comitar (via onBlur).
  const textCancelRef = useRef(false);
  const textInputRef = useRef<HTMLInputElement | null>(null);
  // Foca o input no FRAME seguinte ao abrir (não no mount): com autoFocus, o
  // clique que abriu o editor disparava blur imediato e fechava o campo.
  // Como o input monta SEM foco, não há blur espúrio; o rAF então o foca.
  useEffect(() => {
    if (!textEditor) return undefined;
    const id = requestAnimationFrame(() => {
      const el = textInputRef.current;
      if (el) {
        el.focus();
        el.select();
      }
    });
    return () => cancelAnimationFrame(id);
    // depende só da IDENTIDADE de abertura (não do valor digitado).
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [textEditor?.editId, textEditor?.sx, textEditor?.sy]);

  const hostRef = useRef<HTMLDivElement>(null);
  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  const appRef = useRef<any>(null);
  const docRef = useRef<SicroPlantaDoc | null>(null);

  const [ready, setReady] = useState(false);
  const [busy, setBusy] = useState(false);
  const [feedback, setFeedback] = useState<string | null>(null);
  // Erro de montagem do Pixi (WebGL/WebView2). Capturado no effect e RELANÇADO
  // no render, pois um throw dentro de useEffect não é pego pelo ErrorBoundary.
  const [mountError, setMountError] = useState<Error | null>(null);

  // --- Histórico (Ctrl+Z) baseado em snapshot do floorplan ---
  // Cada item = string do FloorPlan.save(). Cobre TUDO (parede, mover,
  // rotacionar, porta/janela, deletar), pois snapshota o plano inteiro.
  const undoStack = useRef<string[]>([]);
  const redoStack = useRef<string[]>([]);
  const restoringRef = useRef(false);
  const snapTimer = useRef<number | null>(null);
  const [, bumpHist] = useState(0);
  // Refs com o estado de overlay (vestígios/trajetórias/textos/estruturas) para o
  // histórico ler o valor atual sem recriar o listener.
  const evidencesRef = useRef(evidences);
  const trajectoriesRef = useRef(trajectories);
  const textsRef = useRef(texts);
  const structuresRef = useRef(structures);
  useEffect(() => {
    evidencesRef.current = evidences;
  }, [evidences]);
  useEffect(() => {
    trajectoriesRef.current = trajectories;
  }, [trajectories]);
  useEffect(() => {
    textsRef.current = texts;
  }, [texts]);
  useEffect(() => {
    structuresRef.current = structures;
  }, [structures]);
  const captureSnapRef = useRef<(() => void) | null>(null);

  // Restaura um snapshot COMBINADO (floorplan arcada + overlays SICRO). Assim o
  // Ctrl+Z cobre vestígios, trajetórias, textos e estruturas — não só paredes.
  const applySnapshot = useCallback((str: string) => {
    try {
      const o = JSON.parse(str);
      restoreFloorplan(o.fp ?? null);
      setEvidences(Array.isArray(o.ev) ? o.ev : []);
      setTrajectories(Array.isArray(o.tr) ? o.tr : []);
      setTexts(Array.isArray(o.tx) ? o.tx : []);
      setStructures(Array.isArray(o.st) ? o.st : []);
      const ws = o.ws && typeof o.ws === "object" ? o.ws : {};
      setWallStyles(ws);
      loadWallStyles(ws);
      const lo = o.lo && typeof o.lo === "object" ? o.lo : {};
      setLabelOffsets(lo);
      loadLabelOffsets(lo);
    } catch {
      restoreFloorplan(str); // compat: snapshot antigo (só floorplan)
    }
  }, []);

  // Monta o motor Pixi + carrega o documento.
  useEffect(() => {
    const host = hostRef.current;
    if (!workspacePath || !activeCroqui || !host) return undefined;

    // Um throw dentro do useEffect é assíncrono ao render → o ErrorBoundary não
    // pega. Capturamos no state e relançamos no render (vide `mountError`).
    let app;
    try {
      app = mountPlanta(host);
    } catch (e) {
      setMountError(e instanceof Error ? e : new Error(String(e)));
      return undefined;
    }
    appRef.current = app;
    setTool(Tool.View);

    // Captura snapshot pro histórico (debounced) ao soltar o ponteiro — cobre
    // criar/mover/rotacionar/deletar, pois qualquer interação termina em pointerup.
    const captureSnap = () => {
      if (restoringRef.current) return;
      // Parede em andamento com só o 1º clique (nó solto): estado transitório,
      // não vira passo de histórico — senão o Ctrl+Z volta pro nó órfão.
      if (wallChainPending()) return;
      // Snapshot COMBINADO: floorplan + overlays SICRO (lidos via refs).
      const s = JSON.stringify({
        fp: floorplanSnapshot(),
        ev: evidencesRef.current,
        tr: trajectoriesRef.current,
        tx: textsRef.current,
        st: structuresRef.current,
        ws: wallStylesRef.current,
        lo: labelOffsetsRef.current,
      });
      const stack = undoStack.current;
      if (s === stack[stack.length - 1]) return;
      stack.push(s);
      if (stack.length > 120) stack.shift();
      redoStack.current = [];
      bumpHist((v) => v + 1);
    };
    captureSnapRef.current = captureSnap;
    const onPointerUp = () => {
      if (snapTimer.current) window.clearTimeout(snapTimer.current);
      snapTimer.current = window.setTimeout(captureSnap, 320);
    };
    host.addEventListener("pointerup", onPointerUp);

    let alive = true;
    void commands
      .readCroqui(workspacePath, activeCroqui.id)
      .then((payload) => {
        if (!alive) return;
        const d = coercePlantaDoc(payload.doc);
        docRef.current = d;
        loadFloorplan(d.floorplan);
        setEvidences(d.evidences ?? []);
        setTrajectories(d.trajectories ?? []);
        setTexts(d.texts ?? []);
        setStructures(d.structures ?? []);
        setWallStyles(d.wallStyles ?? {});
        loadWallStyles(d.wallStyles ?? {});
        setLabelOffsets(d.labelOffsets ?? {});
        loadLabelOffsets(d.labelOffsets ?? {});
        setLabelKind(d.label_kind ?? "letra");
        // Estado inicial = base do histórico (combinado: floorplan + overlays).
        const init = JSON.stringify({
          fp: floorplanSnapshot(),
          ev: d.evidences ?? [],
          tr: d.trajectories ?? [],
          tx: d.texts ?? [],
          st: d.structures ?? [],
          ws: d.wallStyles ?? {},
          lo: d.labelOffsets ?? {},
        });
        undoStack.current = [init];
        redoStack.current = [];
        bumpHist((v) => v + 1);
        setReady(true);
      })
      .catch((err) => {
        if (alive) setFeedback(`Falha ao abrir: ${toSicroError(err).message}`);
      });

    return () => {
      alive = false;
      host.removeEventListener("pointerup", onPointerUp);
      if (snapTimer.current) window.clearTimeout(snapTimer.current);
      destroyPlanta(appRef.current);
      appRef.current = null;
      setReady(false);
    };
  }, [workspacePath, activeCroqui, setTool]);

  const undo = useCallback(() => {
    // Se há uma parede em andamento com só o 1º clique (nó solto), o Ctrl+Z
    // cancela esse desenho (remove o nó órfão) — igual ao botão direito — em
    // vez de mexer no histórico.
    if (wallChainPending()) {
      cancelWallChain();
      bumpHist((v) => v + 1);
      return;
    }
    const stack = undoStack.current;
    if (stack.length < 2) return;
    const cur = stack.pop() as string;
    redoStack.current.push(cur);
    const prev = stack[stack.length - 1] as string;
    restoringRef.current = true;
    applySnapshot(prev);
    window.setTimeout(() => {
      restoringRef.current = false;
    }, 120);
    bumpHist((v) => v + 1);
  }, [applySnapshot]);

  const redo = useCallback(() => {
    // Simetria com undo(): se há parede em andamento (nó solto), Ctrl+Y cancela
    // o desenho em vez de aplicar o redo sobre um estado transitório.
    if (wallChainPending()) {
      cancelWallChain();
      bumpHist((v) => v + 1);
      return;
    }
    if (!redoStack.current.length) return;
    const s = redoStack.current.pop() as string;
    undoStack.current.push(s);
    restoringRef.current = true;
    applySnapshot(s);
    window.setTimeout(() => {
      restoringRef.current = false;
    }, 120);
    bumpHist((v) => v + 1);
  }, [applySnapshot]);

  // Ctrl+Z / Ctrl+Shift+Z / Ctrl+Y (ignora quando digitando em campos).
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      const t = e.target as HTMLElement | null;
      if (
        t &&
        (t.tagName === "INPUT" ||
          t.tagName === "TEXTAREA" ||
          t.isContentEditable)
      )
        return;
      if (!(e.ctrlKey || e.metaKey)) return;
      const k = e.key.toLowerCase();
      if (k === "z" && !e.shiftKey) {
        e.preventDefault();
        undo();
      } else if (k === "y" || (k === "z" && e.shiftKey)) {
        e.preventDefault();
        redo();
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [undo, redo]);

  // Carrega o catálogo de mobília uma vez (local/offline).
  useEffect(() => {
    let alive = true;
    void (async () => {
      try {
        const cats = await (await getCategoriesRequest()).json();
        const entries = await Promise.all(
          cats.map(async (cat) => ({
            cat,
            items: await (await getCategoryInfo(cat._id)).json(),
          })),
        );
        if (alive) setCatalog(entries);
      } catch {
        /* catálogo indisponível — ignora */
      }
    })();
    return () => {
      alive = false;
    };
  }, []);

  // Seleciona uma peça → entra no modo de posicionar (clica na planta p/ soltar).
  const handlePickFurniture = useCallback(
    (def: FurnitureDef) => {
      setEvidenceType(null);
      setTrajetoriaActive(false);
      setTextMode(false);
      setStructKind(null);
      setStructOpen(false);
      setPendingFurniture(def);
      setTool(Tool.FurnitureAdd);
      setFurnitureOpen(false);
      setPeopleOpen(false);
      setVestigiosOpen(false);
      setFeedback(`Clique na planta para posicionar: ${def.name}`);
    },
    [setPendingFurniture, setTool],
  );

  const handleSave = useCallback(async () => {
    if (!workspacePath || !activeCroqui || !docRef.current) return;
    setBusy(true);
    setFeedback(null);
    try {
      const floorplan = saveFloorplan();
      const next: SicroPlantaDoc = {
        ...docRef.current,
        floorplan,
        evidences,
        trajectories,
        texts,
        structures,
        wallStyles,
        labelOffsets,
        label_kind: labelKind,
        updated_at: new Date().toISOString(),
      };
      await commands.saveCroqui(workspacePath, activeCroqui.id, next);
      docRef.current = next;
      setFeedback("Planta salva.");
    } catch (err) {
      setFeedback(`Falha ao salvar: ${toSicroError(err).message}`);
    } finally {
      setBusy(false);
    }
  }, [
    workspacePath,
    activeCroqui,
    evidences,
    trajectories,
    texts,
    structures,
    wallStyles,
    labelOffsets,
    labelKind,
  ]);

  // Render dos marcadores de vestígio sempre que a lista/rótulo mudam.
  useEffect(() => {
    if (!ready) return;
    renderEvidenceMarkers(evidences, labelKind);
  }, [evidences, labelKind, ready]);

  // Render das trajetórias balísticas.
  useEffect(() => {
    if (!ready) return;
    renderTrajectories(trajectories);
  }, [trajectories, ready]);

  // Render das estruturas (muro/cerca/calçada) + highlight da selecionada.
  useEffect(() => {
    if (!ready) return;
    renderStructures(structures, selectedStructureId);
  }, [structures, selectedStructureId, ready]);

  // Mudança em qualquer overlay SICRO agenda um snapshot (debounced) — assim o
  // Ctrl+Z cobre vestígios/trajetórias/textos/estruturas, não só o floorplan.
  useEffect(() => {
    if (!ready || restoringRef.current) return;
    if (snapTimer.current) window.clearTimeout(snapTimer.current);
    snapTimer.current = window.setTimeout(
      () => captureSnapRef.current?.(),
      340,
    );
  }, [
    evidences,
    trajectories,
    texts,
    structures,
    wallStyles,
    labelOffsets,
    ready,
  ]);

  // Nós (handles) aparecem só onde fazem sentido: desenhando (Parede) → todos
  // (snap/conexão); Selecionar com parede selecionada → só os 2 nós dela;
  // Navegar/demais → escondidos. Mantém a planta limpa (os pontos somem).
  useEffect(() => {
    if (!ready) return;
    if (activeTool === Tool.WallAdd) {
      setNodesVisible(true);
    } else if (activeTool === Tool.Edit && selectedWall) {
      showOnlyNodes(selectedWall.nodeIds);
    } else {
      setNodesVisible(false);
    }
  }, [activeTool, selectedWall, ready]);

  // Delete/Backspace remove a ESTRUTURA selecionada (só no modo Selecionar, e
  // não enquanto digita num campo). Esc desseleciona.
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      const t = e.target as HTMLElement | null;
      if (
        t &&
        (t.tagName === "INPUT" ||
          t.tagName === "TEXTAREA" ||
          t.isContentEditable)
      ) {
        return;
      }
      if (e.key === "Escape") {
        setSelectedStructureId(null);
        setSelectedWall(null);
        return;
      }
      if (e.key !== "Delete" && e.key !== "Backspace") return;
      // não apaga enquanto há um modo de colocação ativo.
      const placing =
        textModeRef.current ||
        !!structKindRef.current ||
        trajetoriaActiveRef.current ||
        !!evidenceTypeRef.current;
      if (placing) return;
      // PAREDE selecionada → remove via motor (mesmo efeito do Remover).
      const wall = selectedWallRef.current;
      if (wall) {
        e.preventDefault();
        deleteWallByNodes(wall.nodeIds[0], wall.nodeIds[1]);
        // limpa estilo/offset órfãos do par de nós + desseleciona.
        setWallStyle(wall.key, "parede");
        setWallStyles((prev) => {
          const n = { ...prev };
          delete n[wall.key];
          return n;
        });
        setLabelOffsets((prev) => {
          const n = { ...prev };
          delete n[wall.key];
          return n;
        });
        setSelectedWall(null);
        // agenda snapshot p/ Ctrl+Z cobrir a remoção via teclado.
        window.setTimeout(() => captureSnapRef.current?.(), 60);
        return;
      }
      // ESTRUTURA (legado) selecionada.
      const id = selectedStructIdRef.current;
      if (!id) return;
      e.preventDefault();
      setStructures((prev) => prev.filter((s) => s.id !== id));
      setSelectedStructureId(null);
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, []);

  // Render dos textos livres — com mover (arraste) e editar (duplo-clique).
  useEffect(() => {
    if (!ready) return;
    renderTexts(texts, {
      onMove: (id: string, x: number, y: number) =>
        setTexts((prev) =>
          prev.map((t) => (t.id === id ? { ...t, x, y } : t)),
        ),
      onEdit: (id: string) => {
        const t = texts.find((tx) => tx.id === id);
        const sp = t ? worldToScreenPoint(t.x, t.y) : null;
        if (!t || !sp) return;
        const host = hostRef.current;
        const r = host
          ? host.getBoundingClientRect()
          : { left: 0, top: 0 };
        setTextEditor({
          sx: sp.x - r.left,
          sy: sp.y - r.top,
          wx: t.x,
          wy: t.y,
          value: t.text,
          editId: id,
        });
      },
    });
  }, [texts, ready]);

  // Colocação de vestígio: clique na planta quando há um tipo selecionado.
  useEffect(() => {
    const host = hostRef.current;
    if (!host || !ready) return undefined;
    const onDown = (e: PointerEvent) => {
      // Botão direito cancela a sequência pendente (trajetória ou estrutura).
      if (e.button === 2) {
        if (trajStartRef.current) {
          trajStartRef.current = null;
          setFeedback("Trajetória cancelada.");
        }
        if (structStartRef.current) {
          structStartRef.current = null;
          setFeedback("Estrutura cancelada.");
        }
        return;
      }
      if (e.button !== 0) return;
      const p = screenToWorldPoint(e.clientX, e.clientY);
      if (!p) return;
      const x = Math.max(0, Math.min(6000, p.x)); // clampa ao mundo (60 m)
      const y = Math.max(0, Math.min(6000, p.y));
      const uid = (): string =>
        (globalThis.crypto?.randomUUID?.() as string) ??
        `id_${Date.now()}_${Math.round(x)}_${Math.round(y)}`;

      // Estrutura (muro/cerca/calçada): 2 cliques (origem → fim).
      const sk = structKindRef.current;
      if (sk) {
        if (!structStartRef.current) {
          structStartRef.current = { x, y };
          setFeedback("Clique no ponto final da estrutura. Botão direito cancela.");
        } else {
          const s = structStartRef.current;
          structStartRef.current = null;
          setStructures((prev) => [
            ...prev,
            { id: uid(), kind: sk, x1: s.x, y1: s.y, x2: x, y2: y },
          ]);
          setFeedback("Estrutura adicionada.");
        }
        return;
      }

      // Texto livre: 1 clique abre o editor inline no ponto.
      if (textModeRef.current) {
        const host = hostRef.current;
        const r = host ? host.getBoundingClientRect() : { left: 0, top: 0 };
        setTextEditor({
          sx: e.clientX - r.left,
          sy: e.clientY - r.top,
          wx: x,
          wy: y,
          value: "",
          editId: null,
        });
        return;
      }

      // Trajetória balística: 2 cliques (origem → impacto).
      if (trajetoriaActiveRef.current) {
        if (!trajStartRef.current) {
          trajStartRef.current = { x, y };
          setFeedback("Clique no ponto de impacto para concluir a trajetória.");
        } else {
          const s = trajStartRef.current;
          trajStartRef.current = null;
          setTrajectories((prev) => [
            ...prev,
            { id: uid(), x1: s.x, y1: s.y, x2: x, y2: y },
          ]);
          setFeedback("Trajetória adicionada.");
        }
        return;
      }

      // Vestígio: 1 clique.
      const tipo = evidenceTypeRef.current;
      if (tipo) {
        setEvidences((prev) => [...prev, { id: uid(), x, y, tipo }]);
        return;
      }
      // Selecionar (modo Selecionar): clicar numa PAREDE abre Propriedades.
      const wallHit = pickWallAt(x, y) as {
        key: string;
        kind: PlantaStructureKind | "parede";
        lengthM: number;
        nodeIds: number[];
      } | null;
      if (wallHit) {
        setSelectedWall(wallHit);
        setSelectedStructureId(null);
        return;
      }
      setSelectedWall(null);

      // (legado) seleção de estrutura overlay — desativável.
      const sList = structuresRef.current || [];
      let bestId: string | null = null;
      let bestD = Infinity;
      for (const st of sList) {
        const d = distPointToSegment(x, y, st.x1, st.y1, st.x2, st.y2);
        const thr = Math.max((st.espessura || 16) / 2, 18);
        if (d <= thr && d < bestD) {
          bestD = d;
          bestId = st.id;
        }
      }
      setSelectedStructureId(bestId);
    };
    host.addEventListener("pointerdown", onDown);
    return () => host.removeEventListener("pointerdown", onDown);
  }, [ready]);

  // Seleciona um tipo de vestígio → entra no modo de marcação (clica na planta).
  const handlePickEvidence = useCallback(
    (tipo: EvidenceTipo) => {
      // refs síncronos (o listener lê refs; evita corrida no 1º clique rápido).
      evidenceTypeRef.current = tipo;
      trajetoriaActiveRef.current = false;
      trajStartRef.current = null;
      textModeRef.current = false;
      structKindRef.current = null;
      structStartRef.current = null;
      setPendingFurniture(null);
      setTrajetoriaActive(false);
      setTextMode(false);
      setStructKind(null);
      setStructOpen(false);
      setTool(Tool.View);
      setEvidenceType(tipo);
      setVestigiosOpen(false);
      setFurnitureOpen(false);
      setPeopleOpen(false);
      setFeedback(`Clique na planta para marcar: ${evidenceMeta(tipo).label}`);
    },
    [setPendingFurniture, setTool],
  );

  // Texto livre: liga/desliga o modo (clique na planta abre o editor inline).
  const handleTexto = useCallback(() => {
    const next = !textModeRef.current; // verdade do ref (sempre sincronizado)
    textModeRef.current = next;
    setTextMode(next);
    if (next) {
      evidenceTypeRef.current = null;
      trajetoriaActiveRef.current = false;
      trajStartRef.current = null;
      structKindRef.current = null;
      structStartRef.current = null;
      setEvidenceType(null);
      setTrajetoriaActive(false);
      setStructKind(null);
      setStructOpen(false);
      setPendingFurniture(null);
      setTool(Tool.View);
      setVestigiosOpen(false);
      setFurnitureOpen(false);
      setPeopleOpen(false);
      setFeedback("Texto: clique na planta onde quer inserir o rótulo.");
    }
  }, [setPendingFurniture, setTool]);

  // Estrutura (muro/cerca/calçada): seleciona o tipo → modo de 2 cliques.
  const handlePickStruct = useCallback(
    (kind: PlantaStructureKind) => {
      structKindRef.current = kind;
      structStartRef.current = null;
      evidenceTypeRef.current = null;
      trajetoriaActiveRef.current = false;
      trajStartRef.current = null;
      textModeRef.current = false;
      setEvidenceType(null);
      setTrajetoriaActive(false);
      setTextMode(false);
      setPendingFurniture(null);
      setTool(Tool.View);
      setVestigiosOpen(false);
      setFurnitureOpen(false);
      setPeopleOpen(false);
      setStructOpen(false);
      setStructKind(kind);
      const labels: Record<PlantaStructureKind, string> = {
        muro: "muro",
        cerca_madeira: "cerca de madeira",
        cerca_arame: "cerca de arame",
        calcada: "calçada",
      };
      setFeedback(
        `${labels[kind]}: clique no início e no fim. Botão direito cancela.`,
      );
    },
    [setPendingFurniture, setTool],
  );

  // Trajetória balística: liga/desliga o modo de 2 cliques (origem → impacto).
  const handleTrajetoria = useCallback(() => {
    const next = !trajetoriaActiveRef.current;
    trajetoriaActiveRef.current = next;
    if (!next) trajStartRef.current = null;
    setTrajetoriaActive(next);
    if (next) {
      evidenceTypeRef.current = null;
      textModeRef.current = false;
      structKindRef.current = null;
      structStartRef.current = null;
      setEvidenceType(null);
      setPendingFurniture(null);
      setTextMode(false);
      setStructKind(null);
      setStructOpen(false);
      setTool(Tool.View);
      setVestigiosOpen(false);
      setFurnitureOpen(false);
      setPeopleOpen(false);
      setFeedback(
        "Trajetória: clique na origem (saída do disparo) e depois no ponto de impacto. Botão direito cancela.",
      );
    }
  }, [setPendingFurniture, setTool]);

  // Painel Propriedades: troca o TIPO (skin) da parede selecionada.
  const applyWallStyle = useCallback(
    (kind: PlantaStructureKind | "parede") => {
      if (!selectedWall) return;
      const key = selectedWall.key;
      setWallStyles((prev) => {
        const n = { ...prev };
        if (kind === "parede") delete n[key];
        else n[key] = kind;
        return n;
      });
      setWallStyle(key, kind); // motor: redesenha aquela parede
      setSelectedWall((c) => (c ? { ...c, kind } : c));
    },
    [selectedWall],
  );

  const removeEvidence = useCallback((id: string) => {
    setEvidences((prev) => prev.filter((e) => e.id !== id));
  }, []);

  const updateEvidenceDesc = useCallback((id: string, descricao: string) => {
    setEvidences((prev) =>
      prev.map((e) => (e.id === id ? { ...e, descricao } : e)),
    );
  }, []);

  // Troca de ferramenta pela toolbar encerra o modo de marcar vestígio.
  const selectTool = useCallback(
    (tool: Tool) => {
      setEvidenceType(null);
      setTrajetoriaActive(false);
      setTextMode(false);
      setStructKind(null);
      setStructOpen(false);
      setPendingFurniture(null);
      setTool(tool);
    },
    [setTool, setPendingFurniture],
  );

  // Compõe a PRANCHA TÉCNICA (captura Pixi enquadrada + cabeçalho + escala +
  // rosa dos ventos + legenda dos vestígios). Retorna data URL PNG.
  const composePlate = useCallback(async (): Promise<string | null> => {
    const app = appRef.current;
    if (!app) return null;
    // Remove o highlight de seleção da captura (não deve sair na prancha) e
    // restaura logo após.
    const selId = selectedStructIdRef.current;
    if (selId) renderStructures(structuresRef.current, null);
    const cap = capturePlantaDataUrl(app);
    if (selId) renderStructures(structuresRef.current, selId);
    if (!cap) return null;
    const legend = buildLegendRows(evidences, labelKind);
    return stampPlantaPng(
      cap.dataUrl,
      cap.imgPxPerM,
      {
        title: activeCroqui?.title ?? "Croqui de planta",
        occurrence: occurrence
          ? {
              numero_bo: (occurrence as { numero_bo?: string }).numero_bo,
              tipo_pericia: (occurrence as { tipo_pericia?: string })
                .tipo_pericia,
              municipio: (occurrence as { municipio?: string }).municipio,
            }
          : null,
        timestamp: new Date(),
        compassDeg: docRef.current?.compass_deg ?? 0,
        labelKind,
      },
      legend,
    );
  }, [evidences, labelKind, activeCroqui, occurrence]);

  // PNG técnico → salvo em croquis/exports (vira inserível no laudo).
  const handleExportPng = useCallback(async () => {
    if (!workspacePath || !activeCroqui) return;
    setBusy(true);
    setFeedback("Gerando prancha técnica…");
    try {
      await handleSave();
      const plate = await composePlate();
      if (!plate) {
        setFeedback("Falha ao gerar a imagem da planta.");
        return;
      }
      const path = await exportPngStore(workspacePath, plate);
      setFeedback(`Prancha técnica (PNG) salva em ${path}`);
    } catch (err) {
      setFeedback(`Falha ao exportar PNG: ${toSicroError(err).message}`);
    } finally {
      setBusy(false);
    }
  }, [workspacePath, activeCroqui, handleSave, composePlate, exportPngStore]);

  // PDF → abre view de impressão A4 (perito salva como PDF). Também salva o PNG
  // (pra ficar disponível ao laudo).
  const handleExportPdf = useCallback(async () => {
    if (!workspacePath || !activeCroqui) return;
    setBusy(true);
    setFeedback("Preparando prancha para PDF…");
    try {
      await handleSave();
      const plate = await composePlate();
      if (!plate) {
        setFeedback("Falha ao gerar a imagem da planta.");
        return;
      }
      void exportPngStore(workspacePath, plate).catch(() => undefined);
      openPlantaPrintView(plate, activeCroqui.title ?? "Croqui de planta");
      setFeedback('Janela de impressão aberta — escolha "Salvar como PDF".');
    } catch (err) {
      setFeedback(`Falha ao exportar PDF: ${toSicroError(err).message}`);
    } finally {
      setBusy(false);
    }
  }, [workspacePath, activeCroqui, handleSave, composePlate, exportPngStore]);

  const canUndo = undoStack.current.length > 1;
  const canRedo = redoStack.current.length > 0;

  // Relança o erro de montagem do Pixi pra o PlantaBoundary exibir um card
  // legível (em vez de canvas em branco silencioso).
  if (mountError) throw mountError;

  return (
    <div
      style={{
        position: "absolute",
        inset: 0,
        display: "flex",
        flexDirection: "column",
        background: "var(--sicro-bg)",
      }}
    >
      {/* Cabeçalho */}
      <div
        style={{
          display: "flex",
          alignItems: "center",
          gap: 12,
          padding: "8px 12px",
          borderBottom: "1px solid var(--sicro-divider)",
          background: "var(--sicro-surface-1)",
        }}
      >
        <button
          type="button"
          onClick={() => clearCurrent()}
          title="Voltar à lista"
          style={iconBtn}
        >
          <ArrowLeft size={16} />
        </button>
        <strong style={{ fontSize: 14, color: "var(--sicro-fg)" }}>
          {activeCroqui?.title ?? "Croqui de planta"}
        </strong>
        <span style={{ flex: 1 }} />
        {feedback && (
          <span style={{ fontSize: 12, color: "var(--sicro-fg-muted)" }}>
            {feedback}
          </span>
        )}
        <button
          type="button"
          onClick={() => void handleExportPng()}
          disabled={busy || !ready}
          title="Prancha técnica (PNG) — salva e fica disponível para inserir no laudo"
          style={{ ...secondaryBtn, opacity: busy || !ready ? 0.6 : 1 }}
        >
          <FileImage size={14} /> Prancha PNG
        </button>
        <button
          type="button"
          onClick={() => void handleExportPdf()}
          disabled={busy || !ready}
          title="Prancha técnica em PDF (impressão A4 → Salvar como PDF)"
          style={{ ...secondaryBtn, opacity: busy || !ready ? 0.6 : 1 }}
        >
          <Printer size={14} /> PDF
        </button>
        <button
          type="button"
          onClick={() => void handleSave()}
          disabled={busy || !ready}
          style={{ ...primaryBtn, opacity: busy || !ready ? 0.6 : 1 }}
        >
          <Save size={14} /> Salvar
        </button>
      </div>

      {/* Corpo: toolbar + canvas */}
      <div style={{ flex: 1, display: "flex", minHeight: 0 }}>
        <div
          style={{
            display: "flex",
            flexDirection: "column",
            gap: 4,
            padding: 8,
            borderRight: "1px solid var(--sicro-divider)",
            background: "var(--sicro-surface-1)",
          }}
        >
          {TOOLS.map((t) => {
            const Icon = t.icon;
            const active = activeTool === t.tool;
            return (
              <button
                key={t.label}
                type="button"
                onClick={() => selectTool(t.tool)}
                title={t.label}
                style={{
                  ...toolBtn,
                  background: active ? "var(--sicro-surface-2)" : "transparent",
                  color: active ? "var(--sicro-accent)" : "var(--sicro-fg-muted)",
                  borderColor: active
                    ? "var(--sicro-border-strong)"
                    : "transparent",
                }}
              >
                <Icon size={18} />
                <span style={{ fontSize: 9 }}>{t.label}</span>
              </button>
            );
          })}

          {/* Mobília — seletor por cômodo (placeholders; o perito troca a arte) */}
          <div style={{ position: "relative" }}>
            <button
              type="button"
              onClick={() => setFurnitureOpen((v) => !v)}
              title="Mobília — inserir móvel / objeto"
              style={{
                ...toolBtn,
                background:
                  activeTool === Tool.FurnitureAdd || furnitureOpen
                    ? "var(--sicro-surface-2)"
                    : "transparent",
                color:
                  activeTool === Tool.FurnitureAdd || furnitureOpen
                    ? "var(--sicro-accent)"
                    : "var(--sicro-fg-muted)",
                borderColor:
                  activeTool === Tool.FurnitureAdd || furnitureOpen
                    ? "var(--sicro-border-strong)"
                    : "transparent",
              }}
            >
              <Sofa size={18} />
              <span style={{ fontSize: 9 }}>Mobília</span>
            </button>

            {furnitureOpen && (
              <div style={furnitureFlyout}>
                <div style={furnitureFlyoutTitle}>Inserir móvel / objeto</div>
                {catalog.map(({ cat, items }) => (
                  <div key={cat._id} style={{ marginBottom: 6 }}>
                    <div style={furnitureCatTitle}>{cat.name}</div>
                    {items.map((it) => (
                      <button
                        key={it._id}
                        type="button"
                        onClick={() => handlePickFurniture(it)}
                        style={furnitureItem}
                      >
                        <span>{it.name}</span>
                        <span
                          style={{ color: "var(--sicro-fg-dim)", fontSize: 10 }}
                        >
                          {it.width}×{it.height} m
                        </span>
                      </button>
                    ))}
                  </div>
                ))}
                <div style={furnitureHint}>
                  Selecione → clique na planta para posicionar. Mova/gire no modo
                  Selecionar.
                </div>
              </div>
            )}
          </div>

          {/* Vestígios — marcadores periciais (A/B/C) + legenda automática */}
          <div style={{ position: "relative" }}>
            <button
              type="button"
              onClick={() => setVestigiosOpen((v) => !v)}
              title="Vestígios — marcar evidência na planta"
              style={{
                ...toolBtn,
                background:
                  evidenceType || vestigiosOpen
                    ? "var(--sicro-surface-2)"
                    : "transparent",
                color:
                  evidenceType || vestigiosOpen
                    ? "var(--sicro-accent)"
                    : "var(--sicro-fg-muted)",
                borderColor:
                  evidenceType || vestigiosOpen
                    ? "var(--sicro-border-strong)"
                    : "transparent",
              }}
            >
              <MapPin size={18} />
              <span style={{ fontSize: 9 }}>Vestígios</span>
            </button>

            {vestigiosOpen && (
              <div style={furnitureFlyout}>
                <div style={furnitureFlyoutTitle}>Marcar vestígio</div>
                {EVIDENCE_TIPOS.map((m) => (
                  <button
                    key={m.tipo}
                    type="button"
                    onClick={() => handlePickEvidence(m.tipo)}
                    style={furnitureItem}
                  >
                    <span
                      style={{
                        width: 12,
                        height: 12,
                        borderRadius: "50%",
                        background: m.color,
                        flexShrink: 0,
                        border: "1px solid rgba(255,255,255,0.5)",
                      }}
                    />
                    <span style={{ flex: 1 }}>{m.label}</span>
                  </button>
                ))}
                <div style={furnitureHint}>
                  Selecione → clique na planta. O rótulo (A,B,C…) e a legenda saem
                  automáticos pela ordem.
                </div>
              </div>
            )}
          </div>

          {/* Pessoas — poses (vista de cima), colocadas como mobília */}
          <div style={{ position: "relative" }}>
            <button
              type="button"
              onClick={() => setPeopleOpen((v) => !v)}
              title="Pessoas — inserir pose (em pé / caída / sentada)"
              style={{
                ...toolBtn,
                background: peopleOpen ? "var(--sicro-surface-2)" : "transparent",
                color: peopleOpen
                  ? "var(--sicro-accent)"
                  : "var(--sicro-fg-muted)",
                borderColor: peopleOpen
                  ? "var(--sicro-border-strong)"
                  : "transparent",
              }}
            >
              <Users size={18} />
              <span style={{ fontSize: 9 }}>Pessoas</span>
            </button>
            {peopleOpen && (
              <div style={furnitureFlyout}>
                <div style={furnitureFlyoutTitle}>Inserir pessoa</div>
                {getPeople().map((p) => (
                  <button
                    key={p._id}
                    type="button"
                    onClick={() => handlePickFurniture(p)}
                    style={furnitureItem}
                  >
                    <span>{p.name}</span>
                    <span
                      style={{ color: "var(--sicro-fg-dim)", fontSize: 10 }}
                    >
                      {p.width}×{p.height} m
                    </span>
                  </button>
                ))}
                <div style={furnitureHint}>
                  Selecione → clique na planta. Gire/mova no modo Selecionar.
                </div>
              </div>
            )}
          </div>

          {/* Trajetória balística — 2 cliques (origem → impacto) */}
          <button
            type="button"
            onClick={handleTrajetoria}
            title="Trajetória balística — clique origem → impacto (botão direito cancela)"
            style={{
              ...toolBtn,
              background: trajetoriaActive
                ? "var(--sicro-surface-2)"
                : "transparent",
              color: trajetoriaActive
                ? "var(--sicro-accent)"
                : "var(--sicro-fg-muted)",
              borderColor: trajetoriaActive
                ? "var(--sicro-border-strong)"
                : "transparent",
            }}
          >
            <Crosshair size={18} />
            <span style={{ fontSize: 9 }}>Trajetória</span>
          </button>

          {/* Texto livre — clique na planta abre o editor inline */}
          <button
            type="button"
            onClick={handleTexto}
            title="Texto livre — clique na planta para inserir um rótulo"
            style={{
              ...toolBtn,
              background: textMode ? "var(--sicro-surface-2)" : "transparent",
              color: textMode
                ? "var(--sicro-accent)"
                : "var(--sicro-fg-muted)",
              borderColor: textMode
                ? "var(--sicro-border-strong)"
                : "transparent",
            }}
          >
            <Type size={18} />
            <span style={{ fontSize: 9 }}>Texto</span>
          </button>

          {/* Estruturas — muro / cerca / calçada (2 cliques, em escala) */}
          <div style={{ position: "relative" }}>
            <button
              type="button"
              onClick={() => setStructOpen((v) => !v)}
              title="Estruturas — muro, cerca, calçada (clique início → fim)"
              style={{
                ...toolBtn,
                background:
                  structKind || structOpen
                    ? "var(--sicro-surface-2)"
                    : "transparent",
                color:
                  structKind || structOpen
                    ? "var(--sicro-accent)"
                    : "var(--sicro-fg-muted)",
                borderColor:
                  structKind || structOpen
                    ? "var(--sicro-border-strong)"
                    : "transparent",
              }}
            >
              <Fence size={18} />
              <span style={{ fontSize: 9 }}>Estrutura</span>
            </button>
            {structOpen && (
              <div style={furnitureFlyout}>
                <div style={furnitureFlyoutTitle}>Inserir estrutura</div>
                {(
                  [
                    ["muro", "Muro (alvenaria)"],
                    ["cerca_madeira", "Cerca de madeira"],
                    ["cerca_arame", "Cerca de arame"],
                    ["calcada", "Calçada"],
                  ] as [PlantaStructureKind, string][]
                ).map(([k, label]) => (
                  <button
                    key={k}
                    type="button"
                    onClick={() => handlePickStruct(k)}
                    style={furnitureItem}
                  >
                    <span>{label}</span>
                  </button>
                ))}
                <div style={furnitureHint}>
                  Clique no início e no fim. Botão direito cancela.
                </div>
              </div>
            )}
          </div>

          <div
            style={{
              height: 1,
              background: "var(--sicro-divider)",
              margin: "6px 4px",
            }}
            aria-hidden
          />

          {/* Desfazer / Refazer (Ctrl+Z / Ctrl+Y) */}
          <button
            type="button"
            onClick={undo}
            disabled={!canUndo}
            title="Desfazer (Ctrl+Z)"
            style={{ ...toolBtn, opacity: canUndo ? 1 : 0.4 }}
          >
            <Undo2 size={18} />
            <span style={{ fontSize: 9 }}>Desfazer</span>
          </button>
          <button
            type="button"
            onClick={redo}
            disabled={!canRedo}
            title="Refazer (Ctrl+Y)"
            style={{ ...toolBtn, opacity: canRedo ? 1 : 0.4 }}
          >
            <Redo2 size={18} />
            <span style={{ fontSize: 9 }}>Refazer</span>
          </button>

          <div
            style={{
              height: 1,
              background: "var(--sicro-divider)",
              margin: "6px 4px",
            }}
            aria-hidden
          />

          {/* Snap pra grade (0,5 m) — desliga pra posicionar livre */}
          <button
            type="button"
            onClick={() => setSnap(!snap)}
            title={snap ? "Snap à grade: ligado" : "Snap à grade: desligado"}
            style={{
              ...toolBtn,
              background: snap ? "var(--sicro-surface-2)" : "transparent",
              color: snap ? "var(--sicro-accent)" : "var(--sicro-fg-muted)",
              borderColor: snap ? "var(--sicro-border-strong)" : "transparent",
            }}
          >
            <Magnet size={18} />
            <span style={{ fontSize: 9 }}>Snap</span>
          </button>

          {/* Andares / pisos */}
          <div
            style={{
              display: "flex",
              flexDirection: "column",
              alignItems: "center",
              gap: 2,
              marginTop: 4,
            }}
          >
            <button
              type="button"
              onClick={() => changeFloor(1)}
              title="Andar acima (cria se não existir)"
              style={floorBtn}
            >
              <Plus size={14} />
            </button>
            <span
              style={{ fontSize: 9, color: "#334155", textAlign: "center" }}
              title="Andar/piso atual"
            >
              {floor === 0 ? "Térreo" : `${floor}º`}
            </span>
            <button
              type="button"
              onClick={() => changeFloor(-1)}
              title="Andar abaixo"
              disabled={floor <= 0}
              style={{ ...floorBtn, opacity: floor <= 0 ? 0.4 : 1 }}
            >
              <Minus size={14} />
            </button>
          </div>
        </div>

        <div
          style={{ flex: 1, position: "relative", minHeight: 0, overflow: "hidden" }}
        >
          <div ref={hostRef} style={{ position: "absolute", inset: 0 }} />

          {/* Painel Propriedades da parede selecionada (modo Selecionar). */}
          {selectedWall && (
            <div
              style={{
                position: "absolute",
                top: 12,
                right: 12,
                width: 236,
                background: "#ffffff",
                border: "1px solid rgba(0,0,0,0.12)",
                borderRadius: 10,
                boxShadow: "0 8px 28px rgba(0,0,0,0.20)",
                padding: 12,
                zIndex: 30,
                fontSize: 13,
                color: "#1a1a1a",
              }}
            >
              <div
                style={{
                  display: "flex",
                  justifyContent: "space-between",
                  alignItems: "center",
                  marginBottom: 8,
                }}
              >
                <strong>Propriedades da parede</strong>
                <button
                  type="button"
                  onClick={() => setSelectedWall(null)}
                  aria-label="Fechar"
                  style={{
                    border: "none",
                    background: "transparent",
                    cursor: "pointer",
                    fontSize: 18,
                    lineHeight: 1,
                    color: "#666",
                  }}
                >
                  ×
                </button>
              </div>
              <label
                style={{
                  display: "block",
                  fontSize: 12,
                  opacity: 0.7,
                  marginBottom: 4,
                }}
              >
                Tipo (skin)
              </label>
              <select
                value={selectedWall.kind}
                onChange={(e) =>
                  applyWallStyle(
                    e.target.value as PlantaStructureKind | "parede",
                  )
                }
                style={{
                  width: "100%",
                  padding: "6px 8px",
                  borderRadius: 6,
                  border: "1px solid rgba(0,0,0,0.2)",
                  background: "#fff",
                }}
              >
                <option value="parede">Parede</option>
                <option value="muro">Muro</option>
                <option value="cerca_madeira">Cerca de madeira</option>
                <option value="cerca_arame">Cerca de arame</option>
                <option value="calcada">Calçada</option>
              </select>
              <div style={{ marginTop: 8, fontSize: 12, opacity: 0.65 }}>
                Comprimento: {selectedWall.lengthM.toFixed(2)} m
              </div>
            </div>
          )}

          {/* Editor de texto livre inline (comita no blur; Esc cancela). */}
          {textEditor &&
            (() => {
              const commit = () => {
                if (textCancelRef.current) {
                  textCancelRef.current = false;
                  setTextEditor(null);
                  return;
                }
                const te = textEditor;
                const v = te.value.trim();
                setTextEditor(null);
                if (te.editId) {
                  setTexts((prev) =>
                    v
                      ? prev.map((t) =>
                          t.id === te.editId ? { ...t, text: v } : t,
                        )
                      : prev.filter((t) => t.id !== te.editId),
                  );
                } else if (v) {
                  const id =
                    (globalThis.crypto?.randomUUID?.() as string) ??
                    `tx_${Date.now()}`;
                  setTexts((prev) => [
                    ...prev,
                    { id, x: te.wx, y: te.wy, text: v, size: 28 },
                  ]);
                }
              };
              return (
                <input
                  ref={textInputRef}
                  key={`${textEditor.editId ?? "new"}@${Math.round(
                    textEditor.sx,
                  )},${Math.round(textEditor.sy)}`}
                  type="text"
                  value={textEditor.value}
                  placeholder="Texto…"
                  onChange={(e) =>
                    setTextEditor((te) =>
                      te ? { ...te, value: e.target.value } : te,
                    )
                  }
                  onKeyDown={(e) => {
                    if (e.key === "Enter") {
                      e.preventDefault();
                      e.currentTarget.blur();
                    } else if (e.key === "Escape") {
                      e.preventDefault();
                      textCancelRef.current = true;
                      e.currentTarget.blur();
                    }
                  }}
                  onBlur={commit}
                  style={{
                    position: "absolute",
                    left: textEditor.sx,
                    top: textEditor.sy,
                    transform: "translate(-50%, -50%)",
                    zIndex: 20,
                    minWidth: 90,
                    padding: "4px 8px",
                    border: "2px solid var(--sicro-accent)",
                    borderRadius: 4,
                    background: "#fff",
                    color: "#111827",
                    fontSize: 14,
                    fontFamily: "Arial, sans-serif",
                    outline: "none",
                    boxShadow: "0 2px 10px rgba(0,0,0,0.35)",
                  }}
                />
              );
            })()}
          {/* Caixa (invisível) p/ digitar a medida da parede ao clicar na cota,
              no modo Selecionar. O texto é transparente — o que aparece é o
              próprio rótulo do Pixi atualizando ao vivo. (port arcada PR #14) */}
          <input
            id="label-input"
            type="text"
            inputMode="decimal"
            onBlur={(e) => {
              // Estaciona o campo fora da tela ao fechar (ele é visível agora).
              e.currentTarget.style.pointerEvents = "none";
              e.currentTarget.style.left = "-9999px";
              e.currentTarget.style.top = "-9999px";
            }}
            onKeyDown={(e) => {
              if (e.key === "Enter" || e.key === "Escape") e.currentTarget.blur();
            }}
            style={labelInputStyle}
          />
          {/* Legenda dos vestígios (auto, pela ordem) + gerência (obs./remover) */}
          {evidences.length > 0 && (
            <div style={legendPanel}>
              <div style={legendHeader}>
                <span>Legenda — vestígios ({evidences.length})</span>
                <div style={{ display: "flex", gap: 4 }}>
                  <button
                    type="button"
                    onClick={() => setLabelKind("letra")}
                    title="Rótulos A, B, C…"
                    style={{
                      ...legendKindBtn,
                      ...(labelKind === "letra" ? legendKindActive : null),
                    }}
                  >
                    A,B
                  </button>
                  <button
                    type="button"
                    onClick={() => setLabelKind("numero")}
                    title="Rótulos 1, 2, 3…"
                    style={{
                      ...legendKindBtn,
                      ...(labelKind === "numero" ? legendKindActive : null),
                    }}
                  >
                    1,2
                  </button>
                </div>
              </div>
              <div style={legendList}>
                {evidences.map((ev, i) => {
                  const meta = evidenceMeta(ev.tipo);
                  const label = evidenceLabelFor(i + 1, labelKind);
                  return (
                    <div key={ev.id} style={legendRow}>
                      <span
                        style={{ ...legendBadge, background: ev.cor || meta.color }}
                      >
                        {label}
                      </span>
                      <div style={{ flex: 1, minWidth: 0 }}>
                        <div style={{ fontSize: 11, color: "var(--sicro-fg)" }}>
                          {meta.label}
                        </div>
                        <input
                          value={ev.descricao ?? ""}
                          onChange={(e) =>
                            updateEvidenceDesc(ev.id, e.target.value)
                          }
                          placeholder="observação (opcional)"
                          style={legendDescInput}
                        />
                      </div>
                      <button
                        type="button"
                        onClick={() => removeEvidence(ev.id)}
                        title="Remover vestígio"
                        style={legendRemove}
                      >
                        <X size={13} />
                      </button>
                    </div>
                  );
                })}
              </div>
            </div>
          )}

          {/* Controle de zoom flutuante (o scroll do mouse também dá zoom) */}
          <div
            style={{
              position: "absolute",
              right: 12,
              bottom: 12,
              display: "flex",
              flexDirection: "column",
              gap: 6,
              zIndex: 5,
            }}
          >
            <button type="button" onClick={zoomIn} title="Aproximar" style={zoomBtn}>
              <ZoomIn size={16} />
            </button>
            <button type="button" onClick={zoomOut} title="Afastar" style={zoomBtn}>
              <ZoomOut size={16} />
            </button>
            <button
              type="button"
              onClick={zoomReset}
              title="Zoom 100%"
              style={zoomBtn}
            >
              <Maximize size={16} />
            </button>
          </div>
        </div>
      </div>

      {/* Rodapé §13 */}
      <div
        style={{
          padding: "4px 12px",
          borderTop: "1px solid var(--sicro-divider)",
          background: "var(--sicro-surface-2)",
          fontSize: 11,
          color: "var(--sicro-fg-dim)",
        }}
      >
        Planta esquemática de apoio — posições e medidas conforme levantamento do
        perito. Botão direito: navegar/zoom. Escala: 1 m = 100 px.
      </div>
    </div>
  );
}

const iconBtn: React.CSSProperties = {
  display: "inline-flex",
  alignItems: "center",
  justifyContent: "center",
  width: 30,
  height: 30,
  border: "1px solid var(--sicro-border)",
  borderRadius: 6,
  background: "var(--sicro-surface-2)",
  color: "var(--sicro-fg-muted)",
  cursor: "pointer",
};

const primaryBtn: React.CSSProperties = {
  display: "inline-flex",
  alignItems: "center",
  gap: 6,
  padding: "6px 12px",
  border: "none",
  borderRadius: 6,
  background: "var(--sicro-accent)",
  color: "#1a1205",
  fontSize: 13,
  fontWeight: 600,
  cursor: "pointer",
};

const secondaryBtn: React.CSSProperties = {
  display: "inline-flex",
  alignItems: "center",
  gap: 6,
  padding: "6px 10px",
  border: "1px solid var(--sicro-border)",
  borderRadius: 6,
  background: "var(--sicro-surface-2)",
  color: "var(--sicro-fg-muted)",
  fontSize: 12,
  cursor: "pointer",
};

const toolBtn: React.CSSProperties = {
  display: "flex",
  flexDirection: "column",
  alignItems: "center",
  gap: 2,
  width: 56,
  padding: "8px 4px",
  border: "1px solid transparent",
  borderRadius: 8,
  background: "transparent",
  color: "var(--sicro-fg-muted)",
  cursor: "pointer",
};

const floorBtn: React.CSSProperties = {
  display: "inline-flex",
  alignItems: "center",
  justifyContent: "center",
  width: 28,
  height: 24,
  border: "1px solid var(--sicro-border)",
  borderRadius: 6,
  background: "var(--sicro-surface-2)",
  color: "var(--sicro-fg-muted)",
  cursor: "pointer",
};

const zoomBtn: React.CSSProperties = {
  display: "inline-flex",
  alignItems: "center",
  justifyContent: "center",
  width: 34,
  height: 34,
  border: "1px solid var(--sicro-border-strong)",
  borderRadius: 8,
  background: "var(--sicro-surface-1)",
  color: "var(--sicro-fg)",
  boxShadow: "0 1px 4px rgba(0,0,0,0.35)",
  cursor: "pointer",
};

// Caixa de digitação da cota (medida da parede). Texto transparente: só o caret
// aparece; o rótulo do Pixi muda ao vivo. Escala com o zoom via --viewport-zoom.
// Campo VISÍVEL de edição da cota, centrado sobre o rótulo. Antes era invisível
// (só caret) e tinha que casar pixel-a-pixel com o Text do Pixi — desalinhava.
// Agora é um campinho real centrado no ponto que o Label.onClick fornece
// (transform translate(-50%,-50%)), então o caret bate com os dígitos.
const labelInputStyle: React.CSSProperties = {
  position: "fixed",
  top: -9999,
  left: -9999,
  zIndex: 1000,
  pointerEvents: "none",
  width: 76,
  height: 24,
  fontSize: 14,
  fontFamily: "Arial, sans-serif",
  textAlign: "center",
  transform: "translate(-50%, -50%)",
  transformOrigin: "center center",
  color: "#1a1a1a",
  background: "#ffffff",
  caretColor: "#1a1a1a",
  border: "2px solid #d7a84f",
  borderRadius: 5,
  boxShadow: "0 2px 8px rgba(0,0,0,0.25)",
  outline: "none",
  padding: "1px 4px",
  margin: 0,
};

/** Distância (px de mundo) de um ponto ao segmento (x1,y1)-(x2,y2). */
function distPointToSegment(
  px: number,
  py: number,
  x1: number,
  y1: number,
  x2: number,
  y2: number,
): number {
  const dx = x2 - x1;
  const dy = y2 - y1;
  const len2 = dx * dx + dy * dy;
  if (len2 === 0) return Math.hypot(px - x1, py - y1);
  let t = ((px - x1) * dx + (py - y1) * dy) / len2;
  t = Math.max(0, Math.min(1, t));
  return Math.hypot(px - (x1 + t * dx), py - (y1 + t * dy));
}

const furnitureFlyout: React.CSSProperties = {
  position: "absolute",
  left: 60,
  top: 0,
  width: 210,
  maxHeight: 420,
  overflowY: "auto",
  background: "var(--sicro-surface-1)",
  border: "1px solid var(--sicro-border)",
  borderRadius: 8,
  boxShadow: "0 6px 20px rgba(0,0,0,0.4)",
  padding: 8,
  zIndex: 20,
};

const furnitureFlyoutTitle: React.CSSProperties = {
  fontSize: 11,
  fontWeight: 600,
  color: "var(--sicro-fg)",
  marginBottom: 6,
};

const furnitureCatTitle: React.CSSProperties = {
  fontSize: 9,
  textTransform: "uppercase",
  letterSpacing: 0.5,
  color: "var(--sicro-fg-dim)",
  margin: "4px 0 2px",
};

const furnitureItem: React.CSSProperties = {
  display: "flex",
  alignItems: "center",
  justifyContent: "space-between",
  width: "100%",
  gap: 8,
  padding: "5px 8px",
  border: "1px solid transparent",
  borderRadius: 6,
  background: "transparent",
  color: "var(--sicro-fg-muted)",
  fontSize: 12,
  cursor: "pointer",
  textAlign: "left",
};

const furnitureHint: React.CSSProperties = {
  marginTop: 6,
  paddingTop: 6,
  borderTop: "1px solid var(--sicro-divider)",
  fontSize: 10,
  color: "var(--sicro-fg-dim)",
  lineHeight: 1.3,
};

const legendPanel: React.CSSProperties = {
  position: "absolute",
  left: 12,
  bottom: 12,
  width: 280,
  maxHeight: "55%",
  display: "flex",
  flexDirection: "column",
  background: "var(--sicro-surface-1)",
  border: "1px solid var(--sicro-border)",
  borderRadius: 8,
  boxShadow: "0 6px 20px rgba(0,0,0,0.4)",
  zIndex: 6,
  overflow: "hidden",
};

const legendHeader: React.CSSProperties = {
  display: "flex",
  alignItems: "center",
  justifyContent: "space-between",
  gap: 8,
  padding: "8px 10px",
  borderBottom: "1px solid var(--sicro-divider)",
  fontSize: 11,
  fontWeight: 600,
  color: "var(--sicro-fg)",
};

const legendKindBtn: React.CSSProperties = {
  border: "1px solid var(--sicro-border)",
  borderRadius: 5,
  background: "transparent",
  color: "var(--sicro-fg-muted)",
  fontSize: 10,
  padding: "2px 6px",
  cursor: "pointer",
};

const legendKindActive: React.CSSProperties = {
  background: "var(--sicro-surface-2)",
  color: "var(--sicro-accent)",
  borderColor: "var(--sicro-border-strong)",
};

const legendList: React.CSSProperties = {
  overflowY: "auto",
  padding: 6,
  display: "flex",
  flexDirection: "column",
  gap: 4,
};

const legendRow: React.CSSProperties = {
  display: "flex",
  alignItems: "center",
  gap: 8,
  padding: "4px 4px",
};

const legendBadge: React.CSSProperties = {
  display: "inline-flex",
  alignItems: "center",
  justifyContent: "center",
  width: 22,
  height: 22,
  borderRadius: "50%",
  color: "#fff",
  fontSize: 11,
  fontWeight: 700,
  flexShrink: 0,
  border: "1.5px solid rgba(255,255,255,0.85)",
  boxShadow: "0 0 0 1px rgba(0,0,0,0.3)",
};

const legendDescInput: React.CSSProperties = {
  width: "100%",
  marginTop: 2,
  background: "var(--sicro-surface-2)",
  border: "1px solid var(--sicro-border)",
  borderRadius: 4,
  color: "var(--sicro-fg)",
  fontSize: 10,
  padding: "2px 5px",
  outline: "none",
};

const legendRemove: React.CSSProperties = {
  display: "inline-flex",
  alignItems: "center",
  justifyContent: "center",
  width: 22,
  height: 22,
  border: "none",
  borderRadius: 5,
  background: "transparent",
  color: "var(--sicro-fg-dim)",
  cursor: "pointer",
  flexShrink: 0,
};
