/**
 * CanvasStage — the Konva Stage + Layers + Shapes.
 *
 * Why React-Konva (vs SVG/Canvas/own engine): see
 * `docs/archive/SPIKE_E_CROQUI_ENGINE_RELATORIO.md` §1.
 *
 * Layer split (each Layer is a separate <canvas> under the hood):
 *   - bgLayer        — grid + background image
 *   - objectsLayer   — every SicroObject + the Transformer for the selection
 *   - uiLayer        — transient cues (pending two-click first point,
 *                      preview line during measurement, etc.)
 *
 * The Stage exposes the underlying Konva Stage via the `stageRef` so the
 * parent can call `toDataURL()` for the PNG export.
 */

import {
  forwardRef,
  useEffect,
  useImperativeHandle,
  useMemo,
  useRef,
  useState,
} from "react";
import {
  Circle,
  Ellipse,
  Group,
  Image as KonvaImage,
  Layer,
  Line,
  Rect,
  Stage,
  Text as KonvaText,
  Transformer,
} from "react-konva";
import type Konva from "konva";
import { convertFileSrc } from "@tauri-apps/api/core";
import {
  angleDeg,
  distancePx,
  formatMeasurement,
  midpoint,
  type SicroCroquiBackgroundImage,
  type SicroCroquiDoc,
  type SicroLineObject,
  type SicroMarkerObject,
  type SicroMeasurementObject,
  type SicroObject,
  type SicroPoint,
  type SicroTextObject,
  type SicroVehicleObject,
  type VehicleBodyType,
} from "../engine";
// Fase S clean cut — Road v1 (RoadNode) e Road v2 (RoadNetworkLayerV2,
// RoundaboutMeshNode) foram removidos. Vias e rotatórias são
// EXCLUSIVAMENTE renderizadas pelo `RoadParityRenderer` do Python
// Parity Engine.
import {
  RoadParityRenderer,
  isParityObject,
  type SicroParityObject,
} from "../engine/road-parity";
import {
  getCachedPessoaArtImage,
  getCachedVehicleArtImage,
  getPessoaArt,
  loadPessoaArtImage,
  loadVehicleArtImage,
} from "../engine/vehicleArt";
import {
  getObjectBoundsStagePx,
  rectFromPoints,
  rectsIntersect,
  translateObjectPatch,
} from "./bounds";
import type { EditorState, Tool } from "./useEditorState";

export interface CanvasStageHandle {
  /** Returns a PNG data URL of the current scene (incl. background + grid). */
  toPng(pixelRatio?: number): string | null;
  getStageSize(): { width: number; height: number };
}

/**
 * Limites de zoom do canvas do croqui. Generosos por design: peritos
 * trabalham com arte vetorial detalhada (manchas de sangue, pegadas,
 * fragmentos) e precisam de muito zoom de aproximação. Em cima a faixa
 * cobre desde visão geral de cena ampla (5 %) até trabalho pixel-perfect
 * em detalhes mínimos (10 000 % = 100×). Konva aguenta esse range sem
 * problemas — o stage é Canvas2D, então não há custo de "DOM zoom".
 */
export const CROQUI_ZOOM_MIN = 0.05;
export const CROQUI_ZOOM_MAX = 100;
/** Fator multiplicativo aplicado a cada "tick" do scroll wheel. */
export const CROQUI_ZOOM_WHEEL_FACTOR_IN = 1.08;
export const CROQUI_ZOOM_WHEEL_FACTOR_OUT = 0.92;

/**
 * Sentinel `selectedId` used to mean "the user has clicked the
 * background image and wants to edit it" (MVP 9 Round 5). Picking a
 * sentinel that can't collide with any UUID keeps the regular
 * selection model intact.
 */
export const BACKGROUND_SELECTION_ID = "_background";

interface Props {
  doc: SicroCroquiDoc;
  editor: EditorState;
  containerWidth: number;
  containerHeight: number;
  /** Called when the user clicks the canvas with an "add object" tool. */
  onCanvasClick: (worldPoint: SicroPoint) => void;
  /** Called when the user double-clicks the canvas (used to finish road drafts). */
  onCanvasDblClick?: () => void;
  /** Called when the user finishes a Konva drag/transform on an object. */
  onObjectChange: (id: string, patch: Partial<SicroObject>) => void;
  /**
   * Fase H.3 — handler dedicado para parity_objects (que vivem em
   * array separado de doc.objects). Opcional; quando ausente, handles
   * parity ficam estáticos (renderiza mas não dragga).
   */
  onParityObjectChange?: (
    id: string,
    patch: Partial<SicroParityObject>,
  ) => void;
  /**
   * Called when the user drags / transforms the background image
   * (MVP 9 Round 5). The patch carries the deltas: `x`, `y`, `width`,
   * `height`, `rotation` — whichever the gesture changed.
   */
  onBackgroundChange?: (patch: Partial<SicroCroquiBackgroundImage>) => void;
  onSelect: (id: string | null) => void;
  workspacePath: string;
}

