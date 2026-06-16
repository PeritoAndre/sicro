/**
 * ImageEditor — bancada do MVP 7 (Editor de Imagem Pericial).
 *
 * Layout (inspirado em Peritus, com Design System SICRO):
 *
 *   ┌──────────────────────────────────────────────────────────────┐
 *   │ ← Voltar  [Título]                          [Salvar][Export] │
 *   ├────┬────────────────────────────────────────────┬────────────┤
 *   │ T  │                                            │  Right     │
 *   │ b  │            Konva canvas                    │  panel     │
 *   │ ar │       (image base + annotations)           │  (tabs)    │
 *   │    │                                            │            │
 *   ├────┴────────────────────────────────────────────┴────────────┤
 *   │ zoom · x,y · ferramenta · tamanho · hash · status · feedback │
 *   └──────────────────────────────────────────────────────────────┘
 *
 * Tabs do painel direito:
 *   Camadas · Ajustes · Anotações · Histórico · Metadados.
 *
 * Decisão arquitetural (MVP):
 *   - Os ajustes (brightness/contrast/etc) são aplicados em **CSS
 *     filter** para o preview ao vivo — barato e instantâneo;
 *   - Na exportação enviamos os ajustes ao **backend Rust** que
 *     re-aplica via `image` crate produzindo bytes destrutivos
 *     reproduzíveis. O sidecar JSON registra exatamente os valores.
 *
 * As anotações são renderizadas em Konva por cima da imagem e ficam
 * no `.sicroimage` (não destrutivas).
 */

import {
  Fragment,
  useCallback,
  useEffect,
  useMemo,
  useRef,
  useState,
} from "react";
import {
  ArrowLeft,
  ArrowUpRight,
  Check,
  ChevronDown,
  Circle as CircleIcon,
  Crop as CropIcon,
  Eye,
  EyeOff,
  FileImage,
  BoxSelect,
  CircleDashed,
  Hand,
  Hash,
  Hexagon,
  Lasso,
  Layers,
  ListOrdered,
  Magnet,
  MousePointer2,
  PenLine,
  Ruler,
  Save,
  Search,
  Square,
  Trash2,
  Type as TypeIcon,
  X as XIcon,
  XSquare,
  type LucideIcon,
} from "lucide-react";
import { useNavigate } from "react-router-dom";
import { useShortcuts } from "@core/useShortcuts";
import { useImageEditRoundtripStore } from "@stores/imageEditRoundtripStore";
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
import { Button } from "@components/Button/Button";
import { commands } from "@core/commands";
import { toSicroError } from "@core/errors";
import type {
  BackendAdjustments,
  BackendOperation,
  ImageOperationLog,
} from "@domain/image_analysis";
import { useImagemStore } from "../store/imagemStore";
import {
  makeArrow,
  makeEllipse,
  makeLine,
  makeMeasurement,
  makeNumberedMarker,
  makePoint,
  makeRect,
  makeRedaction,
  makeText,
  type SicroAnnotation,
  type SicroImageDoc,
} from "../engine";
import {
  adjustmentsToCssFilter,
  assetUrl,
  channelsAllOn,
  CHANNEL_MIX_FILTER_ID,
  prettyBytes,
  shortHash,
} from "./shared";
import { CanvasRulers } from "./CanvasRulers";
import {
  ToolOptionsBar,
  DEFAULT_TOOL_STYLE,
  type ToolStyle,
} from "./ToolOptionsBar";
import { HistogramPanel } from "./HistogramPanel";
import { ExifPanel } from "./ExifPanel";
import {
  FilterGallery,
  LayersBar,
  FILTER_INDEX,
  makeProcessingOp,
  processingOpToBackendOperation,
  selectionToMaskSpec,
} from "./ProcessingStackPanel";
import {
  SelectionMarqueeLayer,
  type SelDraft,
} from "./SelectionMarqueeLayer";
import { LayersPanelPro } from "./LayersPanelPro";
import { simplifyPath, selectionArea } from "./selectionGeom";
import type {
  PixelLayerSource,
  ProcessingOp,
  ProcessingOpKind,
  SicroImageLayer,
  SicroImagePoint,
  SicroImageSelection,
} from "../engine/schema";
import { CommandPalette, type PaletteCommand } from "./CommandPalette";
import { ReportPreviewDialog } from "./ReportPreviewDialog";
import { UnsavedChangesModal } from "./UnsavedChangesModal";
import { useNavGuard } from "@app/navGuard";
import { FileText } from "lucide-react";
import styles from "./ImageEditor.module.css";

type Tool =
  | "select"
  | "pan"
  | "arrow"
  | "line"
  | "rect"
  | "ellipse"
  | "text"
  | "marker"
  | "point"
  | "measurement"
  | "redaction"
  | "set_scale"
  | "crop"
  // W20 — ferramentas de SELEÇÃO (região / máscara estilo Photoshop)
  | "select_rect"
  | "select_ellipse"
  | "select_lasso"
  | "select_polygon"
  | "select_magnetic";

/** W20 — true para as 5 ferramentas de seleção de região. */
function isSelectionTool(t: Tool): boolean {
  return (
    t === "select_rect" ||
    t === "select_ellipse" ||
    t === "select_lasso" ||
    t === "select_polygon" ||
    t === "select_magnetic"
  );
}

/**
 * W20 (S2) — kinds GEOMÉTRICOS não podem ser confinados a uma seleção (mudam
 * a dimensão da imagem; mascarar não faz sentido). Só filtros/tonais/cor
 * recebem o escopo "seleção".
 */
const NON_MASKABLE_KINDS: ReadonlySet<ProcessingOpKind> = new Set<ProcessingOpKind>(
  [
    "crop",
    "resize",
    "rotate_90_cw",
    "rotate_90_ccw",
    "rotate_180",
    "flip_horizontal",
    "flip_vertical",
    "perspective",
    "rotate_arbitrary",
  ],
);

/** W20 (S3) — carrega uma <img> a partir de uma URL/data-uri (promessa). */
function loadImageEl(src: string): Promise<HTMLImageElement> {
  return new Promise((resolve, reject) => {
    const img = new window.Image();
    img.onload = () => resolve(img);
    img.onerror = () =>
      reject(new Error("falha ao carregar imagem para composição"));
    img.src = src;
  });
}

/**
 * W20 (S3) — desenha as camadas de pixels sobre uma imagem base (full-res),
 * devolvendo PNG base64 (sem prefixo). Usado no export com filtros, onde o
 * backend produz só a base filtrada e as camadas precisam ser compostas por
 * cima. Camadas em coords de px da imagem (= base full-res).
 */
function compositePixelLayersToBase64(
  base: HTMLImageElement,
  layers: SicroImageLayer[],
  images: Record<string, HTMLImageElement>,
): string | null {
  const w = base.naturalWidth || base.width;
  const h = base.naturalHeight || base.height;
  if (!w || !h) return null;
  const canvas = document.createElement("canvas");
  canvas.width = w;
  canvas.height = h;
  const ctx = canvas.getContext("2d");
  if (!ctx) return null;
  ctx.drawImage(base, 0, 0, w, h);
  for (const l of layers) {
    const img = images[l.id];
    if (!img) continue;
    const rot = ((l.rotation ?? 0) * Math.PI) / 180;
    ctx.save();
    ctx.globalAlpha = l.opacity ?? 1;
    // Pivô no canto sup-esq (offset), idêntico ao render do Konva.
    ctx.translate(l.offset_x ?? 0, l.offset_y ?? 0);
    if (rot) ctx.rotate(rot);
    ctx.drawImage(img, 0, 0, l.width ?? img.width, l.height ?? img.height);
    ctx.restore();
  }
  ctx.globalAlpha = 1;
  const url = canvas.toDataURL("image/png");
  return url.replace(/^data:image\/png;base64,/, "") || null;
}

// W14.1 — limites de zoom. Teto alto (64x, paridade com a Documentoscopia)
// para inspeção em **nível de pixel** — o perito não deve se sentir limitado.
const ZOOM_MIN = 0.05;
const ZOOM_MAX = 64;
/** Acima deste zoom o render fica nítido (nearest-neighbor) para ver o pixel
 * real, sem interpolação. */
const PIXEL_CRISP_ZOOM = 3;
const clampZoom = (s: number) => Math.max(ZOOM_MIN, Math.min(ZOOM_MAX, s));

interface Props {
  workspacePath: string;
  onClose: () => void;
}

// W13.3 — Painel direito reorganizado por INTENÇÃO em 3 modos (estilo
// Lightroom/Affinity Personas), cada um com seções colapsáveis (accordion).
// Substitui as 8 abas planas que misturavam fazer / inspecionar / gerenciar.
type RightMode = "realcar" | "filtros" | "analisar" | "anotar";

const RIGHT_MODES: Array<{ key: RightMode; label: string; hint: string }> = [
  {
    key: "realcar",
    label: "Realçar",
    hint: "Ajustes de visualização (brilho/contraste/matiz/canais) — NÃO altera a evidência (§13).",
  },
  {
    key: "filtros",
    label: "Filtros",
    hint: "Catálogo de filtros forenses + pilha de processamento reprodutível.",
  },
  {
    key: "analisar",
    label: "Analisar",
    hint: "Histograma, EXIF, metadados e histórico — somente leitura/medição.",
  },
  { key: "anotar", label: "Anotar", hint: "Objetos sobre a imagem e camadas." },
];

