/**
 * CorpoCanvas — palco Konva do croqui corporal.
 *
 * Renderiza a prancha de corpo (SVG → imagem de fundo) + os marcadores de lesão
 * numerados e tipados por cima. Zoom (scroll), pan (arrastar o palco) e:
 *   - ferramenta de lesão ativa + clique no vazio → coloca marcador (onPlace);
 *   - clique num marcador → seleciona (onSelect);
 *   - arrastar um marcador → move (onMove).
 *
 * Padrão herdado do croqui viário (CanvasStage) e do marcador numerado do
 * módulo Imagem (Group: círculo + número branco).
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
  Group,
  Image as KonvaImage,
  Layer,
  Rect,
  Stage,
  Text as KonvaText,
} from "react-konva";
import type Konva from "konva";
import {
  BODY_TEMPLATES,
  POP_COSTAS,
  POP_FRENTE,
  lesaoMeta,
  popPointKey,
  type BodyTemplateView,
  type LesaoTipo,
  type PopCalibration,
  type PopRegiao,
  type PopSide,
  type SicroCorpoDoc,
} from "../engine";

/** Numeração de REGIÃO do POP (fixa sobre a arte): só o número, em preto,
 *  com um halo branco fino pra legibilidade — sem círculo (ocupava espaço
 *  demais e poluía o modelo). */
const REGION_NUM_SIZE = 11;

interface PopDot {
  key: string;
  n: number;
  x: number;
  y: number;
  // box da vista (pra normalizar de volta ao arrastar na calibração).
  bx: number;
  by: number;
  bw: number;
  bh: number;
}

/** Pontos absolutos (px da arte) da numeração POP de uma vista, já com a
 *  calibração manual do perito aplicada sobre o default (se houver). */
function popDotsForView(
  view: BodyTemplateView,
  regioes: ReadonlyArray<PopRegiao>,
  templateId: string,
  overrides: PopCalibration,
): PopDot[] {
  const out: PopDot[] = [];
  const side = view.id as PopSide; // "frente" | "costas"
  for (const r of regioes) {
    r.pts.forEach(([dnx, dny], i) => {
      const key = popPointKey(templateId, side, r.n, i);
      const ov = overrides[key];
      const nx = ov ? ov[0] : dnx;
      const ny = ov ? ov[1] : dny;
      out.push({
        key,
        n: r.n,
        bx: view.box.x,
        by: view.box.y,
        bw: view.box.w,
        bh: view.box.h,
        x: view.box.x + nx * view.box.w,
        y: view.box.y + ny * view.box.h,
      });
    });
  }
  return out;
}

export interface CorpoCanvasHandle {
  /** PNG data URL da prancha + marcadores (sem chrome). */
  toPng(pixelRatio?: number): string | null;
}

export type CorpoTool = LesaoTipo | "select";

interface Props {
  doc: SicroCorpoDoc;
  tool: CorpoTool;
  selectedId: string | null;
  containerWidth: number;
  containerHeight: number;
  onPlace: (x: number, y: number) => void;
  onSelect: (id: string | null) => void;
  onMove: (id: string, x: number, y: number) => void;
  /** Modo calibração: a numeração POP vira alça arrastável. */
  calibrating?: boolean;
  /** Overrides de posição da numeração POP (chave → [nx, ny]). */
  regionOverrides?: PopCalibration;
  /** Chamado ao soltar um número arrastado (posição normalizada na vista). */
  onRegionDrag?: (key: string, nx: number, ny: number) => void;
}

const ZOOM_MIN = 0.2;
const ZOOM_MAX = 8;
const PADDING = 0.88; // sobra ao redor da prancha no fit inicial