export const CanvasStage = forwardRef<CanvasStageHandle, Props>(function CanvasStage(
  {
    doc,
    editor,
    containerWidth,
    containerHeight,
    onCanvasClick,
    onCanvasDblClick,
    onObjectChange,
    onParityObjectChange,
    onBackgroundChange,
    onSelect,
    workspacePath,
  },
  ref,
) {
  const stageRef = useRef<Konva.Stage | null>(null);
  const transformerRef = useRef<Konva.Transformer | null>(null);
  const objectsLayerRef = useRef<Konva.Layer | null>(null);
  // Flag pra evitar que o click logo após mouseUp do marquee limpe
  // a seleção recém-criada. Browser emite mousedown→mouseup→click como
  // 3 eventos separados; cancelBubble no mouseUp não impede o click.
  const justFinishedMarqueeRef = useRef(false);
  // Sessão de drag em grupo: quando o usuário arrasta um objeto que
  // está em `selectedIds` (com mais de 1 item), snapshotamos as posições
  // dos OUTROS aqui pra sincronizar visualmente durante o drag e
  // commitar via `onObjectChange` no final.
  const dragSessionRef = useRef<{
    draggedId: string;
    startX: number;
    startY: number;
    others: Array<{ id: string; startX: number; startY: number }>;
  } | null>(null);

  useImperativeHandle(ref, () => ({
    toPng(pixelRatio = 2) {
      const stage = stageRef.current;
      if (!stage) return null;
      return stage.toDataURL({ pixelRatio, mimeType: "image/png" });
    },
    getStageSize() {
      return {
        width: containerWidth,
        height: containerHeight,
      };
    },
  }));

  // Group drag — listeners de stage-level (eventos do Konva bubbleiam).
  // Quando o usuário arrasta um objeto que está em `selectedIds` E há
  // mais de 1 item selecionado, snapshotamos as posições dos OUTROS
  // no dragstart, sincronizamos visualmente durante dragmove (direto
  // no Konva, sem state), e commitamos via onObjectChange no dragend.
  useEffect(() => {
    const stage = stageRef.current;
    if (!stage) return;

    const dragStart = (e: Konva.KonvaEventObject<DragEvent>) => {
      const node = e.target;
      // O target é o Konva.Group/Shape do objeto. Pega o id.
      const id = typeof node.id === "function" ? node.id() : undefined;
      if (!id) return;
      if (!editor.selectedIds.includes(id)) return;
      if (editor.selectedIds.length <= 1) return; // single = drag normal

      const layer = objectsLayerRef.current;
      if (!layer) return;
      const others: Array<{ id: string; startX: number; startY: number }> = [];
      for (const sid of editor.selectedIds) {
        if (sid === id) continue;
        const other = layer.findOne(`#${sid}`);
        if (other) {
          others.push({ id: sid, startX: other.x(), startY: other.y() });
        }
      }
      dragSessionRef.current = {
        draggedId: id,
        startX: node.x(),
        startY: node.y(),
        others,
      };
    };

    const dragMove = (e: Konva.KonvaEventObject<DragEvent>) => {
      const session = dragSessionRef.current;
      if (!session) return;
      const node = e.target;
      const id = typeof node.id === "function" ? node.id() : undefined;
      if (id !== session.draggedId) return;
      const dx = node.x() - session.startX;
      const dy = node.y() - session.startY;
      const layer = objectsLayerRef.current;
      if (!layer) return;
      for (const o of session.others) {
        const other = layer.findOne(`#${o.id}`);
        if (other) {
          other.position({ x: o.startX + dx, y: o.startY + dy });
        }
      }
      layer.batchDraw();
    };

    const dragEnd = (e: Konva.KonvaEventObject<DragEvent>) => {
      const session = dragSessionRef.current;
      if (!session) return;
      const node = e.target;
      const id = typeof node.id === "function" ? node.id() : undefined;
      if (id !== session.draggedId) return;
      const dx = node.x() - session.startX;
      const dy = node.y() - session.startY;
      dragSessionRef.current = null;
      if (Math.abs(dx) < 0.001 && Math.abs(dy) < 0.001) return;

      // Commit cada um dos OUTROS via o callback apropriado (parity vs
      // não-parity). O dragged é commitado pelo handler dele próprio
      // (no renderer específico do tipo).
      const pxPerM = doc.scale?.px_per_m ?? 1;
      for (const o of session.others) {
        const obj = doc.objects.find((x) => x.id === o.id);
        if (!obj) continue;
        const patch = translateObjectPatch(obj, dx, dy, pxPerM);
        if (!patch) continue;
        if (isParityObject(obj)) {
          onParityObjectChange?.(o.id, patch as Partial<SicroParityObject>);
        } else {
          onObjectChange(o.id, patch);
        }
      }
    };

    // Namespace `.group` permite remover só esses handlers no cleanup
    // sem afetar outros listeners eventualmente atachados ao stage.
    stage.on("dragstart.group", dragStart);
    stage.on("dragmove.group", dragMove);
    stage.on("dragend.group", dragEnd);
    return () => {
      stage.off("dragstart.group");
      stage.off("dragmove.group");
      stage.off("dragend.group");
    };
  }, [editor.selectedIds, doc, onObjectChange, onParityObjectChange]);

  // Attach the Transformer to the currently-selected Konva node(s).
  // Multi-select via marquee: o Transformer aceita uma lista de nodes
  // e desenha uma bounding box ao redor de todos eles.
  useEffect(() => {
    const transformer = transformerRef.current;
    const layer = objectsLayerRef.current;
    if (!transformer || !layer) return;
    if (editor.selectedIds.length === 0) {
      transformer.nodes([]);
      transformer.getLayer()?.batchDraw();
      return;
    }
    const nodes: Konva.Node[] = [];
    for (const id of editor.selectedIds) {
      const node = layer.findOne(`#${id}`);
      if (node) nodes.push(node);
    }
    transformer.nodes(nodes);
    transformer.getLayer()?.batchDraw();
  }, [editor.selectedIds, doc.objects]);

  const handleStageMouseMove = () => {
    const stage = stageRef.current;
    if (!stage) return;
    const pos = stage.getPointerPosition();
    if (!pos) return;
    const world = toWorld(stage, pos);
    editor.setPointerWorld(world);
    // Marquee em curso: atualiza o canto "current" pra estender o
    // retângulo conforme o cursor.
    if (editor.marquee) {
      editor.setMarquee({
        ...editor.marquee,
        currentWorldX: world.x,
        currentWorldY: world.y,
      });
    }
  };

  // Marquee de seleção (rubber-band): no modo `select`, mousedown sobre
  // a área vazia da stage inicia o drag de um retângulo. mousemove
  // estende o retângulo (via handleStageMouseMove acima). mouseup
  // computa quais objetos caem dentro e seleciona todos via
  // `setSelectedIds`. Esc cancela.
  const handleStageMouseDown = (e: Konva.KonvaEventObject<MouseEvent>) => {
    // Sempre reseta a flag — novo gesto começa do zero.
    justFinishedMarqueeRef.current = false;
    if (editor.tool !== "select") return;
    // Só inicia marquee quando o clique foi no FUNDO (não em um objeto).
    if (e.target !== e.target.getStage()) return;
    // Botão esquerdo apenas. Botão direito reservado pra menu contextual
    // (que ainda não existe — futuro).
    if (e.evt.button !== 0) return;
    const stage = stageRef.current;
    if (!stage) return;
    const pos = stage.getPointerPosition();
    if (!pos) return;
    const world = toWorld(stage, pos);
    editor.setMarquee({
      startWorldX: world.x,
      startWorldY: world.y,
      currentWorldX: world.x,
      currentWorldY: world.y,
    });
  };

  const handleStageMouseUp = (e: Konva.KonvaEventObject<MouseEvent>) => {
    if (!editor.marquee) return;
    const m = editor.marquee;
    editor.setMarquee(null);
    // Se o usuário só clicou (sem arrastar — área < 9 px²), trata
    // como deseleção (clique no vazio = limpar).
    const rect = rectFromPoints(
      m.startWorldX,
      m.startWorldY,
      m.currentWorldX,
      m.currentWorldY,
    );
    const isTinyClick = rect.width * rect.height < 9;
    if (isTinyClick) {
      onSelect(null);
      return;
    }
    // Hit test: pega todos os objetos cujo AABB intersecta o retângulo.
    const pxPerM = doc.scale?.px_per_m ?? 1;
    const hits: string[] = [];
    for (const obj of doc.objects) {
      const b = getObjectBoundsStagePx(obj, pxPerM);
      if (rectsIntersect(b, rect)) hits.push(obj.id);
    }
    editor.setSelectedIds(hits);
    // CRÍTICO — o browser emite `click` LOGO DEPOIS de mouseup (são
    // eventos separados, cancelBubble do mouseup não afeta o click).
    // Sem essa flag, o handleStageClick rodaria em seguida e chamaria
    // onSelect(null), apagando a seleção recém-criada pelo marquee.
    // A flag é resetada no próximo mousedown.
    justFinishedMarqueeRef.current = true;
    e.cancelBubble = true;
  };

  const handleStageClick = (e: Konva.KonvaEventObject<MouseEvent>) => {
    // Se o usuário acabou de fazer um marquee com seleção, ignora o
    // `click` espúrio que vem logo depois do mouseup — senão o
    // onSelect(null) abaixo apagaria a seleção. A flag é resetada no
    // próximo mousedown (handleStageMouseDown).
    if (justFinishedMarqueeRef.current) {
      justFinishedMarqueeRef.current = false;
      return;
    }
    // Clicks that hit an interactive node bubble through Konva; we only care
    // about clicks on empty canvas here.
    if (e.target !== e.target.getStage()) {
      // Click on an object — leave it to the Group's own handler unless we're
      // in an "add" tool (in which case the user still wants to add).
      if (isAddTool(editor.tool)) {
        const stage = stageRef.current;
        if (!stage) return;
        const pos = stage.getPointerPosition();
        if (!pos) return;
        onCanvasClick(toWorld(stage, pos));
      }
      return;
    }
    const stage = stageRef.current;
    if (!stage) return;
    const pos = stage.getPointerPosition();
    if (!pos) return;
    const world = toWorld(stage, pos);
    if (editor.tool === "select") {
      onSelect(null);
      return;
    }
    if (editor.tool === "pan") {
      return;
    }
    onCanvasClick(world);
  };

  // Fase S clean cut — separa vias/rotatórias parity dos demais objetos
  // (vehicle/line/marker/text/measurement) para que cada grupo seja
  // renderizado pelo renderer apropriado. O `RoadParityRenderer` cuida
  // de TODAS as vias e rotatórias parity.
  const { parityObjects, otherObjects } = useMemo(() => {
    const parity: SicroParityObject[] = [];
    const others: SicroObject[] = [];
    for (const o of doc.objects) {
      if (isParityObject(o)) parity.push(o);
      else others.push(o);
    }
    return { parityObjects: parity, otherObjects: others };
  }, [doc.objects]);

  const handleWheel = (e: Konva.KonvaEventObject<WheelEvent>) => {
    e.evt.preventDefault();
    const stage = stageRef.current;
    if (!stage) return;
    const pointer = stage.getPointerPosition();
    if (!pointer) return;

    // Windows precision touchpad e mouse wheel geram `wheel` event. Pra
    // distinguir intenção:
    //   - `ctrlKey: true`  → pinch zoom (touchpad pinch OU Ctrl+roda do mouse)
    //   - `ctrlKey: false` → pan (touchpad scroll de 2 dedos)
    // Em macOS, `metaKey` (Cmd) tem o mesmo papel que Ctrl no Windows pra
    // zoom forçado, então tratamos ambos.
    const isPinch = e.evt.ctrlKey || e.evt.metaKey;

    if (isPinch) {
      // Zoom ancorado no ponto do gesto (mesma lógica de antes).
      const oldScale = editor.viewport.scale;
      const direction = e.evt.deltaY > 0 ? -1 : 1;
      const factor =
        direction > 0
          ? CROQUI_ZOOM_WHEEL_FACTOR_IN
          : CROQUI_ZOOM_WHEEL_FACTOR_OUT;
      const newScale = clamp(
        oldScale * factor,
        CROQUI_ZOOM_MIN,
        CROQUI_ZOOM_MAX,
      );
      const pointToWorld = {
        x: (pointer.x - editor.viewport.x) / oldScale,
        y: (pointer.y - editor.viewport.y) / oldScale,
      };
      editor.setViewport({
        scale: newScale,
        x: pointer.x - pointToWorld.x * newScale,
        y: pointer.y - pointToWorld.y * newScale,
      });
      return;
    }

    // Pan: arrasta a viewport pelo delta do scroll de 2 dedos. `deltaX` e
    // `deltaY` são em pixels (em Precision Touchpads). Negativo porque o
    // gesto natural é "dedos descem → conteúdo sobe" (viewport.y aumenta
    // pra mostrar conteúdo de cima).
    editor.setViewport({
      scale: editor.viewport.scale,
      x: editor.viewport.x - e.evt.deltaX,
      y: editor.viewport.y - e.evt.deltaY,
    });
  };

  return (
    <Stage
      ref={stageRef}
      width={containerWidth}
      height={containerHeight}
      x={editor.viewport.x}
      y={editor.viewport.y}
      scaleX={editor.viewport.scale}
      scaleY={editor.viewport.scale}
      draggable={editor.tool === "pan"}
      onClick={handleStageClick}
      onTap={handleStageClick}
      onDblClick={() => onCanvasDblClick?.()}
      onDblTap={() => onCanvasDblClick?.()}
      onMouseDown={handleStageMouseDown}
      onMouseMove={handleStageMouseMove}
      onMouseUp={handleStageMouseUp}
      onWheel={handleWheel}
      onDragEnd={(e) => {
        // Stage drag emits position changes — keep our state in sync so
        // wheel-zoom anchors correctly afterwards.
        if (editor.tool !== "pan") return;
        const stage = e.target;
        editor.setViewport({
          scale: stage.scaleX(),
          x: stage.x(),
          y: stage.y(),
        });
      }}
      style={{ background: "#a3a3a3" }}
    >
      <Layer listening={false}>
        <CanvasBackground doc={doc} />
      </Layer>
      <BackgroundImageLayer
        doc={doc}
        workspacePath={workspacePath}
        selected={editor.selectedId === BACKGROUND_SELECTION_ID}
        onSelect={() => onSelect(BACKGROUND_SELECTION_ID)}
        onChange={(patch) => onBackgroundChange?.(patch)}
      />

      <Layer ref={objectsLayerRef}>
        {/* Road Engine Pro — render roads first so subsequent objects
            (vehicles / markers / measurements) stack visually above the
            asphalt. Intersection patches sit between the two passes so
            they cover the road markings at crossings but don't hide the
            objects above. */}
        {/* Fase S — Python Parity Engine: único motor de vias e
            rotatórias. Renderiza primeiro (camada de "asfalto") para
            que demais objetos (veículos, vestígios, anotações)
            empilhem visualmente por cima. */}
        <RoadParityRenderer
          objects={parityObjects}
          pxPerM={doc.scale?.px_per_m ?? null}
          selectedId={editor.selectedId}
          onSelect={(id) => onSelect(id ?? null)}
          onObjectChange={
            onParityObjectChange
              ? (id, patch) => onParityObjectChange(id, patch)
              : undefined
          }
        />
        {/* Demais objetos (vehicle / line / marker / text / measurement)
            renderizam em cima das vias. */}
        {otherObjects.map((obj) => (
          <ObjectNode
            key={obj.id}
            obj={obj}
            doc={doc}
            tool={editor.tool}
            // `selectedIds.includes` mostra destaque individual pra TODOS
            // os itens marquee-selecionados, não só pro primeiro.
            selected={editor.selectedIds.includes(obj.id)}
            onSelect={() => onSelect(obj.id)}
            onChange={(patch) => onObjectChange(obj.id, patch)}
          />
        ))}
        <Transformer
          ref={transformerRef}
          rotateEnabled
          enabledAnchors={[
            "top-left",
            "top-right",
            "bottom-left",
            "bottom-right",
          ]}
          boundBoxFunc={(_old, next) => {
            // Reject negative size to avoid flips during scaling.
            if (Math.abs(next.width) < 4 || Math.abs(next.height) < 4) return _old;
            return next;
          }}
        />
        {/* Retângulo do marquee — só visível enquanto o usuário está
            arrastando no modo `select`. Coords em world (stage), então
            a stage já aplica viewport.scale/x/y automaticamente. */}
        {editor.marquee && (() => {
          const m = editor.marquee;
          const r = rectFromPoints(
            m.startWorldX,
            m.startWorldY,
            m.currentWorldX,
            m.currentWorldY,
          );
          // Stroke fino dividido pelo scale pra ficar com aparência
          // constante no zoom. Idem dash.
          const invScale = 1 / Math.max(editor.viewport.scale, 0.0001);
          return (
            <Rect
              x={r.x}
              y={r.y}
              width={r.width}
              height={r.height}
              fill="rgba(59, 130, 246, 0.12)"
              stroke="#3b82f6"
              strokeWidth={1 * invScale}
              dash={[4 * invScale, 3 * invScale]}
              listening={false}
            />
          );
        })()}
      </Layer>

      <Layer listening={false}>
        <PendingTwoClickPreview editor={editor} />
        <RoadDraftPreview editor={editor} />
      </Layer>
    </Stage>
  );
});