export function ImageEditor({ workspacePath, onClose }: Props) {
  const analysis = useImagemStore((s) => s.activeAnalysis)!;
  const initialDoc = useImagemStore((s) => s.activeDoc)!;
  const saveActive = useImagemStore((s) => s.saveActive);

  // Pós-laudo S — Round-trip Laudo ↔ Imagem.
  const navigate = useNavigate();
  const roundtripState = useImageEditRoundtripStore((s) => s.state);
  const roundtripRequest = useImageEditRoundtripStore((s) => s.request);
  const completeRoundtrip = useImageEditRoundtripStore((s) => s.completeEdit);
  const isRoundtripActive = roundtripState === "editing" && !!roundtripRequest;
  const [returningToLaudo, setReturningToLaudo] = useState(false);

  const [doc, setDoc] = useState<SicroImageDoc>(initialDoc);
  // G12.22 — Modal de relatório pericial.
  const [reportOpen, setReportOpen] = useState(false);
  const [tool, setTool] = useState<Tool>("select");
  const [selectedId, setSelectedId] = useState<string | null>(null);
  const [pending, setPending] = useState<{ x: number; y: number } | null>(null);
  // W20 — rascunho da seleção em andamento (rect/elipse arrastando, laço, ou
  // polígono/magnética acumulando vértices). A seleção CONFIRMADA vive em
  // `doc.selection`.
  const [selDraft, setSelDraft] = useState<SelDraft | null>(null);
  // W20 — campo de gradiente (Sobel) reduzido para o snap da ferramenta
  // magnética; computado sob demanda no 1º uso e memoizado por imagem.
  const edgeFieldRef = useRef<{
    src: HTMLImageElement;
    buf: Float32Array;
    w: number;
    h: number;
    sx: number;
    sy: number;
  } | null>(null);
  // W20 (S3) — bitmaps das camadas de pixels (id → <img> carregada), camada
  // de pixels selecionada, e o diálogo de origem da cópia (original × resultado).
  const [pixelImages, setPixelImages] = useState<
    Record<string, HTMLImageElement>
  >({});
  const [selectedLayerId, setSelectedLayerId] = useState<string | null>(null);
  const [copyPrompt, setCopyPrompt] = useState(false);
  const [copyBusy, setCopyBusy] = useState(false);
  // W20 (S3) — refs dos nós Konva das camadas de pixels + o Transformer
  // (handles de redimensionar/rotacionar) que se prende à camada selecionada.
  const pixelNodeRefs = useRef<Record<string, Konva.Image | null>>({});
  const pixelTransformerRef = useRef<Konva.Transformer | null>(null);

  // Pós-laudo S — Crop tool state.
  // O fluxo: clica "Cortar" → tool="crop" → um retângulo de seleção
  // aparece já posicionado no centro da imagem (ou no crop atual, se
  // houver um). O perito ajusta arrastando o retângulo inteiro (move)
  // ou as 8 handles (4 cantos + 4 lados). "Aplicar" empurra a op
  // pra processing_stack + atualiza `cropApplied` para que o
  // KonvaImage renderize só a região recortada.
  const [cropPending, setCropPending] = useState<{
    x: number;
    y: number;
    width: number;
    height: number;
  } | null>(null);
  // Limite mínimo do retângulo de crop em pixels (impede colapsos).
  const CROP_MIN_PX = 16;

  // Crop ATIVO derivado do processing_stack: pega o último op crop habilitado
  // e usa as params {x,y,width,height} pra crop o KonvaImage.
  const cropApplied = useMemo(() => {
    const ops = doc.processing_stack ?? [];
    for (let i = ops.length - 1; i >= 0; i--) {
      const op = ops[i];
      if (op && op.enabled && op.kind === "crop") {
        const p = op.params as {
          x?: number;
          y?: number;
          width?: number;
          height?: number;
        };
        if (
          typeof p.x === "number" &&
          typeof p.y === "number" &&
          typeof p.width === "number" &&
          typeof p.height === "number"
        ) {
          return { x: p.x, y: p.y, width: p.width, height: p.height };
        }
      }
    }
    return null;
  }, [doc.processing_stack]);
  const [rightMode, setRightMode] = useState<RightMode>("realcar");
  // W13.3 — seções abertas do accordion (várias podem ficar abertas).
  const [openSections, setOpenSections] = useState<Set<string>>(
    () => new Set(["histogram", "annotations"]),
  );
  const toggleSection = useCallback((key: string) => {
    setOpenSections((prev) => {
      const next = new Set(prev);
      if (next.has(key)) next.delete(key);
      else next.add(key);
      return next;
    });
  }, []);
  const [feedback, setFeedback] = useState<string | null>(null);
  const [saving, setSaving] = useState(false);
  const [exporting, setExporting] = useState(false);
  const [pointer, setPointer] = useState({ x: 0, y: 0 });
  const [viewport, setViewport] = useState({ scale: 1, x: 0, y: 0 });
  // W13 — réguas (topo+esquerda) que seguem o mouse; ligadas por padrão.
  const [showRulers, setShowRulers] = useState(true);
  // W13.2 — estilo padrão aplicado às novas anotações (barra de contexto).
  const [toolStyle, setToolStyle] = useState<ToolStyle>(DEFAULT_TOOL_STYLE);
  // W13.4 — trilha em grupos com flyout: qual grupo está aberto e qual a
  // ferramenta "lembrada" por grupo (estilo Photoshop: o slot mostra a última
  // usada do grupo).
  const [openGroup, setOpenGroup] = useState<string | null>(null);
  const [groupRep, setGroupRep] = useState<Record<string, Tool>>({});
  // W13.6 — paleta de comandos (⌘K).
  const [paletteOpen, setPaletteOpen] = useState(false);
  const [logs, setLogs] = useState<ImageOperationLog[]>([]);

  const stageRef = useRef<Konva.Stage | null>(null);
  const transformerRef = useRef<Konva.Transformer | null>(null);
  const objectsLayerRef = useRef<Konva.Layer | null>(null);
  // W14.2 fix — camada da imagem base (recebe o CSS filter dos ajustes; o
  // fundo do canvas e as anotações ficam de fora).
  const baseLayerRef = useRef<Konva.Layer | null>(null);
  const canvasWrapRef = useRef<HTMLDivElement>(null);
  const [stageSize, setStageSize] = useState({ width: 800, height: 600 });

  // W18 — guarda de alterações não salvas (espelha o Croqui). `dirty` = doc
  // atual difere do último snapshot salvo. Ao tentar sair (Voltar) ou trocar
  // de módulo (ActivityRail), pergunta antes: Salvar / Sair sem salvar.
  const [lastSavedJson, setLastSavedJson] = useState<string | null>(null);
  const dirty = useMemo(
    () => (lastSavedJson ? JSON.stringify(doc) !== lastSavedJson : false),
    [doc, lastSavedJson],
  );
  const [pendingNav, setPendingNav] = useState<null | {
    proceed: () => void;
    label?: string;
    resolve?: (proceed: boolean) => void;
  }>(null);
  // W18 — pilha (dock) pode ser recolhida para ganhar altura quando preciso.
  const [pipelineCollapsed, setPipelineCollapsed] = useState(false);
  const registerNavGuard = useNavGuard((s) => s.register);
  const unregisterNavGuard = useNavGuard((s) => s.unregister);

  // ----- Sync doc when store changes (e.g. opened a different analysis) -----
  // Também sincroniza o snapshot "salvo" para o `dirty` recomeçar limpo a cada
  // análise aberta.
  useEffect(() => {
    setDoc(initialDoc);
    setLastSavedJson(JSON.stringify(initialDoc));
  }, [initialDoc]);

  // ----- Resize observer for the canvas column -----
  useEffect(() => {
    const el = canvasWrapRef.current;
    if (!el) return;
    const ro = new ResizeObserver(() => {
      const rect = el.getBoundingClientRect();
      setStageSize({
        width: Math.max(200, rect.width),
        height: Math.max(200, rect.height),
      });
    });
    ro.observe(el);
    return () => ro.disconnect();
  }, []);

  // ----- Load the image and fit-to-screen on first paint -----
  const imageUrl = useMemo(
    () => assetUrl(workspacePath, doc.source.original_relative_path),
    [workspacePath, doc.source.original_relative_path],
  );
  const [htmlImage, setHtmlImage] = useState<HTMLImageElement | null>(null);
  useEffect(() => {
    if (!imageUrl) return;
    const img = new window.Image();
    img.crossOrigin = "anonymous";
    img.src = imageUrl;
    img.onload = () => {
      setHtmlImage(img);
      // Fit to screen if the source dimensions and stage are known.
      const sw = img.width;
      const sh = img.height;
      const padding = 20;
      const scale = Math.min(
        (stageSize.width - padding * 2) / sw,
        (stageSize.height - padding * 2) / sh,
        1,
      );
      const ox = (stageSize.width - sw * scale) / 2;
      const oy = (stageSize.height - sh * scale) / 2;
      setViewport({ scale, x: ox, y: oy });
    };
    img.onerror = () => setHtmlImage(null);
  }, [imageUrl, stageSize.width, stageSize.height]);

  // ----- W17 — Preview AO VIVO da pilha de filtros forenses -----
  // Antes, a processing_stack (Sobel/CLAHE/ELA/DStretch/…) NUNCA era aplicada
  // — adicionar filtro não mudava nada. Aqui o backend Rust aplica a pilha
  // (na ordem, incluindo crop) sobre a imagem ORIGINAL e o resultado é
  // renderizado no canvas; o export captura o canvas, então também sai com os
  // filtros. Debounce p/ os sliders. §13: derivado reprodutível do original.
  // `scale` = fator de downscale do preview (1 = sem reduzir). A imagem volta a
  // ser desenhada no tamanho LÓGICO (÷scale), preservando coordenadas/anotações.
  const [previewImage, setPreviewImage] = useState<{
    image: HTMLImageElement;
    scale: number;
  } | null>(null);
  const [previewBusy, setPreviewBusy] = useState(false);
  useEffect(() => {
    const ops = (doc.processing_stack ?? []).filter((o) => o.enabled !== false);
    const hasFilters = ops.some((o) => o.kind !== "crop");
    if (!hasFilters) {
      setPreviewImage(null);
      setPreviewBusy(false);
      return;
    }
    let cancelled = false;
    setPreviewBusy(true);
    const timer = setTimeout(() => {
      // §13: o preview é só VISUALIZAÇÃO; o derivado exportado é o fiel
      // (reaplicado em resolução cheia no backend).
      //
      // O original pericial pode ter dezenas de megapixels. Reabri-lo, filtrar
      // em resolução cheia, recodificar um PNG gigante e trafegar dezenas de MB
      // por base64 no IPC a cada filtro custava MINUTOS — até para uma operação
      // barata como "Níveis". Como a imagem já está decodificada no navegador
      // (`htmlImage`), reduzimos no canvas e mandamos só o bitmap pequeno ao
      // backend (apply_operation_stack_preview): decodificar/filtrar/codificar/
      // trafegar fica trivial e o preview é rápido independentemente do tamanho
      // do original.
      const sw = doc.source.width || htmlImage?.naturalWidth || 0;
      const sh = doc.source.height || htmlImage?.naturalHeight || 0;
      // Filtros O(w·h·k²) — mediana (raio ≥ 3) e bilateral — são caros; usam um
      // teto menor. Os baratos (Sobel, CLAHE, ELA, limiar, níveis…) ficam
      // nítidos no teto maior.
      const heavy = ops.some(
        (o) =>
          (o.kind === "blur_median" &&
            Number((o.params as { radius?: number }).radius ?? 0) >= 3) ||
          o.kind === "blur_bilateral",
      );
      const MAX_PREVIEW = heavy ? 720 : 1280;
      const longest = Math.max(sw, sh);
      const k = longest > MAX_PREVIEW && longest > 0 ? MAX_PREVIEW / longest : 1;
      const sc = (n: number) => Math.max(1, Math.round(n * k));
      const pw = sc(sw);
      const ph = sc(sh);

      // Operações: as coords de crop estão em px do original; como o bitmap vai
      // reduzido por `k`, escala as coords de crop também.
      const buildOps = (): BackendOperation[] => {
        const out: BackendOperation[] = [];
        for (const o of ops) {
          // W20 (S2) — passa as dims da fonte p/ normalizar a máscara da
          // seleção (op com escopo "seleção" → wrapper `masked`).
          const be = processingOpToBackendOperation(o, sw, sh) as Record<
            string,
            unknown
          >;
          if (k < 1 && o.kind === "crop") {
            be.x = sc(Number(be.x) || 0);
            be.y = sc(Number(be.y) || 0);
            be.width = sc(Number(be.width) || sw);
            be.height = sc(Number(be.height) || sh);
          }
          out.push(be as unknown as BackendOperation);
        }
        return out;
      };

      const onResult = (b64: string, mime: string) => {
        if (cancelled) return;
        const img = new window.Image();
        img.onload = () => {
          if (!cancelled) {
            setPreviewImage({ image: img, scale: k });
            setPreviewBusy(false);
          }
        };
        img.onerror = () => {
          if (!cancelled) setPreviewBusy(false);
        };
        img.src = `data:${mime};base64,${b64}`;
      };
      const onFail = (e: unknown) => {
        if (cancelled) return;
        setPreviewBusy(false);
        setFeedback(`Falha ao aplicar filtros: ${toSicroError(e).message}`);
      };

      // Caminho rápido: reduzir no cliente (canvas) e mandar o bitmap pequeno.
      let clientBitmap: string | null = null;
      if (htmlImage && longest > 0) {
        try {
          const canvas = document.createElement("canvas");
          canvas.width = pw;
          canvas.height = ph;
          const ctx = canvas.getContext("2d");
          if (ctx) {
            ctx.imageSmoothingEnabled = true;
            ctx.imageSmoothingQuality = "high";
            ctx.drawImage(htmlImage, 0, 0, pw, ph);
            const url = canvas.toDataURL("image/png");
            clientBitmap = url.slice(url.indexOf(",") + 1) || null;
          }
        } catch {
          // canvas "tainted"/erro → cai no fallback do backend.
          clientBitmap = null;
        }
      }

      if (clientBitmap) {
        commands
          .applyOperationStackPreview({
            image_base64: clientBitmap,
            operations: buildOps(),
          })
          .then((res) => onResult(res.image_base64, "image/png"))
          .catch(onFail);
      } else {
        // Fallback: o backend abre o original e aplica (prepende um `resize`
        // para reduzir). Caminho antigo, usado se o canvas não puder exportar.
        const backendOps: BackendOperation[] = [];
        if (k < 1) {
          backendOps.push({ kind: "resize", width: pw, height: ph });
        }
        backendOps.push(...buildOps());
        commands
          .applyOperationStack(workspacePath, {
            relative_path: doc.source.original_relative_path,
            operations: backendOps,
            as_jpeg: false,
          })
          .then((res) => onResult(res.image_base64, res.mime))
          .catch(onFail);
      }
    }, 220);
    return () => {
      cancelled = true;
      clearTimeout(timer);
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [
    doc.processing_stack,
    workspacePath,
    doc.source.original_relative_path,
    doc.source.width,
    doc.source.height,
    htmlImage,
  ]);

  // ----- Pós-laudo S — Inicializa o retângulo de crop ao entrar em modo -----
  //
  // Quando o perito ativa a tool "crop", o retângulo aparece pronto:
  //   - se já existe um crop aplicado: começa por ele (deixa ajustar);
  //   - senão: 80% centralizado da imagem.
  // Saída do modo (tool muda) limpa cropPending.
  useEffect(() => {
    if (tool !== "crop") {
      setCropPending(null);
      return;
    }
    if (!htmlImage) return;
    if (cropPending) return; // já inicializado
    if (cropApplied) {
      setCropPending({ ...cropApplied });
      return;
    }
    const iw = htmlImage.width;
    const ih = htmlImage.height;
    const w = Math.round(iw * 0.8);
    const h = Math.round(ih * 0.8);
    setCropPending({
      x: Math.round((iw - w) / 2),
      y: Math.round((ih - h) / 2),
      width: w,
      height: h,
    });
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [tool, htmlImage]);

  // ----- Load logs when the Analisar mode opens (Histórico vive nele) -----
  useEffect(() => {
    if (rightMode !== "analisar") return;
    commands
      .listImageOperationLogs(workspacePath, analysis.id, 50)
      .then(setLogs)
      .catch(() => setLogs([]));
  }, [rightMode, workspacePath, analysis.id]);

  // ----- Transformer wiring -----
  useEffect(() => {
    const tr = transformerRef.current;
    const layer = objectsLayerRef.current;
    if (!tr || !layer) return;
    if (!selectedId) {
      tr.nodes([]);
      tr.getLayer()?.batchDraw();
      return;
    }
    const node = layer.findOne(`#${selectedId}`);
    if (node) {
      tr.nodes([node]);
      tr.getLayer()?.batchDraw();
    } else {
      tr.nodes([]);
    }
  }, [selectedId, doc.annotations]);

  // ----- Mutators -----
  const addAnnotation = (raw: SicroAnnotation) => {
    // W13.2 — aplica o estilo atual da barra de contexto às anotações
    // desenháveis. Marcador numerado e ponto mantêm a identidade de cor
    // própria (não recebem o estilo padrão).
    const STYLEABLE = new Set([
      "arrow",
      "line",
      "rect",
      "ellipse",
      "text",
      "measurement",
      "redaction",
    ]);
    let a = raw;
    if (STYLEABLE.has(raw.kind)) {
      a = { ...raw, stroke: toolStyle.stroke, stroke_width: toolStyle.strokeWidth };
      if (raw.kind === "rect" || raw.kind === "ellipse") a.fill = toolStyle.fill;
      if (raw.kind === "text") a.fill = toolStyle.stroke; // texto: cor = stroke
    }
    setDoc((d) => ({
      ...d,
      annotations: [...d.annotations, a],
    }));
    setSelectedId(a.id);
  };

  const updateAnnotation = (id: string, patch: Partial<SicroAnnotation>) => {
    setDoc((d) => ({
      ...d,
      annotations: d.annotations.map((a) =>
        a.id === id ? { ...a, ...patch } : a,
      ),
    }));
  };

  const deleteAnnotation = (id: string) => {
    setDoc((d) => ({
      ...d,
      annotations: d.annotations.filter((a) => a.id !== id),
    }));
    setSelectedId(null);
  };

  const updateAdjustments = (patch: Partial<BackendAdjustments>) => {
    setDoc((d) => ({
      ...d,
      view_adjustments: { ...d.view_adjustments, ...patch },
    }));
  };

  // ----- Canvas click dispatcher -----
  const handleStageClick = (e: Konva.KonvaEventObject<MouseEvent>) => {
    if (e.target !== e.target.getStage()) {
      if (!isAddTool(tool)) return;
    }
    if (tool === "select") {
      setSelectedId(null);
      setSelectedLayerId(null); // W20 (S3) — clique vazio solta a camada (some handles)
      return;
    }
    if (tool === "pan") return;
    const stage = stageRef.current;
    if (!stage) return;
    const screen = stage.getPointerPosition();
    if (!screen) return;
    const world = {
      x: (screen.x - viewport.x) / viewport.scale,
      y: (screen.y - viewport.y) / viewport.scale,
    };

    // W20 — Seleção: poligonal/magnética adicionam um vértice por clique
    // (vértices ilimitados). Fecha de 3 formas: clicando de volta perto do
    // 1º vértice, duplo-clique no último ponto, ou Enter. Magnética dá um
    // "snap" do ponto à borda mais próxima. Rect/elipse/laço são por ARRASTO
    // (mousedown→up), então o clique é no-op aqui.
    if (tool === "select_polygon" || tool === "select_magnetic") {
      const pt = tool === "select_magnetic" ? snapToEdge(world) : world;
      // Fechar ao clicar de volta sobre o 1º vértice (≥ 3 pontos) — padrão
      // das ferramentas poligonais. O raio de "imã" acompanha o zoom.
      if (selDraft?.mode === "polygon" && selDraft.points.length >= 3) {
        const first = selDraft.points[0];
        if (
          first &&
          Math.hypot(world.x - first.x, world.y - first.y) < 10 / viewport.scale
        ) {
          closePolygonDraft();
          return;
        }
      }
      setSelDraft((d) =>
        d && d.mode === "polygon"
          ? { ...d, points: [...d.points, pt] }
          : { mode: "polygon", points: [pt], live: null },
      );
      return;
    }
    if (
      tool === "select_rect" ||
      tool === "select_ellipse" ||
      tool === "select_lasso"
    ) {
      return;
    }

    // Pós-laudo S — Em modo "crop", clique no canvas é no-op. O retângulo
    // de seleção é manipulado direto via drag das handles + corpo (ver
    // CropOverlayLayer abaixo).
    if (tool === "crop") return;

    if (tool === "marker") {
      const nextNumber =
        doc.annotations.filter((a) => a.kind === "numbered_marker").length + 1;
      addAnnotation(makeNumberedMarker(world.x, world.y, nextNumber));
      setTool("select");
      return;
    }
    if (tool === "point") {
      addAnnotation(makePoint(world.x, world.y));
      setTool("select");
      return;
    }
    if (tool === "text") {
      const text = window.prompt("Texto:", "Anotação");
      if (text == null || text.trim() === "") return;
      addAnnotation(makeText(world.x, world.y, text.trim()));
      setTool("select");
      return;
    }
    // Two-click tools: arrow, line, rect, ellipse, measurement, redaction, set_scale
    if (!pending) {
      setPending(world);
      return;
    }
    const p1 = pending;
    const p2 = world;
    setPending(null);
    if (tool === "arrow") addAnnotation(makeArrow(p1.x, p1.y, p2.x, p2.y));
    else if (tool === "line") addAnnotation(makeLine(p1.x, p1.y, p2.x, p2.y));
    else if (tool === "rect")
      addAnnotation(makeRect(p1.x, p1.y, p2.x - p1.x, p2.y - p1.y));
    else if (tool === "ellipse")
      addAnnotation(makeEllipse(p1.x, p1.y, p2.x - p1.x, p2.y - p1.y));
    else if (tool === "redaction")
      addAnnotation(makeRedaction(p1.x, p1.y, p2.x - p1.x, p2.y - p1.y));
    else if (tool === "measurement")
      addAnnotation(makeMeasurement(p1.x, p1.y, p2.x, p2.y));
    else if (tool === "set_scale") {
      const px = Math.hypot(p2.x - p1.x, p2.y - p1.y);
      const declared = window.prompt(
        `Distância real entre os dois pontos (em metros)?\nPixels medidos: ${px.toFixed(1)}`,
        "1",
      );
      if (!declared) return;
      const real = Number(declared.replace(",", "."));
      if (!Number.isFinite(real) || real <= 0) {
        setFeedback("Valor inválido — escala não atualizada.");
        return;
      }
      const pxPerUnit = px / real;
      setDoc((d) => ({
        ...d,
        scale: {
          px_per_unit: pxPerUnit,
          unit: "m",
          calibrated_by: [p1, p2],
          calibration_real_distance: real,
          created_at: new Date().toISOString(),
        },
      }));
      setFeedback(`Escala definida: ${pxPerUnit.toFixed(2)} px/m.`);
    }
    setTool("select");
  };

  // ----- Mouse + zoom handlers -----
  const handleStageMouseMove = () => {
    const stage = stageRef.current;
    if (!stage) return;
    const pos = stage.getPointerPosition();
    if (!pos) return;
    const world = {
      x: (pos.x - viewport.x) / viewport.scale,
      y: (pos.y - viewport.y) / viewport.scale,
    };
    setPointer(world);
    // W20 — atualiza o rascunho da seleção em andamento.
    setSelDraft((d) => {
      if (!d) return d;
      if (d.mode === "rect" || d.mode === "ellipse") {
        return { ...d, x1: world.x, y1: world.y };
      }
      if (d.mode === "lasso") {
        const last = d.points[d.points.length - 1];
        if (
          last &&
          Math.hypot(world.x - last.x, world.y - last.y) < 2 / viewport.scale
        ) {
          return d;
        }
        return { ...d, points: [...d.points, world] };
      }
      if (d.mode === "polygon") return { ...d, live: world };
      return d;
    });
  };

  // W20 — início do arrasto: rect/elipse iniciam a "borracha"; laço começa a
  // coletar o traço. (Poligonal/magnética usam clique, não arrasto.)
  const handleStageMouseDown = () => {
    if (
      tool !== "select_rect" &&
      tool !== "select_ellipse" &&
      tool !== "select_lasso"
    ) {
      return;
    }
    const stage = stageRef.current;
    if (!stage) return;
    const pos = stage.getPointerPosition();
    if (!pos) return;
    const world = {
      x: (pos.x - viewport.x) / viewport.scale,
      y: (pos.y - viewport.y) / viewport.scale,
    };
    if (tool === "select_lasso") {
      setSelDraft({ mode: "lasso", points: [world] });
    } else {
      setSelDraft({
        mode: tool === "select_rect" ? "rect" : "ellipse",
        x0: world.x,
        y0: world.y,
        x1: world.x,
        y1: world.y,
      });
    }
  };

  // W20 — fim do arrasto: confirma rect/elipse/laço (≥ tamanho mínimo).
  const handleStageMouseUp = () => {
    if (!selDraft) return;
    if (selDraft.mode === "rect" || selDraft.mode === "ellipse") {
      const x = Math.min(selDraft.x0, selDraft.x1);
      const y = Math.min(selDraft.y0, selDraft.y1);
      const w = Math.abs(selDraft.x1 - selDraft.x0);
      const h = Math.abs(selDraft.y1 - selDraft.y0);
      if (w >= 4 && h >= 4) {
        commitSelection({
          kind: selDraft.mode,
          x,
          y,
          width: w,
          height: h,
          source_tool: selDraft.mode === "rect" ? "select_rect" : "select_ellipse",
        });
      }
      setSelDraft(null);
    } else if (selDraft.mode === "lasso") {
      const pts = simplifyPath(selDraft.points, 1.5 / viewport.scale);
      if (pts.length >= 3) {
        commitSelection({
          kind: "polygon",
          points: pts,
          source_tool: "select_lasso",
        });
      }
      setSelDraft(null);
    }
    // polígono: continua acumulando (fecha por Enter / duplo-clique).
  };

  const handleStageDblClick = () => {
    if (selDraft?.mode !== "polygon") return;
    const pts = selDraft.points;
    // O Konva dispara `dblclick` sempre que dois cliques caem em < ~400ms,
    // INDEPENDENTE da posição. Se fechássemos em qualquer dblclick, clicar
    // vértices num ritmo normal encerraria o polígono cedo (era o bug dos
    // "3 cliques"). Por isso só tratamos como "finalizar" quando os dois
    // últimos vértices estão praticamente no mesmo ponto — ou seja, o 2º
    // clique do duplo-clique. Aí removemos o vértice duplicado e fechamos.
    if (pts.length < 2) return;
    const a = pts[pts.length - 1];
    const b = pts[pts.length - 2];
    if (!a || !b) return;
    if (Math.hypot(a.x - b.x, a.y - b.y) > 6 / viewport.scale) return;
    const deduped = pts.slice(0, -1);
    if (deduped.length >= 3) {
      commitSelection({
        kind: "polygon",
        points: deduped,
        source_tool: "select_polygon",
      });
    }
    setSelDraft(null);
  };

  const handleWheel = (e: Konva.KonvaEventObject<WheelEvent>) => {
    e.evt.preventDefault();
    const stage = stageRef.current;
    if (!stage) return;
    const pointerPos = stage.getPointerPosition();
    if (!pointerPos) return;
    const oldScale = viewport.scale;
    const direction = e.evt.deltaY > 0 ? -1 : 1;
    const factor = direction > 0 ? 1.08 : 0.92;
    const newScale = clampZoom(oldScale * factor);
    const mousePointTo = {
      x: (pointerPos.x - viewport.x) / oldScale,
      y: (pointerPos.y - viewport.y) / oldScale,
    };
    setViewport({
      scale: newScale,
      x: pointerPos.x - mousePointTo.x * newScale,
      y: pointerPos.y - mousePointTo.y * newScale,
    });
  };

  // ----- View / zoom helpers (teclado; o zoom por roda fica em handleWheel) ---
  // Mesmos limites do handleWheel (ZOOM_MIN..ZOOM_MAX). Zoom in/out ancorado no
  // centro visível; "1:1" volta para escala 1 centralizando a imagem;
  // "enquadrar" recalcula o fit-to-screen (mesma fórmula do carregamento).
  const fitToScreen = useCallback(() => {
    if (!htmlImage) return;
    const sw = htmlImage.width;
    const sh = htmlImage.height;
    const padding = 20;
    const scale = Math.min(
      (stageSize.width - padding * 2) / sw,
      (stageSize.height - padding * 2) / sh,
      1,
    );
    setViewport({
      scale,
      x: (stageSize.width - sw * scale) / 2,
      y: (stageSize.height - sh * scale) / 2,
    });
  }, [htmlImage, stageSize.width, stageSize.height]);

  const zoomAroundCenter = useCallback(
    (factor: number) => {
      setViewport((vp) => {
        const cx = stageSize.width / 2;
        const cy = stageSize.height / 2;
        const newScale = clampZoom(vp.scale * factor);
        const worldX = (cx - vp.x) / vp.scale;
        const worldY = (cy - vp.y) / vp.scale;
        return {
          scale: newScale,
          x: cx - worldX * newScale,
          y: cy - worldY * newScale,
        };
      });
    },
    [stageSize.width, stageSize.height],
  );

  const handleZoomActual = useCallback(() => {
    if (!htmlImage) {
      setViewport((vp) => ({ ...vp, scale: 1 }));
      return;
    }
    const sw = htmlImage.width;
    const sh = htmlImage.height;
    setViewport({
      scale: 1,
      x: (stageSize.width - sw) / 2,
      y: (stageSize.height - sh) / 2,
    });
  }, [htmlImage, stageSize.width, stageSize.height]);

  const toggleAnnotations = useCallback(() => {
    setDoc((d) => ({
      ...d,
      layers: d.layers.map((l) =>
        l.id === "layer_annotations" ? { ...l, visible: l.visible === false } : l,
      ),
    }));
  }, []);

  // ----- Keyboard shortcuts (customizáveis, escopo `imagem`) -----
  // `useShortcuts` já ignora foco em INPUT/TEXTAREA/SELECT (mesmo guard do
  // listener manual). handleSave / handleExport / applyCrop / cancelCrop são
  // definidos mais abaixo; como só são lidos no disparo da tecla, a ordem de
  // declaração não importa.
  useShortcuts({
    // Esc — unifica: em modo corte cancela o corte; senão limpa pending /
    // seleção e volta pra ferramenta de seleção.
    "imagem.cancel": () => {
      if (selDraft) {
        setSelDraft(null);
        return;
      }
      if (tool === "crop") {
        cancelCrop();
        return;
      }
      if (doc.selection) {
        clearSelection();
        return;
      }
      setPending(null);
      setSelectedId(null);
      setTool("select");
    },
    // Ferramentas.
    "imagem.tool.select": () => setTool("select"),
    "imagem.tool.pan": () => setTool("pan"),
    "imagem.tool.arrow": () => setTool("arrow"),
    "imagem.tool.line": () => setTool("line"),
    "imagem.tool.rect": () => setTool("rect"),
    "imagem.tool.ellipse": () => setTool("ellipse"),
    "imagem.tool.text": () => setTool("text"),
    "imagem.tool.marker": () => setTool("marker"),
    "imagem.tool.point": () => setTool("point"),
    "imagem.tool.measurement": () => setTool("measurement"),
    "imagem.tool.scale": () => setTool("set_scale"),
    "imagem.tool.redaction": () => setTool("redaction"),
    "imagem.tool.crop": () => {
      setCropPending(null);
      setTool("crop");
    },
    // W20 — ferramentas de seleção.
    "imagem.tool.select_rect": () => selectTool("select_rect"),
    "imagem.tool.select_ellipse": () => selectTool("select_ellipse"),
    "imagem.tool.select_lasso": () => selectTool("select_lasso"),
    "imagem.tool.select_polygon": () => selectTool("select_polygon"),
    "imagem.tool.select_magnetic": () => selectTool("select_magnetic"),
    "imagem.selectInvert": () => invertSelection(),
    "imagem.selectClear": () => clearSelection(),
    "imagem.selectAll": () => selectAllSelection(),
    // W20 (S3) — recortar a seleção numa nova camada de pixels.
    "imagem.duplicateSelectionLayer": () => {
      if (doc.selection) setCopyPrompt(true);
    },
    // Edição.
    "imagem.delete": () => {
      if (selectedId) deleteAnnotation(selectedId);
      else if (selectedLayerId) deleteLayer(selectedLayerId);
    },
    "imagem.save": () => void handleSave(),
    "imagem.cropApply": () => {
      if (tool === "crop" && cropPending) applyCrop();
      else if (selDraft?.mode === "polygon") closePolygonDraft();
    },
    "imagem.toggleAnnotations": toggleAnnotations,
    // Vista.
    "imagem.zoomIn": () => zoomAroundCenter(1.2),
    "imagem.zoomOut": () => zoomAroundCenter(1 / 1.2),
    "imagem.zoomActual": handleZoomActual,
    "imagem.fit": fitToScreen,
    // Exportação.
    "imagem.export": () => void handleExport(),
    // Geral.
    "imagem.commandPalette": () => setPaletteOpen(true),
  });

  // ----- Crop tool actions (pós-laudo S) -----
  const applyCrop = () => {
    if (!cropPending) return;
    // Clamp ao tamanho da imagem natural (htmlImage).
    if (!htmlImage) return;
    const iw = htmlImage.width;
    const ih = htmlImage.height;
    const x = Math.max(0, Math.min(iw - 1, Math.round(cropPending.x)));
    const y = Math.max(0, Math.min(ih - 1, Math.round(cropPending.y)));
    const width = Math.max(1, Math.min(iw - x, Math.round(cropPending.width)));
    const height = Math.max(1, Math.min(ih - y, Math.round(cropPending.height)));
    const op = {
      id: `crop_${Date.now().toString(36)}_${Math.random().toString(36).slice(2, 8)}`,
      kind: "crop" as const,
      enabled: true,
      params: { x, y, width, height },
      created_at: new Date().toISOString(),
    };
    // Substitui qualquer crop anterior (apenas um crop ativo por vez).
    setDoc((d) => ({
      ...d,
      processing_stack: [
        ...d.processing_stack.filter(
          (o) => !(o.kind === "crop" && o.enabled),
        ),
        op,
      ],
    }));
    setCropPending(null);
    setTool("select");
    setFeedback(`Corte aplicado (${width}×${height}px).`);
  };
  const cancelCrop = () => {
    setCropPending(null);
    setTool("select");
  };
  const removeAppliedCrop = () => {
    setDoc((d) => ({
      ...d,
      processing_stack: d.processing_stack.filter((o) => o.kind !== "crop"),
    }));
    setFeedback("Corte removido — imagem original restaurada.");
  };

  // W13.4 — seleciona uma ferramenta via trilha agrupada: lembra qual foi a
  // última do grupo (para o slot exibi-la), fecha o flyout e preserva o reset
  // do crop (o retângulo é reinicializado pelo useEffect ao limpar cropPending).
  const selectTool = useCallback((t: Tool) => {
    if (t === "crop") setCropPending(null);
    setSelDraft(null); // W20 — abandona rascunho de seleção ao trocar de tool
    setTool(t);
    setOpenGroup(null);
    const g = TOOL_GROUPS.find((grp) => grp.tools.some((td) => td.tool === t));
    if (g) setGroupRep((m) => (m[g.key] === t ? m : { ...m, [g.key]: t }));
  }, []);
  // Ferramenta representante do slot: a ativa, se pertencer ao grupo; senão a
  // lembrada; senão a primeira do grupo.
  const repToolFor = (g: ToolGroupDef): Tool => {
    if (g.tools.some((td) => td.tool === tool)) return tool;
    return groupRep[g.key] ?? g.tools[0]?.tool ?? "select";
  };

  // ----- W20 — Seleção (estilo Photoshop): ações sobre `doc.selection` -----
  const commitSelection = useCallback(
    (geom: Omit<SicroImageSelection, "id" | "created_at" | "inverted">) => {
      setDoc((d) => ({
        ...d,
        selection: {
          ...geom,
          id: `sel-${Date.now()}-${Math.random().toString(36).slice(2, 6)}`,
          inverted: false,
          created_at: new Date().toISOString(),
        },
      }));
    },
    [],
  );
  const clearSelection = useCallback(() => {
    setSelDraft(null);
    setDoc((d) => (d.selection ? { ...d, selection: null } : d));
  }, []);
  const invertSelection = useCallback(() => {
    setDoc((d) =>
      d.selection
        ? {
            ...d,
            selection: { ...d.selection, inverted: !d.selection.inverted },
          }
        : d,
    );
  }, []);
  const selectAllSelection = useCallback(() => {
    const w = doc.source.width || htmlImage?.naturalWidth || 0;
    const h = doc.source.height || htmlImage?.naturalHeight || 0;
    if (w <= 0 || h <= 0) return;
    setSelDraft(null);
    setDoc((d) => ({
      ...d,
      selection: {
        id: `sel-${Date.now()}-${Math.random().toString(36).slice(2, 6)}`,
        kind: "rect",
        x: 0,
        y: 0,
        width: w,
        height: h,
        inverted: false,
        source_tool: "select_all",
        created_at: new Date().toISOString(),
      },
    }));
  }, [doc.source.width, doc.source.height, htmlImage]);
  const closePolygonDraft = useCallback(() => {
    setSelDraft((d) => {
      if (d?.mode === "polygon" && d.points.length >= 3) {
        commitSelection({
          kind: "polygon",
          points: d.points,
          source_tool: "select_polygon",
        });
        return null;
      }
      return d;
    });
  }, [commitSelection]);

  // Snap magnético (básico): leva o ponto clicado ao pixel de MAIOR gradiente
  // (borda) num raio pequeno, via buffer Sobel reduzido do htmlImage.
  const ensureEdgeField = useCallback(() => {
    const cur = edgeFieldRef.current;
    if (cur && cur.src === htmlImage) return cur;
    if (!htmlImage) return null;
    try {
      const iw = htmlImage.naturalWidth || htmlImage.width;
      const ih = htmlImage.naturalHeight || htmlImage.height;
      if (!iw || !ih) return null;
      const MAX = 1024;
      const k = Math.min(1, MAX / Math.max(iw, ih));
      const w = Math.max(1, Math.round(iw * k));
      const h = Math.max(1, Math.round(ih * k));
      const cv = document.createElement("canvas");
      cv.width = w;
      cv.height = h;
      const ctx = cv.getContext("2d");
      if (!ctx) return null;
      ctx.drawImage(htmlImage, 0, 0, w, h);
      const data = ctx.getImageData(0, 0, w, h).data;
      const lum = new Float32Array(w * h);
      for (let i = 0; i < w * h; i++) {
        lum[i] =
          0.299 * (data[i * 4] ?? 0) +
          0.587 * (data[i * 4 + 1] ?? 0) +
          0.114 * (data[i * 4 + 2] ?? 0);
      }
      const buf = new Float32Array(w * h);
      for (let y = 1; y < h - 1; y++) {
        for (let x = 1; x < w - 1; x++) {
          const gx = (lum[y * w + x + 1] ?? 0) - (lum[y * w + x - 1] ?? 0);
          const gy = (lum[(y + 1) * w + x] ?? 0) - (lum[(y - 1) * w + x] ?? 0);
          buf[y * w + x] = Math.abs(gx) + Math.abs(gy);
        }
      }
      const field = { src: htmlImage, buf, w, h, sx: w / iw, sy: h / ih };
      edgeFieldRef.current = field;
      return field;
    } catch {
      return null;
    }
  }, [htmlImage]);
  const snapToEdge = useCallback(
    (p: SicroImagePoint): SicroImagePoint => {
      const f = ensureEdgeField();
      if (!f) return p;
      const r = Math.max(1, Math.round(10 * f.sx));
      const cx = Math.round(p.x * f.sx);
      const cy = Math.round(p.y * f.sy);
      let best = -1;
      let bx = cx;
      let by = cy;
      for (let dy = -r; dy <= r; dy++) {
        for (let dx = -r; dx <= r; dx++) {
          const x = cx + dx;
          const y = cy + dy;
          if (x < 0 || y < 0 || x >= f.w || y >= f.h) continue;
          const g = f.buf[y * f.w + x] ?? 0;
          if (g > best) {
            best = g;
            bx = x;
            by = y;
          }
        }
      }
      return { x: bx / f.sx, y: by / f.sy };
    },
    [ensureEdgeField],
  );

  // W20 (S2) — cria um ProcessingOp e, se houver seleção ativa e o filtro for
  // mascarável, já "congela" a máscara nele (escopo "seleção"). Assim o filtro
  // recém-adicionado nasce confinado à região; o perito pode trocar p/ "imagem
  // inteira" depois no card da pilha.
  const makeScopedOp = useCallback(
    (kind: ProcessingOpKind): ProcessingOp => {
      const op = makeProcessingOp(kind);
      if (doc.selection && !NON_MASKABLE_KINDS.has(kind)) {
        return { ...op, scope: "selection", mask: doc.selection };
      }
      return op;
    },
    [doc.selection],
  );

  // W20 (S3) — carrega os bitmaps das camadas de pixels (id → <img>) a partir
  // do workspace. Bitmaps recém-criados são semeados direto no `pixelImages`
  // (base64), então aqui só carregamos os que faltam (ex.: ao reabrir a sessão).
  useEffect(() => {
    const pixelLayers = doc.layers.filter(
      (l) => l.kind === "pixels" && l.bitmap_relative_path,
    );
    let cancelled = false;
    for (const l of pixelLayers) {
      if (pixelImages[l.id]) continue;
      const url = assetUrl(workspacePath, l.bitmap_relative_path as string);
      if (!url) continue;
      const img = new window.Image();
      img.onload = () => {
        if (!cancelled) setPixelImages((m) => ({ ...m, [l.id]: img }));
      };
      img.src = url;
    }
    return () => {
      cancelled = true;
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [doc.layers, workspacePath]);

  // W20 (S3) — prende o Transformer (handles) à camada de pixels selecionada.
  // Só quando a ferramenta é "Selecionar", a camada está visível e destravada.
  useEffect(() => {
    const tr = pixelTransformerRef.current;
    if (!tr) return;
    const sel = doc.layers.find(
      (l) => l.id === selectedLayerId && l.kind === "pixels",
    );
    const node =
      sel && tool === "select" && !sel.locked && sel.visible !== false
        ? (pixelNodeRefs.current[sel.id] ?? null)
        : null;
    tr.nodes(node ? [node] : []);
    tr.getLayer()?.batchDraw();
  }, [selectedLayerId, doc.layers, tool, pixelImages]);

  // W20 (S3) — atualiza uma camada (ex.: mover/visibilidade/opacidade).
  const updateLayer = useCallback(
    (id: string, patch: Partial<SicroImageLayer>) => {
      setDoc((d) => ({
        ...d,
        layers: d.layers.map((l) => (l.id === id ? { ...l, ...patch } : l)),
      }));
    },
    [],
  );
  const deleteLayer = useCallback((id: string) => {
    setDoc((d) => ({ ...d, layers: d.layers.filter((l) => l.id !== id) }));
    setSelectedLayerId((cur) => (cur === id ? null : cur));
    setPixelImages((m) => {
      if (!m[id]) return m;
      const next = { ...m };
      delete next[id];
      return next;
    });
  }, []);
  // W20 (S3) — reordena as camadas a partir da ordem TOPO-primeiro do painel
  // (o painel mostra o topo da pilha em cima; `doc.layers` guarda fundo→topo).
  const reorderLayers = useCallback((orderedTopFirst: string[]) => {
    setDoc((d) => {
      const byId = new Map(d.layers.map((l) => [l.id, l]));
      const next = orderedTopFirst
        .map((id) => byId.get(id))
        .filter((l): l is SicroImageLayer => !!l)
        .reverse();
      // Preserva camadas que (por algum motivo) não vieram na lista.
      for (const l of d.layers) if (!orderedTopFirst.includes(l.id)) next.unshift(l);
      return { ...d, layers: next };
    });
  }, []);

  // W20 (S3) — recorta a seleção numa NOVA camada de pixels (estilo Photoshop
  // "Layer via Copy"). `source` decide a origem: "original" (evidência fiel) ou
  // "processed" (o resultado com filtros que o perito vê). A escolha vai na
  // custódia (pixel_source). O bitmap é gravado em imagens/camadas/ pelo backend.
  const duplicateSelectionToLayer = useCallback(
    async (source: PixelLayerSource) => {
      const sel = doc.selection;
      if (!sel) return;
      const sw = doc.source.width || htmlImage?.naturalWidth || 0;
      const sh = doc.source.height || htmlImage?.naturalHeight || 0;
      const maskSpec = selectionToMaskSpec(sel, sw, sh);
      if (!maskSpec) {
        setFeedback("Não consegui montar a máscara da seleção.");
        return;
      }
      setCopyBusy(true);
      const layerId = `pl-${Date.now().toString(36)}-${Math.random()
        .toString(36)
        .slice(2, 7)}`;
      try {
        const enabledOps = (doc.processing_stack ?? []).filter(
          (o) => o.enabled !== false,
        );
        const ops =
          source === "processed"
            ? enabledOps.map(
                (o) =>
                  processingOpToBackendOperation(
                    o,
                    sw,
                    sh,
                  ) as unknown as BackendOperation,
              )
            : [];
        const res = await commands.copyRegionToLayer(workspacePath, {
          relative_path: doc.source.original_relative_path,
          mask: maskSpec,
          layer_id: layerId,
          apply_processing: source === "processed",
          adjustments:
            source === "processed" ? doc.view_adjustments : undefined,
          operations: ops,
        });
        // Semeia o bitmap recém-criado para exibir já (sem reler o disco).
        const img = new window.Image();
        img.onload = () =>
          setPixelImages((m) => ({ ...m, [layerId]: img }));
        img.src = `data:${res.mime};base64,${res.base64}`;
        const layer: SicroImageLayer = {
          id: layerId,
          name: `Camada ${source === "processed" ? "(resultado)" : "(original)"}`,
          kind: "pixels",
          visible: true,
          locked: false,
          opacity: 1,
          offset_x: res.x,
          offset_y: res.y,
          width: res.width,
          height: res.height,
          bitmap_relative_path: res.relative_path,
          pixel_source: source,
          hash_sha256: res.hash_sha256,
          created_at: new Date().toISOString(),
        };
        setDoc((d) => ({ ...d, layers: [...d.layers, layer] }));
        setSelectedLayerId(layerId);
        setTool("select");
        // Após criar a camada, a seleção cumpriu seu papel — limpa o marquee
        // (não deve persistir na foto). A camada vira o novo foco.
        clearSelection();
        setFeedback(
          `Camada criada (${res.width}×${res.height}px, ${
            source === "processed" ? "com filtros" : "original"
          }). Arraste/handles para ajustar.`,
        );
      } catch (e) {
        setFeedback(`Falha ao criar camada: ${toSicroError(e).message}`);
      } finally {
        setCopyBusy(false);
        setCopyPrompt(false);
      }
    },
    [doc, htmlImage, workspacePath, clearSelection],
  );

  // ----- Round-trip: Salvar e voltar pro laudo (pós-laudo S) -----
  const handleReturnToLaudo = async () => {
    if (!roundtripRequest) return;
    setReturningToLaudo(true);
    setFeedback("Salvando e exportando…");
    try {
      // Salva o doc atual (com processing_stack incluindo o crop).
      const metadata = {
        annotations_count: doc.annotations.length,
        has_scale: !!doc.scale,
        view_adjustments: doc.view_adjustments,
        roundtrip: true,
      };
      await saveActive(workspacePath, doc, JSON.stringify(metadata));

      // Compõe o PNG visual via Konva — só a região da imagem (com
      // crop aplicado, ou natural). ANTES capturava o stage inteiro
      // (com viewport + padding em volta), e o resultado virava uma
      // PNG enorme com a imagem cortada perdida no meio — quando
      // voltava pro laudo, ficava minúscula. Agora, calculamos o
      // bounding box exato da KonvaImage em coords de stage e
      // pedimos só essa área. PixelRatio normalizado pra 2x da
      // resolução source (independente do zoom).
      const dataUrl: string | null = (() => {
        if (!stageRef.current || !htmlImage) return null;
        const imageWorldW = cropApplied ? cropApplied.width : htmlImage.width;
        const imageWorldH = cropApplied
          ? cropApplied.height
          : htmlImage.height;
        const scale = viewport.scale || 1;
        return stageRef.current.toDataURL({
          x: viewport.x,
          y: viewport.y,
          width: imageWorldW * scale,
          height: imageWorldH * scale,
          // pixelRatio normaliza: queremos PNG do tamanho source × 2
          // (alta resolução pra preservar qualidade). A região
          // capturada em stage coords é (imageWorldW × scale).
          // Pra obter output de imageWorldW × 2, pixelRatio = 2/scale.
          pixelRatio: 2 / scale,
          mimeType: "image/png",
        });
      })();
      const composedBase64 = dataUrl
        ? dataUrl.replace(/^data:image\/png;base64,/, "")
        : null;

      const exp = await commands.exportImageDerivative(
        workspacePath,
        analysis.id,
        {
          apply_backend_adjustments: false,
          composed_png_base64: composedBase64,
          adjustments: doc.view_adjustments,
          operations: [],
          format: "png",
          operation_summary_json: JSON.stringify({
            roundtrip_source: roundtripRequest.source_relative_path,
            laudo_id: roundtripRequest.laudo_id,
            crop_applied: cropApplied,
            annotations: doc.annotations.length,
          }),
        },
      );

      // Sinaliza o store: o laudo vai pegar isso ao montar.
      completeRoundtrip({
        output_relative_path: exp.output_relative_path,
        source_relative_path: roundtripRequest.source_relative_path,
      });
      // Volta pro laudo. O LaudoEditorView aplica o novo path nas figures.
      navigate("/laudo");
    } catch (err) {
      setFeedback(`Falha ao voltar para o laudo: ${toSicroError(err).message}`);
      setReturningToLaudo(false);
    }
  };

  // ----- Actions -----
  const handleSave = async (): Promise<boolean> => {
    setSaving(true);
    setFeedback(null);
    const snapshot = JSON.stringify(doc);
    try {
      const metadata = {
        annotations_count: doc.annotations.length,
        has_scale: !!doc.scale,
        view_adjustments: doc.view_adjustments,
      };
      await saveActive(workspacePath, doc, JSON.stringify(metadata));
      // W18 — marca este estado como "salvo" para limpar o `dirty`.
      setLastSavedJson(snapshot);
      setFeedback("Análise salva.");
      setTimeout(() => setFeedback(null), 2500);
      return true;
    } catch (err) {
      setFeedback(`Falha ao salvar: ${toSicroError(err).message}`);
      return false;
    } finally {
      setSaving(false);
    }
  };

  // W18 — guarda de alterações não salvas -----------------------------------
  // Entrada única para qualquer saída do editor (botão Voltar). Sem `dirty`,
  // navega direto; com `dirty`, abre o modal e estaciona o destino pendente.
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

  // Registra um guard global enquanto há alterações: a ActivityRail consulta
  // antes de levar o perito a outro módulo (resolve true=segue / false=fica).
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

  const handleModalSaveAndLeave = async () => {
    if (!pendingNav) return;
    const ok = await handleSave();
    if (!ok) return; // falhou: permanece no editor para tentar de novo
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

  const handleExport = async () => {
    setExporting(true);
    setFeedback(null);
    try {
      // Save sessão antes do export para sidecar refletir o último estado.
      await saveActive(workspacePath, doc);
      // W18 — exportar também persiste a sessão; limpa o `dirty`.
      setLastSavedJson(JSON.stringify(doc));
      // Compose PNG client-side via Konva (image + annotations + filter
      // via stage option pixelRatio + transform). The filter applied
      // here is CSS, so we duplicate the math in the backend pipeline
      // to keep the bytes truly reproducible. The backend will run the
      // adjustments again on top of the composed PNG IF
      // `apply_backend_adjustments=true`. To avoid double application
      // we send composed PNG and skip backend adjustments.
      // W17 — quando há filtros na pilha, o EXPORT é gerado em resolução
      // CHEIA pelo backend (reaplica os ops sobre o ORIGINAL), não do canvas
      // (que mostra o preview reduzido). Sem filtros, compõe o canvas (com
      // anotações). §13: o derivado exportado é sempre o fiel.
      const enabledOps = (doc.processing_stack ?? []).filter(
        (o) => o.enabled !== false,
      );
      const hasFilters = enabledOps.some((o) => o.kind !== "crop");
      // W20 (S2) — export em resolução cheia: a máscara da seleção é
      // normalizada com as dims da fonte (backend rasteriza no tamanho cheio).
      const expSw = doc.source.width || htmlImage?.naturalWidth || 0;
      const expSh = doc.source.height || htmlImage?.naturalHeight || 0;
      const fullResOps = enabledOps.map(
        (o) =>
          processingOpToBackendOperation(
            o,
            expSw,
            expSh,
          ) as unknown as BackendOperation,
      );
      // W20 (S3) — camadas de pixels visíveis (com bitmap carregado).
      const pixelLayers = doc.layers.filter(
        (l) =>
          l.kind === "pixels" && l.visible !== false && pixelImages[l.id],
      );
      let composedBase64: string | null = null;
      let opsForBackend: BackendOperation[] = [];
      let applyBackendAdj = false;
      if (hasFilters && pixelLayers.length > 0) {
        // Filtros + camadas de pixels: pega o resultado full-res do backend e
        // composita as camadas por cima no cliente (anotações já não entram no
        // caminho com filtros — comportamento existente). Mantém fidelidade.
        const stack = await commands.applyOperationStack(workspacePath, {
          relative_path: doc.source.original_relative_path,
          adjustments: doc.view_adjustments,
          operations: fullResOps,
          as_jpeg: false,
        });
        const baseImg = await loadImageEl(
          `data:${stack.mime};base64,${stack.image_base64}`,
        );
        composedBase64 = compositePixelLayersToBase64(
          baseImg,
          pixelLayers,
          pixelImages,
        );
        applyBackendAdj = false;
        opsForBackend = [];
      } else if (hasFilters) {
        // Só filtros: backend reaplica sobre o original (full-res).
        composedBase64 = null;
        applyBackendAdj = true;
        opsForBackend = fullResOps;
      } else {
        // Sem filtros: o stage já compõe imagem + anotações + camadas de pixels.
        const dataUrl =
          stageRef.current?.toDataURL({
            pixelRatio: 2,
            mimeType: "image/png",
          }) ?? null;
        composedBase64 = dataUrl
          ? dataUrl.replace(/^data:image\/png;base64,/, "")
          : null;
      }
      const exp = await commands.exportImageDerivative(
        workspacePath,
        analysis.id,
        {
          apply_backend_adjustments: applyBackendAdj,
          composed_png_base64: composedBase64,
          adjustments: doc.view_adjustments,
          operations: opsForBackend,
          format: "png",
          operation_summary_json: JSON.stringify({
            annotations: doc.annotations.length,
            has_scale: !!doc.scale,
            scale: doc.scale,
            view_adjustments: doc.view_adjustments,
            // W17 — pilha de filtros aplicada (resolução cheia no backend).
            processing_stack: enabledOps.map((o) => ({
              kind: o.kind,
              params: o.params,
            })),
            filters_full_res: hasFilters,
          }),
        },
      );
      setFeedback(
        hasFilters
          ? `Imagem com filtros exportada (resolução cheia): ${exp.output_relative_path}`
          : `Imagem exportada: ${exp.output_relative_path}`,
      );
    } catch (err) {
      setFeedback(`Falha ao exportar: ${toSicroError(err).message}`);
    } finally {
      setExporting(false);
    }
  };

  // ----- Render -----
  const cssFilter = adjustmentsToCssFilter(doc.view_adjustments);
  const annotationsLayerVisible = doc.layers.find(
    (l) => l.id === "layer_annotations",
  )?.visible !== false;

  // W14.2 fix — aplica o filtro de preview SÓ no <canvas> da imagem base
  // (não no fundo do canvas nem nas anotações). Reaplica quando o filtro muda
  // ou quando o canvas pode ter sido recriado (resize / nova imagem / crop).
  useEffect(() => {
    const layer = baseLayerRef.current;
    if (!layer) return;
    const native = (
      layer.getCanvas() as unknown as { _canvas?: HTMLCanvasElement }
    )._canvas;
    if (native) native.style.filter = cssFilter || "";
  }, [cssFilter, stageSize.width, stageSize.height, htmlImage, cropApplied]);

  // W13.6 — comandos da paleta (⌘K): atalho para ações que já existem na UI.
  const paletteCommands: PaletteCommand[] = [
    ...TOOL_GROUPS.flatMap((g) =>
      g.tools.map(
        (t): PaletteCommand => ({
          id: `tool:${t.tool}`,
          label: t.label,
          group: "Ferramenta",
          hint: t.keyHint,
          keywords: g.label,
          run: () => selectTool(t.tool),
        }),
      ),
    ),
    ...RIGHT_MODES.map(
      (m): PaletteCommand => ({
        id: `mode:${m.key}`,
        label: `Modo: ${m.label}`,
        group: "Painel",
        keywords: m.hint,
        run: () => setRightMode(m.key),
      }),
    ),
    {
      id: "view:fit",
      label: "Enquadrar à tela",
      group: "Vista",
      hint: "F",
      keywords: "zoom ajustar enquadrar",
      run: fitToScreen,
    },
    {
      id: "view:actual",
      label: "Pixels reais (1:1)",
      group: "Vista",
      hint: "Ctrl+0",
      keywords: "zoom 100 real",
      run: handleZoomActual,
    },
    {
      id: "view:zoomin",
      label: "Ampliar",
      group: "Vista",
      hint: "Ctrl+=",
      keywords: "zoom mais aproximar",
      run: () => zoomAroundCenter(1.2),
    },
    {
      id: "view:zoomout",
      label: "Reduzir",
      group: "Vista",
      hint: "Ctrl+-",
      keywords: "zoom menos afastar",
      run: () => zoomAroundCenter(1 / 1.2),
    },
    {
      id: "view:rulers",
      label: showRulers ? "Ocultar réguas" : "Mostrar réguas",
      group: "Vista",
      keywords: "regua guia mouse posicao",
      run: () => setShowRulers((v) => !v),
    },
    {
      id: "view:annot",
      label: annotationsLayerVisible ? "Ocultar anotações" : "Mostrar anotações",
      group: "Vista",
      hint: "Shift+A",
      keywords: "esconder mostrar marcacoes",
      run: toggleAnnotations,
    },
    {
      id: "edit:resetAdj",
      label: "Resetar ajustes de visualização",
      group: "Ação",
      keywords: "brilho contraste gama saturacao zerar limpar",
      run: () =>
        updateAdjustments({
          brightness: 0,
          contrast: 0,
          gamma: 1,
          saturation: 0,
          grayscale: false,
          invert: false,
          hue: 0,
          channel_r: true,
          channel_g: true,
          channel_b: true,
        }),
    },
    {
      id: "file:save",
      label: "Salvar análise",
      group: "Ação",
      hint: "Ctrl+S",
      keywords: "gravar persistir",
      run: () => void handleSave(),
    },
    {
      id: "file:export",
      label: "Exportar imagem (derivado)",
      group: "Ação",
      hint: "Ctrl+E",
      keywords: "png exportar derivado salvar",
      run: () => void handleExport(),
    },
    ...FILTER_INDEX.map(
      (f): PaletteCommand => ({
        id: `filter:${f.kind}`,
        label: `Filtro: ${f.label}`,
        group: f.group,
        keywords: `filtro adicionar ${f.keywords ?? ""} ${f.note ?? ""}`,
        run: () => {
          setDoc((d) => ({
            ...d,
            processing_stack: [
              ...(d.processing_stack ?? []),
              makeScopedOp(f.kind),
            ],
          }));
          setRightMode("filtros");
          setFeedback(
            doc.selection && !NON_MASKABLE_KINDS.has(f.kind)
              ? `Filtro adicionado (na seleção): ${f.label}`
              : `Filtro adicionado: ${f.label}`,
          );
        },
      }),
    ),
  ];

  return (
    <div className={styles.wrap}>
      {/* Pós-laudo S — Banner do round-trip Laudo↔Imagem. */}
      {isRoundtripActive && roundtripRequest && (
        <div
          style={{
            background:
              "linear-gradient(90deg, rgba(14,165,233,0.15), rgba(14,165,233,0.05))",
            borderBottom: "1px solid rgba(14,165,233,0.4)",
            padding: "8px 16px",
            display: "flex",
            alignItems: "center",
            gap: 12,
            fontSize: 12,
            color: "var(--sicro-fg)",
          }}
        >
          <CropIcon size={14} color="#0ea5e9" />
          <strong>Editando foto do laudo</strong>
          {roundtripRequest.laudo_title && (
            <span style={{ color: "var(--sicro-fg-dim)" }}>
              — {roundtripRequest.laudo_title}
            </span>
          )}
          <span style={{ marginLeft: "auto", color: "var(--sicro-fg-dim)" }}>
            Clique <strong>Salvar e voltar</strong> para devolver ao laudo
            com as edições aplicadas.
          </span>
        </div>
      )}
      <header className={styles.topBar}>
        <button
          type="button"
          className={styles.backBtn}
          onClick={() => tryNavigateAway(onClose, "a lista de análises")}
        >
          <ArrowLeft size={14} /> Voltar
        </button>
        <input
          className={styles.titleInput}
          value={doc.title}
          onChange={(e) => setDoc((d) => ({ ...d, title: e.target.value }))}
          aria-label="Título da análise"
        />
        <div className={styles.topActions}>
          {/* W13.7 — descobribilidade da paleta de comandos (⌘K). */}
          <button
            type="button"
            className={styles.cmdkBtn}
            onClick={() => setPaletteOpen(true)}
            title="Paleta de comandos — buscar ferramentas, filtros e ações"
          >
            <Search size={13} />
            <span>Comandos</span>
            <kbd>Ctrl K</kbd>
          </button>
          {dirty && (
            <span
              title="Há alterações não salvas"
              style={{
                display: "inline-flex",
                alignItems: "center",
                gap: 4,
                fontSize: 11,
                color: "#fbbf24",
                marginRight: 2,
                whiteSpace: "nowrap",
              }}
            >
              ● não salvo
            </span>
          )}
          <Button
            variant="secondary"
            leftIcon={<Save size={14} />}
            onClick={() => void handleSave()}
            disabled={saving}
          >
            {saving ? "Salvando…" : "Salvar"}
          </Button>
          {/* G12.22 — Botão de relatório pericial. */}
          <Button
            variant="secondary"
            leftIcon={<FileText size={14} />}
            onClick={() => setReportOpen(true)}
          >
            Relatório
          </Button>
          {isRoundtripActive ? (
            // Pós-laudo S — Botão dedicado de round-trip. Substitui o
            // botão "Exportar" enquanto o roundtrip estiver ativo —
            // o perito só precisa decidir Salvar+voltar, não exportação
            // genérica.
            <Button
              variant="primary"
              leftIcon={<Check size={14} />}
              onClick={() => void handleReturnToLaudo()}
              disabled={returningToLaudo}
            >
              {returningToLaudo ? "Voltando…" : "Salvar e voltar pro laudo"}
            </Button>
          ) : (
            <Button
              variant="primary"
              leftIcon={<FileImage size={14} />}
              onClick={() => void handleExport()}
              disabled={exporting}
            >
              {exporting ? "Exportando…" : "Exportar"}
            </Button>
          )}
        </div>
      </header>

      {/* W13.2 — Barra de contexto: opções da ferramenta ativa (ou estilo do
          objeto selecionado). Faixa full-width entre o topo e o corpo. */}
      <ToolOptionsBar
        tool={tool}
        toolStyle={toolStyle}
        onToolStyle={(patch) => setToolStyle((s) => ({ ...s, ...patch }))}
        selected={doc.annotations.find((a) => a.id === selectedId) ?? null}
        onSelectedPatch={(patch) => {
          if (selectedId) updateAnnotation(selectedId, patch);
        }}
        scale={doc.scale}
      />

      <div className={styles.body}>
        {/* W13.4 — trilha em grupos com flyout (Navegar / Anotar / Medir /
            Proteger / Recortar). O reset do crop é tratado em selectTool. */}
        <aside className={styles.toolbar} aria-label="Ferramentas">
          {TOOL_GROUPS.map((g, i) => (
            <Fragment key={g.key}>
              {i > 0 && <div className={styles.toolDivider} />}
              <ToolGroupFlyout
                group={g}
                activeTool={tool}
                repTool={repToolFor(g)}
                open={openGroup === g.key}
                onOpen={() => setOpenGroup(g.key)}
                onClose={() => setOpenGroup((k) => (k === g.key ? null : k))}
                onSelect={selectTool}
              />
            </Fragment>
          ))}
          {cropApplied && tool !== "crop" && (
            <ToolBtn
              icon={<XIcon size={14} />}
              active={false}
              onClick={removeAppliedCrop}
              title="Remover corte aplicado"
            />
          )}
        </aside>

        <div
          className={styles.canvasArea}
          ref={canvasWrapRef}
          style={{ position: "relative" }}
        >
          {/* Pós-laudo S — Barra flutuante de Apply/Cancel do crop tool. */}
          {tool === "crop" && (
            <div
              style={{
                position: "absolute",
                top: 12,
                left: "50%",
                transform: "translateX(-50%)",
                zIndex: 10,
                background: "rgba(15, 23, 42, 0.92)",
                color: "#fff",
                padding: "8px 12px",
                borderRadius: 6,
                display: "flex",
                gap: 8,
                alignItems: "center",
                fontSize: 12,
                boxShadow: "0 4px 12px rgba(0,0,0,0.3)",
              }}
            >
              {cropPending ? (
                <span>
                  Arraste o retângulo ou as alças —{" "}
                  <strong>
                    {Math.round(cropPending.width)}×
                    {Math.round(cropPending.height)} px
                  </strong>
                </span>
              ) : (
                <span>Preparando seleção…</span>
              )}
              <div
                style={{
                  width: 1,
                  height: 16,
                  background: "rgba(255,255,255,0.2)",
                  margin: "0 4px",
                }}
              />
              <button
                type="button"
                onClick={applyCrop}
                disabled={!cropPending}
                style={{
                  display: "flex",
                  alignItems: "center",
                  gap: 4,
                  padding: "4px 10px",
                  background: cropPending ? "#0ea5e9" : "rgba(255,255,255,0.1)",
                  color: "#fff",
                  border: "none",
                  borderRadius: 4,
                  cursor: cropPending ? "pointer" : "not-allowed",
                  fontFamily: "inherit",
                  fontSize: 12,
                  fontWeight: 600,
                }}
              >
                <Check size={12} /> Aplicar
              </button>
              <button
                type="button"
                onClick={cancelCrop}
                style={{
                  display: "flex",
                  alignItems: "center",
                  gap: 4,
                  padding: "4px 10px",
                  background: "rgba(255,255,255,0.1)",
                  color: "#fff",
                  border: "none",
                  borderRadius: 4,
                  cursor: "pointer",
                  fontFamily: "inherit",
                  fontSize: 12,
                }}
              >
                <XIcon size={12} /> Cancelar
              </button>
            </div>
          )}
          <Stage
            ref={stageRef}
            width={stageSize.width}
            height={stageSize.height}
            x={viewport.x}
            y={viewport.y}
            scaleX={viewport.scale}
            scaleY={viewport.scale}
            draggable={tool === "pan"}
            onClick={handleStageClick}
            onMouseMove={handleStageMouseMove}
            onMouseDown={handleStageMouseDown}
            onMouseUp={handleStageMouseUp}
            onDblClick={handleStageDblClick}
            onWheel={handleWheel}
            onDragEnd={(e) => {
              if (tool !== "pan") return;
              const stage = e.target;
              setViewport({
                scale: stage.scaleX(),
                x: stage.x(),
                y: stage.y(),
              });
            }}
            style={{
              background: doc.canvas.background_color,
              cursor:
                tool === "pan"
                  ? "grab"
                  : isSelectionTool(tool)
                    ? "crosshair"
                    : "default",
              // W14.2 — o `filter` dos ajustes NÃO vai aqui (afetaria o fundo
              // do canvas). É aplicado só no <canvas> da camada da imagem
              // base, via baseLayerRef (useEffect abaixo).
            }}
          >
            {/* W14.2 — Camada da imagem base. O CSS `filter` dos ajustes é
                aplicado SÓ no <canvas> desta camada (via baseLayerRef +
                useEffect), de modo que o fundo do canvas e as anotações (em
                outras camadas) NÃO são afetados. */}
            <Layer
              ref={baseLayerRef}
              listening={false}
              imageSmoothingEnabled={viewport.scale < PIXEL_CRISP_ZOOM}
            >
              {previewImage ? (
                // W17 — resultado da pilha de filtros (backend) já com crop e
                // ops aplicados na ordem. O bitmap pode vir reduzido (preview
                // rápido); desenhamos no tamanho LÓGICO (÷scale) para manter
                // as coordenadas e a sobreposição de anotações.
                <KonvaImage
                  image={previewImage.image}
                  x={0}
                  y={0}
                  width={previewImage.image.width / previewImage.scale}
                  height={previewImage.image.height / previewImage.scale}
                />
              ) : htmlImage && cropApplied ? (
                // Pós-laudo S — crop aplicado: usa `crop` do Konva.Image pra
                // limitar a região renderizada à do crop_op. x/y/width/height
                // viram as dimensões cortadas — a imagem aparece "encolhida"
                // pro tamanho do recorte.
                <KonvaImage
                  image={htmlImage}
                  x={0}
                  y={0}
                  width={cropApplied.width}
                  height={cropApplied.height}
                  crop={{
                    x: cropApplied.x,
                    y: cropApplied.y,
                    width: cropApplied.width,
                    height: cropApplied.height,
                  }}
                />
              ) : htmlImage ? (
                <KonvaImage
                  image={htmlImage}
                  x={0}
                  y={0}
                  width={htmlImage.width}
                  height={htmlImage.height}
                />
              ) : null}
            </Layer>
            {/* W20 (S3) — Camadas de pixels (recortes de seleção) acima da
                base, movíveis com a ferramenta de seleção. Em coords de px da
                imagem (= mesma convenção das anotações). */}
            <Layer>
              {doc.layers
                .filter((l) => l.kind === "pixels" && l.visible !== false)
                .map((l) => {
                  const img = pixelImages[l.id];
                  if (!img) return null;
                  const selectLayer = (
                    e: Konva.KonvaEventObject<MouseEvent | TouchEvent>,
                  ) => {
                    // Não deixa o clique borbulhar pro Stage (que deselecionaria).
                    e.cancelBubble = true;
                    setSelectedId(null);
                    setSelectedLayerId(l.id);
                  };
                  return (
                    <KonvaImage
                      key={l.id}
                      ref={(n) => {
                        pixelNodeRefs.current[l.id] = n;
                      }}
                      image={img}
                      x={l.offset_x ?? 0}
                      y={l.offset_y ?? 0}
                      width={l.width ?? img.width}
                      height={l.height ?? img.height}
                      rotation={l.rotation ?? 0}
                      opacity={l.opacity ?? 1}
                      draggable={tool === "select" && !l.locked}
                      onMouseDown={selectLayer}
                      onClick={selectLayer}
                      onTap={selectLayer}
                      onDragEnd={(e) =>
                        updateLayer(l.id, {
                          offset_x: Math.round(e.target.x()),
                          offset_y: Math.round(e.target.y()),
                        })
                      }
                      onTransformEnd={(e) => {
                        const node = e.target as Konva.Image;
                        const nw = Math.max(
                          8,
                          Math.round(node.width() * node.scaleX()),
                        );
                        const nh = Math.max(
                          8,
                          Math.round(node.height() * node.scaleY()),
                        );
                        node.scaleX(1);
                        node.scaleY(1);
                        updateLayer(l.id, {
                          offset_x: Math.round(node.x()),
                          offset_y: Math.round(node.y()),
                          width: nw,
                          height: nh,
                          rotation: Math.round(node.rotation()),
                        });
                      }}
                    />
                  );
                })}
              {/* W20 (S3) — Handles (redimensionar + rotacionar) presos à
                  camada de pixels selecionada. */}
              <Transformer
                ref={pixelTransformerRef}
                rotateEnabled
                anchorSize={8}
                borderStroke="#22d3ee"
                anchorStroke="#22d3ee"
                anchorFill="#0b0f14"
                rotateAnchorOffset={20}
                boundBoxFunc={(oldBox, newBox) =>
                  newBox.width < 8 || newBox.height < 8 ? oldBox : newBox
                }
              />
            </Layer>
            {/* Pós-laudo S — Crop tool: retângulo arrastável + 8 handles +
                dim outside. Tudo em escala de "imagem source" (coords
                world). Os handles usam 1/viewport.scale pra ficar com
                tamanho constante na tela mesmo quando o usuário dá zoom. */}
            {tool === "crop" && cropPending && htmlImage && (
              <CropOverlayLayer
                imageWidth={htmlImage.width}
                imageHeight={htmlImage.height}
                crop={cropPending}
                viewportScale={viewport.scale}
                minSize={CROP_MIN_PX}
                onChange={(next) => setCropPending(next)}
              />
            )}
            <Layer ref={objectsLayerRef} visible={annotationsLayerVisible}>
              {doc.annotations.map((a) => (
                <AnnotationNode
                  key={a.id}
                  a={a}
                  draggable={tool === "select" && !a.locked}
                  onSelect={() => setSelectedId(a.id)}
                  onChange={(patch) => updateAnnotation(a.id, patch)}
                />
              ))}
              <Transformer
                ref={transformerRef}
                rotateEnabled
                enabledAnchors={["top-left", "top-right", "bottom-left", "bottom-right"]}
                boundBoxFunc={(_, next) => {
                  if (Math.abs(next.width) < 4 || Math.abs(next.height) < 4) return _;
                  return next;
                }}
              />
            </Layer>
            {/* W20 — Marquee da seleção (marching ants). Entre objetos e
                pending; não-interativa. */}
            <SelectionMarqueeLayer
              selection={doc.selection ?? null}
              draft={selDraft}
              imageWidth={doc.source.width || htmlImage?.naturalWidth || 0}
              imageHeight={doc.source.height || htmlImage?.naturalHeight || 0}
              viewportScale={viewport.scale}
            />
            <Layer listening={false}>
              {pending && (
                <PendingMarker x={pending.x} y={pending.y} cursor={pointer} />
              )}
            </Layer>
          </Stage>

          {/* W14.2 — <filter> SVG do mix de canais (R/G/B). Referenciado por
              `url(#…)` no `filter` do container do Konva. Matriz diagonal:
              canal desligado → 0 na saída (GIMP-style). Só renderiza quando há
              canal desligado. */}
          {!channelsAllOn(doc.view_adjustments) && (
            <svg
              aria-hidden
              width={0}
              height={0}
              style={{ position: "absolute", width: 0, height: 0 }}
            >
              <filter
                id={CHANNEL_MIX_FILTER_ID}
                colorInterpolationFilters="sRGB"
              >
                <feColorMatrix
                  type="matrix"
                  values={`${doc.view_adjustments.channel_r === false ? 0 : 1} 0 0 0 0  0 ${doc.view_adjustments.channel_g === false ? 0 : 1} 0 0 0  0 0 ${doc.view_adjustments.channel_b === false ? 0 : 1} 0 0  0 0 0 1 0`}
                />
              </filter>
            </svg>
          )}

          {/* W13 — Réguas que seguem o mouse (topo + esquerda). */}
          {showRulers && (
            <CanvasRulers
              imageWidth={doc.source.width}
              imageHeight={doc.source.height}
              viewport={viewport}
              pointer={pointer}
              width={stageSize.width}
              height={stageSize.height}
              scale={doc.scale}
            />
          )}
          {/* Toggle das réguas (flutuante, canto superior-direito). */}
          <button
            type="button"
            onClick={() => setShowRulers((v) => !v)}
            title={showRulers ? "Ocultar réguas" : "Mostrar réguas"}
            aria-pressed={showRulers}
            style={{
              position: "absolute",
              top: 6,
              right: 6,
              zIndex: 11,
              display: "flex",
              alignItems: "center",
              gap: 4,
              padding: "3px 8px",
              fontSize: 11,
              fontFamily: "inherit",
              color: showRulers ? "#38bdf8" : "rgba(203,213,225,0.8)",
              background: "rgba(17,24,39,0.82)",
              border: `1px solid ${showRulers ? "rgba(56,189,248,0.5)" : "rgba(148,163,184,0.25)"}`,
              borderRadius: 5,
              cursor: "pointer",
            }}
          >
            <Ruler size={12} /> Réguas
          </button>

          {/* W20 — Indicador da seleção ativa (kind + área + inverter/limpar).
              Flutua abaixo do toggle de réguas. */}
          {doc.selection &&
            (() => {
              const sel = doc.selection;
              const kindLabel =
                sel.kind === "rect"
                  ? "Seleção retangular"
                  : sel.kind === "ellipse"
                    ? "Seleção elíptica"
                    : "Seleção poligonal";
              const aPx = selectionArea(sel);
              const pxLabel = `${Math.round(aPx).toLocaleString("pt-BR")} px²`;
              const realLabel =
                doc.scale && doc.scale.px_per_unit > 0
                  ? `${(aPx / (doc.scale.px_per_unit * doc.scale.px_per_unit)).toLocaleString(
                      "pt-BR",
                      { maximumFractionDigits: 2 },
                    )} ${doc.scale.unit}²`
                  : null;
              return (
                <div
                  style={{
                    position: "absolute",
                    top: 38,
                    right: 6,
                    zIndex: 11,
                    display: "flex",
                    flexDirection: "column",
                    gap: 4,
                    padding: "6px 8px",
                    fontSize: 11,
                    fontFamily: "inherit",
                    color: "rgba(203,213,225,0.9)",
                    background: "rgba(17,24,39,0.86)",
                    border: "1px solid rgba(34,211,238,0.45)",
                    borderRadius: 6,
                    minWidth: 150,
                  }}
                >
                  <div
                    style={{
                      display: "flex",
                      alignItems: "center",
                      gap: 6,
                      color: "#22d3ee",
                      fontWeight: 600,
                    }}
                  >
                    <BoxSelect size={12} />
                    {kindLabel}
                    {sel.inverted ? " · invertida" : ""}
                  </div>
                  <div style={{ fontVariantNumeric: "tabular-nums" }}>
                    {realLabel ? `${realLabel} · ${pxLabel}` : pxLabel}
                  </div>
                  <div style={{ display: "flex", gap: 6 }}>
                    <button
                      type="button"
                      onClick={invertSelection}
                      title="Inverter seleção (Ctrl+Shift+I)"
                      style={{
                        flex: 1,
                        padding: "3px 6px",
                        fontSize: 10.5,
                        fontFamily: "inherit",
                        color: sel.inverted ? "#0b0f14" : "rgba(203,213,225,0.9)",
                        background: sel.inverted
                          ? "rgba(34,211,238,0.85)"
                          : "rgba(30,41,59,0.9)",
                        border: "1px solid rgba(34,211,238,0.4)",
                        borderRadius: 4,
                        cursor: "pointer",
                      }}
                    >
                      Inverter
                    </button>
                    <button
                      type="button"
                      onClick={clearSelection}
                      title="Deselecionar (Ctrl+D)"
                      style={{
                        flex: 1,
                        padding: "3px 6px",
                        fontSize: 10.5,
                        fontFamily: "inherit",
                        color: "rgba(203,213,225,0.9)",
                        background: "rgba(30,41,59,0.9)",
                        border: "1px solid rgba(148,163,184,0.3)",
                        borderRadius: 4,
                        cursor: "pointer",
                      }}
                    >
                      Limpar
                    </button>
                  </div>
                  {/* W20 (S3) — recortar a seleção numa nova camada de pixels. */}
                  <button
                    type="button"
                    onClick={() => setCopyPrompt(true)}
                    title="Recortar a seleção em uma nova camada (Ctrl+J)"
                    style={{
                      padding: "4px 6px",
                      fontSize: 10.5,
                      fontFamily: "inherit",
                      color: "#0b0f14",
                      background: "rgba(34,211,238,0.9)",
                      border: "1px solid rgba(34,211,238,0.5)",
                      borderRadius: 4,
                      cursor: "pointer",
                      fontWeight: 600,
                    }}
                  >
                    Nova camada da seleção
                  </button>
                </div>
              );
            })()}

          {/* W20 — Dica enquanto desenha um polígono/laço-poligonal: mostra a
              contagem de vértices e como fechar (vértices ilimitados). */}
          {selDraft?.mode === "polygon" && (
            <div
              style={{
                position: "absolute",
                bottom: 12,
                left: "50%",
                transform: "translateX(-50%)",
                zIndex: 12,
                display: "flex",
                alignItems: "center",
                gap: 8,
                padding: "5px 12px",
                fontSize: 11.5,
                fontFamily: "inherit",
                color: "rgba(226,232,240,0.95)",
                background: "rgba(17,24,39,0.9)",
                border: "1px solid rgba(34,211,238,0.5)",
                borderRadius: 999,
                whiteSpace: "nowrap",
                pointerEvents: "none",
              }}
            >
              <Hexagon size={13} color="#22d3ee" />
              <strong style={{ color: "#22d3ee" }}>
                {selDraft.points.length}{" "}
                {selDraft.points.length === 1 ? "vértice" : "vértices"}
              </strong>
              <span style={{ color: "rgba(148,163,184,0.85)" }}>
                clique para adicionar · clique no 1º ponto, duplo-clique ou{" "}
                <kbd
                  style={{
                    padding: "1px 5px",
                    border: "1px solid rgba(148,163,184,0.4)",
                    borderRadius: 4,
                    fontSize: 10,
                  }}
                >
                  Enter
                </kbd>{" "}
                para fechar ·{" "}
                <kbd
                  style={{
                    padding: "1px 5px",
                    border: "1px solid rgba(148,163,184,0.4)",
                    borderRadius: 4,
                    fontSize: 10,
                  }}
                >
                  Esc
                </kbd>{" "}
                cancela
              </span>
            </div>
          )}

          {/* W20 (S3) — Diálogo de origem da cópia: o perito escolhe na hora
              se a nova camada vem dos PIXELS ORIGINAIS (evidência fiel) ou do
              RESULTADO com filtros (o que vê). A escolha vai na custódia. */}
          {copyPrompt && (
            <div
              className={styles.previewOverlay}
              // `previewOverlay` é passivo (pointer-events:none) p/ o overlay de
              // "Aplicando filtros" não bloquear navegação. Aqui o diálogo é
              // INTERATIVO — reabilita os cliques (senão atravessam pro canvas).
              style={{ zIndex: 30, pointerEvents: "auto", cursor: "auto" }}
              onClick={() => !copyBusy && setCopyPrompt(false)}
            >
              <div
                onClick={(e) => e.stopPropagation()}
                style={{
                  width: 380,
                  maxWidth: "90%",
                  background: "rgba(17,24,39,0.98)",
                  border: "1px solid rgba(34,211,238,0.4)",
                  borderRadius: 10,
                  padding: 16,
                  display: "flex",
                  flexDirection: "column",
                  gap: 10,
                  color: "rgba(226,232,240,0.95)",
                  fontFamily: "inherit",
                  boxShadow: "0 12px 40px rgba(0,0,0,0.5)",
                }}
              >
                <strong style={{ fontSize: 14, color: "#22d3ee" }}>
                  Nova camada a partir da seleção
                </strong>
                <p
                  style={{
                    margin: 0,
                    fontSize: 12,
                    color: "rgba(148,163,184,0.95)",
                    lineHeight: 1.5,
                  }}
                >
                  De onde recortar os pixels desta região? A escolha fica
                  registrada na custódia da camada (§13).
                </p>
                <button
                  type="button"
                  disabled={copyBusy}
                  onClick={() => void duplicateSelectionToLayer("original")}
                  style={{
                    textAlign: "left",
                    padding: "8px 10px",
                    borderRadius: 6,
                    border: "1px solid rgba(148,163,184,0.3)",
                    background: "rgba(30,41,59,0.8)",
                    color: "inherit",
                    cursor: copyBusy ? "wait" : "pointer",
                    fontFamily: "inherit",
                  }}
                >
                  <strong style={{ fontSize: 12.5 }}>Pixels originais</strong>
                  <div style={{ fontSize: 11, color: "rgba(148,163,184,0.85)" }}>
                    Recorte fiel da evidência, sem filtros.
                  </div>
                </button>
                <button
                  type="button"
                  disabled={copyBusy}
                  onClick={() => void duplicateSelectionToLayer("processed")}
                  style={{
                    textAlign: "left",
                    padding: "8px 10px",
                    borderRadius: 6,
                    border: "1px solid rgba(148,163,184,0.3)",
                    background: "rgba(30,41,59,0.8)",
                    color: "inherit",
                    cursor: copyBusy ? "wait" : "pointer",
                    fontFamily: "inherit",
                  }}
                >
                  <strong style={{ fontSize: 12.5 }}>
                    Resultado (com filtros)
                  </strong>
                  <div style={{ fontSize: 11, color: "rgba(148,163,184,0.85)" }}>
                    Recorte do que você vê: ajustes + pilha de filtros aplicados.
                  </div>
                </button>
                <button
                  type="button"
                  disabled={copyBusy}
                  onClick={() => setCopyPrompt(false)}
                  style={{
                    alignSelf: "flex-end",
                    padding: "5px 12px",
                    borderRadius: 6,
                    border: "1px solid rgba(148,163,184,0.25)",
                    background: "transparent",
                    color: "rgba(203,213,225,0.8)",
                    cursor: "pointer",
                    fontFamily: "inherit",
                    fontSize: 12,
                  }}
                >
                  {copyBusy ? "Recortando…" : "Cancelar"}
                </button>
              </div>
            </div>
          )}

          {/* W20.1 — overlay de processamento cobrindo a foto: chamativo, com
              spinner + barra INDETERMINADA. O backend processa a pilha numa
              única chamada e não reporta %; por isso a barra é animada/
              indeterminada (honesta — não inventa porcentagem). */}
          {previewBusy && (
            <div
              className={styles.previewOverlay}
              aria-live="polite"
              aria-busy="true"
            >
              <div className={styles.previewCard}>
                <div className={styles.previewSpinner} />
                <div className={styles.previewTitle}>Aplicando filtros…</div>
                <div className={styles.previewSub}>
                  Processando a pré-visualização. Filtros pesados (mediana,
                  bilateral, difference of gaussians) podem levar alguns
                  segundos — aguarde.
                </div>
                <div className={styles.previewBar}>
                  <div className={styles.previewBarFill} />
                </div>
              </div>
            </div>
          )}

          {/* W19 — Camadas: a pilha de filtros como barra horizontal embaixo
              do canvas (substitui o dock vertical do painel direito). Cada
              filtro/crop vira uma camada; a galeria (aba Filtros) adiciona. */}
          <LayersBar
            stack={doc.processing_stack ?? []}
            onChange={(next) =>
              setDoc((d) => ({ ...d, processing_stack: next }))
            }
            collapsed={pipelineCollapsed}
            onToggleCollapsed={() => setPipelineCollapsed((v) => !v)}
            activeSelection={doc.selection ?? null}
          />
          <StatusBar
            tool={tool}
            pointer={pointer}
            viewport={viewport}
            doc={doc}
            saving={saving}
            exporting={exporting}
            feedback={feedback}
          />
        </div>

        <aside className={styles.right} aria-label="Painel direito">
          {/* W19.1 — metade de cima: as 4 abas (Realçar/Filtros/Analisar/
              Anotar). A metade de baixo é dedicada às Camadas. */}
          <div className={styles.rightTop}>
          <nav className={styles.rightTabs} role="tablist">
            {RIGHT_MODES.map((m) => (
              <button
                key={m.key}
                type="button"
                role="tab"
                aria-selected={rightMode === m.key}
                className={`${styles.rightTab} ${rightMode === m.key ? styles.rightTabActive : ""}`}
                onClick={() => setRightMode(m.key)}
                title={m.hint}
              >
                {m.label}
              </button>
            ))}
          </nav>
          <div className={styles.rightBody}>
            {rightMode === "realcar" && (
              <>
                <p className={styles.modeNote}>
                  Auxílio de visualização — <strong>não altera a evidência
                  original</strong>. Os ajustes só afetam o preview e os
                  derivados exportados; nada é gravado sobre o original (§13).
                </p>
                <AdjustmentsPanel
                  adjustments={doc.view_adjustments}
                  onChange={updateAdjustments}
                  onReset={() =>
                    updateAdjustments({
                      brightness: 0,
                      contrast: 0,
                      gamma: 1,
                      saturation: 0,
                      grayscale: false,
                      invert: false,
                      hue: 0,
                      channel_r: true,
                      channel_g: true,
                      channel_b: true,
                    })
                  }
                />
              </>
            )}
            {rightMode === "filtros" && (
              // W18 — só o CATÁLOGO (galeria buscável) na aba; a pilha ATIVA
              // vive no dock sempre visível, abaixo. Assim a galeria não perde
              // espaço conforme a pilha cresce.
              <FilterGallery
                onAdd={(kind) => {
                  setDoc((d) => ({
                    ...d,
                    processing_stack: [
                      ...(d.processing_stack ?? []),
                      makeScopedOp(kind),
                    ],
                  }));
                  if (doc.selection && !NON_MASKABLE_KINDS.has(kind)) {
                    setFeedback("Filtro adicionado — confinado à seleção.");
                  }
                }}
              />
            )}
            {rightMode === "analisar" && (
              <>
                <AccordionSection
                  title="Histograma e estatísticas"
                  sectionKey="histogram"
                  open={openSections.has("histogram")}
                  onToggle={toggleSection}
                >
                  <HistogramPanel
                    workspacePath={workspacePath}
                    relativePath={doc.source.original_relative_path}
                  />
                </AccordionSection>
                <AccordionSection
                  title="EXIF"
                  sectionKey="exif"
                  open={openSections.has("exif")}
                  onToggle={toggleSection}
                >
                  <ExifPanel
                    workspacePath={workspacePath}
                    relativePath={doc.source.original_relative_path}
                  />
                </AccordionSection>
                <AccordionSection
                  title="Metadados e custódia"
                  sectionKey="meta"
                  open={openSections.has("meta")}
                  onToggle={toggleSection}
                >
                  <MetaPanel doc={doc} />
                </AccordionSection>
                <AccordionSection
                  title="Histórico de operações"
                  sectionKey="history"
                  open={openSections.has("history")}
                  onToggle={toggleSection}
                >
                  <HistoryPanel logs={logs} />
                </AccordionSection>
              </>
            )}
            {rightMode === "anotar" && (
              <>
                <AccordionSection
                  title="Objetos (anotações e medições)"
                  sectionKey="annotations"
                  open={openSections.has("annotations")}
                  onToggle={toggleSection}
                >
                  <AnnotationsListPanel
                    doc={doc}
                    selectedId={selectedId}
                    onSelect={setSelectedId}
                    onDelete={deleteAnnotation}
                    onToggleVisibility={(id) => {
                      const a = doc.annotations.find((x) => x.id === id);
                      if (!a) return;
                      updateAnnotation(id, { visible: a.visible === false });
                    }}
                  />
                </AccordionSection>
              </>
            )}
          </div>
          </div>
          {/* W19.1 — Camadas (estilo Photoshop) na metade de baixo do painel,
              separadas das 4 abas. Saiu da aba Anotar. Em S3 ganham as camadas
              de pixels (copiar/colar). */}
          <div className={styles.rightLayers}>
            <header className={styles.rightLayersHead}>
              <Layers size={13} />
              <strong>Camadas</strong>
            </header>
            <div className={styles.rightLayersBody}>
              <LayersPanelPro
                layers={doc.layers}
                pixelImages={pixelImages}
                selectedLayerId={selectedLayerId}
                onSelect={setSelectedLayerId}
                onToggleVisible={(id) =>
                  setDoc((d) => ({
                    ...d,
                    layers: d.layers.map((l) =>
                      l.id === id ? { ...l, visible: !l.visible } : l,
                    ),
                  }))
                }
                onToggleLock={(id) =>
                  setDoc((d) => ({
                    ...d,
                    layers: d.layers.map((l) =>
                      l.id === id ? { ...l, locked: !l.locked } : l,
                    ),
                  }))
                }
                onSetOpacity={(id, opacity) => updateLayer(id, { opacity })}
                onRename={(id, name) => updateLayer(id, { name })}
                onDelete={deleteLayer}
                onReorder={reorderLayers}
              />
            </div>
          </div>
        </aside>
      </div>
      {/* G12.22 — Modal global do relatório pericial */}
      <ReportPreviewDialog
        open={reportOpen}
        workspacePath={workspacePath}
        analysisId={analysis.id}
        onClose={() => setReportOpen(false)}
      />
      {/* W13.6 — Paleta de comandos (⌘K) */}
      {paletteOpen && (
        <CommandPalette
          commands={paletteCommands}
          onClose={() => setPaletteOpen(false)}
        />
      )}
      {/* W18 — guarda de alterações não salvas (Voltar / trocar de módulo) */}
      {pendingNav && (
        <UnsavedChangesModal
          saving={saving}
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
// Tool button

function ToolBtn({
  icon,
  active,
  onClick,
  title,
}: {
  icon: React.ReactNode;
  active: boolean;
  onClick: () => void;
  title: string;
}) {
  return (
    <button
      type="button"
      className={`${styles.toolBtn} ${active ? styles.toolBtnActive : ""}`}
      onClick={onClick}
      title={title}
    >
      {icon}
    </button>
  );
}

// ---------------------------------------------------------------------------
// W13.4 — Trilha de ferramentas em GRUPOS com flyout (estilo Photoshop/
// Affinity). Cada grupo ocupa um único slot que mostra a ferramenta ativa/
// usada por último; ao passar o mouse, abre um flyout com as demais do grupo
// (rótulo + atalho). Reduz a barra vertical de ~12 botões para 5 slots e
// agrupa por INTENÇÃO, espelhando os 3 modos do painel direito.

interface ToolDef {
  tool: Tool;
  Icon: LucideIcon;
  label: string;
  keyHint: string;
}
interface ToolGroupDef {
  key: string;
  label: string;
  tools: ToolDef[];
}

const TOOL_GROUPS: ToolGroupDef[] = [
  {
    key: "nav",
    label: "Navegar",
    tools: [
      { tool: "select", Icon: MousePointer2, label: "Selecionar / mover", keyHint: "V" },
      { tool: "pan", Icon: Hand, label: "Mão (pan)", keyHint: "H" },
    ],
  },
  {
    key: "selecao",
    label: "Seleção",
    tools: [
      { tool: "select_rect", Icon: BoxSelect, label: "Seleção retangular", keyHint: "Shift+R" },
      { tool: "select_ellipse", Icon: CircleDashed, label: "Seleção elíptica", keyHint: "Shift+E" },
      { tool: "select_lasso", Icon: Lasso, label: "Laço (segue o mouse)", keyHint: "Shift+L" },
      { tool: "select_polygon", Icon: Hexagon, label: "Poligonal (por cliques)", keyHint: "Shift+P" },
      { tool: "select_magnetic", Icon: Magnet, label: "Magnética (básica)", keyHint: "Shift+M" },
    ],
  },
  {
    key: "annotate",
    label: "Anotar",
    tools: [
      { tool: "arrow", Icon: ArrowUpRight, label: "Seta", keyHint: "A" },
      { tool: "line", Icon: PenLine, label: "Linha", keyHint: "L" },
      { tool: "rect", Icon: Square, label: "Retângulo", keyHint: "R" },
      { tool: "ellipse", Icon: CircleIcon, label: "Elipse", keyHint: "E" },
      { tool: "text", Icon: TypeIcon, label: "Texto", keyHint: "T" },
      { tool: "marker", Icon: ListOrdered, label: "Marcador numerado", keyHint: "N" },
    ],
  },
  {
    key: "measure",
    label: "Medir",
    tools: [
      { tool: "measurement", Icon: Ruler, label: "Medida", keyHint: "M" },
      { tool: "set_scale", Icon: Hash, label: "Definir escala", keyHint: "K" },
    ],
  },
  {
    key: "protect",
    label: "Proteger",
    tools: [
      { tool: "redaction", Icon: XSquare, label: "Tarja (anonimização)", keyHint: "X" },
    ],
  },
  {
    key: "crop",
    label: "Recortar",
    tools: [{ tool: "crop", Icon: CropIcon, label: "Cortar imagem", keyHint: "C" }],
  },
];

function ToolGroupFlyout({
  group,
  activeTool,
  repTool,
  open,
  onOpen,
  onClose,
  onSelect,
}: {
  group: ToolGroupDef;
  activeTool: Tool;
  repTool: Tool;
  open: boolean;
  onOpen: () => void;
  onClose: () => void;
  onSelect: (t: Tool) => void;
}) {
  const rep = group.tools.find((t) => t.tool === repTool) ?? group.tools[0];
  if (!rep) return null;
  const RepIcon = rep.Icon;
  const groupActive = group.tools.some((t) => t.tool === activeTool);
  const multi = group.tools.length > 1;
  return (
    <div
      className={styles.toolGroup}
      onMouseEnter={multi ? onOpen : undefined}
      onMouseLeave={multi ? onClose : undefined}
    >
      <button
        type="button"
        className={`${styles.toolBtn} ${groupActive ? styles.toolBtnActive : ""}`}
        onClick={() => onSelect(rep.tool)}
        title={`${group.label} — ${rep.label} (${rep.keyHint})`}
        aria-haspopup={multi || undefined}
        aria-expanded={multi ? open : undefined}
      >
        <RepIcon size={15} />
        {multi && <span className={styles.toolGroupMore} aria-hidden="true" />}
      </button>
      {multi && open && (
        <div className={styles.toolFlyout} role="menu" aria-label={group.label}>
          <div className={styles.toolFlyoutHead}>{group.label}</div>
          {group.tools.map((t) => {
            const ItemIcon = t.Icon;
            return (
              <button
                key={t.tool}
                type="button"
                role="menuitem"
                className={`${styles.toolFlyoutItem} ${activeTool === t.tool ? styles.toolFlyoutItemActive : ""}`}
                onClick={() => onSelect(t.tool)}
              >
                <ItemIcon size={14} />
                <span>{t.label}</span>
                <kbd className={styles.toolFlyoutKey}>{t.keyHint}</kbd>
              </button>
            );
          })}
        </div>
      )}
    </div>
  );
}

// ---------------------------------------------------------------------------
// Pós-laudo S — Crop Overlay (Konva)
//
// Camada de seleção do crop tool. Renderiza:
//   - 4 retângulos escuros cobrindo a área que será descartada (dim);
//   - 1 retângulo brilhante (claro) com borda azul = a área que fica;
//   - 8 handles (4 cantos + 4 lados) draggables.
//
// Todas as coordenadas estão no espaço da imagem source (mundo).
// `viewportScale` escala os elementos visuais (handles, traços) para
// manter tamanho constante na tela mesmo com zoom.
//
// O componente é totalmente controlado pelo pai via `crop` + `onChange`.

interface CropRect {
  x: number;
  y: number;
  width: number;
  height: number;
}

function CropOverlayLayer({
  imageWidth,
  imageHeight,
  crop,
  viewportScale,
  minSize,
  onChange,
}: {
  imageWidth: number;
  imageHeight: number;
  crop: CropRect;
  viewportScale: number;
  minSize: number;
  onChange: (next: CropRect) => void;
}) {
  // Tamanhos visuais constantes em pixels de tela (independentes do zoom).
  const handleSize = 10 / viewportScale; // lado do quadrado handle
  const handleHit = 16 / viewportScale; // área de clique invisível extra
  const strokeWidth = 2 / viewportScale;
  const dashLen = 6 / viewportScale;

  // Clamp helper — garante: dentro da imagem + tamanho mínimo.
  const clampRect = (r: CropRect): CropRect => {
    const x = Math.max(0, Math.min(imageWidth - minSize, r.x));
    const y = Math.max(0, Math.min(imageHeight - minSize, r.y));
    const width = Math.max(minSize, Math.min(imageWidth - x, r.width));
    const height = Math.max(minSize, Math.min(imageHeight - y, r.height));
    return { x, y, width, height };
  };

  // Mover o retângulo inteiro (drag no corpo).
  const onBodyDragMove = (e: Konva.KonvaEventObject<DragEvent>) => {
    const node = e.target;
    const nx = node.x();
    const ny = node.y();
    onChange(clampRect({ ...crop, x: nx, y: ny }));
  };

  // Helpers para handles: cada um atualiza 1 ou 2 bordas (left, top, right, bottom).
  type Edge = "l" | "t" | "r" | "b";
  const dragHandle = (edges: Edge[]) =>
    (e: Konva.KonvaEventObject<DragEvent>) => {
      const node = e.target;
      const nx = node.x();
      const ny = node.y();
      let left = crop.x;
      let top = crop.y;
      let right = crop.x + crop.width;
      let bottom = crop.y + crop.height;
      if (edges.includes("l")) left = nx;
      if (edges.includes("t")) top = ny;
      if (edges.includes("r")) right = nx;
      if (edges.includes("b")) bottom = ny;
      // Garante invariantes (left < right, top < bottom).
      if (right < left + minSize) {
        if (edges.includes("r")) right = left + minSize;
        else left = right - minSize;
      }
      if (bottom < top + minSize) {
        if (edges.includes("b")) bottom = top + minSize;
        else top = bottom - minSize;
      }
      onChange(
        clampRect({
          x: left,
          y: top,
          width: right - left,
          height: bottom - top,
        }),
      );
    };

  // Posições das 8 handles (cx, cy) + edges que cada uma controla + cursor.
  const cx = crop.x + crop.width / 2;
  const cy = crop.y + crop.height / 2;
  const x2 = crop.x + crop.width;
  const y2 = crop.y + crop.height;
  const handles: Array<{
    key: string;
    cx: number;
    cy: number;
    edges: Edge[];
    cursor: string;
  }> = [
    { key: "nw", cx: crop.x, cy: crop.y, edges: ["l", "t"], cursor: "nwse-resize" },
    { key: "n", cx, cy: crop.y, edges: ["t"], cursor: "ns-resize" },
    { key: "ne", cx: x2, cy: crop.y, edges: ["r", "t"], cursor: "nesw-resize" },
    { key: "e", cx: x2, cy, edges: ["r"], cursor: "ew-resize" },
    { key: "se", cx: x2, cy: y2, edges: ["r", "b"], cursor: "nwse-resize" },
    { key: "s", cx, cy: y2, edges: ["b"], cursor: "ns-resize" },
    { key: "sw", cx: crop.x, cy: y2, edges: ["l", "b"], cursor: "nesw-resize" },
    { key: "w", cx: crop.x, cy, edges: ["l"], cursor: "ew-resize" },
  ];

  return (
    <Layer>
      {/* Dim outside: 4 retângulos ao redor da área de crop. */}
      <Rect
        x={0}
        y={0}
        width={imageWidth}
        height={crop.y}
        fill="rgba(0,0,0,0.55)"
        listening={false}
      />
      <Rect
        x={0}
        y={crop.y + crop.height}
        width={imageWidth}
        height={Math.max(0, imageHeight - crop.y - crop.height)}
        fill="rgba(0,0,0,0.55)"
        listening={false}
      />
      <Rect
        x={0}
        y={crop.y}
        width={crop.x}
        height={crop.height}
        fill="rgba(0,0,0,0.55)"
        listening={false}
      />
      <Rect
        x={crop.x + crop.width}
        y={crop.y}
        width={Math.max(0, imageWidth - crop.x - crop.width)}
        height={crop.height}
        fill="rgba(0,0,0,0.55)"
        listening={false}
      />

      {/* Retângulo principal: borda + corpo draggable para mover tudo. */}
      <Rect
        x={crop.x}
        y={crop.y}
        width={crop.width}
        height={crop.height}
        stroke="#0ea5e9"
        strokeWidth={strokeWidth}
        dash={[dashLen, dashLen * 0.6]}
        fill="transparent"
        draggable
        onDragMove={onBodyDragMove}
        onMouseEnter={(e) => {
          const stage = e.target.getStage();
          if (stage) stage.container().style.cursor = "move";
        }}
        onMouseLeave={(e) => {
          const stage = e.target.getStage();
          if (stage) stage.container().style.cursor = "";
        }}
      />

      {/* Linhas de regra dos terços — ajudam composição visual. */}
      {[1, 2].map((i) => (
        <Line
          key={`v${i}`}
          points={[
            crop.x + (crop.width * i) / 3,
            crop.y,
            crop.x + (crop.width * i) / 3,
            crop.y + crop.height,
          ]}
          stroke="rgba(255,255,255,0.35)"
          strokeWidth={1 / viewportScale}
          listening={false}
        />
      ))}
      {[1, 2].map((i) => (
        <Line
          key={`h${i}`}
          points={[
            crop.x,
            crop.y + (crop.height * i) / 3,
            crop.x + crop.width,
            crop.y + (crop.height * i) / 3,
          ]}
          stroke="rgba(255,255,255,0.35)"
          strokeWidth={1 / viewportScale}
          listening={false}
        />
      ))}

      {/* 8 handles draggables. Cada handle é um Rect branco com borda azul. */}
      {handles.map((h) => (
        <Rect
          key={h.key}
          x={h.cx}
          y={h.cy}
          width={handleSize}
          height={handleSize}
          offsetX={handleSize / 2}
          offsetY={handleSize / 2}
          fill="#fff"
          stroke="#0ea5e9"
          strokeWidth={1.5 / viewportScale}
          draggable
          onDragMove={dragHandle(h.edges)}
          // Aumenta área de clique invisível (hitStrokeWidth → padding clicável).
          hitStrokeWidth={handleHit}
          onMouseEnter={(e) => {
            const stage = e.target.getStage();
            if (stage) stage.container().style.cursor = h.cursor;
          }}
          onMouseLeave={(e) => {
            const stage = e.target.getStage();
            if (stage) stage.container().style.cursor = "";
          }}
        />
      ))}
    </Layer>
  );
}

// ---------------------------------------------------------------------------
// Annotation node (Konva)

function AnnotationNode({
  a,
  draggable,
  onSelect,
  onChange,
}: {
  a: SicroAnnotation;
  draggable: boolean;
  onSelect: () => void;
  onChange: (patch: Partial<SicroAnnotation>) => void;
}) {
  if (a.visible === false) return null;
  const stroke = a.stroke ?? "#ef4444";
  const fill = a.fill ?? "transparent";
  const strokeWidth = a.stroke_width ?? 2;
  const opacity = a.opacity ?? 1;

  const commonHandlers = {
    onClick: onSelect,
    onTap: onSelect,
    onDragEnd: (e: Konva.KonvaEventObject<DragEvent>) =>
      onChange({ x: e.target.x(), y: e.target.y() }),
  };

  if (a.kind === "rect" || a.kind === "redaction") {
    return (
      <Rect
        id={a.id}
        x={a.x}
        y={a.y}
        width={a.width ?? 0}
        height={a.height ?? 0}
        stroke={stroke}
        strokeWidth={strokeWidth}
        fill={a.kind === "redaction" ? "#000000" : fill}
        opacity={a.kind === "redaction" ? 1 : opacity}
        draggable={draggable}
        {...commonHandlers}
      />
    );
  }
  if (a.kind === "ellipse") {
    return (
      <Ellipse
        id={a.id}
        x={a.x + (a.width ?? 0) / 2}
        y={a.y + (a.height ?? 0) / 2}
        radiusX={Math.abs((a.width ?? 0) / 2)}
        radiusY={Math.abs((a.height ?? 0) / 2)}
        stroke={stroke}
        strokeWidth={strokeWidth}
        fill={fill}
        opacity={opacity}
        draggable={draggable}
        {...commonHandlers}
      />
    );
  }
  if (a.kind === "arrow" || a.kind === "line" || a.kind === "measurement") {
    return (
      <Group
        id={a.id}
        draggable={draggable}
        {...commonHandlers}
        onDragEnd={(e) => {
          const dx = e.target.x();
          const dy = e.target.y();
          e.target.position({ x: 0, y: 0 });
          onChange({
            x: a.x + dx,
            y: a.y + dy,
            x2: (a.x2 ?? 0) + dx,
            y2: (a.y2 ?? 0) + dy,
          });
        }}
      >
        <Line
          points={[a.x, a.y, a.x2 ?? a.x, a.y2 ?? a.y]}
          stroke={stroke}
          strokeWidth={strokeWidth}
          opacity={opacity}
          dash={a.kind === "measurement" ? [8, 4] : undefined}
          hitStrokeWidth={Math.max(strokeWidth, 12)}
        />
        {a.kind === "arrow" && a.x2 != null && a.y2 != null && (
          <ArrowHead x1={a.x} y1={a.y} x2={a.x2} y2={a.y2} color={stroke} size={Math.max(strokeWidth * 4, 12)} />
        )}
        {a.kind === "measurement" && (
          <KonvaText
            x={(a.x + (a.x2 ?? a.x)) / 2}
            y={(a.y + (a.y2 ?? a.y)) / 2 - 14}
            text={`${Math.hypot((a.x2 ?? a.x) - a.x, (a.y2 ?? a.y) - a.y).toFixed(1)} px`}
            fontSize={11}
            fill={stroke}
            listening={false}
          />
        )}
      </Group>
    );
  }
  if (a.kind === "text") {
    return (
      <KonvaText
        id={a.id}
        x={a.x}
        y={a.y}
        text={a.text ?? ""}
        fontSize={16}
        fontStyle="bold"
        fill={a.fill ?? stroke}
        opacity={opacity}
        draggable={draggable}
        {...commonHandlers}
      />
    );
  }
  if (a.kind === "numbered_marker") {
    return (
      <Group
        id={a.id}
        x={a.x}
        y={a.y}
        draggable={draggable}
        {...commonHandlers}
      >
        <Circle radius={11} fill={a.fill ?? "#dc2626"} stroke={a.stroke ?? "#ffffff"} strokeWidth={2} />
        <KonvaText
          text={String(a.number ?? a.text ?? "")}
          fontSize={12}
          fontStyle="bold"
          fill="#ffffff"
          align="center"
          verticalAlign="middle"
          width={22}
          height={22}
          offsetX={11}
          offsetY={11}
          listening={false}
        />
      </Group>
    );
  }
  if (a.kind === "point") {
    return (
      <Circle
        id={a.id}
        x={a.x}
        y={a.y}
        radius={5}
        fill={a.fill ?? "#22c55e"}
        stroke={stroke}
        strokeWidth={1}
        draggable={draggable}
        {...commonHandlers}
      />
    );
  }
  return null;
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

function PendingMarker({
  x,
  y,
  cursor,
}: {
  x: number;
  y: number;
  cursor: { x: number; y: number };
}) {
  return (
    <Group listening={false}>
      <Line
        points={[x, y, cursor.x, cursor.y]}
        stroke="#0ea5e9"
        strokeWidth={1}
        dash={[6, 4]}
      />
      <Rect x={x - 3} y={y - 3} width={6} height={6} fill="#0ea5e9" />
    </Group>
  );
}

function isAddTool(t: Tool): boolean {
  return (
    t !== "select" &&
    t !== "pan"
  );
}

// ---------------------------------------------------------------------------
// StatusBar

function StatusBar({
  tool,
  pointer,
  viewport,
  doc,
  saving,
  exporting,
  feedback,
}: {
  tool: Tool;
  pointer: { x: number; y: number };
  viewport: { scale: number };
  doc: SicroImageDoc;
  saving: boolean;
  exporting: boolean;
  feedback: string | null;
}) {
  return (
    <div className={styles.statusBar}>
      <span>
        Ferramenta: <code>{tool}</code>
      </span>
      <span>
        x={pointer.x.toFixed(0)} · y={pointer.y.toFixed(0)}
      </span>
      <span>zoom {(viewport.scale * 100).toFixed(0)}%</span>
      <span>
        {doc.source.width}×{doc.source.height} px
      </span>
      <span>
        {doc.scale
          ? `escala ${doc.scale.px_per_unit.toFixed(2)} px/${doc.scale.unit}`
          : "escala indefinida"}
      </span>
      <span>{doc.annotations.length} anot.</span>
      <span>{prettyBytes(doc.source.size_bytes)}</span>
      <span>SHA {shortHash(doc.source.original_hash_sha256)}</span>
      <span className={styles.statusFeedback}>
        {saving ? "salvando…" : exporting ? "exportando…" : feedback}
      </span>
    </div>
  );
}

// ---------------------------------------------------------------------------
// Right panel — Accordion (W13.3): seção recolhível reaproveitada pelos 3 modos

function AccordionSection({
  title,
  sectionKey,
  open,
  onToggle,
  children,
}: {
  title: string;
  sectionKey: string;
  open: boolean;
  onToggle: (key: string) => void;
  children: React.ReactNode;
}) {
  return (
    <section className={styles.accordion}>
      <button
        type="button"
        className={styles.accordionHeader}
        onClick={() => onToggle(sectionKey)}
        aria-expanded={open}
      >
        <ChevronDown
          size={13}
          className={`${styles.accordionChevron} ${open ? styles.accordionChevronOpen : ""}`}
        />
        <span className={styles.accordionTitle}>{title}</span>
      </button>
      {open && <div className={styles.accordionBody}>{children}</div>}
    </section>
  );
}

// Right panel — Layers: ver `LayersPanelPro` (componente dedicado, W20 S3).

// ---------------------------------------------------------------------------
// Right panel — Adjustments

function AdjustmentsPanel({
  adjustments,
  onChange,
  onReset,
}: {
  adjustments: BackendAdjustments;
  onChange: (patch: Partial<BackendAdjustments>) => void;
  onReset: () => void;
}) {
  return (
    <>
      <h3 className={styles.sectionTitle}>Ajustes não destrutivos</h3>
      <Slider
        label="Brilho"
        min={-100}
        max={100}
        step={1}
        value={adjustments.brightness}
        onChange={(v) => onChange({ brightness: v })}
      />
      <Slider
        label="Contraste"
        min={-100}
        max={100}
        step={1}
        value={adjustments.contrast}
        onChange={(v) => onChange({ contrast: v })}
      />
      <Slider
        label="Gamma"
        min={0.2}
        max={3}
        step={0.05}
        value={adjustments.gamma}
        onChange={(v) => onChange({ gamma: v })}
        format={(v) => v.toFixed(2)}
      />
      <Slider
        label="Saturação"
        min={-100}
        max={100}
        step={1}
        value={adjustments.saturation}
        onChange={(v) => onChange({ saturation: v })}
      />
      {/* W14.2 — Matiz (rotação de cor, graus). Útil para destacar dominantes
          de cor sutis sem alterar a evidência. */}
      <Slider
        label="Matiz (°)"
        min={-180}
        max={180}
        step={1}
        value={adjustments.hue ?? 0}
        onChange={(v) => onChange({ hue: v })}
      />
      {/* W14.2 — Canais R/G/B (estilo GIMP): desligar zera o canal de saída,
          isolando/realçando a contribuição de cor. */}
      <div className={styles.channelRow}>
        <span className={styles.channelLabel}>Canais</span>
        {(["r", "g", "b"] as const).map((ch) => {
          const key =
            ch === "r" ? "channel_r" : ch === "g" ? "channel_g" : "channel_b";
          const on = adjustments[key] ?? true;
          return (
            <button
              key={ch}
              type="button"
              className={`${styles.channelChip} ${styles[`channel_${ch}`] ?? ""} ${on ? styles.channelOn : ""}`}
              aria-pressed={on}
              onClick={() => onChange({ [key]: !on })}
              title={`${on ? "Ocultar" : "Mostrar"} canal ${ch.toUpperCase()}`}
            >
              {ch.toUpperCase()}
            </button>
          );
        })}
      </div>
      <label className={styles.checkRow}>
        <input
          type="checkbox"
          checked={adjustments.grayscale}
          onChange={(e) => onChange({ grayscale: e.target.checked })}
        />
        Tons de cinza
      </label>
      <label className={styles.checkRow}>
        <input
          type="checkbox"
          checked={adjustments.invert}
          onChange={(e) => onChange({ invert: e.target.checked })}
        />
        Inverter cores
      </label>
      <button type="button" className={styles.resetBtn} onClick={onReset}>
        Resetar ajustes
      </button>
      <p style={{ fontSize: 10, color: "var(--sicro-fg-dim)", margin: 0, lineHeight: 1.4 }}>
        Os ajustes são <strong>não destrutivos</strong>: aplicados em preview
        no canvas e re-aplicados no export pelo backend.
      </p>
    </>
  );
}

function Slider({
  label,
  min,
  max,
  step,
  value,
  onChange,
  format,
}: {
  label: string;
  min: number;
  max: number;
  step: number;
  value: number;
  onChange: (v: number) => void;
  format?: (v: number) => string;
}) {
  return (
    <div className={styles.slider}>
      <header>
        <strong>{label}</strong>
        <code>{format ? format(value) : value}</code>
      </header>
      <input
        type="range"
        min={min}
        max={max}
        step={step}
        value={value}
        onChange={(e) => onChange(Number(e.target.value))}
      />
    </div>
  );
}

// ---------------------------------------------------------------------------
// Right panel — Annotations list

function AnnotationsListPanel({
  doc,
  selectedId,
  onSelect,
  onDelete,
  onToggleVisibility,
}: {
  doc: SicroImageDoc;
  selectedId: string | null;
  onSelect: (id: string | null) => void;
  onDelete: (id: string) => void;
  onToggleVisibility: (id: string) => void;
}) {
  if (doc.annotations.length === 0) {
    return (
      <p style={{ fontSize: 11, color: "var(--sicro-fg-dim)", margin: 0 }}>
        Nenhuma anotação ainda. Use a barra à esquerda para inserir.
      </p>
    );
  }
  return (
    <>
      <h3 className={styles.sectionTitle}>Anotações ({doc.annotations.length})</h3>
      <ul className={styles.objectList}>
        {doc.annotations.map((a) => (
          <li
            key={a.id}
            className={`${styles.objectItem} ${selectedId === a.id ? styles.objectItemActive : ""}`}
          >
            <button
              type="button"
              className={styles.objectVisBtn}
              onClick={(e) => {
                e.stopPropagation();
                onToggleVisibility(a.id);
              }}
            >
              {a.visible !== false ? <Eye size={11} /> : <EyeOff size={11} />}
            </button>
            <span className={styles.objectKind}>{a.kind}</span>
            <button
              type="button"
              className={styles.objectLabel}
              style={{
                background: "transparent",
                border: "none",
                color: "inherit",
                cursor: "pointer",
                padding: 0,
                textAlign: "left",
              }}
              onClick={() => onSelect(a.id)}
            >
              {a.label ?? a.text ?? a.id.slice(0, 8)}
            </button>
            <button
              type="button"
              className={styles.objectVisBtn}
              onClick={(e) => {
                e.stopPropagation();
                onDelete(a.id);
              }}
              title="Excluir"
            >
              <Trash2 size={10} />
            </button>
          </li>
        ))}
      </ul>
    </>
  );
}

// ---------------------------------------------------------------------------
// Right panel — History

function HistoryPanel({ logs }: { logs: ImageOperationLog[] }) {
  if (logs.length === 0) {
    return (
      <p style={{ fontSize: 11, color: "var(--sicro-fg-dim)", margin: 0 }}>
        Sem operações registradas ainda.
      </p>
    );
  }
  return (
    <>
      <h3 className={styles.sectionTitle}>Histórico</h3>
      {logs.map((l) => (
        <div key={l.id} className={styles.logEntry}>
          <code>{l.action}</code>
          <span style={{ color: "var(--sicro-fg-dim)", fontSize: 10 }}>
            {new Date(l.created_at).toLocaleString("pt-BR")}
          </span>
        </div>
      ))}
    </>
  );
}

// ---------------------------------------------------------------------------
// Right panel — Metadata

function MetaPanel({ doc }: { doc: SicroImageDoc }) {
  return (
    <>
      <h3 className={styles.sectionTitle}>Metadados da origem</h3>
      <dl className={styles.metaList}>
        <dt>Origem</dt>
        <dd>{doc.source.kind}</dd>
        <dt>Caminho</dt>
        <dd>{doc.source.original_relative_path}</dd>
        <dt>Mime</dt>
        <dd>{doc.source.mime_type ?? "—"}</dd>
        <dt>Dimensões</dt>
        <dd>
          {doc.source.width} × {doc.source.height}
        </dd>
        <dt>Tamanho</dt>
        <dd>{prettyBytes(doc.source.size_bytes)}</dd>
        <dt>SHA-256</dt>
        <dd>{shortHash(doc.source.original_hash_sha256, 24)}</dd>
      </dl>
      {doc.scale && (
        <>
          <h3 className={styles.sectionTitle}>Escala</h3>
          <dl className={styles.metaList}>
            <dt>px / {doc.scale.unit}</dt>
            <dd>{doc.scale.px_per_unit.toFixed(2)}</dd>
            <dt>Calibrado</dt>
            <dd>
              {doc.scale.calibration_real_distance.toFixed(2)} {doc.scale.unit}
            </dd>
          </dl>
        </>
      )}
    </>
  );
}

