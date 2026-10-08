/**
 * Stage Konva do croqui: layers de fundo, objetos (+ Transformer) e UI
 * transitória. Expõe `toPng()` para o export.
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
  RegularPolygon,
  Shape,
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
  type SicroTraceObject,
  type SicroFixtureObject,
  type SicroPersonObject,
  type SicroVehicleObject,
  type VehicleBodyType,
  FIXTURE_SPECS,
  personRig,
  personToLocal,
  personToWorld,
  vadd,
  vmul,
  vsub,
  fixtureExtentM,
  fixtureGeomM,
  fn,
  fp,
  fanGeom,
  tn,
  tp,
  traceAt,
  traceGeom,
  traceGeomM,
  traceSpanM,
} from "../engine";
import {
  RoadParityRenderer,
  isParityObject,
  type ParityTema,
  type SicroParityObject,
  resolveParityStyle,
  resolvePxPerM,
} from "../engine/road-parity";
import { drawTrace, tracePalette, traceHitShape } from "./traceDraw";
import { drawFixture, fixtureHitShape, fixtureOnGround } from "./fixtureDraw";
import { drawPerson, hitPerson, setPersonExporting } from "./personDraw";
import {
  getCachedPessoaArtImage,
  getCachedVehicleArtImage,
  getPessoaArt,
  loadPessoaArtImage,
  loadVehicleArtImage,
} from "../engine/vehicleArt";
import { labelDefaults, textWidthPx, type LabelFields } from "./labels";
import {
  getObjectBoundsStagePx,
  rectFromPoints,
  rectsIntersect,
  translateObjectPatch,
} from "./bounds";
import type { EditorState, Tool } from "./useEditorState";

export interface CanvasStageHandle {
  /**
   * PNG da FOLHA (`rect` em px de mundo), independente do zoom/pan da tela,
   * com `targetWidthPx` de largura.
   */
  toPng(rect: { x: number; y: number; width: number; height: number }, targetWidthPx: number): string | null;
  getStageSize(): { width: number; height: number };
  /** Ponto de viewport → mundo; null fora da tela do croqui. */
  clientToWorld(clientX: number, clientY: number): SicroPoint | null;
}

// Zoom de 5 % a 100×: o perito precisa chegar perto de vestígios pequenos
// (manchas, fragmentos). Canvas2D aguenta sem custo de "DOM zoom".
export const CROQUI_ZOOM_MIN = 0.05;
export const CROQUI_ZOOM_MAX = 100;
const CROQUI_ZOOM_WHEEL_FACTOR_IN = 1.08;
const CROQUI_ZOOM_WHEEL_FACTOR_OUT = 0.92;

/** `selectedId` sentinela (não colide com UUID) para a imagem de fundo selecionada. */
export const BACKGROUND_SELECTION_ID = "_background";

interface Props {
  doc: SicroCroquiDoc;
  editor: EditorState;
  containerWidth: number;
  containerHeight: number;
  /** Clique no canvas com ferramenta de inserir objeto. */
  onCanvasClick: (worldPoint: SicroPoint) => void;
  /** Duplo clique (finaliza rascunho de via). */
  onCanvasDblClick?: () => void;
  /** Fim de drag/transform de um objeto. */
  onObjectChange: (id: string, patch: Partial<SicroObject>) => void;
  /** Idem para objetos parity; sem ele, os handles parity ficam estáticos. */
  onParityObjectChange?: (
    id: string,
    patch: Partial<SicroParityObject>,
  ) => void;
  /** Fim de drag/transform da imagem de fundo. */
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
  // Camada de interface (prévias, rascunho, laço): nunca vai para o PNG.
  const uiLayerRef = useRef<Konva.Layer | null>(null);
  // O browser emite `click` depois do mouseup do marquee (cancelBubble não
  // impede); a flag evita que esse click limpe a seleção recém-criada.
  const justFinishedMarqueeRef = useRef(false);
  // Ferramenta de dois pontos: pressionar marca o primeiro; arrastar e soltar fecha o segundo.
  const dragDrawRef = useRef<{ x: number; y: number; moved: boolean } | null>(null);
  const suppressClickRef = useRef(false);
  // Arrasto em grupo: deslocamento lido no dragmove (no dragend a via/linha já zerou a própria
  // posição) e partes "obj_<id>" das vias movidas junto na prévia.
  const dragSessionRef = useRef<{
    node: Konva.Node;
    startX: number;
    startY: number;
    dx: number;
    dy: number;
    parts: Array<{ node: Konva.Node; x: number; y: number }>;
    others: string[];
  } | null>(null);

  useImperativeHandle(ref, () => ({
    toPng(rect, targetWidthPx) {
      const stage = stageRef.current;
      if (!stage || rect.width <= 0 || rect.height <= 0) return null;
      // Zera a transformação da tela só durante a captura: mundo = px.
      const prev = { x: stage.x(), y: stage.y(), scale: stage.scaleX() };
      const tr = transformerRef.current;
      const ui = uiLayerRef.current;
      const trVisible = tr?.visible() ?? false;
      const uiVisible = ui?.visible() ?? false;
      stage.scale({ x: 1, y: 1 });
      stage.position({ x: 0, y: 0 });
      tr?.visible(false);
      ui?.visible(false);
      setPersonExporting(true);
      try {
        const pixelRatio = Math.min(targetWidthPx, 8000) / rect.width;
        return stage.toDataURL({ ...rect, pixelRatio, mimeType: "image/png" });
      } finally {
        setPersonExporting(false);
        tr?.visible(trVisible);
        ui?.visible(uiVisible);
        stage.scale({ x: prev.scale, y: prev.scale });
        stage.position({ x: prev.x, y: prev.y });
        stage.batchDraw();
      }
    },
    clientToWorld(clientX, clientY) {
      const stage = stageRef.current;
      if (!stage) return null;
      const r = stage.container().getBoundingClientRect();
      if (clientX < r.left || clientX > r.right || clientY < r.top || clientY > r.bottom) return null;
      return toWorld(stage, { x: clientX - r.left, y: clientY - r.top });
    },
    getStageSize() {
      return {
        width: containerWidth,
        height: containerHeight,
      };
    },
  }));

  // Listeners no stage (eventos do Konva sobem). As partes de uma via (calçada, meio-fio,
  // sinalização, alças) ficam espalhadas pelas passadas do renderer, por isso o nome obj_<id>.
  useEffect(() => {
    const stage = stageRef.current;
    if (!stage) return;
    const partsOf = (layer: Konva.Layer, id: string, except?: Konva.Node) =>
      [...layer.find(`.obj_${id}`), layer.findOne(`#${id}`)]
        .filter((n): n is Konva.Node => !!n && n !== except)
        .map((n) => ({ node: n, x: n.x(), y: n.y() }));

    const dragStart = (e: Konva.KonvaEventObject<DragEvent>) => {
      const node = e.target;
      const id = typeof node.id === "function" ? node.id() : "";
      const layer = objectsLayerRef.current;
      if (!id || !layer || !doc.objects.some((o) => o.id === id)) return;
      const others = editor.selectedIds.includes(id)
        ? editor.selectedIds.filter((s) => s !== id)
        : [];
      dragSessionRef.current = {
        node,
        startX: node.x(),
        startY: node.y(),
        dx: 0,
        dy: 0,
        parts: [...partsOf(layer, id, node), ...others.flatMap((oid) => partsOf(layer, oid))],
        others,
      };
    };

    const dragMove = (e: Konva.KonvaEventObject<DragEvent>) => {
      const s = dragSessionRef.current;
      if (!s || e.target !== s.node) return;
      s.dx = s.node.x() - s.startX;
      s.dy = s.node.y() - s.startY;
      for (const p of s.parts) p.node.position({ x: p.x + s.dx, y: p.y + s.dy });
      objectsLayerRef.current?.batchDraw();
    };

    const dragEnd = (e: Konva.KonvaEventObject<DragEvent>) => {
      const s = dragSessionRef.current;
      if (!s || e.target !== s.node) return;
      dragSessionRef.current = null;
      // Devolve as partes ao lugar: quem desenha em coordenadas absolutas não tem x/y como prop
      // e o patch já leva o deslocamento.
      for (const p of s.parts) p.node.position({ x: p.x, y: p.y });
      if (Math.abs(s.dx) < 0.001 && Math.abs(s.dy) < 0.001) return;
      // O arrastado é commitado pelo próprio nó; aqui só os outros.
      const pxPerM = resolvePxPerM(doc.scale?.px_per_m);
      for (const oid of s.others) {
        const obj = doc.objects.find((x) => x.id === oid);
        if (!obj) continue;
        const patch = translateObjectPatch(obj, s.dx, s.dy, pxPerM);
        if (!patch) continue;
        if (isParityObject(obj)) {
          onParityObjectChange?.(oid, patch as Partial<SicroParityObject>);
        } else {
          onObjectChange(oid, patch);
        }
      }
    };

    stage.on("dragstart.group", dragStart);
    stage.on("dragmove.group", dragMove);
    stage.on("dragend.group", dragEnd);
    return () => {
      stage.off("dragstart.group");
      stage.off("dragmove.group");
      stage.off("dragend.group");
    };
  }, [editor.selectedIds, doc, onObjectChange, onParityObjectChange]);