// ===========================================================================
// Background (color + grid) — drawn on its own non-listening layer.

function CanvasBackground({ doc }: { doc: SicroCroquiDoc }) {
  const { width_px, height_px, background_color, grid } = doc.canvas;
  const gridSize = grid?.size_px ?? 50;
  const gridEnabled = grid?.enabled ?? true;

  // Branco puro cansa a vista — substitui por off-white levemente
  // tingido. Croquis com cor customizada (não-branca) são respeitados
  // como vieram.
  const effectiveBg =
    background_color === "#ffffff" || background_color === "#FFFFFF"
      ? "#f5f6f8"
      : background_color;

  const lines = useMemo(() => {
    if (!gridEnabled) return [] as number[][];
    const out: number[][] = [];
    for (let x = 0; x <= width_px; x += gridSize) {
      out.push([x, 0, x, height_px]);
    }
    for (let y = 0; y <= height_px; y += gridSize) {
      out.push([0, y, width_px, y]);
    }
    return out;
  }, [width_px, height_px, gridSize, gridEnabled]);

  return (
    <>
      <Rect
        x={0}
        y={0}
        width={width_px}
        height={height_px}
        fill={effectiveBg}
        stroke="#525252"
        strokeWidth={2}
      />
      {lines.map((pts, i) => (
        <Line
          key={i}
          points={pts}
          stroke="#b8b8b8"
          strokeWidth={1}
          listening={false}
        />
      ))}
    </>
  );
}

