/**
 * Workbench de um documento selecionado: visualizador (centro) + painel técnico
 * com abas (direita). Carrega blocos OCR/campos/regiões/histórico sob demanda.
 *
 * §13 / metodologia documentoscópica: tudo aqui é APOIO. O texto vem do OCR
 * (RapidOCR/PP-OCRv5, para imagens e PDFs escaneados) ou da camada de texto
 * embutida do PDF (pdf.js, para PDFs digitais) — a origem fica registrada na
 * execução; a extração de campos é heurística e exige revisão; nenhuma função
 * afirma autenticidade/falsidade. A conclusão é do perito.
 */

import { useEffect, useMemo, useRef, useState, type CSSProperties } from "react";
import { convertFileSrc } from "@tauri-apps/api/core";
import {
  AlertTriangle,
  ChevronLeft,
  ChevronRight,
  Columns2,
  Copy,
  FileOutput,
  FileSearch,
  FolderOpen,
  Layers,
  Maximize2,
  Plus,
  RotateCw,
  ScanText,
  SquarePlus,
  Trash2,
  Wand2,
  ZoomIn,
  ZoomOut,
} from "lucide-react";

import { Button } from "@components/Button/Button";
import { commands } from "@core/commands";
import { toSicroError } from "@core/errors";
import { useShortcuts } from "@core/useShortcuts";
import {
  docStatusInfo,
  type DetectedField,
  type DocumentCaseFile,
  type DocumentLog,
  type DocumentRegion,
  type FieldInput,
  type OcrRun,
  type OcrRunInput,
  type OcrRunResult,
  type OcrTextBlock,
  type TextBlockInput,
} from "@domain/documentoscopia";
import { extractFields } from "./fieldExtractors";
import {
  extractPdfText,
  getPdfInfo,
  renderPdfPageToDataUrl,
  renderPdfPageToPngBase64,
  type PdfInfo,
} from "./pdf";
import styles from "./DocumentoscopiaModule.module.css";

type Tab =
  | "texto"
  | "campos"
  | "layout"
  | "realce"
  | "forense"
  | "integridade"
  | "metadados"
  | "historico";

const TABS: { id: Tab; label: string }[] = [
  { id: "texto", label: "Texto" },
  { id: "campos", label: "Campos" },
  { id: "layout", label: "Layout" },
  { id: "realce", label: "Realce" },
  { id: "forense", label: "Indícios" },
  { id: "integridade", label: "Integridade" },
  { id: "metadados", label: "Metadados" },
  { id: "historico", label: "Histórico" },
];

/**
 * Agrupamento das abas por fase do exame (R2). Em vez de 9 abas soltas numa
 * tira que estoura, 4 grupos com sub-abas. A saída (Relatório) virou fase
 * própria do módulo; o confronto idem.
 */
const TAB_GROUPS: { id: string; label: string; tabs: Tab[] }[] = [
  { id: "leitura", label: "Leitura", tabs: ["texto", "realce"] },
  { id: "extracao", label: "Extração", tabs: ["campos", "layout"] },
  { id: "indicios", label: "Indícios digitais", tabs: ["forense"] },
  {
    id: "proveniencia",
    label: "Proveniência",
    tabs: ["integridade", "metadados", "historico"],
  },
];

/** Tipos de região marcáveis manualmente pelo perito. */
const REGION_TYPES: { id: string; label: string }[] = [
  { id: "assinatura", label: "Assinatura" },
  { id: "carimbo", label: "Carimbo" },
  { id: "tabela", label: "Tabela" },
  { id: "logo", label: "Logo / marca" },
  { id: "foto", label: "Foto / imagem" },
  { id: "qrcode", label: "QR Code" },
  { id: "barcode", label: "Código de barras" },
  { id: "rasura", label: "Rasura / alteração" },
  { id: "outro", label: "Outro" },
];

/** Operações de pré-processamento (Fase 4) — ids batem com o backend. */
const REALCE_OPS: { id: string; label: string; hint: string }[] = [
  { id: "cinza", label: "Tons de cinza", hint: "remove cor" },
  { id: "endireitar", label: "Endireitar", hint: "corrige inclinação (deskew)" },
  { id: "niveis", label: "Auto-níveis", hint: "estica o contraste" },
  { id: "clahe", label: "Contraste local", hint: "CLAHE — realça texto fraco" },
  { id: "otsu", label: "Binarizar", hint: "preto e branco (Otsu)" },
  { id: "inverter", label: "Inverter", hint: "negativo (texto claro em fundo escuro)" },
];

function regionTypeLabel(id: string): string {
  return REGION_TYPES.find((t) => t.id === id)?.label ?? id;
}

function confLevel(c: number | null | undefined): "hi" | "mid" | "lo" {
  const v = c ?? 0;
  if (v >= 0.7) return "hi";
  if (v >= 0.45) return "mid";
  return "lo";
}
function pct(c: number | null | undefined): string {
  return c == null ? "—" : `${Math.round(c * 100)}%`;
}
function prettyBytes(n: number): string {
  if (n < 1024) return `${n} B`;
  if (n < 1024 * 1024) return `${(n / 1024).toFixed(1)} KB`;
  return `${(n / 1024 / 1024).toFixed(2)} MB`;
}
function fmtDate(iso: string): string {
  try {
    return new Date(iso).toLocaleString("pt-BR");
  } catch {
    return iso;
  }
}

/** Retângulo normalizado (0..1) — usado para desenhar/posicionar blocos. */
interface Rect {
  x: number;
  y: number;
  w: number;
  h: number;
}

interface Props {
  ws: string;
  doc: DocumentCaseFile;
  onDocChanged: (doc: DocumentCaseFile) => void;
  onDeleted: () => void;
}