export const CorpoCanvas = forwardRef<CorpoCanvasHandle, Props>(
  function CorpoCanvas(
    {
      doc,
      tool,
      selectedId,
      containerWidth,
      containerHeight,
      onPlace,
      onSelect,
      onMove,
      calibrating = false,
      regionOverrides = {},
      onRegionDrag,
    },
    ref,
  ) {
    const stageRef = useRef<Konva.Stage | null>(null);
    const tpl = BODY_TEMPLATES[doc.template_id];

    // Imagem da prancha (PNG bitmap → HTMLImageElement), recarrega ao trocar
    // de prancha. `tpl.src` é a URL do asset (import do Vite).
    const [img, setImg] = useState<HTMLImageElement | null>(null);
    useEffect(() => {
      let alive = true;
      const im = new Image();
      im.onload = () => {
        if (alive) setImg(im);
      };
      im.src = tpl.src;
      setImg(null);
      return () => {
        alive = false;
      };
    }, [tpl.src]);

    // Numeração POP (frente 1–28 / costas 1–21) — só nas pranchas numeradas.
    const popDots = useMemo(() => {
      if (!tpl.numbered) return [];
      return tpl.views.flatMap((v) =>
        popDotsForView(
          v,
          v.id === "frente" ? POP_FRENTE : POP_COSTAS,
          doc.template_id,
          regionOverrides,
        ),
      );
    }, [tpl, doc.template_id, regionOverrides]);

    // Fit inicial (centraliza a prancha na viewport). Recalcula quando a
    // viewport ou a prancha mudam de tamanho.
    const fit = useMemo(() => {
      if (containerWidth <= 0 || containerHeight <= 0) {
        return { scale: 1, x: 0, y: 0 };
      }
      const s =
        Math.min(containerWidth / tpl.width, containerHeight / tpl.height) *
        PADDING;
      return {
        scale: s,
        x: (containerWidth - tpl.width * s) / 2,
        y: (containerHeight - tpl.height * s) / 2,
      };
    }, [containerWidth, containerHeight, tpl.width, tpl.height]);

    const [view, setView] = useState(fit);
    // Re-fit quando a prancha troca ou a viewport muda significativamente.
    useEffect(() => {
      setView(fit);
    }, [fit]);

    useImperativeHandle(ref, () => ({
      /**
       * PNG SÓ DA PRANCHA (a "folha" branca), em resolução nativa da arte ×
       * `scale` — INDEPENDENTE do zoom/pan e do tamanho da janela. Antes
       * capturava o palco inteiro (viewport), o que gerava exports gigantes
       * cheios de vazio quando a tela era grande.
       */
      toPng(scale = 1) {
        const stage = stageRef.current;
        if (!stage) return null;
        try {
          const sc = stage.scaleX() || 1;
          return stage.toDataURL({
            x: stage.x(),
            y: stage.y(),
            width: tpl.width * sc,
            height: tpl.height * sc,
            pixelRatio: scale / sc,
          });
        } catch {
          return null;
        }
      },
    }));

    const onWheel = (e: Konva.KonvaEventObject<WheelEvent>) => {
      e.evt.preventDefault();
      const stage = stageRef.current;
      if (!stage) return;
      const pointer = stage.getPointerPosition();
      if (!pointer) return;
      const oldScale = view.scale;
      const factor = e.evt.deltaY > 0 ? 0.9 : 1.1;
      const next = Math.max(ZOOM_MIN, Math.min(ZOOM_MAX, oldScale * factor));
      const worldX = (pointer.x - view.x) / oldScale;
      const worldY = (pointer.y - view.y) / oldScale;
      setView({
        scale: next,
        x: pointer.x - worldX * next,
        y: pointer.y - worldY * next,
      });
    };

    const isLesionTool = tool !== "select" && !calibrating;

    const onStageMouseDown = (e: Konva.KonvaEventObject<MouseEvent>) => {
      // Na calibração o palco só serve pra pan: não coloca lesão nem deseleciona.
      if (calibrating) return;
      // Clique no fundo (palco/imagem), não num marcador.
      const targetName = e.target.name?.() ?? "";
      const clickedEmpty =
        e.target === e.target.getStage() || targetName === "bg" || targetName === "";
      if (!clickedEmpty) return;
      if (isLesionTool) {
        const stage = stageRef.current;
        const pos = stage?.getRelativePointerPosition();
        if (pos) onPlace(pos.x, pos.y);
      } else {
        onSelect(null);
      }
    };

    return (
      <Stage
        ref={stageRef}
        width={Math.max(1, containerWidth)}
        height={Math.max(1, containerHeight)}
        scaleX={view.scale}
        scaleY={view.scale}
        x={view.x}
        y={view.y}
        draggable={tool === "select"}
        onWheel={onWheel}
        onMouseDown={onStageMouseDown}
        onDragEnd={(e) => {
          // Só o palco (pan) atualiza a view; drag de marcador é tratado nele.
          if (e.target === e.target.getStage()) {
            setView((v) => ({ ...v, x: e.target.x(), y: e.target.y() }));
          }
        }}
        style={{ background: "#e2e8f0", cursor: isLesionTool ? "crosshair" : "default" }}
      >
        <Layer listening={false}>
          {/* "Folha" da prancha — delimita EXATAMENTE a área exportada. */}
          <Rect
            x={0}
            y={0}
            width={tpl.width}
            height={tpl.height}
            fill="#ffffff"
            shadowColor="#0f172a"
            shadowBlur={18}
            shadowOpacity={0.25}
            shadowOffsetY={3}
            name="bg"
          />
          {img && (
            <KonvaImage
              image={img}
              width={tpl.width}
              height={tpl.height}
              // Pranchas que usam só um RECORTE da arte (ex.: perfis masc/fem
              // dividem o mesmo PNG) — o crop seleciona a figura.
              crop={
                tpl.crop
                  ? { x: tpl.crop.x, y: tpl.crop.y, width: tpl.crop.w, height: tpl.crop.h }
                  : undefined
              }
              name="bg"
            />
          )}
        </Layer>
        {/* Numeração de regiões do POP + título de cada vista. Camada NÃO
            interativa entre a arte e os marcadores de lesão — entra no export
            do palco (toDataURL) de graça. */}
        <Layer listening={calibrating}>
          {popDots.map((d) =>
            calibrating ? (
              // Modo calibração: alça âmbar arrastável. Ao soltar, devolve a
              // posição normalizada no box da vista (clampada em 0..1).
              <Group
                key={d.key}
                x={d.x}
                y={d.y}
                draggable
                onMouseDown={(e) => {
                  e.cancelBubble = true;
                }}
                onDragEnd={(e) => {
                  const nx = (e.target.x() - d.bx) / d.bw;
                  const ny = (e.target.y() - d.by) / d.bh;
                  onRegionDrag?.(
                    d.key,
                    Math.min(1, Math.max(0, nx)),
                    Math.min(1, Math.max(0, ny)),
                  );
                }}
              >
                <Circle
                  radius={11}
                  fill="#f59e0b"
                  opacity={0.92}
                  stroke="#ffffff"
                  strokeWidth={2}
                />
                <KonvaText
                  text={String(d.n)}
                  fontSize={11}
                  fontStyle="bold"
                  fontFamily="Arial"
                  fill="#1f2937"
                  width={30}
                  height={16}
                  offsetX={15}
                  offsetY={8}
                  align="center"
                  verticalAlign="middle"
                  listening={false}
                />
              </Group>
            ) : (
              <KonvaText
                key={d.key}
                x={d.x}
                y={d.y}
                text={String(d.n)}
                fontSize={REGION_NUM_SIZE}
                fontStyle="bold"
                fontFamily="Arial"
                fill="#0f172a"
                stroke="#ffffff"
                strokeWidth={2.2}
                fillAfterStrokeEnabled
                lineJoin="round"
                width={44}
                height={REGION_NUM_SIZE + 6}
                offsetX={22}
                offsetY={(REGION_NUM_SIZE + 6) / 2}
                align="center"
                verticalAlign="middle"
              />
            ),
          )}
          {tpl.views.map((v) => (
            <KonvaText
              key={`caption-${v.id}`}
              text={v.label}
              fontSize={18}
              fontStyle="bold"
              fill="#475569"
              x={v.box.x}
              y={Math.min(v.box.y + v.box.h + 14, tpl.height - 24)}
              width={v.box.w}
              align="center"
              listening={false}
            />
          ))}
        </Layer>
        <Layer>
          {/* Marcações AUTOCONTIDAS por prancha: só as desta prancha aparecem
              (lesão marcada no perfil não "vaza" pro corpo inteiro). Markers
              antigos sem o campo (pré-campo template) pertencem à prancha
              atual do doc. */}
          {doc.markers
            .filter((m) => (m.template ?? doc.template_id) === doc.template_id)
            .map((m) => {
            const meta = lesaoMeta(m.tipo);
            const color = m.color || meta.color;
            const r = m.size ?? 12;
            const selected = m.id === selectedId;
            return (
              <Group
                key={m.id}
                x={m.x}
                y={m.y}
                draggable={tool === "select" && !calibrating}
                listening={!calibrating}
                onMouseDown={(e) => {
                  if (calibrating) return;
                  e.cancelBubble = true;
                  onSelect(m.id);
                }}
                onDragEnd={(e) => {
                  onMove(m.id, e.target.x(), e.target.y());
                }}
              >
                {selected && (
                  <Circle
                    radius={r + 4}
                    stroke="#0f172a"
                    strokeWidth={2}
                    dash={[4, 3]}
                  />
                )}
                <Circle
                  radius={r}
                  fill={color}
                  stroke="#ffffff"
                  strokeWidth={2}
                  shadowColor="#000000"
                  shadowBlur={2}
                  shadowOpacity={0.4}
                />
                <KonvaText
                  text={String(m.number)}
                  fontSize={r * 1.1}
                  fontStyle="bold"
                  fill="#ffffff"
                  width={r * 2}
                  height={r * 2}
                  offsetX={r}
                  offsetY={r}
                  align="center"
                  verticalAlign="middle"
                  listening={false}
                />
              </Group>
            );
          })}
        </Layer>
      </Stage>
    );
  },
);