// ===========================================================================
// Background image (separate layer so the user can toggle visibility).

function BackgroundImageLayer({
  doc,
  workspacePath,
  selected,
  onSelect,
  onChange,
}: {
  doc: SicroCroquiDoc;
  workspacePath: string;
  selected: boolean;
  onSelect: () => void;
  onChange: (patch: Partial<SicroCroquiBackgroundImage>) => void;
}) {
  const bg = doc.background_image;
  const [image, setImage] = useState<HTMLImageElement | null>(null);
  const groupRef = useRef<Konva.Group | null>(null);
  const transformerRef = useRef<Konva.Transformer | null>(null);

  useEffect(() => {
    if (!bg) {
      setImage(null);
      return;
    }
    const path = resolveAssetPath(workspacePath, bg.source_path);
    if (!path) return;
    const img = new window.Image();
    img.crossOrigin = "anonymous";
    img.src = path;
    img.onload = () => setImage(img);
    img.onerror = () => setImage(null);
  }, [bg, workspacePath]);

  // Attach the Transformer when the background is selected AND unlocked.
  // We always rebind even if the bg.locked flag flips, so the user gets
  // visual feedback immediately when they toggle the lock in the toolbar.
  useEffect(() => {
    const transformer = transformerRef.current;
    const node = groupRef.current;
    if (!transformer) return;
    if (!selected || !node || !bg || bg.locked) {
      transformer.nodes([]);
      transformer.getLayer()?.batchDraw();
      return;
    }
    transformer.nodes([node]);
    transformer.getLayer()?.batchDraw();
  }, [selected, bg]);

  if (!bg || !image) {
    return <Layer listening={false} />;
  }

  // Resolve display dimensions: when the doc was saved without a
  // size (legacy 0/0), fall back to the image's natural dimensions.
  // Future drag/transform commits the resolved values back to the doc.
  const resolvedW = bg.width || image.width;
  const resolvedH = bg.height || image.height;
  const rotation = bg.rotation ?? 0;

  // The Group's origin lives at the image's centre so rotation rotates
  // around the geometric centre (the natural pivot). The internal
  // KonvaImage is positioned at (-w/2, -h/2) inside the Group so it
  // still occupies the same screen rectangle.
  return (
    <Layer listening={!bg.locked}>
      <Group
        ref={groupRef}
        id={BACKGROUND_SELECTION_ID}
        x={bg.x + resolvedW / 2}
        y={bg.y + resolvedH / 2}
        rotation={rotation}
        draggable={!bg.locked && selected}
        onClick={(e) => {
          // Stop the Stage's onClick from firing an `onSelect(null)`.
          e.cancelBubble = true;
          onSelect();
        }}
        onTap={(e) => {
          e.cancelBubble = true;
          onSelect();
        }}
        onDragEnd={(e) => {
          const cx = e.target.x();
          const cy = e.target.y();
          onChange({
            x: cx - resolvedW / 2,
            y: cy - resolvedH / 2,
          });
        }}
        onTransformEnd={(e) => {
          const node = e.target;
          const sx = node.scaleX();
          const sy = node.scaleY();
          node.scaleX(1);
          node.scaleY(1);
          const newW = Math.max(20, resolvedW * sx);
          const newH = Math.max(20, resolvedH * sy);
          const newRot = node.rotation();
          onChange({
            x: node.x() - newW / 2,
            y: node.y() - newH / 2,
            width: newW,
            height: newH,
            rotation: newRot,
          });
        }}
      >
        <KonvaImage
          image={image}
          x={-resolvedW / 2}
          y={-resolvedH / 2}
          width={resolvedW}
          height={resolvedH}
          opacity={bg.opacity}
        />
      </Group>
      <Transformer
        ref={transformerRef}
        rotateEnabled
        enabledAnchors={[
          "top-left",
          "top-right",
          "bottom-left",
          "bottom-right",
          "middle-left",
          "middle-right",
          "top-center",
          "bottom-center",
        ]}
        boundBoxFunc={(_old, next) => {
          // Refuse tiny boxes so the image can't be scaled to invisibility.
          if (Math.abs(next.width) < 20 || Math.abs(next.height) < 20) return _old;
          return next;
        }}
      />
    </Layer>
  );
}

// ===========================================================================
// Per-object rendering

function ObjectNode({
  obj,
  doc,
  tool,
  selected,
  onSelect,
  onChange,
}: {
  obj: SicroObject;
  doc: SicroCroquiDoc;
  tool: Tool;
  selected: boolean;
  onSelect: () => void;
  onChange: (patch: Partial<SicroObject>) => void;
}) {
  // Only allow drag when the "select" tool is active.
  const draggable = tool === "select";

  switch (obj.kind) {
    case "vehicle":
      return (
        <VehicleNode
          obj={obj}
          draggable={draggable}
          selected={selected}
          onSelect={onSelect}
          onChange={onChange}
        />
      );
    case "line":
      return (
        <LineNode
          obj={obj}
          draggable={draggable}
          selected={selected}
          onSelect={onSelect}
          onChange={onChange}
        />
      );
    case "marker":
      return (
        <MarkerNode
          obj={obj}
          draggable={draggable}
          selected={selected}
          onSelect={onSelect}
          onChange={onChange}
        />
      );
    case "text":
      return (
        <TextNode
          obj={obj}
          draggable={draggable}
          selected={selected}
          onSelect={onSelect}
          onChange={onChange}
        />
      );
    case "measurement":
      return (
        <MeasurementNode
          obj={obj}
          doc={doc}
          draggable={draggable}
          selected={selected}
          onSelect={onSelect}
          onChange={onChange}
        />
      );
    case "road_parity":
    case "roundabout_parity":
      // Fase S — vias e rotatórias parity NÃO passam pelo ObjectNode:
      // são renderizadas em camada separada pelo `RoadParityRenderer`.
      // Defensivo: se algum caller passar por aqui, não renderiza nada
      // (o renderer parity já cuidou).
      return null;
  }
}

/**
 * Arte SVG do designer pro par (tipo, cor) — carrega 1× (cache no engine) e
 * devolve a HTMLImageElement pronta pro Konva. `null` = sem arte (tipo sem SVG
 * ou ainda carregando) → o caller cai na silhueta vetorial antiga.
 */
function useVehicleArtImage(
  body: VehicleBodyType,
  color: string | null | undefined,
): HTMLImageElement | null {
  const [img, setImg] = useState<HTMLImageElement | null>(() =>
    getCachedVehicleArtImage(body, color),
  );
  useEffect(() => {
    let alive = true;
    const cached = getCachedVehicleArtImage(body, color);
    if (cached) {
      setImg(cached);
      return;
    }
    setImg(null);
    void loadVehicleArtImage(body, color).then((loaded) => {
      if (alive) setImg(loaded);
    });
    return () => {
      alive = false;
    };
  }, [body, color]);
  return img;
}