export function DocWorkbench({ ws, doc, onDocChanged, onDeleted }: Props) {
  const fileUrl = useMemo(
    () => convertFileSrc(`${ws}/${doc.relative_path}`),
    [ws, doc.relative_path],
  );
  const isImage = doc.file_type === "image";

  const [tab, setTab] = useState<Tab>("texto");
  const activeGroup = TAB_GROUPS.find((g) => g.tabs.includes(tab));
  // Atalhos de navegação entre grupos (customizáveis em Configurações).
  useShortcuts({
    "exame.group.leitura": () => setTab("texto"),
    "exame.group.extracao": () => setTab("campos"),
    "exame.group.indicios": () => setTab("forense"),
    "exame.group.proveniencia": () => setTab("integridade"),
  });
  const [blocks, setBlocks] = useState<OcrTextBlock[]>([]);
  const [runs, setRuns] = useState<OcrRun[]>([]);
  const [fields, setFields] = useState<DetectedField[]>([]);
  const [regions, setRegions] = useState<DocumentRegion[]>([]);
  const [logs, setLogs] = useState<DocumentLog[]>([]);
  const [workingText, setWorkingText] = useState("");
  const [err, setErr] = useState<string | null>(null);
  const [ocrBusy, setOcrBusy] = useState(false);
  const [fieldsBusy, setFieldsBusy] = useState(false);
  const [pdfBusy, setPdfBusy] = useState(false);
  const [pdfInfo, setPdfInfo] = useState<PdfInfo | null>(null);
  const [pdfErr, setPdfErr] = useState<string | null>(null);
  const [pdfPageNum, setPdfPageNum] = useState(1);
  const [pdfPageSrc, setPdfPageSrc] = useState<string | null>(null);
  const [pdfRendering, setPdfRendering] = useState(false);
  const [selectedBlockId, setSelectedBlockId] = useState<string | null>(null);
  const [editingBlockId, setEditingBlockId] = useState<string | null>(null);
  const [editingText, setEditingText] = useState("");
  const [drawMode, setDrawMode] = useState(false);
  const [drawRect, setDrawRect] = useState<Rect | null>(null);
  const [pendingBlock, setPendingBlock] = useState<Rect | null>(null);
  const [newBlockText, setNewBlockText] = useState("");
  // Fase 3 — regiões manuais + detecção automática.
  const [regionMode, setRegionMode] = useState(false);
  const [pendingRegion, setPendingRegion] = useState<Rect | null>(null);
  const [regionType, setRegionType] = useState("assinatura");
  const [regionLabel, setRegionLabel] = useState("");
  const [detectBusy, setDetectBusy] = useState(false);
  const [decodeBusy, setDecodeBusy] = useState(false);
  // Fase 4 — pré-processamento.
  const [realceOps, setRealceOps] = useState<string[]>([]);
  const [processed, setProcessed] = useState<string | null>(null);
  const [processedBusy, setProcessedBusy] = useState(false);
  // Fase 5 (Bloco B) — indícios de manipulação digital.
  const [forensicSrc, setForensicSrc] = useState<string | null>(null);
  const [forensicKind, setForensicKind] = useState<
    "ela" | "noise" | "copymove" | null
  >(null);
  const [forensicBusy, setForensicBusy] = useState(false);
  const [elaQuality, setElaQuality] = useState(90);
  const [elaGain, setElaGain] = useState(15);
  const [noiseWindow, setNoiseWindow] = useState(4);
  const [copyBlock, setCopyBlock] = useState(16);
  const [indicioMsg, setIndicioMsg] = useState<string | null>(null);
  const stageRef = useRef<HTMLDivElement>(null);
  const blockListRef = useRef<HTMLDivElement>(null);
  const drawLayerRef = useRef<HTMLDivElement>(null);
  const drawStart = useRef<{ x: number; y: number } | null>(null);
  const pendingBoxScroll = useRef(false);
  const pendingListScroll = useRef(false);
  const overlayRef = useRef<HTMLDivElement>(null);
  // Geometria ao vivo durante o arraste/redimensionamento de um bloco.
  const [draft, setDraft] = useState<{ id: string; rect: Rect } | null>(null);
  const dragRef = useRef<{
    id: string;
    mode: string;
    sx: number;
    sy: number;
    rect: Rect;
    live?: Rect;
  } | null>(null);

  // Viewer
  const [zoom, setZoom] = useState(1);
  const [rot, setRot] = useState(0);
  const [showBlocks, setShowBlocks] = useState(true);
  const [showRegions, setShowRegions] = useState(true);

  // Campo manual
  const [mName, setMName] = useState("");
  const [mValue, setMValue] = useState("");

  const latestRun = runs[0];

  useEffect(() => {
    let alive = true;
    setZoom(1);
    setRot(0);
    setTab("texto");
    setPdfPageNum(1);
    setPdfPageSrc(null);
    setSelectedBlockId(null);
    setEditingBlockId(null);
    setDrawMode(false);
    setDrawRect(null);
    setPendingBlock(null);
    setNewBlockText("");
    setRegionMode(false);
    setPendingRegion(null);
    setRegionLabel("");
    setProcessed(null);
    setRealceOps([]);
    (async () => {
      try {
        const [r, f, rg, lg] = await Promise.all([
          commands.listOcrRuns(ws, doc.id),
          commands.listFields(ws, doc.id),
          commands.listRegions(ws, doc.id),
          commands.listDocumentLog(ws, doc.id),
        ]);
        if (!alive) return;
        setRuns(r);
        setFields(f);
        setRegions(rg);
        setLogs(lg);
        const latest = r[0];
        if (latest) {
          const blks = await commands.getRunBlocks(ws, latest.id);
          if (!alive) return;
          setBlocks(blks);
          setWorkingText(
            blks.map((b) => b.corrected_text || b.text).join("\n\n"),
          );
        } else {
          setBlocks([]);
          setWorkingText("");
        }
      } catch (e) {
        if (alive) setErr(toSicroError(e).message);
      }
    })();
    return () => {
      alive = false;
    };
  }, [ws, doc.id]);

  // PDF: inspeciona nº de páginas + camada de texto (pdf.js) e persiste o
  // pageinfo/metadados uma única vez. Imagens não passam por aqui. Falha aqui
  // não bloqueia a visualização (o iframe ainda mostra o PDF).
  useEffect(() => {
    if (isImage) {
      setPdfInfo(null);
      setPdfErr(null);
      return;
    }
    let alive = true;
    setPdfInfo(null);
    setPdfErr(null);
    (async () => {
      try {
        const info = await getPdfInfo(fileUrl);
        if (!alive) return;
        setPdfInfo(info);
        if (
          info.pageCount !== doc.page_count ||
          info.hasTextLayer !== doc.has_text_layer
        ) {
          let metaJson = doc.metadata_json || "{}";
          try {
            const base = JSON.parse(metaJson) as Record<string, unknown>;
            if (info.title) base["PDF: Título"] = info.title;
            if (info.author) base["PDF: Autor"] = info.author;
            if (info.producer) base["PDF: Produtor"] = info.producer;
            base["PDF: Páginas"] = info.pageCount;
            metaJson = JSON.stringify(base);
          } catch {
            /* mantém o metadata original se não for JSON válido */
          }
          const updated = await commands.setDocumentPageinfo(
            ws,
            doc.id,
            info.pageCount,
            info.hasTextLayer,
            metaJson,
          );
          if (alive) onDocChanged(updated);
        }
      } catch (e) {
        if (alive) setPdfErr(toSicroError(e).message);
      }
    })();
    return () => {
      alive = false;
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [fileUrl, isImage, doc.id]);

  // Renderiza a página atual do PDF como imagem (pdf.js) para o visualizador —
  // assim o PDF usa o MESMO <img> + overlay da imagem (blocos sobrepostos,
  // zoom, rotação). Re-renderiza ao mudar de página.
  useEffect(() => {
    if (isImage || !pdfInfo) {
      setPdfPageSrc(null);
      return;
    }
    let alive = true;
    setPdfRendering(true);
    (async () => {
      try {
        const src = await renderPdfPageToDataUrl(fileUrl, pdfPageNum, 1.8);
        if (alive) setPdfPageSrc(src);
      } catch (e) {
        if (alive) {
          setPdfPageSrc(null);
          setPdfErr(toSicroError(e).message);
        }
      } finally {
        if (alive) setPdfRendering(false);
      }
    })();
    return () => {
      alive = false;
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [fileUrl, isImage, pdfInfo, pdfPageNum]);

  // Ctrl + scroll para zoom (imagem e PDF). Listener nativo não-passivo —
  // o onWheel do React é passivo e não deixa chamar preventDefault.
  useEffect(() => {
    const el = stageRef.current;
    if (!el) return;
    const onWheel = (e: WheelEvent) => {
      if (!e.ctrlKey) return;
      e.preventDefault();
      const step = e.deltaY < 0 ? 0.15 : -0.15;
      setZoom((z) => Math.min(6, Math.max(0.25, Math.round((z + step) * 100) / 100)));
    };
    el.addEventListener("wheel", onWheel, { passive: false });
    return () => el.removeEventListener("wheel", onWheel);
  }, []);

  // Realce bidirecional: ao mudar a seleção, rola a CONTRAPARTE até a vista
  // (caixa no documento ↔ item na lista). Os deps cobrem troca de página
  // (pdfPageSrc) e de aba (tab), pois a contraparte pode só existir depois.
  useEffect(() => {
    if (!selectedBlockId) return;
    if (pendingBoxScroll.current) {
      const el = stageRef.current?.querySelector<HTMLElement>(
        `[data-block-id="${selectedBlockId}"]`,
      );
      if (el) {
        el.scrollIntoView({ block: "center", inline: "center", behavior: "smooth" });
        pendingBoxScroll.current = false;
      }
    }
    if (pendingListScroll.current) {
      const el = blockListRef.current?.querySelector<HTMLElement>(
        `[data-block-id="${selectedBlockId}"]`,
      );
      if (el) {
        el.scrollIntoView({ block: "nearest", behavior: "smooth" });
        pendingListScroll.current = false;
      }
    }
  }, [selectedBlockId, pdfPageSrc, tab, showBlocks]);

  // Clique na CAIXA do documento → seleciona e leva o item da lista à vista.
  const selectFromOverlay = (id: string) => {
    setSelectedBlockId(id);
    setTab("texto");
    pendingListScroll.current = true;
  };

  // Clique no ITEM da lista → seleciona, garante a camada visível, vai à
  // página do bloco (PDF) e leva a caixa do documento à vista.
  const selectFromList = (b: OcrTextBlock) => {
    setSelectedBlockId(b.id);
    setShowBlocks(true);
    if (!isImage && b.page_number !== pdfPageNum) {
      setPdfPageNum(b.page_number);
    }
    pendingBoxScroll.current = true;
  };

  // Texto de trabalho recomposto a partir dos blocos (ordem de leitura).
  const blocksToText = (bs: OcrTextBlock[]) =>
    [...bs]
      .sort((a, b) => a.page_number - b.page_number || a.reading_order - b.reading_order)
      .map((b) => b.corrected_text || b.text)
      .join("\n\n");

  // --- Revisão de bloco: corrigir o texto reconhecido ---
  const startEditBlock = (b: OcrTextBlock) => {
    setSelectedBlockId(b.id);
    setEditingBlockId(b.id);
    setEditingText(b.corrected_text || b.text);
  };
  const saveEditBlock = async () => {
    const id = editingBlockId;
    if (!id) return;
    setEditingBlockId(null);
    const txt = editingText;
    try {
      await commands.reviewTextBlock(ws, id, txt, true);
      const nb = blocks.map((x) =>
        x.id === id ? { ...x, corrected_text: txt, reviewed: true } : x,
      );
      setBlocks(nb);
      setWorkingText(blocksToText(nb));
    } catch (e) {
      setErr(toSicroError(e).message);
    }
  };
  const deleteBlock = async (b: OcrTextBlock) => {
    try {
      await commands.deleteTextBlock(ws, b.id);
      const nb = blocks.filter((x) => x.id !== b.id);
      setBlocks(nb);
      setWorkingText(blocksToText(nb));
      if (selectedBlockId === b.id) setSelectedBlockId(null);
      if (editingBlockId === b.id) setEditingBlockId(null);
    } catch (e) {
      setErr(toSicroError(e).message);
    }
  };

  // --- Criação de bloco manual: desenhar um retângulo no documento ---
  const exitDrawMode = () => {
    setDrawMode(false);
    setRegionMode(false);
    setDrawRect(null);
    setPendingBlock(null);
    setNewBlockText("");
    setPendingRegion(null);
    setRegionLabel("");
    drawStart.current = null;
  };
  const frac = (e: React.MouseEvent): { x: number; y: number } => {
    const r = drawLayerRef.current?.getBoundingClientRect();
    if (!r || r.width === 0 || r.height === 0) return { x: 0, y: 0 };
    return {
      x: Math.min(1, Math.max(0, (e.clientX - r.left) / r.width)),
      y: Math.min(1, Math.max(0, (e.clientY - r.top) / r.height)),
    };
  };
  const onDrawDown = (e: React.MouseEvent) => {
    if (pendingBlock || pendingRegion) return; // já há algo aguardando confirmação
    e.preventDefault();
    const p = frac(e);
    drawStart.current = p;
    setDrawRect({ x: p.x, y: p.y, w: 0, h: 0 });
  };
  const onDrawMove = (e: React.MouseEvent) => {
    const s = drawStart.current;
    if (!s) return;
    const p = frac(e);
    setDrawRect({
      x: Math.min(s.x, p.x),
      y: Math.min(s.y, p.y),
      w: Math.abs(p.x - s.x),
      h: Math.abs(p.y - s.y),
    });
  };
  const onDrawUp = () => {
    const r = drawRect;
    drawStart.current = null;
    setDrawRect(null);
    // ignora cliques/retângulos minúsculos (evita marcações acidentais)
    if (r && r.w > 0.012 && r.h > 0.006) {
      if (regionMode) {
        setPendingRegion(r);
        setTab("layout");
      } else {
        setPendingBlock(r);
        setNewBlockText("");
        setTab("texto");
      }
    }
  };
  const cancelNewBlock = () => {
    setPendingBlock(null);
    setNewBlockText("");
  };
  const confirmNewBlock = async () => {
    if (!pendingBlock || !newBlockText.trim()) {
      cancelNewBlock();
      return;
    }
    try {
      const blk = await commands.addManualBlock(
        ws,
        doc.id,
        isImage ? 1 : pdfPageNum,
        newBlockText.trim(),
        pendingBlock,
      );
      const nb = [...blocks, blk];
      setBlocks(nb);
      setWorkingText(blocksToText(nb));
      setShowBlocks(true);
      setSelectedBlockId(blk.id);
    } catch (e) {
      setErr(toSicroError(e).message);
    }
    cancelNewBlock();
  };

  // Imagem (base64 PNG, sem prefixo) + nº da página atual — para detecção/realce.
  const currentPageBase64 = async (): Promise<{ base64: string; page: number }> => {
    if (isImage) {
      const blob = await (await fetch(fileUrl)).blob();
      const dataUrl: string = await new Promise((resolve, reject) => {
        const fr = new FileReader();
        fr.onload = () => resolve(fr.result as string);
        fr.onerror = () => reject(new Error("falha ao ler a imagem"));
        fr.readAsDataURL(blob);
      });
      return { base64: dataUrl.split(",")[1] ?? "", page: 1 };
    }
    const b64 = await renderPdfPageToPngBase64(fileUrl, pdfPageNum, 2.0);
    return { base64: b64, page: pdfPageNum };
  };

  // --- Fase 3: região manual (desenhada) ---
  const cancelNewRegion = () => {
    setPendingRegion(null);
    setRegionLabel("");
  };
  const confirmNewRegion = async () => {
    if (!pendingRegion) return;
    try {
      const saved = await commands.saveRegion(ws, doc.id, {
        page_number: isImage ? 1 : pdfPageNum,
        region_type: regionType,
        bbox_x: pendingRegion.x,
        bbox_y: pendingRegion.y,
        bbox_w: pendingRegion.w,
        bbox_h: pendingRegion.h,
        label: regionLabel.trim(),
        confidence: null,
        notes: "",
      });
      setRegions((prev) => [...prev, saved]);
      setShowRegions(true);
    } catch (e) {
      setErr(toSicroError(e).message);
    }
    cancelNewRegion();
  };
  // Tenta decodificar um QR/código de barras dentro da região desenhada
  // (recorte isolado + ampliado — funciona onde a detecção da página falha).
  const tryDecodeRegion = async () => {
    if (!pendingRegion) return;
    setDecodeBusy(true);
    setErr(null);
    try {
      const { base64 } = await currentPageBase64();
      const res = await commands.decodeRegion(base64, pendingRegion);
      if (res) {
        setRegionType(res.region_type);
        setRegionLabel(res.label);
      } else {
        setErr(
          "não consegui ler o código nesta região — tente desenhar mais justo " +
            "ao redor dele, ou melhore a imagem na aba Realce antes.",
        );
      }
    } catch (e) {
      setErr(toSicroError(e).message);
    } finally {
      setDecodeBusy(false);
    }
  };

  // --- Fase 3: detecção automática (QR/código de barras/tabela) ---
  const runDetectLayout = async () => {
    setDetectBusy(true);
    setErr(null);
    try {
      const { base64, page } = await currentPageBase64();
      const found = await commands.detectLayout(ws, doc.id, page, base64);
      setRegions((prev) => [...prev, ...found]);
      setShowRegions(true);
    } catch (e) {
      setErr(toSicroError(e).message);
    } finally {
      setDetectBusy(false);
    }
  };

  // --- Fase 4: pré-processamento (realce) ---
  const toggleRealceOp = (id: string) =>
    setRealceOps((p) => (p.includes(id) ? p.filter((x) => x !== id) : [...p, id]));
  const applyRealce = async () => {
    setProcessedBusy(true);
    setErr(null);
    try {
      const { base64 } = await currentPageBase64();
      const out = await commands.preprocessImage(base64, realceOps);
      setProcessed(`data:image/png;base64,${out}`);
    } catch (e) {
      setErr(toSicroError(e).message);
    } finally {
      setProcessedBusy(false);
    }
  };
  const ocrOnProcessed = async () => {
    if (!processed) return;
    setOcrBusy(true);
    setErr(null);
    try {
      const raw = processed.split(",")[1] ?? "";
      const res = await commands.runOcrPageImage(
        ws,
        doc.id,
        isImage ? 1 : pdfPageNum,
        raw,
        "por",
      );
      setBlocks(res.blocks);
      setWorkingText(
        res.blocks.map((b) => b.corrected_text || b.text).join("\n\n"),
      );
      const [r, lg] = await Promise.all([
        commands.listOcrRuns(ws, doc.id),
        commands.listDocumentLog(ws, doc.id),
      ]);
      setRuns(r);
      setLogs(lg);
      await refreshDoc();
      setTab("texto");
    } catch (e) {
      setErr(toSicroError(e).message);
    } finally {
      setOcrBusy(false);
    }
  };

  // --- Fase 5 (Bloco B): indícios de manipulação digital ---
  const runEla = async () => {
    setForensicBusy(true);
    setErr(null);
    try {
      let base64: string;
      if (!isImage) {
        // PDF: o ELA só faz sentido sobre o JPEG embutido (documento
        // escaneado), nunca sobre a página re-renderizada.
        const jpeg = await commands.extractPdfJpeg(ws, doc.relative_path, pdfPageNum);
        if (!jpeg) {
          setErr(
            "Esta página do PDF não tem imagem JPEG embutida (provável texto " +
              "vetorial) — o ELA não se aplica aqui.",
          );
          return;
        }
        base64 = jpeg;
      } else {
        base64 = (await currentPageBase64()).base64;
      }
      const out = await commands.docEla(base64, elaQuality, elaGain);
      setForensicSrc(`data:image/png;base64,${out}`);
      setForensicKind("ela");
    } catch (e) {
      setErr(toSicroError(e).message);
    } finally {
      setForensicBusy(false);
    }
  };
  const runNoise = async () => {
    setForensicBusy(true);
    setErr(null);
    try {
      const { base64 } = await currentPageBase64();
      const out = await commands.docNoiseMap(base64, noiseWindow);
      setForensicSrc(`data:image/png;base64,${out}`);
      setForensicKind("noise");
    } catch (e) {
      setErr(toSicroError(e).message);
    } finally {
      setForensicBusy(false);
    }
  };
  const runCopyMove = async () => {
    setForensicBusy(true);
    setErr(null);
    try {
      // Copy-move trabalha na estatística dos pixels — vale para qualquer
      // raster (imagem ou página de PDF rasterizada), sem depender de JPEG.
      const { base64 } = await currentPageBase64();
      const out = await commands.docCopyMove(base64, copyBlock, Math.max(4, copyBlock / 2));
      setForensicSrc(`data:image/png;base64,${out}`);
      setForensicKind("copymove");
    } catch (e) {
      setErr(toSicroError(e).message);
    } finally {
      setForensicBusy(false);
    }
  };
  // Salva o heatmap atual na bandeja (documentoscopia/indicios). reveal=true
  // abre a pasta (exportar); reveal=false só disponibiliza no laudo.
  const saveIndicio = async (reveal: boolean) => {
    if (!forensicSrc) return;
    setErr(null);
    try {
      const raw = forensicSrc.split(",")[1] ?? "";
      const kindSlug =
        forensicKind === "ela"
          ? "ela"
          : forensicKind === "noise"
            ? "ruido"
            : "copymove";
      const name = `indicio_${kindSlug}_${(doc.title || "doc").slice(0, 24)}_p${
        isImage ? 1 : pdfPageNum
      }.png`;
      const rel = await commands.saveDocIndicio(ws, raw, name);
      if (reveal) {
        await commands.revealEvidenceInFolder(ws, rel);
        setIndicioMsg("Exportado — pasta aberta.");
      } else {
        setIndicioMsg("Enviado: aparece em Evidências → Indícios ao abrir o laudo.");
      }
    } catch (e) {
      setErr(toSicroError(e).message);
    }
  };

  // --- Exportar PDF pesquisável (imagem + camada de texto invisível) ---
  const exportSearchable = async () => {
    setPdfBusy(true);
    setErr(null);
    try {
      const toSB = (b: OcrTextBlock) => ({
        text: b.corrected_text || b.text,
        bbox_x: b.bbox_x,
        bbox_y: b.bbox_y,
        bbox_w: b.bbox_w,
        bbox_h: b.bbox_h,
      });
      const imgDims = (src: string) =>
        new Promise<{ w: number; h: number }>((resolve) => {
          const im = new Image();
          im.onload = () =>
            resolve({ w: im.naturalWidth || 1, h: im.naturalHeight || 1 });
          im.onerror = () => resolve({ w: 1, h: 1 });
          im.src = src;
        });
      type Pg = {
        image_base64: string;
        width: number;
        height: number;
        blocks: ReturnType<typeof toSB>[];
      };
      const pages: Pg[] = [];

      if (isImage) {
        // Imagem original → data URL + dimensões naturais.
        const blob = await (await fetch(fileUrl)).blob();
        const dataUrl: string = await new Promise((resolve, reject) => {
          const fr = new FileReader();
          fr.onload = () => resolve(fr.result as string);
          fr.onerror = () => reject(new Error("falha ao ler a imagem"));
          fr.readAsDataURL(blob);
        });
        const dims = await imgDims(dataUrl);
        pages.push({
          image_base64: dataUrl,
          width: dims.w,
          height: dims.h,
          blocks: blocks.map(toSB),
        });
      } else {
        // PDF → rasteriza cada página e agrupa os blocos por página.
        const total = pdfInfo?.pageCount ?? 1;
        for (let p = 1; p <= total; p++) {
          const dataUrl = await renderPdfPageToDataUrl(fileUrl, p, 2.0);
          const dims = await imgDims(dataUrl);
          pages.push({
            image_base64: dataUrl,
            width: dims.w,
            height: dims.h,
            blocks: blocks.filter((b) => b.page_number === p).map(toSB),
          });
        }
      }
      const rel = await commands.exportSearchablePdf(ws, doc.id, pages);
      await commands.revealEvidenceInFolder(ws, rel);
    } catch (e) {
      setErr(toSicroError(e).message);
    } finally {
      setPdfBusy(false);
    }
  };

  // --- Mover / redimensionar a caixa do bloco selecionado sobre o documento ---
  const HANDLES: { dir: string; x: number; y: number; cur: string }[] = [
    { dir: "nw", x: 0, y: 0, cur: "nwse-resize" },
    { dir: "n", x: 0.5, y: 0, cur: "ns-resize" },
    { dir: "ne", x: 1, y: 0, cur: "nesw-resize" },
    { dir: "w", x: 0, y: 0.5, cur: "ew-resize" },
    { dir: "e", x: 1, y: 0.5, cur: "ew-resize" },
    { dir: "sw", x: 0, y: 1, cur: "nesw-resize" },
    { dir: "s", x: 0.5, y: 1, cur: "ns-resize" },
    { dir: "se", x: 1, y: 1, cur: "nwse-resize" },
  ];
  const beginGeom = (e: React.PointerEvent, b: OcrTextBlock, mode: string) => {
    e.preventDefault();
    e.stopPropagation();
    const rect = { x: b.bbox_x, y: b.bbox_y, w: b.bbox_w, h: b.bbox_h };
    dragRef.current = {
      id: b.id,
      mode,
      sx: e.clientX,
      sy: e.clientY,
      rect,
      live: rect,
    };
    setSelectedBlockId(b.id);
    setDraft({ id: b.id, rect });
    (e.currentTarget as HTMLElement).setPointerCapture?.(e.pointerId);
  };
  const moveGeom = (e: React.PointerEvent) => {
    const d = dragRef.current;
    const rb = overlayRef.current?.getBoundingClientRect();
    if (!d || !rb || rb.width === 0 || rb.height === 0) return;
    const dx = (e.clientX - d.sx) / rb.width;
    const dy = (e.clientY - d.sy) / rb.height;
    const MIN = 0.006;
    let { x, y, w, h } = d.rect;
    if (d.mode === "move") {
      x = d.rect.x + dx;
      y = d.rect.y + dy;
    } else {
      if (d.mode.includes("e")) w = d.rect.w + dx;
      if (d.mode.includes("s")) h = d.rect.h + dy;
      if (d.mode.includes("w")) {
        x = d.rect.x + dx;
        w = d.rect.w - dx;
      }
      if (d.mode.includes("n")) {
        y = d.rect.y + dy;
        h = d.rect.h - dy;
      }
    }
    if (w < MIN) {
      if (d.mode.includes("w")) x = d.rect.x + d.rect.w - MIN;
      w = MIN;
    }
    if (h < MIN) {
      if (d.mode.includes("n")) y = d.rect.y + d.rect.h - MIN;
      h = MIN;
    }
    x = Math.max(0, Math.min(x, 1 - w));
    y = Math.max(0, Math.min(y, 1 - h));
    w = Math.min(w, 1 - x);
    h = Math.min(h, 1 - y);
    const rect = { x, y, w, h };
    d.live = rect;
    setDraft({ id: d.id, rect });
  };
  const endGeom = async (e: React.PointerEvent) => {
    const d = dragRef.current;
    dragRef.current = null;
    setDraft(null);
    (e.currentTarget as HTMLElement).releasePointerCapture?.(e.pointerId);
    if (!d) return;
    const r = d.live ?? d.rect;
    const moved =
      Math.abs(r.x - d.rect.x) +
      Math.abs(r.y - d.rect.y) +
      Math.abs(r.w - d.rect.w) +
      Math.abs(r.h - d.rect.h);
    if (moved < 0.002) {
      // Quase não mexeu → tratou-se de um clique: seleciona e localiza na lista.
      if (d.mode === "move") selectFromOverlay(d.id);
      return;
    }
    try {
      await commands.setBlockBbox(ws, d.id, r);
      setBlocks((prev) =>
        prev.map((x) =>
          x.id === d.id
            ? { ...x, bbox_x: r.x, bbox_y: r.y, bbox_w: r.w, bbox_h: r.h }
            : x,
        ),
      );
    } catch (err) {
      setErr(toSicroError(err).message);
    }
  };

  const refreshDoc = async () => {
    try {
      const fresh = await commands.getDocument(ws, doc.id);
      onDocChanged(fresh);
    } catch {
      /* silencioso */
    }
  };

  const runOcr = async () => {
    setOcrBusy(true);
    setErr(null);
    try {
      let res: OcrRunResult;
      if (isImage) {
        // Imagem → OCR direto do arquivo (o backend lê os pixels).
        res = await commands.runOcr(ws, doc.id, 1, "por");
      } else if (pdfInfo?.hasTextLayer) {
        // PDF digital → extrai a camada de texto embutida, SEM OCR
        // (instantâneo e fiel ao original). Um bloco por página.
        const pages = await extractPdfText(fileUrl);
        const blocks: TextBlockInput[] = pages
          .filter((p) => p.text.trim())
          .map((p, i) => ({
            page_number: p.page,
            text: p.text,
            confidence: 1,
            bbox_x: 0.05,
            bbox_y: 0.05,
            bbox_w: 0.9,
            bbox_h: 0.9,
            block_type: "paragraph",
            reading_order: i,
          }));
        if (blocks.length === 0) {
          throw new Error(
            "o PDF foi lido, mas não retornou texto embutido — reimporte a " +
              "página como imagem para usar o OCR.",
          );
        }
        const run: OcrRunInput = {
          page_number: null,
          engine: "pdf_text_layer",
          engine_version: "pdf.js",
          language: "pt",
          mode: "full_document",
          parameters_json: JSON.stringify({ source: "pdf_text_layer" }),
          blocks,
        };
        res = await commands.saveOcrRun(ws, doc.id, run);
      } else {
        // PDF escaneado → rasteriza a página ATUAL (pdf.js) e manda pro RapidOCR.
        const b64 = await renderPdfPageToPngBase64(fileUrl, pdfPageNum, 2.5);
        res = await commands.runOcrPageImage(ws, doc.id, pdfPageNum, b64, "por");
      }
      setBlocks(res.blocks);
      setWorkingText(
        res.blocks.map((b) => b.corrected_text || b.text).join("\n\n"),
      );
      const [r, lg] = await Promise.all([
        commands.listOcrRuns(ws, doc.id),
        commands.listDocumentLog(ws, doc.id),
      ]);
      setRuns(r);
      setLogs(lg);
      await refreshDoc();
    } catch (e) {
      setErr(toSicroError(e).message);
    } finally {
      setOcrBusy(false);
    }
  };

  const saveReviewedText = async () => {
    setOcrBusy(true);
    setErr(null);
    try {
      const run: OcrRunInput = {
        page_number: 1,
        engine: "manual",
        engine_version: "",
        language: "pt",
        mode: "page",
        parameters_json: "{}",
        blocks: [
          {
            page_number: 1,
            text: workingText,
            confidence: 1,
            bbox_x: 0.05,
            bbox_y: 0.05,
            bbox_w: 0.9,
            bbox_h: 0.9,
            block_type: "paragraph",
            reading_order: 0,
          },
        ],
      };
      await commands.saveOcrRun(ws, doc.id, run);
      const [r, lg] = await Promise.all([
        commands.listOcrRuns(ws, doc.id),
        commands.listDocumentLog(ws, doc.id),
      ]);
      setRuns(r);
      setLogs(lg);
      await refreshDoc();
    } catch (e) {
      setErr(toSicroError(e).message);
    } finally {
      setOcrBusy(false);
    }
  };

  const detectFields = async () => {
    setFieldsBusy(true);
    setErr(null);
    try {
      const found = extractFields(workingText);
      const inputs: FieldInput[] = found.map((f) => ({
        page_number: 1,
        field_name: f.field_name,
        field_value: f.field_value,
        confidence: f.confidence,
        source: "heuristica",
        bbox_x: null,
        bbox_y: null,
        bbox_w: null,
        bbox_h: null,
      }));
      const saved = await commands.saveFields(ws, doc.id, inputs, "heuristica");
      setFields(saved);
      setTab("campos");
    } catch (e) {
      setErr(toSicroError(e).message);
    } finally {
      setFieldsBusy(false);
    }
  };

  const addManualField = async () => {
    if (!mName.trim() || !mValue.trim()) return;
    setFieldsBusy(true);
    try {
      const input: FieldInput = {
        page_number: null,
        field_name: mName.trim(),
        field_value: mValue.trim(),
        confidence: 1,
        source: "manual",
        bbox_x: null,
        bbox_y: null,
        bbox_w: null,
        bbox_h: null,
      };
      const saved = await commands.saveFields(ws, doc.id, [input]);
      setFields(saved);
      setMName("");
      setMValue("");
    } catch (e) {
      setErr(toSicroError(e).message);
    } finally {
      setFieldsBusy(false);
    }
  };

  const toggleFieldReview = async (f: DetectedField) => {
    try {
      await commands.reviewField(ws, f.id, f.corrected_value, !f.reviewed);
      setFields((prev) =>
        prev.map((x) => (x.id === f.id ? { ...x, reviewed: !x.reviewed } : x)),
      );
    } catch (e) {
      setErr(toSicroError(e).message);
    }
  };

  const metaEntries = useMemo<[string, string][]>(() => {
    try {
      const obj = JSON.parse(doc.metadata_json || "{}") as Record<string, unknown>;
      return Object.entries(obj).map(
        ([k, v]) => [k, String(v)] as [string, string],
      );
    } catch {
      return [];
    }
  }, [doc.metadata_json]);

  const copy = (text: string) => {
    void navigator.clipboard?.writeText(text);
  };

  const status = docStatusInfo(doc.status);

  // Rótulo/estado do botão de extração conforme a natureza do documento.
  const ocrLabel = isImage
    ? "Rascunho OCR"
    : pdfInfo?.hasTextLayer
      ? "Extrair texto do PDF"
      : "OCR da página (PDF)";
  const ocrDisabled = ocrBusy || (!isImage && !pdfInfo);

  // Visualizador unificado: imagem e PDF usam o MESMO <img> + overlay. No PDF,
  // a "imagem" é a página renderizada pelo pdf.js; os overlays são filtrados
  // pela página atual.
  const rasterSrc = isImage ? fileUrl : pdfPageSrc;
  const hasRaster = !!rasterSrc;
  const pageCount = isImage ? 1 : pdfInfo?.pageCount ?? 1;
  const canPageNav = !isImage && pageCount > 1;
  const visibleBlocks = isImage
    ? blocks
    : blocks.filter((b) => b.page_number === pdfPageNum);
  const visibleRegions = isImage
    ? regions
    : regions.filter((r) => r.page_number === pdfPageNum);

  // Zoom pela propriedade `zoom` (afeta o LAYOUT) em vez de `transform: scale`:
  // com scale, o overflow de topo/esquerda fica inacessível no container de
  // scroll e "come" a parte de cima ao ampliar. Rotação continua no transform.
  const wrapStyle: CSSProperties = {
    transform: rot ? `rotate(${rot}deg)` : undefined,
  };
  (wrapStyle as Record<string, unknown>).zoom = zoom;

  return (
    <>
      {/* ----- Visualizador ----- */}
      <section className={styles.viewer}>
        <div className={styles.viewerToolbar}>
          <span className={styles.viewerName} title={doc.original_filename}>
            {doc.title}
          </span>
          <span className={styles.badge} data-tone={status.tone}>
            {status.label}
          </span>
          <span className={styles.flexSpacer} />
          {canPageNav && (
            <>
              <button
                className={styles.iconBtn}
                title="Página anterior"
                disabled={pdfPageNum <= 1}
                onClick={() => setPdfPageNum((p) => Math.max(1, p - 1))}
              >
                <ChevronLeft size={15} />
              </button>
              <span className={styles.zoomLabel}>
                {pdfPageNum} / {pageCount}
              </span>
              <button
                className={styles.iconBtn}
                title="Próxima página"
                disabled={pdfPageNum >= pageCount}
                onClick={() => setPdfPageNum((p) => Math.min(pageCount, p + 1))}
              >
                <ChevronRight size={15} />
              </button>
              <span className={styles.divider} />
            </>
          )}
          {hasRaster && (
            <>
              <button
                className={styles.iconBtn}
                title="Diminuir zoom"
                onClick={() => setZoom((z) => Math.max(0.25, z - 0.25))}
              >
                <ZoomOut size={15} />
              </button>
              <span className={styles.zoomLabel}>{Math.round(zoom * 100)}%</span>
              <button
                className={styles.iconBtn}
                title="Aumentar zoom"
                onClick={() => setZoom((z) => Math.min(6, z + 0.25))}
              >
                <ZoomIn size={15} />
              </button>
              <button
                className={styles.iconBtn}
                title="Ajustar"
                onClick={() => {
                  setZoom(1);
                  setRot(0);
                }}
              >
                <Maximize2 size={15} />
              </button>
              <button
                className={styles.iconBtn}
                title="Girar 90°"
                onClick={() => setRot((r) => (r + 90) % 360)}
              >
                <RotateCw size={15} />
              </button>
              <span className={styles.divider} />
              <button
                className={styles.iconBtn}
                data-active={showBlocks}
                title="Camada de texto (OCR)"
                onClick={() => setShowBlocks((v) => !v)}
              >
                <ScanText size={15} />
              </button>
              <button
                className={styles.iconBtn}
                data-active={showRegions}
                title="Camada de regiões"
                onClick={() => setShowRegions((v) => !v)}
              >
                <Layers size={15} />
              </button>
              <span className={styles.divider} />
              <button
                className={styles.iconBtn}
                data-active={drawMode}
                title="Novo campo — desenhar um retângulo e digitar o texto (manual)"
                onClick={() =>
                  drawMode ? exitDrawMode() : (setRegionMode(false), setDrawMode(true))
                }
              >
                <SquarePlus size={15} />
              </button>
            </>
          )}
          <button
            className={styles.iconBtn}
            title="Abrir pasta do documento"
            onClick={() => void commands.revealEvidenceInFolder(ws, doc.relative_path)}
          >
            <FolderOpen size={15} />
          </button>
        </div>

        <div className={styles.viewerStage} ref={stageRef}>
          {hasRaster ? (
            <div className={styles.imgWrap} style={wrapStyle}>
              <img
                className={styles.docImg}
                src={rasterSrc as string}
                alt={doc.title}
              />
              <div className={styles.overlay} ref={overlayRef}>
                {showBlocks &&
                  visibleBlocks.map((b) => {
                    const sel = selectedBlockId === b.id;
                    const r =
                      draft && draft.id === b.id
                        ? draft.rect
                        : { x: b.bbox_x, y: b.bbox_y, w: b.bbox_w, h: b.bbox_h };
                    return (
                      <div
                        key={b.id}
                        data-block-id={b.id}
                        className={styles.ovBlock}
                        data-level={confLevel(b.confidence)}
                        data-reviewed={b.reviewed || b.block_type === "manual"}
                        data-selected={sel}
                        data-editable={sel}
                        style={{
                          left: `${r.x * 100}%`,
                          top: `${r.y * 100}%`,
                          width: `${r.w * 100}%`,
                          height: `${r.h * 100}%`,
                        }}
                        title={`${b.text} (${pct(b.confidence)})`}
                        onPointerDown={(e) => beginGeom(e, b, "move")}
                        onPointerMove={moveGeom}
                        onPointerUp={endGeom}
                      >
                        {sel &&
                          HANDLES.map((hd) => (
                            <span
                              key={hd.dir}
                              className={styles.ovHandle}
                              style={{
                                left: `${hd.x * 100}%`,
                                top: `${hd.y * 100}%`,
                                cursor: hd.cur,
                              }}
                              onPointerDown={(e) => beginGeom(e, b, hd.dir)}
                              onPointerMove={moveGeom}
                              onPointerUp={endGeom}
                            />
                          ))}
                      </div>
                    );
                  })}
                {showRegions &&
                  visibleRegions.map((r) => (
                    <div
                      key={r.id}
                      className={styles.ovRegion}
                      style={{
                        left: `${r.bbox_x * 100}%`,
                        top: `${r.bbox_y * 100}%`,
                        width: `${r.bbox_w * 100}%`,
                        height: `${r.bbox_h * 100}%`,
                      }}
                      title={`${r.region_type}${r.label ? ` · ${r.label}` : ""}`}
                    />
                  ))}
              </div>
              {(drawMode || regionMode) && (
                <div
                  ref={drawLayerRef}
                  className={styles.drawLayer}
                  onMouseDown={onDrawDown}
                  onMouseMove={onDrawMove}
                  onMouseUp={onDrawUp}
                  onMouseLeave={onDrawUp}
                >
                  {(drawRect ?? pendingBlock) && (
                    <div
                      className={styles.drawRect}
                      style={{
                        left: `${(drawRect ?? pendingBlock)!.x * 100}%`,
                        top: `${(drawRect ?? pendingBlock)!.y * 100}%`,
                        width: `${(drawRect ?? pendingBlock)!.w * 100}%`,
                        height: `${(drawRect ?? pendingBlock)!.h * 100}%`,
                      }}
                    />
                  )}
                </div>
              )}
            </div>
          ) : (
            <div className={styles.pdfPlaceholder}>
              {pdfErr
                ? "Não foi possível exibir o PDF."
                : pdfRendering
                  ? "Renderizando página…"
                  : "Carregando…"}
            </div>
          )}
        </div>
      </section>

      {/* ----- Painel técnico ----- */}
      <aside className={styles.panel}>
        <div className={styles.tabGroups} role="tablist" aria-label="Grupos do exame">
          {TAB_GROUPS.map((g) => (
            <button
              key={g.id}
              role="tab"
              className={styles.tabGroupBtn}
              data-active={g.tabs.includes(tab)}
              aria-selected={g.tabs.includes(tab)}
              onClick={() => {
                const first = g.tabs[0];
                if (first && !g.tabs.includes(tab)) setTab(first);
              }}
            >
              {g.label}
            </button>
          ))}
        </div>
        {activeGroup && activeGroup.tabs.length > 1 && (
          <div className={styles.tabBar}>
            {activeGroup.tabs.map((tid) => {
              const t = TABS.find((x) => x.id === tid);
              return (
                <button
                  key={tid}
                  className={styles.tabBtn}
                  data-active={tab === tid}
                  onClick={() => setTab(tid)}
                >
                  {t?.label ?? tid}
                </button>
              );
            })}
          </div>
        )}

        {err && (
          <div className={styles.panelError}>
            <AlertTriangle size={13} /> {err}
          </div>
        )}

        <div className={styles.tabBody}>
          {tab === "texto" && (
            <>
              <div className={styles.rowBtns}>
                <Button
                  size="sm"
                  variant="primary"
                  leftIcon={<ScanText size={14} />}
                  onClick={() => void runOcr()}
                  disabled={ocrDisabled}
                >
                  {ocrBusy ? "Processando…" : ocrLabel}
                </Button>
                <Button
                  size="sm"
                  variant="secondary"
                  onClick={() => void saveReviewedText()}
                  disabled={ocrBusy}
                >
                  Salvar texto revisado
                </Button>
                <Button
                  size="sm"
                  variant="secondary"
                  leftIcon={<FileSearch size={14} />}
                  onClick={() => void exportSearchable()}
                  disabled={pdfBusy || !hasRaster}
                  title="Gera um PDF idêntico ao documento, mas com o texto selecionável e pesquisável (camada invisível)"
                >
                  {pdfBusy ? "Gerando…" : "PDF pesquisável"}
                </Button>
                <button
                  className={styles.iconBtn}
                  title="Copiar texto"
                  onClick={() => copy(workingText)}
                >
                  <Copy size={15} />
                </button>
              </div>
              {!isImage && (
                <p className={styles.hintLine}>
                  {pdfErr
                    ? `Não foi possível analisar o PDF: ${pdfErr}`
                    : !pdfInfo
                      ? "Analisando o PDF…"
                      : pdfInfo.hasTextLayer
                        ? `PDF com texto embutido (${pdfInfo.pageCount} pág.) — extração direta, sem OCR.`
                        : `PDF escaneado (${pdfInfo.pageCount} pág.) — a página exibida é rasterizada e lida pelo OCR.${
                            pdfInfo.pageCount > 1
                              ? " O OCR lê a página atual; troque pela navegação acima."
                              : ""
                          }`}
                </p>
              )}
              {drawMode && !pendingBlock && (
                <p className={styles.hintLine}>
                  Modo <strong>novo campo</strong>: arraste um retângulo sobre o
                  documento, na região onde faltou texto, e digite o conteúdo.
                </p>
              )}
              {pendingBlock && (
                <div className={styles.newBlockForm}>
                  <span className={styles.newBlockTitle}>
                    Novo campo manual nesta região
                  </span>
                  <input
                    className={styles.input}
                    autoFocus
                    value={newBlockText}
                    placeholder="Digite o texto deste campo…"
                    onChange={(e) => setNewBlockText(e.target.value)}
                    onKeyDown={(e) => {
                      if (e.key === "Enter") {
                        e.preventDefault();
                        void confirmNewBlock();
                      } else if (e.key === "Escape") {
                        cancelNewBlock();
                      }
                    }}
                  />
                  <div className={styles.newBlockActions}>
                    <Button
                      size="sm"
                      variant="primary"
                      onClick={() => void confirmNewBlock()}
                    >
                      Adicionar
                    </Button>
                    <Button size="sm" variant="ghost" onClick={cancelNewBlock}>
                      Cancelar
                    </Button>
                  </div>
                </div>
              )}
              {latestRun && (
                <p className={styles.metaLine}>
                  Motor: <strong>{latestRun.engine}</strong> · confiança média{" "}
                  <span className={styles.confChip} data-level={confLevel(latestRun.avg_confidence)}>
                    {pct(latestRun.avg_confidence)}
                  </span>{" "}
                  · {latestRun.block_count} blocos
                </p>
              )}
              <textarea
                className={styles.textArea}
                value={workingText}
                onChange={(e) => setWorkingText(e.target.value)}
                placeholder="Rode o OCR (rascunho) ou cole/digite aqui o texto do documento. Este texto alimenta a extração de campos e o quadro do laudo."
                spellCheck={false}
              />
              {blocks.length > 0 && (
                <>
                  <div className={styles.sectionTitle}>Blocos detectados</div>
                  <div className={styles.blockList} ref={blockListRef}>
                    {blocks.map((b) => (
                      <div
                        key={b.id}
                        data-block-id={b.id}
                        data-selected={selectedBlockId === b.id}
                        className={styles.blockItem}
                        onClick={() => selectFromList(b)}
                        onDoubleClick={() => startEditBlock(b)}
                        title="Clique para localizar · duplo-clique para editar"
                      >
                        <span
                          className={styles.confChip}
                          data-level={
                            b.block_type === "manual"
                              ? "hi"
                              : confLevel(b.confidence)
                          }
                        >
                          {b.block_type === "manual" ? "man" : pct(b.confidence)}
                        </span>
                        {editingBlockId === b.id ? (
                          <input
                            className={styles.blockEdit}
                            autoFocus
                            value={editingText}
                            onClick={(e) => e.stopPropagation()}
                            onChange={(e) => setEditingText(e.target.value)}
                            onKeyDown={(e) => {
                              if (e.key === "Enter") {
                                e.preventDefault();
                                void saveEditBlock();
                              } else if (e.key === "Escape") {
                                setEditingBlockId(null);
                              }
                            }}
                            onBlur={() => void saveEditBlock()}
                          />
                        ) : (
                          <>
                            <span className={styles.blockType}>
                              {b.reviewed ? "✓" : b.block_type}
                            </span>
                            <span className={styles.blockText}>
                              {b.corrected_text || b.text}
                            </span>
                          </>
                        )}
                        <button
                          className={styles.blockDel}
                          title="Remover bloco"
                          onClick={(e) => {
                            e.stopPropagation();
                            void deleteBlock(b);
                          }}
                        >
                          <Trash2 size={12} />
                        </button>
                      </div>
                    ))}
                  </div>
                </>
              )}
            </>
          )}

          {tab === "campos" && (
            <>
              <div className={styles.rowBtns}>
                <Button
                  size="sm"
                  variant="primary"
                  leftIcon={<FileSearch size={14} />}
                  onClick={() => void detectFields()}
                  disabled={fieldsBusy}
                >
                  {fieldsBusy ? "Detectando…" : "Detectar campos"}
                </Button>
              </div>
              <p className={styles.hintLine}>
                Heurística (CPF, CNPJ, placa, chassi, datas, valores, processo…)
                sobre o texto da aba <em>Texto</em>. Tudo exige revisão.
              </p>
              <div className={styles.manualForm}>
                <input
                  className={styles.input}
                  placeholder="campo"
                  value={mName}
                  onChange={(e) => setMName(e.target.value)}
                />
                <input
                  className={styles.input}
                  placeholder="valor"
                  value={mValue}
                  onChange={(e) => setMValue(e.target.value)}
                />
                <button
                  className={styles.iconBtn}
                  title="Adicionar campo manual"
                  onClick={() => void addManualField()}
                >
                  <Plus size={15} />
                </button>
              </div>
              {fields.length === 0 ? (
                <p className={styles.emptyLine}>Nenhum campo detectado ainda.</p>
              ) : (
                <div className={styles.fieldList}>
                  {fields.map((f) => (
                    <div key={f.id} className={styles.fieldRow}>
                      <div className={styles.fieldMain}>
                        <span className={styles.fieldName}>{f.field_name}</span>
                        <span className={styles.fieldVal}>
                          {f.corrected_value || f.field_value}
                        </span>
                      </div>
                      <span
                        className={styles.confChip}
                        data-level={confLevel(f.confidence)}
                        title={f.source}
                      >
                        {f.source === "manual" ? "manual" : pct(f.confidence)}
                      </span>
                      <button
                        className={styles.iconBtn}
                        data-active={f.reviewed}
                        title={f.reviewed ? "Revisado" : "Marcar revisado"}
                        onClick={() => void toggleFieldReview(f)}
                      >
                        ✓
                      </button>
                      <button
                        className={styles.iconBtn}
                        title="Copiar valor"
                        onClick={() => copy(f.corrected_value || f.field_value)}
                      >
                        <Copy size={14} />
                      </button>
                    </div>
                  ))}
                </div>
              )}
            </>
          )}

          {tab === "layout" && (
            <>
              <div className={styles.rowBtns}>
                <Button
                  size="sm"
                  variant="primary"
                  leftIcon={<FileSearch size={14} />}
                  onClick={() => void runDetectLayout()}
                  disabled={detectBusy || (!isImage && !pdfInfo)}
                >
                  {detectBusy ? "Detectando…" : "Detectar QR / barras / tabela"}
                </Button>
                <Button
                  size="sm"
                  variant={regionMode ? "primary" : "secondary"}
                  leftIcon={<SquarePlus size={14} />}
                  onClick={() =>
                    regionMode
                      ? exitDrawMode()
                      : (setDrawMode(false), setRegionMode(true))
                  }
                >
                  {regionMode ? "Cancelar marcação" : "Marcar região"}
                </Button>
              </div>
              <p className={styles.hintLine}>
                QR e códigos de barras são <strong>decodificados</strong> (o
                conteúdo vai pro rótulo) e tabelas entram como candidatas. Para
                assinatura, carimbo, logo etc., use <em>Marcar região</em> e
                desenhe sobre o documento.
              </p>
              {regionMode && !pendingRegion && (
                <p className={styles.hintLine}>
                  Modo <strong>marcar região</strong>: arraste um retângulo sobre
                  o documento.
                </p>
              )}
              {pendingRegion && (
                <div className={styles.newBlockForm}>
                  <span className={styles.newBlockTitle}>Nova região</span>
                  <select
                    className={styles.input}
                    value={regionType}
                    onChange={(e) => setRegionType(e.target.value)}
                  >
                    {REGION_TYPES.map((t) => (
                      <option key={t.id} value={t.id}>
                        {t.label}
                      </option>
                    ))}
                  </select>
                  <input
                    className={styles.input}
                    value={regionLabel}
                    placeholder="rótulo (opcional)"
                    onChange={(e) => setRegionLabel(e.target.value)}
                    onKeyDown={(e) => {
                      if (e.key === "Enter") {
                        e.preventDefault();
                        void confirmNewRegion();
                      } else if (e.key === "Escape") {
                        cancelNewRegion();
                      }
                    }}
                  />
                  <div className={styles.newBlockActions}>
                    <Button
                      size="sm"
                      variant="secondary"
                      leftIcon={<FileSearch size={14} />}
                      onClick={() => void tryDecodeRegion()}
                      disabled={decodeBusy}
                      title="Tenta decodificar QR/código de barras nesta região"
                    >
                      {decodeBusy ? "Lendo…" : "Ler código"}
                    </Button>
                    <Button
                      size="sm"
                      variant="primary"
                      onClick={() => void confirmNewRegion()}
                    >
                      Adicionar
                    </Button>
                    <Button size="sm" variant="ghost" onClick={cancelNewRegion}>
                      Cancelar
                    </Button>
                  </div>
                </div>
              )}
              <div className={styles.sectionTitle}>
                Blocos por tipo ({blocks.length})
              </div>
              {blocks.length === 0 ? (
                <p className={styles.emptyLine}>Rode o OCR para ver os blocos.</p>
              ) : (
                <div className={styles.kvList}>
                  {Object.entries(
                    blocks.reduce<Record<string, number>>((acc, b) => {
                      acc[b.block_type] = (acc[b.block_type] ?? 0) + 1;
                      return acc;
                    }, {}),
                  ).map(([type, n]) => (
                    <div key={type} className={styles.kv}>
                      <span className={styles.kvK}>{type}</span>
                      <span className={styles.kvV}>{n}</span>
                    </div>
                  ))}
                </div>
              )}
              <div className={styles.sectionTitle}>
                Regiões marcadas ({regions.length})
              </div>
              {regions.length === 0 ? (
                <p className={styles.emptyLine}>
                  Nenhuma região. (Marcação por seleção no documento: próxima
                  fase.)
                </p>
              ) : (
                <div className={styles.fieldList}>
                  {regions.map((r) => (
                    <div key={r.id} className={styles.fieldRow}>
                      <div className={styles.fieldMain}>
                        <span className={styles.fieldName}>
                          {regionTypeLabel(r.region_type)}
                        </span>
                        <span className={styles.fieldVal} title={r.label}>
                          {r.label || "—"}
                        </span>
                      </div>
                      {r.label && (
                        <button
                          className={styles.iconBtn}
                          title="Copiar conteúdo"
                          onClick={() => copy(r.label)}
                        >
                          <Copy size={14} />
                        </button>
                      )}
                      <button
                        className={styles.iconBtn}
                        title="Remover região"
                        onClick={() => {
                          void commands
                            .deleteDocRegion(ws, r.id)
                            .then(() =>
                              setRegions((prev) =>
                                prev.filter((x) => x.id !== r.id),
                              ),
                            );
                        }}
                      >
                        <Trash2 size={14} />
                      </button>
                    </div>
                  ))}
                </div>
              )}
            </>
          )}

          {tab === "realce" && (
            <>
              <p className={styles.hintLine}>
                Pré-processa a imagem para facilitar a leitura — o original{" "}
                <strong>não</strong> é alterado (§13). Marque as operações e
                aplique; depois rode o OCR sobre o resultado.
              </p>
              <div className={styles.realceOps}>
                {REALCE_OPS.map((op) => (
                  <label key={op.id} className={styles.realceOp}>
                    <input
                      type="checkbox"
                      checked={realceOps.includes(op.id)}
                      onChange={() => toggleRealceOp(op.id)}
                    />
                    <span className={styles.realceOpLabel}>{op.label}</span>
                    <span className={styles.realceOpHint}>{op.hint}</span>
                  </label>
                ))}
              </div>
              <div className={styles.rowBtns}>
                <Button
                  size="sm"
                  variant="primary"
                  leftIcon={<Wand2 size={14} />}
                  onClick={() => void applyRealce()}
                  disabled={
                    processedBusy ||
                    realceOps.length === 0 ||
                    (!isImage && !pdfInfo)
                  }
                >
                  {processedBusy ? "Processando…" : "Aplicar realce"}
                </Button>
                {processed && (
                  <Button
                    size="sm"
                    variant="ghost"
                    onClick={() => setProcessed(null)}
                  >
                    Limpar
                  </Button>
                )}
              </div>
              {processed ? (
                <>
                  <div className={styles.sectionTitle}>Resultado</div>
                  <img
                    className={styles.realcePreview}
                    src={processed}
                    alt="prévia do realce"
                  />
                  <div className={styles.rowBtns}>
                    <Button
                      size="sm"
                      variant="secondary"
                      leftIcon={<ScanText size={14} />}
                      onClick={() => void ocrOnProcessed()}
                      disabled={ocrBusy}
                    >
                      {ocrBusy ? "Processando…" : "Rodar OCR neste realce"}
                    </Button>
                  </div>
                </>
              ) : (
                <p className={styles.emptyLine}>
                  Dica: <em>Endireitar</em> + <em>Contraste local</em> +{" "}
                  <em>Binarizar</em> costuma melhorar bastante a leitura de
                  documentos escaneados ou fotografados.
                </p>
              )}
            </>
          )}

          {tab === "forense" && (
            <>
              <p className={styles.hintLine}>
                Indícios de <strong>manipulação digital</strong> sobre a imagem
                da página atual. São <strong>heatmaps indicativos</strong> —
                destacam regiões a examinar, <strong>não</strong> concluem
                adulteração (§13). O original não é alterado.
              </p>

              {!isImage ? (
                <p className={styles.hintLine}>
                  ⚠ PDF: o <strong>ELA</strong> usa o <strong>JPEG embutido</strong>{" "}
                  da página (documento escaneado). Páginas de texto vetorial não
                  têm histórico JPEG — o ELA não se aplica. O mapa de ruído vale
                  para qualquer página.
                </p>
              ) : !/jpe?g/i.test(doc.extension) ? (
                <p className={styles.hintLine}>
                  ⚠ Fonte <strong>{doc.extension.toUpperCase() || "não-JPEG"}</strong>:
                  o ELA é pouco informativo sem histórico de compressão JPEG (a
                  menos que este {doc.extension.toUpperCase()} tenha vindo de um
                  JPEG). O mapa de ruído não tem essa limitação.
                </p>
              ) : null}

              <div className={styles.sectionTitle}>ELA — Error Level Analysis</div>
              <div className={styles.rowBtns}>
                <label className={styles.realceOpHint}>
                  Qualidade{" "}
                  <input
                    className={styles.cfGridSize}
                    type="number"
                    min={50}
                    max={99}
                    value={elaQuality}
                    onChange={(e) =>
                      setElaQuality(
                        Math.min(99, Math.max(50, Number(e.target.value) || 90)),
                      )
                    }
                  />
                </label>
                <label className={styles.realceOpHint}>
                  Ganho{" "}
                  <input
                    className={styles.cfGridSize}
                    type="number"
                    min={1}
                    max={60}
                    value={elaGain}
                    onChange={(e) =>
                      setElaGain(
                        Math.min(60, Math.max(1, Number(e.target.value) || 15)),
                      )
                    }
                  />
                </label>
                <Button
                  size="sm"
                  variant="primary"
                  leftIcon={<Wand2 size={14} />}
                  onClick={() => void runEla()}
                  disabled={forensicBusy || (!isImage && !pdfInfo)}
                >
                  {forensicBusy ? "Processando…" : "Rodar ELA"}
                </Button>
              </div>
              <p className={styles.emptyLine}>
                Recomprime a imagem e amplifica o erro de recompressão. Regiões
                inseridas/editadas tendem a destoar. Falso-positivo comum: bordas
                e alto contraste acendem naturalmente.
              </p>

              <div className={styles.sectionTitle}>Mapa de ruído</div>
              <div className={styles.rowBtns}>
                <label className={styles.realceOpHint}>
                  Janela{" "}
                  <input
                    className={styles.cfGridSize}
                    type="number"
                    min={1}
                    max={16}
                    value={noiseWindow}
                    onChange={(e) =>
                      setNoiseWindow(
                        Math.min(16, Math.max(1, Number(e.target.value) || 4)),
                      )
                    }
                  />
                </label>
                <Button
                  size="sm"
                  variant="primary"
                  leftIcon={<Wand2 size={14} />}
                  onClick={() => void runNoise()}
                  disabled={forensicBusy || (!isImage && !pdfInfo)}
                >
                  {forensicBusy ? "Processando…" : "Mapa de ruído"}
                </Button>
              </div>
              <p className={styles.emptyLine}>
                Energia local de alta frequência (claro = mais ruído/textura).
                Saltos bruscos entre regiões podem sugerir composição — mas variam
                naturalmente com luz, foco e compressão.
              </p>

              <div className={styles.sectionTitle}>Copy-move (clonagem)</div>
              <div className={styles.rowBtns}>
                <label className={styles.realceOpHint}>
                  Bloco{" "}
                  <input
                    className={styles.cfGridSize}
                    type="number"
                    min={8}
                    max={64}
                    step={4}
                    value={copyBlock}
                    onChange={(e) =>
                      setCopyBlock(
                        Math.min(64, Math.max(8, Number(e.target.value) || 16)),
                      )
                    }
                  />
                </label>
                <Button
                  size="sm"
                  variant="primary"
                  leftIcon={<Wand2 size={14} />}
                  onClick={() => void runCopyMove()}
                  disabled={forensicBusy || (!isImage && !pdfInfo)}
                >
                  {forensicBusy ? "Processando…" : "Detectar clonagem"}
                </Button>
              </div>
              <p className={styles.emptyLine}>
                Procura regiões duplicadas na mesma imagem (carimbo/assinatura
                clonada, área usada para cobrir algo). Fonte e cópia recebem a
                mesma cor. Falso-positivo: tramas/grades repetitivas. Vale para
                qualquer formato (não depende de JPEG).
              </p>

              {forensicSrc ? (
                <>
                  <div className={styles.sectionTitle}>
                    {forensicKind === "ela"
                      ? "ELA"
                      : forensicKind === "noise"
                        ? "Mapa de ruído"
                        : "Copy-move"}{" "}
                    — indício (requer exame humano)
                  </div>
                  <img
                    className={styles.realcePreview}
                    src={forensicSrc}
                    alt="heatmap de indício"
                  />
                  <div className={styles.rowBtns}>
                    <Button
                      size="sm"
                      variant="secondary"
                      leftIcon={<FolderOpen size={14} />}
                      onClick={() => void saveIndicio(true)}
                    >
                      Exportar
                    </Button>
                    <Button
                      size="sm"
                      variant="secondary"
                      leftIcon={<FileOutput size={14} />}
                      onClick={() => void saveIndicio(false)}
                    >
                      Enviar ao laudo
                    </Button>
                    <Button
                      size="sm"
                      variant="ghost"
                      onClick={() => {
                        setForensicSrc(null);
                        setForensicKind(null);
                        setIndicioMsg(null);
                      }}
                    >
                      Limpar
                    </Button>
                  </div>
                  {indicioMsg && <p className={styles.hintLine}>{indicioMsg}</p>}
                </>
              ) : (
                <p className={styles.emptyLine}>
                  Rode uma análise acima. O mapa é triagem visual; a leitura e a
                  conclusão são do perito.
                </p>
              )}
            </>
          )}

          {tab === "integridade" && (
            <div className={styles.kvList}>
              <div className={styles.kv}>
                <span className={styles.kvK}>SHA-256</span>
                <span className={`${styles.kvV} ${styles.mono}`}>
                  {doc.sha256}
                </span>
              </div>
              <div className={styles.kv}>
                <span className={styles.kvK}>Tipo</span>
                <span className={styles.kvV}>
                  {doc.file_type} (.{doc.extension})
                </span>
              </div>
              <div className={styles.kv}>
                <span className={styles.kvK}>Tamanho</span>
                <span className={styles.kvV}>{prettyBytes(doc.size_bytes)}</span>
              </div>
              <div className={styles.kv}>
                <span className={styles.kvK}>Páginas</span>
                <span className={styles.kvV}>{doc.page_count || "—"}</span>
              </div>
              <div className={styles.kv}>
                <span className={styles.kvK}>Camada de texto (PDF)</span>
                <span className={styles.kvV}>
                  {doc.has_text_layer ? "sim" : "não detectada"}
                </span>
              </div>
              <div className={styles.kv}>
                <span className={styles.kvK}>Original</span>
                <span className={styles.kvV}>preservado (cópia + hash)</span>
              </div>
              <p className={styles.methodNote}>
                O arquivo original não é alterado. Toda operação ocorre sobre
                cópias/derivados e fica registrada no Histórico.
              </p>
            </div>
          )}

          {tab === "metadados" && (
            <>
              {metaEntries.length === 0 ? (
                <p className={styles.emptyLine}>
                  Sem metadados estruturados.{" "}
                  {doc.file_type === "pdf"
                    ? "Este PDF não traz metadados embutidos (autor/produtor/datas) — comum em digitalizações."
                    : ""}
                </p>
              ) : (
                <div className={styles.kvList}>
                  {metaEntries.map(([k, v]) => (
                    <div key={k} className={styles.kv}>
                      <span className={styles.kvK}>{k}</span>
                      <span className={styles.kvV}>{v}</span>
                    </div>
                  ))}
                </div>
              )}
            </>
          )}

          {tab === "historico" && (
            <>
              {logs.length === 0 ? (
                <p className={styles.emptyLine}>Sem eventos registrados.</p>
              ) : (
                <div className={styles.logList}>
                  {logs.map((l) => (
                    <div key={l.id} className={styles.logItem}>
                      <span className={styles.logAction}>{l.action}</span>
                      <span className={styles.logResult}>{l.result}</span>
                      <span className={styles.logDate}>{fmtDate(l.created_at)}</span>
                    </div>
                  ))}
                </div>
              )}
            </>
          )}

        </div>

        <div className={styles.panelFoot}>
          <button className={styles.dangerLink} onClick={onDeleted}>
            <Trash2 size={13} /> Remover documento
          </button>
          <span className={styles.assistTag}>
            <Columns2 size={12} /> Ferramenta de apoio — conclusão do perito
          </span>
        </div>
      </aside>
    </>
  );
}