  // Transformer nos nós selecionados (aceita lista → uma caixa em volta de todos).
  useEffect(() => {
    const transformer = transformerRef.current;
    const layer = objectsLayerRef.current;
    if (!transformer || !layer) return;
    if (editor.selectedIds.length === 0) {
      transformer.nodes([]);
      transformer.getLayer()?.batchDraw();
      return;
    }
    const kinds = new Map(doc.objects.map((o) => [o.id, o.kind] as const));
    const nodes: Konva.Node[] = [];
    for (const id of editor.selectedIds) {
      // Linhas e cotas editam-se pelas pontas, não por caixa de escala.
      const k = kinds.get(id);
      if (k === "line" || k === "measurement" || k === "trace" || k === "fixture" || k === "person" || k === "road_parity" || k === "roundabout_parity") continue;
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
    const dd = dragDrawRef.current;
    if (dd && !dd.moved && Math.hypot(world.x - dd.x, world.y - dd.y) * editor.viewport.scale > 4) {
      dd.moved = true;
    }
    if (editor.marquee) {
      editor.setMarquee({
        ...editor.marquee,
        currentWorldX: world.x,
        currentWorldY: world.y,
      });
    }
  };

  // Marquee: mousedown no fundo (modo select) inicia; mouseup seleciona
  // quem intersecta o retângulo.
  const handleStageMouseDown = (e: Konva.KonvaEventObject<MouseEvent>) => {
    justFinishedMarqueeRef.current = false;
    if (isTwoPointTool(editor.tool) && e.evt.button === 0 && !editor.pending) {
      const stage = stageRef.current;
      const pos = stage?.getPointerPosition();
      if (!stage || !pos) return;
      const world = toWorld(stage, pos);
      editor.setPending({ tool: editor.tool, first: world });
      dragDrawRef.current = { x: world.x, y: world.y, moved: false };
      return;
    }
    if (editor.tool !== "select") return;
    // Só no fundo, não sobre um objeto.
    if (e.target !== e.target.getStage()) return;
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
    const dd = dragDrawRef.current;
    if (dd) {
      dragDrawRef.current = null;
      // O click que vem em seguida não pode virar um segundo ponto.
      suppressClickRef.current = true;
      setTimeout(() => {
        suppressClickRef.current = false;
      }, 0);
      if (dd.moved) {
        const stage = stageRef.current;
        const pos = stage?.getPointerPosition();
        if (stage && pos) onCanvasClick(toWorld(stage, pos));
      }
      return;
    }
    if (!editor.marquee) return;
    const m = editor.marquee;
    editor.setMarquee(null);
    // Área < 9 px² = clique sem arrasto → limpa a seleção.
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
    const pxPerM = resolvePxPerM(doc.scale?.px_per_m);
    const hits: string[] = [];
    for (const obj of doc.objects) {
      const b = getObjectBoundsStagePx(obj, pxPerM);
      if (rectsIntersect(b, rect)) hits.push(obj.id);
    }
    editor.setSelectedIds(hits);
    // O `click` que o browser emite em seguida seria tratado como clique
    // no vazio e apagaria a seleção; ver justFinishedMarqueeRef.
    justFinishedMarqueeRef.current = true;
    e.cancelBubble = true;
  };

  const handleStageClick = (e: Konva.KonvaEventObject<MouseEvent>) => {
    if (suppressClickRef.current) {
      suppressClickRef.current = false;
      return;
    }
    // Click espúrio logo após o mouseup do marquee.
    if (justFinishedMarqueeRef.current) {
      justFinishedMarqueeRef.current = false;
      return;
    }
    if (e.target !== e.target.getStage()) {
      // Clique sobre objeto: o Group trata, salvo ferramenta de inserir
      // (o usuário ainda quer adicionar ali).
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

  const { parityObjects, otherObjects } = useMemo(() => {
    const parity: SicroParityObject[] = [];
    const others: SicroObject[] = [];
    for (const o of doc.objects) {
      if (isParityObject(o)) parity.push(o);
      else others.push(o);
    }
    // Pintura no chão, vestígios, elementos de pé e, por cima, veículos e anotações.
    const rank = (o: SicroObject) =>
      o.kind === "fixture" ? (fixtureOnGround(o) ? 0 : 2) : o.kind === "trace" ? 1 : 3;
    const ordered = others.map((o, i) => [rank(o), i, o] as const).sort((a, b) => a[0] - b[0] || a[1] - b[1]).map((x) => x[2]);
    return { parityObjects: parity, otherObjects: ordered };
  }, [doc.objects]);

  const handleWheel = (e: Konva.KonvaEventObject<WheelEvent>) => {
    e.evt.preventDefault();
    const stage = stageRef.current;
    if (!stage) return;
    const pointer = stage.getPointerPosition();
    if (!pointer) return;

    // Touchpad pinch chega como `wheel` com ctrlKey (metaKey no macOS);
    // scroll de dois dedos vem sem modificador e vira pan.
    const isPinch = e.evt.ctrlKey || e.evt.metaKey;

    if (isPinch) {
      // Zoom ancorado no ponteiro.
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

    // Pan; sinal negativo = "dedos descem, conteúdo sobe".
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
        // Sincroniza o pan para o zoom pela roda ancorar certo depois.
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
        {/* Vias primeiro ("asfalto"); o resto empilha por cima. */}
        <RoadParityRenderer
          objects={parityObjects}
          pxPerM={doc.scale?.px_per_m ?? null}
          style={doc.style ?? null}
          selectedId={editor.selectedId}
          selectedIds={editor.selectedIds}
          onSelect={(id) => onSelect(id ?? null)}
          onObjectChange={
            onParityObjectChange
              ? (id, patch) => onParityObjectChange(id, patch)
              : undefined
          }
        />
        {otherObjects.map((obj) => (
          <ObjectNode
            key={obj.id}
            obj={obj}
            doc={doc}
            tool={editor.tool}
            selected={editor.selectedIds.includes(obj.id)}
            solo={editor.selectedIds.length === 1 && editor.selectedIds[0] === obj.id}
            zoom={editor.viewport.scale}
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
            // Evita flip por tamanho negativo.
            if (Math.abs(next.width) < 4 || Math.abs(next.height) < 4) return _old;
            return next;
          }}
        />
        {/* Marquee em coords world; a stage aplica o viewport. */}
        {editor.marquee && (() => {
          const m = editor.marquee;
          const r = rectFromPoints(
            m.startWorldX,
            m.startWorldY,
            m.currentWorldX,
            m.currentWorldY,
          );
          // Stroke e dash divididos pelo scale: espessura constante no zoom.
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

      <Layer listening={false} ref={uiLayerRef}>
        <PendingTwoClickPreview editor={editor} />
        <RoadDraftPreview editor={editor} />
      </Layer>
    </Stage>
  );
});

// ===========================================================================
// Fundo (cor + grid)

function CanvasBackground({ doc }: { doc: SicroCroquiDoc }) {
  const { width_px, height_px, background_color, grid } = doc.canvas;
  const ox = doc.canvas.origin_x ?? 0;
  const oy = doc.canvas.origin_y ?? 0;
  const ppm = doc.scale?.px_per_m;
  let gridSize =
    grid?.size_m && ppm ? Math.max(4, grid.size_m * ppm) : (grid?.size_px ?? 50);
  // Folha enorme com grade fina: dobra o passo até caber em ~2000 linhas.
  while ((width_px + height_px) / gridSize > 2000) gridSize *= 2;
  const gridEnabled = grid?.enabled ?? true;

  // Branco puro cansa a vista; só o branco é trocado por off-white.
  const effectiveBg =
    background_color === "#ffffff" || background_color === "#FFFFFF"
      ? "#f5f6f8"
      : background_color;

  const lines = useMemo(() => {
    if (!gridEnabled) return [] as number[][];
    const out: number[][] = [];
    for (let x = 0; x <= width_px; x += gridSize) {
      out.push([ox + x, oy, ox + x, oy + height_px]);
    }
    for (let y = 0; y <= height_px; y += gridSize) {
      out.push([ox, oy + y, ox + width_px, oy + y]);
    }
    return out;
  }, [width_px, height_px, gridSize, gridEnabled, ox, oy]);

  return (
    <>
      <Rect
        x={ox}
        y={oy}
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
// Imagem de fundo

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

  // Transformer só com fundo selecionado e destravado; depende de `bg` para
  // reagir na hora ao toggle do cadeado.
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

  // Doc antigo salvo com 0/0 → usa o tamanho natural da imagem.
  const resolvedW = bg.width || image.width;
  const resolvedH = bg.height || image.height;
  const rotation = bg.rotation ?? 0;

  // Origem do Group no centro da imagem para a rotação girar em torno dele;
  // a KonvaImage fica em (-w/2, -h/2).
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
          // Senão o onClick da Stage chama onSelect(null).
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
          // Mínimo de 20 px para a imagem não sumir.
          if (Math.abs(next.width) < 20 || Math.abs(next.height) < 20) return _old;
          return next;
        }}
      />
    </Layer>
  );
}

// ===========================================================================
// Objetos

function ObjectNode({
  obj,
  doc,
  tool,
  selected,
  solo,
  zoom,
  onSelect,
  onChange,
}: {
  obj: SicroObject;
  doc: SicroCroquiDoc;
  tool: Tool;
  selected: boolean;
  /** Único selecionado: só então o rótulo se arrasta separado do corpo. */
  solo: boolean;
  /** Escala da tela: alças dos objetos paramétricos têm tamanho fixo na tela. */
  zoom: number;
  onSelect: () => void;
  onChange: (patch: Partial<SicroObject>) => void;
}) {
  const draggable = tool === "select";
  const st = resolveParityStyle(doc.style);
  const outlineTema = st.veiculos === "traco" ? st.tema : undefined;
  const labelProps = { draggable: draggable && !obj.locked && solo, onSelect, onChange, outlineTema };

  switch (obj.kind) {
    case "vehicle":
      return (
        <>
          <VehicleNode
            obj={obj}
            draggable={draggable}
            selected={selected}
            outlineTema={outlineTema}
            onSelect={onSelect}
            onChange={onChange}
          />
          <ObjectLabel obj={obj} anchor={{ x: obj.x, y: obj.y }} {...labelProps} />
        </>
      );
    case "line":
      return (
        <>
          <LineNode
            obj={obj}
            draggable={draggable}
            selected={selected}
            onSelect={onSelect}
            onChange={onChange}
          />
          <ObjectLabel
            obj={obj}
            anchor={{ x: obj.points[0] ?? 0, y: obj.points[1] ?? 0 }}
            {...labelProps}
          />
        </>
      );
    case "marker":
      return (
        <>
          <MarkerNode
            obj={obj}
            draggable={draggable}
            selected={selected}
            onSelect={onSelect}
            onChange={onChange}
          />
          <ObjectLabel obj={obj} anchor={{ x: obj.x, y: obj.y }} {...labelProps} />
        </>
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
          solo={solo}
          onSelect={onSelect}
          onChange={onChange}
        />
      );
    case "trace":
      return (
        <TraceNode
          obj={obj}
          doc={doc}
          zoom={zoom}
          draggable={draggable}
          solo={solo}
          onSelect={onSelect}
          onChange={onChange}
        />
      );
    case "person":
      return (
        <PersonNode
          obj={obj}
          doc={doc}
          zoom={zoom}
          draggable={draggable}
          solo={solo}
          onSelect={onSelect}
          onChange={onChange}
        />
      );
    case "fixture":
      return (
        <FixtureNode
          obj={obj}
          doc={doc}
          zoom={zoom}
          draggable={draggable}
          solo={solo}
          onSelect={onSelect}
          onChange={onChange}
        />
      );
    case "road_parity":
    case "roundabout_parity":
      // Parity é renderizado pelo RoadParityRenderer.
      return null;
  }
}

/** Arte SVG do par (tipo, cor), com cache no engine. `null` → silhueta vetorial. */
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
  outlineTema,
  onSelect,
  onChange,
}: {
  obj: SicroVehicleObject;
  draggable: boolean;
  selected: boolean;
  /** Presente = veículo em traço no tema dado. */
  outlineTema?: ParityTema;
  onSelect: () => void;
  onChange: (patch: Partial<SicroObject>) => void;
}) {
  const body: VehicleBodyType = obj.body_type ?? "car";
  const isTwoWheel = body === "moto" || body === "bike";
  const artImg = useVehicleArtImage(body, obj.color);
  const art = outlineTema ? null : artImg;
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
          {/* Hit cheio invisível: o hit do Konva.Image ignora pixels
              transparentes da arte. */}
          <Rect
            x={-obj.width / 2}
            y={-obj.height / 2}
            width={obj.width}
            height={obj.height}
            fill="#000"
            opacity={0}
          />
          {/* SVG é retrato (frente para cima); gira 90° para a frente
              apontar +x (convenção do croqui), por isso width/height trocados. */}
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
      ) : outlineTema ? (
        <VehicleOutline obj={obj} body={body} tema={outlineTema} selected={selected} />
      ) : (
        <VehicleSilhouette
          body={body}
          width={obj.width}
          height={obj.height}
          color={obj.color ?? "#3b82f6"}
          selected={selected}
        />
      )}
      {/* Triângulo da frente (+x), só no fallback sem arte. */}
      {!art && !outlineTema && !isTwoWheel && (
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
    </Group>
  );
}

/** Silhueta vetorial por tipo de carroceria (fallback sem arte). +x = frente. */
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

  // ---- sedan / suv / hatch / car / other ----
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
  // Pontas arrastáveis: prévia local durante o drag, commit no fim.
  const [live, setLive] = useState<number[] | null>(null);
  const pts = live ?? obj.points;
  const canEdit = draggable && selected && !obj.locked;
  const movePoint = (i: number, x: number, y: number) => {
    const next = [...pts];
    next[i * 2] = x;
    next[i * 2 + 1] = y;
    return next;
  };
  return (
    <Group
      id={obj.id}
      draggable={draggable && !obj.locked}
      onClick={onSelect}
      onTap={onSelect}
      onDragEnd={(e) => {
        if (e.target !== e.currentTarget) return;
        const dx = e.target.x();
        const dy = e.target.y();
        const next = obj.points.map((v, i) => v + (i % 2 === 0 ? dx : dy));
        e.target.position({ x: 0, y: 0 });
        onChange({ points: next } as Partial<SicroObject>);
      }}
    >
      <Line
        points={pts}
        stroke={obj.color ?? "#1f2937"}
        strokeWidth={obj.stroke_width}
        dash={obj.dashed ? [12, 6] : undefined}
        lineCap="round"
        opacity={selected ? 1 : 0.95}
        hitStrokeWidth={Math.max(obj.stroke_width, 12)}
      />
      {obj.subtype === "arrow" && pts.length >= 4 && (
        <ArrowHead
          x1={pts[pts.length - 4]!}
          y1={pts[pts.length - 3]!}
          x2={pts[pts.length - 2]!}
          y2={pts[pts.length - 1]!}
          color={obj.color ?? "#111827"}
          size={Math.max(obj.stroke_width * 4, 12)}
        />
      )}
      {canEdit &&
        Array.from({ length: Math.floor(pts.length / 2) }, (_, i) => (
          <PointHandle
            key={i}
            x={pts[i * 2]!}
            y={pts[i * 2 + 1]!}
            color={obj.color ?? "#1f2937"}
            onMove={(x, y) => setLive(movePoint(i, x, y))}
            onEnd={(x, y) => {
              setLive(null);
              onChange({ points: movePoint(i, x, y) } as Partial<SicroObject>);
            }}
          />
        ))}
    </Group>
  );
}

function setCursor(e: Konva.KonvaEventObject<MouseEvent>, cursor: string) {
  const c = e.target.getStage()?.container();
  if (c) c.style.cursor = cursor;
}

// ===========================================================================
// Pessoa articulada (people.ts + personDraw.ts)

function PersonNode({
  obj,
  doc,
  draggable,
  solo,
  zoom,
  onSelect,
  onChange,
}: {
  obj: SicroPersonObject;
  doc: SicroCroquiDoc;
  draggable: boolean;
  solo: boolean;
  zoom: number;
  onSelect: () => void;
  onChange: (patch: Partial<SicroObject>) => void;
}) {
  const ppm = resolvePxPerM(doc.scale?.px_per_m);
  const st = resolveParityStyle(doc.style);
  const opts = { ppm, tema: st.tema, asfalto: st.asfalto };
  const build = (o: SicroPersonObject): ParamView<SicroPersonObject> => {
    const H = o.altura_m;
    const W = (p: { x: number; y: number }) => personToWorld(o, p, ppm);
    const L = (x: number, y: number) => personToLocal(o, { x, y }, ppm);
    const rotHandle = (at: { x: number; y: number }): ParamHandle<SicroPersonObject> => ({
      ...W(at),
      patch: (x, y) => ({ rotation: Math.round(((Math.atan2(y - o.y, x - o.x) * 180) / Math.PI + 90) * 10) / 10 }),
    });
    let handles: ParamHandle<SicroPersonObject>[];
    if (o.posicao === "empe") {
      handles = [rotHandle({ x: 0, y: -0.3 * H })];
    } else {
      const R = personRig(o);
      handles = [];
      R.arms.forEach((a, i) => {
        handles.push({ ...W(a.T), patch: (x, y) => ({ pose: setPose(o.pose, "wrist", i, vmul(L(x, y), 1 / H)) }) });
        handles.push({ ...W(a.E), diamond: true, patch: (x, y) => ({ pose: setSign(o.pose, "sArm", i, bendSide(a, L(x, y))) }) });
      });
      R.legs.forEach((l, i) => {
        handles.push({ ...W(l.T), patch: (x, y) => ({ pose: setPose(o.pose, "ankle", i, vmul(L(x, y), 1 / H)) }) });
        handles.push({ ...W(l.E), diamond: true, patch: (x, y) => ({ pose: setSign(o.pose, "sLeg", i, bendSide(l, L(x, y))) }) });
      });
      handles.push({
        ...W(R.headC),
        patch: (x, y) => {
          const v = vsub(L(x, y), R.neck);
          const a = Math.atan2(v.y, v.x) - Math.atan2(R.up.y, R.up.x);
          return { cabeca: Math.round(Math.max(-75, Math.min(75, (Math.atan2(Math.sin(a), Math.cos(a)) * 180) / Math.PI))) };
        },
      });
      handles.push(rotHandle(vadd(R.headC, vmul(R.hdir, 0.28 * H))));
    }
    return {
      draw: (ctx) => drawPerson(ctx, o, { ...opts, zoom }),
      hit: { line: [], width: 0, polys: [] },
      hitNative: (ctx, color) => hitPerson(ctx, o, ppm, color),
      handles,
      anchor: W({ x: 0.35 * H, y: -0.3 * H }),
      move: (dx, dy) => ({ x: o.x + dx, y: o.y + dy }),
    };
  };
  return <ParamNode obj={obj} build={build} draggable={draggable} solo={solo} handleR={5 / zoom} onSelect={onSelect} onChange={onChange} />;
}

type Pose = SicroPersonObject["pose"];
function setPose(pose: Pose, k: "wrist" | "ankle", i: number, p: { x: number; y: number }): Pose {
  const list = [...pose[k]] as Pose["wrist"];
  list[i] = p;
  return { ...pose, [k]: list };
}
function setSign(pose: Pose, k: "sArm" | "sLeg", i: number, s: number): Pose {
  const list = [...pose[k]] as [number, number];
  list[i] = s;
  return { ...pose, [k]: list };
}
/** Lado em que o ponteiro está em relação à linha raiz → ponta: define para onde a junta dobra. */
function bendSide(seg: { S: { x: number; y: number }; T: { x: number; y: number } }, p: { x: number; y: number }): number {
  return (seg.T.x - seg.S.x) * (p.y - seg.S.y) - (seg.T.y - seg.S.y) * (p.x - seg.S.x) >= 0 ? 1 : -1;
}

// ===========================================================================
// Vestígios paramétricos (traces.ts + traceDraw.ts)

const HANDLE_COLOR = "#d7a84f";

type PathObj = SicroTraceObject | SicroFixtureObject;
type ParamObj = PathObj | SicroPersonObject;

interface ParamHandle<T extends ParamObj> {
  x: number;
  y: number;
  diamond?: boolean;
  patch: (x: number, y: number) => Partial<T>;
}

interface ParamView<T extends ParamObj> {
  draw: (ctx: CanvasRenderingContext2D) => void;
  hit: { line: SicroPoint[]; width: number; polys: SicroPoint[][] };
  handles: ParamHandle<T>[];
  anchor: SicroPoint;
  extra?: React.ReactNode;
  /** Patch ao arrastar o corpo inteiro por (dx, dy) px. */
  move: (dx: number, dy: number) => Partial<T>;
  /** Área clicável desenhada direto no canvas de hit (substitui `hit`). */
  hitNative?: (ctx: CanvasRenderingContext2D, color: string) => void;
}

const nativeCtx = (kctx: Konva.Context) => (kctx as unknown as { _context: CanvasRenderingContext2D })._context;

/** Objeto paramétrico (vestígio, sinalização, entorno): desenho por função, alças, arrasto e rótulo solto. */
function ParamNode<T extends ParamObj>({
  obj,
  build,
  draggable,
  solo,
  handleR,
  onSelect,
  onChange,
}: {
  obj: T;
  build: (o: T) => ParamView<T>;
  draggable: boolean;
  solo: boolean;
  /** Raio das alças em px de mundo (o chamador divide pelo zoom). */
  handleR: number;
  onSelect: () => void;
  onChange: (patch: Partial<SicroObject>) => void;
}) {
  const [live, setLive] = useState<Partial<T> | null>(null);
  const o = live ? ({ ...obj, ...live } as T) : obj;
  const view = build(o);
  const canEdit = draggable && solo && !obj.locked;
  return (
    <>
      <Group
        id={obj.id}
        draggable={draggable && !obj.locked}
        onClick={onSelect}
        onTap={onSelect}
        onDragEnd={(e) => {
          if (e.target !== e.currentTarget) return;
          const dx = e.target.x();
          const dy = e.target.y();
          e.target.position({ x: 0, y: 0 });
          onChange(build(obj).move(dx, dy) as Partial<SicroObject>);
        }}
      >
        <Shape
          fill="#000"
          stroke="#000"
          strokeWidth={view.hit.width || 1}
          sceneFunc={(kctx) => view.draw(nativeCtx(kctx))}
          hitFunc={(kctx, shape) => {
            if (view.hitNative) {
              view.hitNative(nativeCtx(kctx), (shape as unknown as { colorKey: string }).colorKey);
              return;
            }
            for (const poly of view.hit.polys) {
              kctx.beginPath();
              poly.forEach((p, i) => (i ? kctx.lineTo(p.x, p.y) : kctx.moveTo(p.x, p.y)));
              kctx.closePath();
              kctx.fillShape(shape);
            }
            if (view.hit.line.length > 1) {
              kctx.beginPath();
              view.hit.line.forEach((p, i) => (i ? kctx.lineTo(p.x, p.y) : kctx.moveTo(p.x, p.y)));
              kctx.setAttr("lineCap", "round");
              kctx.setAttr("lineJoin", "round");
              kctx.strokeShape(shape);
            }
          }}
        />
        {view.extra}
        {canEdit &&
          view.handles.map((h, i) => (
            <PointHandle
              key={i}
              x={h.x}
              y={h.y}
              diamond={h.diamond}
              r={handleR}
              color={HANDLE_COLOR}
              onMove={(x, y) => setLive(h.patch(x, y))}
              onEnd={(x, y) => {
                setLive(null);
                onChange(h.patch(x, y) as Partial<SicroObject>);
              }}
            />
          ))}
      </Group>
      <ObjectLabel
        obj={o}
        anchor={view.anchor}
        draggable={draggable && !obj.locked && solo}
        onSelect={onSelect}
        onChange={onChange}
      />
    </>
  );
}

/**
 * Alças de início e fim (o ponto move junto com a direção nos elementos de ponto).
 * `backPx` recua a alça do fim ao longo de p0 → p1, para não cobrir o que fica na ponta (cabeça do semáforo).
 */
function endHandles<T extends PathObj>(o: T, moveTogether: boolean, backPx = 0): ParamHandle<T>[] {
  const d = Math.hypot(o.p1.x - o.p0.x, o.p1.y - o.p0.y);
  const back = d > backPx * 1.5 ? backPx : 0;
  const ux = d > 1e-6 ? (o.p1.x - o.p0.x) / d : 1;
  const uy = d > 1e-6 ? (o.p1.y - o.p0.y) / d : 0;
  return [
    {
      x: o.p0.x,
      y: o.p0.y,
      patch: (x, y) =>
        (moveTogether ? { p0: { x, y }, p1: { x: o.p1.x + x - o.p0.x, y: o.p1.y + y - o.p0.y } } : { p0: { x, y } }) as Partial<T>,
    },
    {
      x: o.p1.x - ux * back,
      y: o.p1.y - uy * back,
      patch: (x, y) => {
        const dd = Math.hypot(x - o.p0.x, y - o.p0.y);
        if (!back || dd < 1e-6) return { p1: { x, y } } as Partial<T>;
        return { p1: { x: x + ((x - o.p0.x) / dd) * back, y: y + ((y - o.p0.y) / dd) * back } } as Partial<T>;
      },
    },
  ];
}

/**
 * Quatro cantos de um retângulo (eixo p0 → p1, `larguraM` de lado a lado): arrastar um canto
 * redimensiona com o canto oposto fixo, mantendo a orientação.
 */
function cornerHandles(o: SicroFixtureObject, larguraM: number, ppm: number): ParamHandle<SicroFixtureObject>[] {
  const L = Math.hypot(o.p1.x - o.p0.x, o.p1.y - o.p0.y) || 1e-6;
  const u = { x: (o.p1.x - o.p0.x) / L, y: (o.p1.y - o.p0.y) / L };
  const n = { x: -u.y, y: u.x };
  const h = (larguraM * ppm) / 2;
  const at = (p: SicroPoint, s: number) => ({ x: p.x + n.x * h * s, y: p.y + n.y * h * s });
  const corners = [at(o.p0, 1), at(o.p1, 1), at(o.p1, -1), at(o.p0, -1)];
  return corners.map((c, i) => {
    const opp = corners[(i + 2) % 4]!;
    return {
      x: c.x,
      y: c.y,
      patch: (x: number, y: number) => {
        const dx = x - opp.x;
        const dy = y - opp.y;
        let du = dx * u.x + dy * u.y;
        let dn = dx * n.x + dy * n.y;
        if (Math.abs(du) < ppm) du = Math.sign(du || 1) * ppm;
        if (Math.abs(dn) < ppm) dn = Math.sign(dn || 1) * ppm;
        const p0 = { x: opp.x + (n.x * dn) / 2, y: opp.y + (n.y * dn) / 2 };
        const p1 = { x: p0.x + u.x * du, y: p0.y + u.y * du };
        return { p0, p1, params: { ...o.params, largura: Math.round((Math.abs(dn) / ppm) * 10) / 10 } };
      },
    };
  });
}

/** Losango da flecha: projeta o ponteiro na normal da corda. */
function bendHandle<T extends PathObj>(o: T): ParamHandle<T> {
  const C = Math.hypot(o.p1.x - o.p0.x, o.p1.y - o.p0.y) || 1e-6;
  const cn = { x: -(o.p1.y - o.p0.y) / C, y: (o.p1.x - o.p0.x) / C };
  const cm = { x: (o.p0.x + o.p1.x) / 2, y: (o.p0.y + o.p1.y) / 2 };
  return {
    x: cm.x + cn.x * (o.bend ?? 0),
    y: cm.y + cn.y * (o.bend ?? 0),
    diamond: true,
    patch: (x, y) => ({ bend: (x - cm.x) * cn.x + (y - cm.y) * cn.y }) as Partial<T>,
  };
}

/** Alça de raio à direita do ponto (edita `params.raio`). */
function raioHandle<T extends PathObj>(o: T, raioM: number, ppm: number, k = 1): ParamHandle<T> {
  return {
    x: o.p0.x + raioM * ppm * k,
    y: o.p0.y,
    patch: (x, y) => {
      const r = Math.hypot(x - o.p0.x, y - o.p0.y) / ppm / k;
      return { params: { ...o.params, raio: Math.round(Math.min(8, Math.max(0.2, r)) * 100) / 100 } } as unknown as Partial<T>;
    },
  };
}

function TraceNode({
  obj,
  doc,
  draggable,
  solo,
  zoom,
  onSelect,
  onChange,
}: {
  obj: SicroTraceObject;
  doc: SicroCroquiDoc;
  draggable: boolean;
  solo: boolean;
  zoom: number;
  onSelect: () => void;
  onChange: (patch: Partial<SicroObject>) => void;
}) {
  const ppm = resolvePxPerM(doc.scale?.px_per_m);
  const st = resolveParityStyle(doc.style);
  const opts = { ppm, tema: st.tema, finish: st.vestigios, asfalto: st.asfalto };
  const build = (o: SicroTraceObject): ParamView<SicroTraceObject> => {
    const raioK = tp(o, "forma") === "area" ? 1.25 : 1;
    let anchor: SicroPoint;
    if (o.subtype === "colisao") {
      const r = tn(o, "raio") * ppm * raioK;
      anchor = { x: o.p0.x + r + 14, y: o.p0.y - r - 8 };
    } else {
      // Rótulo do lado oposto à cota.
      const C = Math.hypot(o.p1.x - o.p0.x, o.p1.y - o.p0.y) || 1e-6;
      const g = o.subtype === "fragmentos" ? null : traceGeomM(o, ppm);
      const m = g
        ? traceAt(g, g.len / 2)
        : { x: (o.p0.x + o.p1.x) / 2 / ppm, y: (o.p0.y + o.p1.y) / 2 / ppm, nx: -(o.p1.y - o.p0.y) / C, ny: (o.p1.x - o.p0.x) / C };
      const off = -(traceSpanM(o) + 0.9) * ppm - 8;
      anchor = { x: m.x * ppm + m.nx * off, y: m.y * ppm + m.ny * off };
    }
    const handles: ParamHandle<SicroTraceObject>[] =
      o.subtype === "colisao"
        ? [
            { x: o.p0.x, y: o.p0.y, patch: (x, y) => ({ p0: { x, y }, p1: { x, y } }) },
            raioHandle(o, tn(o, "raio"), ppm, raioK),
          ]
        : [...endHandles(o, false), ...(o.subtype !== "fragmentos" && o.subtype !== "sulcagem" ? [bendHandle(o)] : [])];
    return {
      draw: (ctx) => drawTrace(ctx, o, opts),
      hit: traceHitShape(o, ppm),
      handles,
      anchor,
      extra: o.show_measure ? <TraceCota o={o} ppm={ppm} tema={st.tema} /> : null,
      move: (dx, dy) => ({ p0: { x: o.p0.x + dx, y: o.p0.y + dy }, p1: { x: o.p1.x + dx, y: o.p1.y + dy } }),
    };
  };
  return <ParamNode obj={obj} build={build} draggable={draggable} solo={solo} handleR={7 / zoom} onSelect={onSelect} onChange={onChange} />;
}

/** Recuo da alça de alcance (m, mais o raio da alça em px): fora da cabeça do semáforo e da luminária. */
function fixtureHandleBackM(o: SicroFixtureObject): number {
  if (o.subtype === "semaforo" && fp(o, "braco")) return fn(o, "tam") * (fp(o, "grupo") === "ped" ? 0.35 : 0.5) + 0.2;
  if (o.subtype === "poste" && fp(o, "luminaria")) return 0.45;
  return 0;
}

function FixtureNode({
  obj,
  doc,
  draggable,
  solo,
  zoom,
  onSelect,
  onChange,
}: {
  obj: SicroFixtureObject;
  doc: SicroCroquiDoc;
  draggable: boolean;
  solo: boolean;
  zoom: number;
  onSelect: () => void;
  onChange: (patch: Partial<SicroObject>) => void;
}) {
  const ppm = resolvePxPerM(doc.scale?.px_per_m);
  const style = resolveParityStyle(doc.style);
  const build = (o: SicroFixtureObject): ParamView<SicroFixtureObject> => {
    const spec = FIXTURE_SPECS[o.subtype];
    const ext = fixtureExtentM(o) * ppm;
    let anchor: SicroPoint;
    let handles: ParamHandle<SicroFixtureObject>[];
    if (spec.forma === "linha") {
      const g = fixtureGeomM(spec.curva ? o : { ...o, bend: 0 }, ppm);
      const m = traceAt(g, g.len / 2);
      const off = -(ext + 0.9 * ppm) - 8;
      anchor = { x: m.x * ppm + m.nx * off, y: m.y * ppm + m.ny * off };
      handles = o.subtype === "area_conflito" ? cornerHandles(o, fn(o, "largura"), ppm) : [...endHandles(o, false), ...(spec.curva ? [bendHandle(o)] : [])];
    } else {
      anchor = { x: o.p0.x + ext + 14, y: o.p0.y - ext - 8 };
      handles =
        spec.p1 === "raio"
          ? [
              { x: o.p0.x, y: o.p0.y, patch: (x, y) => ({ p0: { x, y }, p1: { x, y } }) },
              raioHandle(o, fn(o, "raio"), ppm),
            ]
          : endHandles(o, true, fixtureHandleBackM(o) > 0 ? fixtureHandleBackM(o) * ppm + 11 / zoom : 0);
    }
    return {
      draw: (ctx) => drawFixture(ctx, o, { ppm, style }),
      hit: fixtureHitShape(o, ppm),
      handles,
      anchor,
      move: (dx, dy) => ({ p0: { x: o.p0.x + dx, y: o.p0.y + dy }, p1: { x: o.p1.x + dx, y: o.p1.y + dy } }),
    };
  };
  return <ParamNode obj={obj} build={build} draggable={draggable} solo={solo} handleR={7 / zoom} onSelect={onSelect} onChange={onChange} />;
}

const fmtM = (v: number, d = 1) => `${v.toFixed(d).replace(".", ",")} m`;

/** Cota do vestígio (sempre reta): comprimento ao lado, ou corda e flecha na derrapagem. */
function TraceCota({ o, ppm, tema }: { o: SicroTraceObject; ppm: number; tema: ParityTema }) {
  const color = tema === "escuro" ? "#e8e8e8" : "#333333";
  const halo = tema === "escuro" ? "#1c1c1c" : "#ffffff";
  const size = 12;
  const text = (t: string, x: number, y: number) => (
    <KonvaText
      text={t}
      x={x}
      y={y}
      offsetX={textWidthPx(t, size) / 2}
      offsetY={size / 2}
      fontSize={size}
      fontStyle="bold"
      fill={color}
      stroke={halo}
      strokeWidth={3}
      fillAfterStrokeEnabled
      listening={false}
    />
  );
  if (o.subtype === "colisao") return null;
  if (o.subtype === "derrapagem") {
    const g = traceGeomM(o, ppm);
    const s = (v: number) => v * ppm;
    const side = Math.sign(o.bend || 1);
    const off = 14;
    // "M" do lado de fora da curva, além da alça da flecha.
    const mText = (t: string) =>
      text(t, s(g.mid.x) + g.cn.x * side * (size + 10), s(g.mid.y) + g.cn.y * side * (size + 10));
    return (
      <Group listening={false}>
        <Line points={[o.p0.x, o.p0.y, o.p1.x, o.p1.y]} stroke={color} strokeWidth={1} dash={[4, 3]} />
        <Line points={[s(g.cm.x), s(g.cm.y), s(g.mid.x), s(g.mid.y)]} stroke={color} strokeWidth={1} dash={[4, 3]} />
        {text(`C ${fmtM(g.chord)}`, s(g.cm.x) - g.cn.x * off * side, s(g.cm.y) - g.cn.y * off * side)}
        {mText(`M ${fmtM(Math.abs(o.bend) / ppm, 2)}`)}
      </Group>
    );
  }
  const g =
    o.subtype === "fragmentos"
      ? traceGeom({ x: o.p0.x / ppm, y: o.p0.y / ppm }, { x: o.p1.x / ppm, y: o.p1.y / ppm }, 0)
      : traceGeomM(o, ppm);
  if (g.len < 0.05) return null;
  const d = o.subtype === "fragmentos" ? 0.6 : traceSpanM(o) + 0.9;
  const pts: number[] = [];
  const n = 40;
  for (let i = 0; i <= n; i++) {
    const p = traceAt(g, (g.len * i) / n);
    pts.push((p.x + p.nx * d) * ppm, (p.y + p.ny * d) * ppm);
  }
  const tick = (p: { x: number; y: number; nx: number; ny: number }) => {
    const x = (p.x + p.nx * d) * ppm;
    const y = (p.y + p.ny * d) * ppm;
    return [x - p.nx * 4, y - p.ny * 4, x + p.nx * 4, y + p.ny * 4];
  };
  const a = traceAt(g, 0);
  const b = traceAt(g, g.len);
  const m = traceAt(g, g.len / 2);
  const mx = (m.x + m.nx * d) * ppm + m.nx * 11;
  const my = (m.y + m.ny * d) * ppm + m.ny * 11;
  return (
    <Group listening={false}>
      <Line points={pts} stroke={color} strokeWidth={1} />
      <Line points={tick(a)} stroke={color} strokeWidth={1} />
      <Line points={tick(b)} stroke={color} strokeWidth={1} />
      {text(fmtM(o.subtype === "fragmentos" ? fanGeom(o, ppm).D : g.len), mx, my)}
    </Group>
  );
}

const HEAVY = new Set<VehicleBodyType>(["truck", "caminhao", "caminhao_pesado", "carreta", "reboque_guincho", "trator"]);
const BUS = new Set<VehicleBodyType>(["onibus", "micro_onibus", "onibus_leito"]);

/** Veículo em traço (planta técnica): contorno, para-brisa e vidro traseiro; +x = frente. */
function VehicleOutline({
  obj,
  body,
  tema,
  selected,
}: {
  obj: SicroVehicleObject;
  body: VehicleBodyType;
  tema: ParityTema;
  selected: boolean;
}) {
  const L = obj.width;
  const W = obj.height;
  const pal = tracePalette(tema);
  const fill = tema === "escuro" ? "#2a2f36" : "#ffffff";
  const twoWheel = body.startsWith("moto") || body.startsWith("bike");
  return (
    <>
      <Shape
        fill="#000"
        sceneFunc={(kctx) => {
          const ctx = (kctx as unknown as { _context: CanvasRenderingContext2D })._context;
          ctx.save();
          ctx.lineJoin = "round";
          ctx.lineCap = "round";
          ctx.strokeStyle = pal.traco;
          ctx.fillStyle = fill;
          ctx.lineWidth = 1.5;
          if (obj.tracejado) ctx.setLineDash([5, 3.5]);
          if (twoWheel) {
            ctx.lineWidth = Math.max(1.5, W * 0.2);
            ctx.beginPath();
            ctx.moveTo(L * 0.3, 0);
            ctx.lineTo(L * 0.5, 0);
            ctx.moveTo(-L * 0.5, 0);
            ctx.lineTo(-L * 0.3, 0);
            ctx.stroke();
            ctx.lineWidth = 1.5;
            ctx.beginPath();
            ctx.roundRect(-L * 0.32, -W * 0.24, L * 0.62, W * 0.48, W * 0.2);
            ctx.fill();
            ctx.stroke();
            ctx.beginPath();
            ctx.moveTo(L * 0.24, -W * 0.48);
            ctx.lineTo(L * 0.24, W * 0.48);
            ctx.stroke();
          } else {
            const heavy = HEAVY.has(body);
            const bus = BUS.has(body);
            ctx.beginPath();
            ctx.roundRect(-L / 2, -W / 2, L, W, heavy || bus ? W * 0.08 : Math.min(W * 0.25, L * 0.1));
            ctx.fill();
            ctx.stroke();
            ctx.lineWidth = 1;
            ctx.beginPath();
            if (heavy) {
              const cab = L / 2 - W * 1.05;
              ctx.moveTo(cab, -W / 2);
              ctx.lineTo(cab, W / 2);
              ctx.moveTo(L / 2 - W * 0.14, -W * 0.42);
              ctx.lineTo(L / 2 - W * 0.14, W * 0.42);
            } else if (bus) {
              ctx.moveTo(L / 2 - W * 0.16, -W * 0.44);
              ctx.lineTo(L / 2 - W * 0.16, W * 0.44);
              ctx.moveTo(-L / 2 + W * 0.1, -W * 0.4);
              ctx.lineTo(-L / 2 + W * 0.1, W * 0.4);
            } else {
              ctx.moveTo(L * 0.17, -W * 0.43);
              ctx.quadraticCurveTo(L * 0.23, 0, L * 0.17, W * 0.43);
              ctx.moveTo(-L * 0.28, -W * 0.4);
              ctx.quadraticCurveTo(-L * 0.32, 0, -L * 0.28, W * 0.4);
              ctx.moveTo(L * 0.17, -W * 0.43);
              ctx.lineTo(-L * 0.28, -W * 0.4);
              ctx.moveTo(L * 0.17, W * 0.43);
              ctx.lineTo(-L * 0.28, W * 0.4);
              ctx.moveTo(L * 0.2, -W / 2);
              ctx.lineTo(L * 0.16, -W / 2 - W * 0.1);
              ctx.moveTo(L * 0.2, W / 2);
              ctx.lineTo(L * 0.16, W / 2 + W * 0.1);
            }
            ctx.stroke();
          }
          ctx.restore();
        }}
        hitFunc={(kctx, shape) => {
          kctx.beginPath();
          kctx.rect(-L / 2, -W / 2, L, W);
          kctx.closePath();
          kctx.fillShape(shape);
        }}
      />
      {selected && (
        <Rect
          x={-L / 2 - 3}
          y={-W / 2 - 3}
          width={L + 6}
          height={W + 6}
          stroke="#38bdf8"
          strokeWidth={1.5}
          dash={[6, 4]}
          listening={false}
        />
      )}
    </>
  );
}

/** Alça de ponto (px de mundo, raio fixo como nas vias). Eventos não sobem ao grupo. */
function PointHandle({
  x,
  y,
  color,
  diamond,
  r = 7,
  onMove,
  onEnd,
}: {
  x: number;
  y: number;
  color: string;
  /** Raio (px de mundo); padrão 7, como nas vias. */
  r?: number;
  /** Losango: alça da flecha (curvatura). */
  diamond?: boolean;
  onMove: (x: number, y: number) => void;
  onEnd: (x: number, y: number) => void;
}) {
  const Node = diamond ? RegularPolygon : Circle;
  return (
    <Node
      sides={4}
      x={x}
      y={y}
      radius={diamond ? r * 1.2 : r}
      fill="#ffffff"
      stroke={color}
      strokeWidth={(2 * r) / 7}
      draggable
      onMouseEnter={(e) => setCursor(e, "crosshair")}
      onMouseLeave={(e) => setCursor(e, "default")}
      onClick={(e) => {
        e.cancelBubble = true;
      }}
      onDragStart={(e) => {
        e.cancelBubble = true;
      }}
      onDragMove={(e) => {
        e.cancelBubble = true;
        onMove(e.target.x(), e.target.y());
      }}
      onDragEnd={(e) => {
        e.cancelBubble = true;
        onEnd(e.target.x(), e.target.y());
      }}
    />
  );
}

/** Rótulo solto: arrastável, com deslocamento/tamanho/cor próprios (`label_*`). */
function ObjectLabel({
  obj,
  text,
  anchor,
  rotation = 0,
  draggable,
  outlineTema,
  onSelect,
  onChange,
}: {
  obj: SicroObject;
  text?: string | null;
  anchor: SicroPoint;
  rotation?: number;
  draggable: boolean;
  outlineTema?: ParityTema;
  onSelect: () => void;
  onChange: (patch: Partial<SicroObject>) => void;
}) {
  const t = text ?? obj.label;
  if (!t) return null;
  const d = labelDefaults(obj, outlineTema);
  const lf = obj as LabelFields;
  const size = lf.label_size ?? d.size;
  return (
    <KonvaText
      text={t}
      x={anchor.x + (lf.label_dx ?? d.dx)}
      y={anchor.y + (lf.label_dy ?? d.dy)}
      rotation={lf.label_rotation ?? rotation}
      offsetX={d.center ? textWidthPx(t, size) / 2 : 0}
      offsetY={d.center ? size / 2 : 0}
      fontSize={size}
      fontStyle="bold"
      fill={lf.label_color ?? d.color}
      draggable={draggable}
      listening={draggable}
      onClick={onSelect}
      onTap={onSelect}
      onMouseEnter={(e) => setCursor(e, "move")}
      onMouseLeave={(e) => setCursor(e, "default")}
      onDragStart={(e) => {
        e.cancelBubble = true;
        onSelect();
      }}
      onDragMove={(e) => {
        e.cancelBubble = true;
      }}
      onDragEnd={(e) => {
        e.cancelBubble = true;
        onChange({
          label_dx: e.target.x() - anchor.x,
          label_dy: e.target.y() - anchor.y,
        } as Partial<SicroObject>);
      }}
    />
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
  // 30° de cada lado da ponta.
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

/** Arte de pedestre (decúbito) do subtype, com cache no engine. */
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
  // Pedestre com arte: `size` é o comprimento do corpo deitado; a largura
  // segue a proporção real da prancha.
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

/** Glifo por subtype de `marker`, centrado em (0,0); o Group pai posiciona. */
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

  // ----- Vestígios -----

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
  solo,
  onSelect,
  onChange,
}: {
  obj: SicroMeasurementObject;
  doc: SicroCroquiDoc;
  draggable: boolean;
  selected: boolean;
  solo: boolean;
  onSelect: () => void;
  onChange: (patch: Partial<SicroObject>) => void;
}) {
  const [live, setLive] = useState<{ p1: SicroPoint; p2: SicroPoint } | null>(null);
  const p1 = live?.p1 ?? obj.p1;
  const p2 = live?.p2 ?? obj.p2;
  const px = distancePx(p1, p2);
  const mid = midpoint(p1, p2);
  const a = angleDeg(p1, p2);
  // Texto sempre legível: nunca de cabeça para baixo.
  const rot = a > 90 ? a - 180 : a < -90 ? a + 180 : a;
  const text = obj.label_override ?? formatMeasurement(px, doc.scale?.px_per_m);
  const color = obj.color ?? "#dc2626";
  const size = obj.label_size ?? labelDefaults(obj).size;
  const rad = (rot * Math.PI) / 180;
  // Reto por padrão; null = acompanha a linha.
  const labelRot = obj.label_rotation === null ? rot : (obj.label_rotation ?? 0);
  // Afasta pela meia-largura da caixa girada na normal, para o texto não cruzar a linha.
  const lr = ((labelRot - rot) * Math.PI) / 180;
  const half =
    Math.abs(Math.sin(lr)) * (textWidthPx(text, size) / 2) + Math.abs(Math.cos(lr)) * (size / 2);
  const gap = half + size * 0.4;
  // Rótulo do lado de cima da linha, no sentido de leitura.
  const anchor = { x: mid.x + Math.sin(rad) * gap, y: mid.y - Math.cos(rad) * gap };
  // Traços de extremidade perpendiculares.
  const tick = 6;
  const nx = -Math.sin((a * Math.PI) / 180) * tick;
  const ny = Math.cos((a * Math.PI) / 180) * tick;
  const canEdit = draggable && selected && !obj.locked;
  const commit = (n1: SicroPoint, n2: SicroPoint) => {
    setLive(null);
    onChange({ p1: n1, p2: n2 } as Partial<SicroObject>);
  };

  return (
    <>
      <Group
        id={obj.id}
        draggable={draggable && !obj.locked}
        onClick={onSelect}
        onTap={onSelect}
        onDragEnd={(e) => {
          if (e.target !== e.currentTarget) return;
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
          points={[p1.x, p1.y, p2.x, p2.y]}
          stroke={color}
          strokeWidth={selected ? 2 : 1.5}
          hitStrokeWidth={12}
        />
        <Line points={[p1.x - nx, p1.y - ny, p1.x + nx, p1.y + ny]} stroke={color} strokeWidth={1.5} listening={false} />
        <Line points={[p2.x - nx, p2.y - ny, p2.x + nx, p2.y + ny]} stroke={color} strokeWidth={1.5} listening={false} />
        {canEdit && (
          <>
            <PointHandle
              x={p1.x}
              y={p1.y}
              color={color}
              onMove={(x, y) => setLive({ p1: { x, y }, p2 })}
              onEnd={(x, y) => commit({ x, y }, p2)}
            />
            <PointHandle
              x={p2.x}
              y={p2.y}
              color={color}
              onMove={(x, y) => setLive({ p1, p2: { x, y } })}
              onEnd={(x, y) => commit(p1, { x, y })}
            />
          </>
        )}
      </Group>
      <ObjectLabel
        obj={obj}
        text={text}
        anchor={anchor}
        rotation={labelRot}
        draggable={draggable && !obj.locked && solo}
        onSelect={onSelect}
        onChange={onChange}
      />
    </>
  );
}

// ===========================================================================
// Previews (camada de UI)

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

/** Preview da via em rascunho; mesmo `tension` da via final para bater a forma. */
function RoadDraftPreview({ editor }: { editor: EditorState }) {
  const draft = editor.roadDraft;
  if (!draft || draft.points.length === 0) return null;
  const cursor = editor.pointerWorld;
  const flat: number[] = [];
  for (const pt of draft.points) flat.push(pt.x, pt.y);
  const previewFlat = [...flat, cursor.x, cursor.y];
  return (
    <Group listening={false}>
      {/* Asfalto fantasma: mostra a largura que a via vai ter. */}
      <Line
        points={previewFlat}
        stroke="#3f3f46"
        strokeWidth={40}
        opacity={0.25}
        tension={0.5}
        lineCap="round"
        lineJoin="round"
      />
      <Line
        points={previewFlat}
        stroke="#0ea5e9"
        strokeWidth={1.5}
        dash={[6, 4]}
        tension={0.5}
      />
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
// Helpers

function toWorld(stage: Konva.Stage, screen: SicroPoint): SicroPoint {
  const scale = stage.scaleX();
  const x = (screen.x - stage.x()) / scale;
  const y = (screen.y - stage.y()) / scale;
  return { x, y };
}

/** Linhas, cota, escala e vias: dois pontos (clique-clique ou arrastar). */
function isTwoPointTool(tool: Tool): boolean {
  return (
    tool.startsWith("line_") ||
    tool.startsWith("road_") ||
    (tool.startsWith("trace_") && tool !== "trace_colisao") ||
    isLinearFixtureTool(tool) ||
    tool === "measurement" ||
    tool === "set_scale"
  );
}

function isLinearFixtureTool(tool: Tool): boolean {
  const sub = tool.startsWith("fixture_") ? tool.slice("fixture_".length) : "";
  return sub in FIXTURE_SPECS && FIXTURE_SPECS[sub as keyof typeof FIXTURE_SPECS].forma === "linha";
}

function isAddTool(tool: Tool): boolean {
  return (
    tool.startsWith("vehicle") ||
    tool.startsWith("line_") ||
    tool.startsWith("marker_") ||
    tool.startsWith("road_") ||
    tool.startsWith("trace_") ||
    tool.startsWith("fixture_") ||
    tool.startsWith("person_") ||
    tool === "text" ||
    tool === "measurement" ||
    tool === "set_scale"
  );
}

function clamp(v: number, min: number, max: number): number {
  return Math.max(min, Math.min(max, v));
}

function resolveAssetPath(workspacePath: string, src: string): string | null {
  // Caminho absoluto vai direto; relativo é juntado ao workspace. Sempre
  // passa pelo asset protocol do Tauri.
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