function VehicleNode({
  obj,
  draggable,
  selected,
  onSelect,
  onChange,
}: {
  obj: SicroVehicleObject;
  draggable: boolean;
  selected: boolean;
  onSelect: () => void;
  onChange: (patch: Partial<SicroObject>) => void;
}) {
  const body: VehicleBodyType = obj.body_type ?? "car";
  const isTwoWheel = body === "moto" || body === "bike";
  const art = useVehicleArtImage(body, obj.color);
  return (
    <Group
      id={obj.id}
      x={obj.x}
      y={obj.y}
      rotation={obj.rotation}
      draggable={draggable && !obj.locked}
      onClick={onSelect}
      onTap={onSelect}
      onDragEnd={(e) =>
        onChange({ x: e.target.x(), y: e.target.y() } as Partial<SicroObject>)
      }
      onTransformEnd={(e) => {
        const node = e.target;
        const scaleX = node.scaleX();
        const scaleY = node.scaleY();
        node.scaleX(1);
        node.scaleY(1);
        onChange({
          x: node.x(),
          y: node.y(),
          rotation: node.rotation(),
          width: Math.max(8, obj.width * scaleX),
          height: Math.max(6, obj.height * scaleY),
        } as Partial<SicroObject>);
      }}
    >
      {art ? (
        <>
          {/* Área de clique cheia (o hit do Konva.Image ignora pixels
              transparentes da arte — sem isto, selecionar exigiria acertar
              um pixel pintado). Invisível, mas presente no hit canvas. */}
          <Rect
            x={-obj.width / 2}
            y={-obj.height / 2}
            width={obj.width}
            height={obj.height}
            fill="#000"
            opacity={0}
          />
          {/* Arte do designer: SVG retrato (frente pra cima) girado 90° pra
              frente apontar +x (convenção do croqui). width/height trocados
              por causa do giro; origem no centro do Group. */}
          <KonvaImage
            image={art}
            width={obj.height}
            height={obj.width}
            offsetX={obj.height / 2}
            offsetY={obj.width / 2}
            rotation={90}
            listening={false}
          />
          {selected && (
            <Rect
              x={-obj.width / 2}
              y={-obj.height / 2}
              width={obj.width}
              height={obj.height}
              stroke="#38bdf8"
              strokeWidth={1.5}
              dash={[6, 4]}
              listening={false}
            />
          )}
        </>
      ) : (
        <VehicleSilhouette
          body={body}
          width={obj.width}
          height={obj.height}
          color={obj.color ?? "#3b82f6"}
          selected={selected}
        />
      )}
      {/* "Frente" do veículo — pequeno triângulo apontando para +x (a arte do
          designer já mostra a frente; o triângulo fica só no fallback). */}
      {!art && !isTwoWheel && (
        <Line
          points={[
            obj.width / 2,
            0,
            obj.width / 2 - 8,
            -obj.height / 2 - 6,
            obj.width / 2 + 8,
            -obj.height / 2 - 6,
          ]}
          closed
          fill={obj.color ?? "#3b82f6"}
          opacity={0.5}
          listening={false}
        />
      )}
      {obj.label && (
        <KonvaText
          text={obj.label}
          fontSize={isTwoWheel ? 10 : 12}
          fontStyle="bold"
          fill="#ffffff"
          width={obj.width}
          align="center"
          y={-6}
          offsetX={obj.width / 2}
          listening={false}
        />
      )}
    </Group>
  );
}

/**
 * Vector silhouettes for each vehicle body subtype (MVP 6). Drawn as
 * primitives so we don't ship raster assets and the canvas stays
 * resolution-independent. Top-down orientation: +x = "front".
 */
function VehicleSilhouette({
  body,
  width,
  height,
  color,
  selected,
}: {
  body: VehicleBodyType;
  width: number;
  height: number;
  color: string;
  selected: boolean;
}) {
  const stroke = selected ? "#0ea5e9" : "#1e3a8a";
  const strokeWidth = selected ? 2 : 1.5;

  // ---- Motos / bicicletas ----
  if (body === "moto" || body === "bike" || body === "moto_esportiva") {
    const wheelR = height * 0.45;
    return (
      <Group>
        <Rect
          x={-width / 2}
          y={-height / 2}
          width={width}
          height={height}
          fill={color}
          stroke={stroke}
          strokeWidth={strokeWidth}
          cornerRadius={Math.min(width, height) / 2}
        />
        <Circle x={-width / 2 + wheelR} y={0} radius={wheelR * 0.4} fill="#111827" />
        <Circle x={width / 2 - wheelR} y={0} radius={wheelR * 0.4} fill="#111827" />
        {/* "carenagem" para esportiva */}
        {body === "moto_esportiva" && (
          <Line
            points={[width / 2 - 4, -height / 4, width / 2, 0, width / 2 - 4, height / 4]}
            stroke="#ffffff"
            strokeWidth={1.5}
            listening={false}
          />
        )}
      </Group>
    );
  }

  // ---- Moto carga (com bagageiro/triciclo) ----
  if (body === "moto_carga") {
    return (
      <Group>
        {/* baú traseiro */}
        <Rect
          x={-width / 2}
          y={-height / 2}
          width={width * 0.55}
          height={height}
          fill="#92400e"
          stroke={stroke}
          strokeWidth={strokeWidth}
          cornerRadius={3}
        />
        {/* corpo da moto */}
        <Rect
          x={-width * 0.05}
          y={-height * 0.35}
          width={width * 0.55}
          height={height * 0.7}
          fill={color}
          stroke={stroke}
          strokeWidth={strokeWidth}
          cornerRadius={6}
        />
      </Group>
    );
  }

  // ---- Caminhão / Caminhão pesado ----
  if (body === "truck" || body === "caminhao" || body === "caminhao_pesado") {
    const cabinW = width * (body === "caminhao_pesado" ? 0.25 : 0.32);
    return (
      <Group>
        <Rect
          x={-width / 2}
          y={-height / 2}
          width={width - cabinW}
          height={height}
          fill="#52525b"
          stroke={stroke}
          strokeWidth={strokeWidth}
          cornerRadius={3}
        />
        <Rect
          x={width / 2 - cabinW}
          y={-height / 2}
          width={cabinW}
          height={height}
          fill={color}
          stroke={stroke}
          strokeWidth={strokeWidth}
          cornerRadius={3}
        />
      </Group>
    );
  }

  // ---- Carreta (cavalo + semi-reboque, com gap visível) ----
  if (body === "carreta") {
    const cabinW = width * 0.16;
    const gap = width * 0.02;
    return (
      <Group>
        {/* semi-reboque */}
        <Rect
          x={-width / 2}
          y={-height / 2}
          width={width - cabinW - gap}
          height={height}
          fill="#451a03"
          stroke={stroke}
          strokeWidth={strokeWidth}
          cornerRadius={3}
        />
        {/* cabine */}
        <Rect
          x={width / 2 - cabinW}
          y={-height / 2}
          width={cabinW}
          height={height}
          fill={color}
          stroke={stroke}
          strokeWidth={strokeWidth}
          cornerRadius={3}
        />
        {/* engate visual */}
        <Line
          points={[width / 2 - cabinW, 0, width / 2 - cabinW - gap, 0]}
          stroke="#111827"
          strokeWidth={2}
          listening={false}
        />
      </Group>
    );
  }

  // ---- Ônibus (corpo único + janelas equiespaçadas) ----
  if (body === "onibus") {
    const windowCount = 6;
    const windowGap = width / (windowCount + 1);
    return (
      <Group>
        <Rect
          x={-width / 2}
          y={-height / 2}
          width={width}
          height={height}
          fill={color}
          stroke={stroke}
          strokeWidth={strokeWidth}
          cornerRadius={6}
        />
        {Array.from({ length: windowCount }, (_, i) => (
          <Rect
            key={i}
            x={-width / 2 + windowGap * (i + 0.6)}
            y={-height * 0.3}
            width={windowGap * 0.5}
            height={height * 0.6}
            fill="rgba(255,255,255,0.36)"
            listening={false}
          />
        ))}
      </Group>
    );
  }

  // ---- Van (corpo retangular alto, frente curta) ----
  if (body === "van") {
    return (
      <Group>
        <Rect
          x={-width / 2}
          y={-height / 2}
          width={width}
          height={height}
          fill={color}
          stroke={stroke}
          strokeWidth={strokeWidth}
          cornerRadius={4}
        />
        {/* janela frontal */}
        <Rect
          x={width / 2 - width * 0.16}
          y={-height * 0.32}
          width={width * 0.13}
          height={height * 0.64}
          fill="rgba(255,255,255,0.4)"
          listening={false}
        />
        {/* porta lateral */}
        <Line
          points={[0, -height / 2 + 2, 0, height / 2 - 2]}
          stroke="#ffffff"
          strokeWidth={1}
          dash={[3, 3]}
          listening={false}
        />
      </Group>
    );
  }

  // ---- Pickup (corpo + caçamba traseira visível) ----
  if (body === "pickup") {
    return (
      <Group>
        <Rect
          x={-width / 2}
          y={-height / 2}
          width={width}
          height={height}
          fill={color}
          stroke={stroke}
          strokeWidth={strokeWidth}
          cornerRadius={5}
        />
        {/* caçamba (terço traseiro mais escuro) */}
        <Rect
          x={-width / 2 + 2}
          y={-height / 2 + 3}
          width={width * 0.4}
          height={height - 6}
          fill="rgba(0,0,0,0.18)"
          listening={false}
        />
        {/* cabine */}
        <Rect
          x={-width / 2 + width * 0.4}
          y={-height / 2 + height * 0.15}
          width={width * 0.36}
          height={height * 0.7}
          fill="rgba(255,255,255,0.32)"
          listening={false}
        />
      </Group>
    );
  }

  // ---- sedan / suv / hatch / car / other — fallback do MVP 6 ----
  const radius = body === "sedan" ? 6 : body === "hatch" ? 8 : 5;
  return (
    <Group>
      <Rect
        x={-width / 2}
        y={-height / 2}
        width={width}
        height={height}
        fill={color}
        stroke={stroke}
        strokeWidth={strokeWidth}
        cornerRadius={radius}
      />
      {/* Teto (vidros) — retângulo interno. */}
      <Rect
        x={-width / 2 + width * 0.18}
        y={-height / 2 + height * 0.18}
        width={width * 0.5}
        height={height * 0.64}
        fill="rgba(255,255,255,0.32)"
        listening={false}
      />
    </Group>
  );
}

function LineNode({
  obj,
  draggable,
  selected,
  onSelect,
  onChange,
}: {
  obj: SicroLineObject;
  draggable: boolean;
  selected: boolean;
  onSelect: () => void;
  onChange: (patch: Partial<SicroObject>) => void;
}) {
  const isR = obj.subtype === "r1" || obj.subtype === "r2";
  const labelFontSize = isR ? 14 : 12;
  return (
    <Group
      id={obj.id}
      draggable={draggable && !obj.locked}
      onClick={onSelect}
      onTap={onSelect}
      onDragEnd={(e) => {
        // Translate the entire polyline by the drag offset.
        const dx = e.target.x();
        const dy = e.target.y();
        const next = obj.points.map((v, i) => v + (i % 2 === 0 ? dx : dy));
        e.target.position({ x: 0, y: 0 });
        onChange({ points: next } as Partial<SicroObject>);
      }}
    >
      <Line
        points={obj.points}
        stroke={obj.color ?? "#1f2937"}
        strokeWidth={obj.stroke_width}
        dash={obj.dashed ? [12, 6] : undefined}
        lineCap="round"
        opacity={selected ? 1 : 0.95}
        hitStrokeWidth={Math.max(obj.stroke_width, 12)}
      />
      {obj.subtype === "arrow" && obj.points.length >= 4 && (
        <ArrowHead
          x1={obj.points[obj.points.length - 4]!}
          y1={obj.points[obj.points.length - 3]!}
          x2={obj.points[obj.points.length - 2]!}
          y2={obj.points[obj.points.length - 1]!}
          color={obj.color ?? "#111827"}
          size={Math.max(obj.stroke_width * 4, 12)}
        />
      )}
      {obj.label && (
        <KonvaText
          text={obj.label}
          fontSize={labelFontSize}
          fontStyle="bold"
          fill={obj.color ?? "#1f2937"}
          x={obj.points[0] ?? 0}
          y={(obj.points[1] ?? 0) - 18}
          listening={false}
        />
      )}
    </Group>
  );
}
function ArrowHead({
  x1,
  y1,
  x2,
  y2,
  color,
  size,
}: {
  x1: number;
  y1: number;
  x2: number;
  y2: number;
  color: string;
  size: number;
}) {
  const dx = x2 - x1;
  const dy = y2 - y1;
  const len = Math.sqrt(dx * dx + dy * dy);
  if (len === 0) return null;
  const ux = dx / len;
  const uy = dy / len;
  // 30° on each side of the tip.
  const rad = (30 * Math.PI) / 180;
  const cos = Math.cos(rad);
  const sin = Math.sin(rad);
  const back1x = x2 - size * (ux * cos - uy * sin);
  const back1y = y2 - size * (uy * cos + ux * sin);
  const back2x = x2 - size * (ux * cos + uy * sin);
  const back2y = y2 - size * (uy * cos - ux * sin);
  return (
    <Line
      points={[x2, y2, back1x, back1y, back2x, back2y]}
      closed
      fill={color}
      stroke={color}
      strokeWidth={1}
      listening={false}
    />
  );
}

/** Arte de pedestre (decúbito) pro subtype — mesmo cache do engine. */
function usePessoaArtImage(subtype: string): HTMLImageElement | null {
  const [img, setImg] = useState<HTMLImageElement | null>(() =>
    getCachedPessoaArtImage(subtype),
  );
  useEffect(() => {
    let alive = true;
    const cached = getCachedPessoaArtImage(subtype);
    if (cached) {
      setImg(cached);
      return;
    }
    setImg(null);
    void loadPessoaArtImage(subtype).then((loaded) => {
      if (alive) setImg(loaded);
    });
    return () => {
      alive = false;
    };
  }, [subtype]);
  return img;
}

function MarkerNode({
  obj,
  draggable,
  selected,
  onSelect,
  onChange,
}: {
  obj: SicroMarkerObject;
  draggable: boolean;
  selected: boolean;
  onSelect: () => void;
  onChange: (patch: Partial<SicroObject>) => void;
}) {
  const pessoa = getPessoaArt(obj.subtype);
  const pessoaImg = usePessoaArtImage(obj.subtype);
  // Pedestre com arte: size = COMPRIMENTO (corpo deitado); largura segue a
  // proporção real da prancha. Retrato (sem giro — a rotação é do marker).
  const pessoaW = pessoa ? obj.size * (pessoa.widthM / pessoa.lengthM) : 0;
  return (
    <Group
      id={obj.id}
      x={obj.x}
      y={obj.y}
      rotation={obj.rotation ?? 0}
      draggable={draggable && !obj.locked}
      onClick={onSelect}
      onTap={onSelect}
      onDragEnd={(e) =>
        onChange({ x: e.target.x(), y: e.target.y() } as Partial<SicroObject>)
      }
    >
      {pessoa && pessoaImg ? (
        <>
          {/* Hit cheio invisível (o hit do Image ignora pixels transparentes). */}
          <Rect
            x={-pessoaW / 2}
            y={-obj.size / 2}
            width={pessoaW}
            height={obj.size}
            fill="#000"
            opacity={0}
          />
          <KonvaImage
            image={pessoaImg}
            width={pessoaW}
            height={obj.size}
            offsetX={pessoaW / 2}
            offsetY={obj.size / 2}
            listening={false}
          />
        </>
      ) : (
        <MarkerGlyph obj={obj} selected={selected} />
      )}
      {obj.label && (
        <KonvaText
          text={obj.label}
          fontSize={11}
          fontStyle="bold"
          fill={obj.color ?? "#1f2937"}
          x={obj.size / 2 + 6}
          y={-obj.size / 2}
          listening={false}
        />
      )}
      {selected && (
        <Rect
          x={-obj.size / 2 - 4}
          y={-obj.size / 2 - 4}
          width={obj.size + 8}
          height={obj.size + 8}
          stroke="#0ea5e9"
          strokeWidth={1}
          dash={[4, 3]}
          listening={false}
        />
      )}
    </Group>
  );
}

/**
 * Per-subtype glyph for `marker` objects. Drawn centred at (0,0) — the
 * parent `Group` handles position/rotation/drag.
 */
function MarkerGlyph({
  obj,
  selected,
}: {
  obj: SicroMarkerObject;
  selected: boolean;
}) {
  const size = obj.size;
  const color = obj.color ?? "#1f2937";
  const subtype = obj.subtype;

  if (subtype === "collision_x") {
    return (
      <Group>
        <Line
          points={[-size / 2, -size / 2, size / 2, size / 2]}
          stroke={color}
          strokeWidth={3}
        />
        <Line
          points={[-size / 2, size / 2, size / 2, -size / 2]}
          stroke={color}
          strokeWidth={3}
        />
      </Group>
    );
  }

  if (subtype === "brake_mark" || subtype === "drag_mark") {
    // Faixa retangular tracejada (frenagem) ou listras curtas (arrasto).
    const dash = subtype === "brake_mark" ? [16, 8] : [4, 6];
    return (
      <Group>
        <Line
          points={[-size / 2, -3, size / 2, -3]}
          stroke={color}
          strokeWidth={4}
          dash={dash}
          lineCap="butt"
        />
        <Line
          points={[-size / 2, 3, size / 2, 3]}
          stroke={color}
          strokeWidth={4}
          dash={dash}
          lineCap="butt"
        />
      </Group>
    );
  }

  if (subtype === "fluid" || subtype === "blood") {
    // Mancha — elipse irregular com cor saturada e contorno suave.
    return (
      <Group>
        <Ellipse
          radiusX={size * 0.55}
          radiusY={size * 0.42}
          rotation={20}
          fill={color}
          opacity={0.55}
          stroke={color}
          strokeWidth={1}
        />
        <Ellipse
          x={size * 0.18}
          y={-size * 0.12}
          radiusX={size * 0.18}
          radiusY={size * 0.14}
          fill={color}
          opacity={0.65}
        />
      </Group>
    );
  }

  if (subtype === "debris") {
    // Cluster de triângulos pequenos.
    const tri = (cx: number, cy: number, s: number) => (
      <Line
        key={`${cx},${cy}`}
        points={[cx, cy - s, cx + s, cy + s, cx - s, cy + s]}
        closed
        fill={color}
        opacity={0.7}
      />
    );
    return (
      <Group>
        {tri(-size * 0.25, -size * 0.05, 4)}
        {tri(size * 0.12, size * 0.15, 5)}
        {tri(size * 0.32, -size * 0.18, 3.5)}
        {tri(-size * 0.05, size * 0.3, 3)}
      </Group>
    );
  }

  if (subtype === "pedestrian") {
    // Cabeça + corpo simples vista de cima.
    return (
      <Group>
        <Circle radius={size * 0.32} fill={color} />
        <Rect
          x={-size * 0.18}
          y={size * 0.15}
          width={size * 0.36}
          height={size * 0.5}
          fill={color}
          cornerRadius={3}
        />
      </Group>
    );
  }

  if (subtype === "body") {
    // Vítima em decúbito — corpo elíptico horizontal.
    return (
      <Group>
        <Ellipse
          radiusX={size * 0.65}
          radiusY={size * 0.28}
          fill={color}
          opacity={0.85}
        />
        <Circle x={-size * 0.55} radius={size * 0.18} fill={color} />
      </Group>
    );
  }

  if (subtype === "victim_point" || subtype === "trace_point") {
    return (
      <Circle
        radius={size / 2}
        fill={color}
        stroke={selected ? "#0ea5e9" : "transparent"}
        strokeWidth={2}
      />
    );
  }

  // ---------------------------------------------------------------------
  // MVP 9 — vestígios + mobiliário urbano

  if (subtype === "skid_curve") {
    // Derrapagem em curva — arco tracejado.
    const pts: number[] = [];
    const steps = 12;
    for (let i = 0; i <= steps; i++) {
      const t = i / steps;
      const a = Math.PI * t - Math.PI / 2; // -90° a +90°
      pts.push(Math.cos(a) * size * 0.6, Math.sin(a) * size * 0.45);
    }
    return (
      <Group>
        <Line
          points={pts}
          stroke={color}
          strokeWidth={4}
          dash={[10, 6]}
          lineCap="round"
        />
        <Line
          points={pts.map((v, i) => (i % 2 === 1 ? v + 6 : v))}
          stroke={color}
          strokeWidth={4}
          dash={[10, 6]}
          opacity={0.6}
          lineCap="round"
        />
      </Group>
    );
  }

  if (subtype === "sulcagem" || subtype === "ranhura") {
    // Sulcos profundos / ranhuras paralelas no pavimento.
    const strokes = subtype === "sulcagem" ? 3 : 2;
    return (
      <Group>
        {Array.from({ length: strokes }, (_, i) => {
          const y = (i - (strokes - 1) / 2) * 4;
          return (
            <Line
              key={i}
              points={[-size / 2, y, size / 2, y]}
              stroke={color}
              strokeWidth={subtype === "sulcagem" ? 3 : 2}
              dash={[6, 3]}
              lineCap="butt"
            />
          );
        })}
      </Group>
    );
  }

  if (subtype === "impact_area") {
    // Área de impacto — polígono irregular semi-transparente.
    const pts: number[] = [];
    const points = 8;
    for (let i = 0; i < points; i++) {
      const a = (i / points) * Math.PI * 2 - Math.PI / 2;
      const r = (size / 2) * (0.7 + 0.3 * Math.sin(i * 1.7));
      pts.push(Math.cos(a) * r, Math.sin(a) * r);
    }
    return (
      <Line
        points={pts}
        closed
        fill={color}
        opacity={0.25}
        stroke={color}
        strokeWidth={1.5}
        dash={[6, 4]}
      />
    );
  }

  if (subtype === "rest_position") {
    // Ponto de repouso — losango com "P".
    const s = size * 0.6;
    return (
      <Group>
        <Line
          points={[0, -s, s, 0, 0, s, -s, 0]}
          closed
          fill="#ffffff"
          stroke={color}
          strokeWidth={2}
        />
      </Group>
    );
  }

  // ----- Mobiliário urbano -----

  if (subtype === "semaforo") {
    // Semáforo top-down: 3 círculos verticais (vermelho/amarelo/verde).
    const r = size * 0.18;
    return (
      <Group>
        <Rect
          x={-r * 1.2}
          y={-r * 3.6}
          width={r * 2.4}
          height={r * 7.2}
          fill="#1f2937"
          cornerRadius={3}
        />
        <Circle x={0} y={-r * 2} radius={r} fill="#dc2626" />
        <Circle x={0} y={0} radius={r} fill="#facc15" />
        <Circle x={0} y={r * 2} radius={r} fill="#22c55e" />
      </Group>
    );
  }

  if (subtype === "placa_pare") {
    // Octógono PARE vermelho com texto.
    const r = size * 0.5;
    const pts: number[] = [];
    for (let i = 0; i < 8; i++) {
      const a = (i / 8) * Math.PI * 2 + Math.PI / 8;
      pts.push(Math.cos(a) * r, Math.sin(a) * r);
    }
    return (
      <Group>
        <Line points={pts} closed fill="#dc2626" stroke="#ffffff" strokeWidth={2} />
        <KonvaText
          text="PARE"
          fontSize={size * 0.28}
          fontStyle="bold"
          fill="#ffffff"
          width={size}
          align="center"
          offsetX={size / 2}
          offsetY={size * 0.14}
          listening={false}
        />
      </Group>
    );
  }

  if (subtype === "placa_preferencia") {
    // Triângulo invertido amarelo.
    const r = size * 0.55;
    return (
      <Group>
        <Line
          points={[-r, -r * 0.6, r, -r * 0.6, 0, r]}
          closed
          fill="#facc15"
          stroke="#1f2937"
          strokeWidth={2}
        />
      </Group>
    );
  }

  if (subtype === "poste") {
    // Poste — círculo cinza pequeno preenchido.
    return (
      <Group>
        <Circle radius={size / 2} fill={color} stroke="#111827" strokeWidth={1.5} />
        <Circle radius={size / 4} fill="#111827" />
      </Group>
    );
  }

  if (subtype === "arvore") {
    // Árvore — copa verde + tronco marrom.
    return (
      <Group>
        <Circle radius={size * 0.5} fill={color} opacity={0.65} />
        <Circle radius={size * 0.3} fill={color} />
        <Circle radius={size * 0.1} fill="#78350f" />
      </Group>
    );
  }

  if (subtype === "guia") {
    // Guia / meio-fio (ponto cinza com linha horizontal).
    return (
      <Group>
        <Line
          points={[-size / 2, 0, size / 2, 0]}
          stroke={color}
          strokeWidth={4}
          lineCap="butt"
        />
      </Group>
    );
  }

  if (subtype === "faixa_pedestre") {
    // Faixa de pedestre — barras paralelas brancas (zebrado).
    const bars = 5;
    const barGap = size / bars;
    return (
      <Group>
        {Array.from({ length: bars }, (_, i) => (
          <Rect
            key={i}
            x={-size / 2 + i * barGap + barGap * 0.1}
            y={-size * 0.45}
            width={barGap * 0.8}
            height={size * 0.9}
            fill="#f8fafc"
            stroke={color}
            strokeWidth={1}
          />
        ))}
      </Group>
    );
  }

  // Fallback: círculo sólido.
  return <Circle radius={size / 2} fill={color} />;
}

function TextNode({
  obj,
  draggable,
  selected,
  onSelect,
  onChange,
}: {
  obj: SicroTextObject;
  draggable: boolean;
  selected: boolean;
  onSelect: () => void;
  onChange: (patch: Partial<SicroObject>) => void;
}) {
  return (
    <Group
      id={obj.id}
      x={obj.x}
      y={obj.y}
      rotation={obj.rotation ?? 0}
      draggable={draggable && !obj.locked}
      onClick={onSelect}
      onTap={onSelect}
      onDragEnd={(e) =>
        onChange({ x: e.target.x(), y: e.target.y() } as Partial<SicroObject>)
      }
    >
      <KonvaText
        text={obj.text}
        fontSize={obj.font_size}
        fill={obj.color ?? "#111827"}
      />
      {selected && (
        <Rect
          x={-2}
          y={-2}
          width={obj.text.length * obj.font_size * 0.55 + 4}
          height={obj.font_size + 6}
          stroke="#0ea5e9"
          dash={[4, 3]}
          listening={false}
        />
      )}
    </Group>
  );
}

function MeasurementNode({
  obj,
  doc,
  draggable,
  selected,
  onSelect,
  onChange,
}: {
  obj: SicroMeasurementObject;
  doc: SicroCroquiDoc;
  draggable: boolean;
  selected: boolean;
  onSelect: () => void;
  onChange: (patch: Partial<SicroObject>) => void;
}) {
  const px = distancePx(obj.p1, obj.p2);
  const mid = midpoint(obj.p1, obj.p2);
  const angle = angleDeg(obj.p1, obj.p2);
  const label =
    obj.label_override ?? formatMeasurement(px, doc.scale?.px_per_m);

  return (
    <Group
      id={obj.id}
      draggable={draggable && !obj.locked}
      onClick={onSelect}
      onTap={onSelect}
      onDragEnd={(e) => {
        const dx = e.target.x();
        const dy = e.target.y();
        e.target.position({ x: 0, y: 0 });
        onChange({
          p1: { x: obj.p1.x + dx, y: obj.p1.y + dy },
          p2: { x: obj.p2.x + dx, y: obj.p2.y + dy },
        } as Partial<SicroObject>);
      }}
    >
      <Line
        points={[obj.p1.x, obj.p1.y, obj.p2.x, obj.p2.y]}
        stroke={obj.color ?? "#dc2626"}
        strokeWidth={selected ? 2.5 : 1.5}
        dash={[8, 4]}
        hitStrokeWidth={12}
      />
      <KonvaText
        text={label}
        x={mid.x}
        y={mid.y}
        offsetX={0}
        offsetY={14}
        rotation={angle}
        fontSize={11}
        fontStyle="bold"
        fill={obj.color ?? "#dc2626"}
        listening={false}
      />
    </Group>
  );
}

// ===========================================================================
// Two-click "first point" preview

function PendingTwoClickPreview({ editor }: { editor: EditorState }) {
  if (!editor.pending) return null;
  const first = editor.pending.first;
  const cursor = editor.pointerWorld;
  return (
    <Group listening={false}>
      <Line
        points={[first.x, first.y, cursor.x, cursor.y]}
        stroke="#0ea5e9"
        strokeWidth={1}
        dash={[6, 4]}
      />
      <Rect
        x={first.x - 3}
        y={first.y - 3}
        width={6}
        height={6}
        fill="#0ea5e9"
      />
    </Group>
  );
}

/**
 * Live preview of an in-progress road draft (MVP 9 Road Engine Pro).
 * Drawn on the non-listening UI layer so the user keeps clicking through
 * to add control points. The preview reuses Konva.Line `tension` so the
 * shape the user sees while clicking matches the final committed road.
 */
function RoadDraftPreview({ editor }: { editor: EditorState }) {
  const draft = editor.roadDraft;
  if (!draft || draft.points.length === 0) return null;
  const cursor = editor.pointerWorld;
  const flat: number[] = [];
  for (const pt of draft.points) flat.push(pt.x, pt.y);
  const previewFlat = [...flat, cursor.x, cursor.y];
  return (
    <Group listening={false}>
      {/* Ghost asphalt body so the user sees the eventual paved width */}
      <Line
        points={previewFlat}
        stroke="#3f3f46"
        strokeWidth={40}
        opacity={0.25}
        tension={0.5}
        lineCap="round"
        lineJoin="round"
      />
      {/* Centerline preview */}
      <Line
        points={previewFlat}
        stroke="#0ea5e9"
        strokeWidth={1.5}
        dash={[6, 4]}
        tension={0.5}
      />
      {/* Control-point chips */}
      {draft.points.map((p, i) => (
        <Rect
          key={`pt_${i}`}
          x={p.x - 3}
          y={p.y - 3}
          width={6}
          height={6}
          fill="#0ea5e9"
        />
      ))}
    </Group>
  );
}

// ===========================================================================
// helpers

function toWorld(stage: Konva.Stage, screen: SicroPoint): SicroPoint {
  const scale = stage.scaleX();
  const x = (screen.x - stage.x()) / scale;
  const y = (screen.y - stage.y()) / scale;
  return { x, y };
}

function isAddTool(tool: Tool): boolean {
  return (
    tool.startsWith("vehicle") ||
    tool.startsWith("line_") ||
    tool.startsWith("marker_") ||
    tool.startsWith("road_") ||
    tool === "text" ||
    tool === "measurement" ||
    tool === "set_scale"
  );
}

function clamp(v: number, min: number, max: number): number {
  return Math.max(min, Math.min(max, v));
}

function resolveAssetPath(workspacePath: string, src: string): string | null {
  // Absolute-looking path → resolve through Tauri's asset protocol.
  // Workspace-relative → join with workspacePath.
  try {
    const sep = workspacePath.includes("\\") ? "\\" : "/";
    const looksAbsolute =
      /^([a-zA-Z]:)?[\\/]/.test(src) || src.startsWith("file://");
    const abs = looksAbsolute
      ? src.replace(/^file:\/\//, "")
      : `${workspacePath}${sep}${src.replace(/\//g, sep)}`;
    return convertFileSrc(abs);
  } catch {
    return null;
  }
}
