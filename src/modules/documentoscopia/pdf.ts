/**
 * Helper de PDF (pdf.js) para a Documentoscopia — Fase 2.
 *
 * Resolve as três necessidades do OCR sobre PDF, de forma offline (pdf.js é
 * 100 % JS, roda no WebView, sem rede):
 *
 *  1. **Inspeção** (`getPdfInfo`) — nº de páginas + se há *camada de texto*
 *     embutida (PDF digital) ou não (PDF escaneado/imagem).
 *  2. **Extração de texto** (`extractPdfText`) — para PDF **digital**, pega o
 *     texto real embutido, **sem OCR** (instantâneo e fiel ao original).
 *  3. **Rasterização** (`renderPdfPageToPngBase64`) — para PDF **escaneado**,
 *     renderiza a página em PNG (base64) para o RapidOCR ler no backend.
 *
 * §13 (suporte forense): nada aqui interpreta o conteúdo; só converte o
 * documento para um formato que o motor de OCR consegue ler. A origem
 * (texto embutido vs. OCR de imagem) fica registrada no `engine` da execução,
 * para o perito saber exatamente como o texto foi obtido.
 *
 * O worker do pdf.js é empacotado pelo Vite via `?url` (string pequena em
 * build); a biblioteca pesada é carregada sob demanda (`import()` dinâmico),
 * para não inflar o bundle inicial.
 */

import workerUrl from "pdfjs-dist/build/pdf.worker.min.mjs?url";

/**
 * pdf.js v6 decodifica imagens JPEG2000 (OpenJPEG) e JBIG2 via WASM. Os
 * binários são copiados de `pdfjs-dist/wasm` para `/pdfjs-wasm` pelo
 * vite.config. O pdf.js concatena `wasmUrl + nome-do-arquivo`, então a barra
 * final é obrigatória. Sem isso, PDFs escaneados (JPX/JBIG2) não rasterizam
 * (a página fica em branco), embora a extração de texto continue funcionando.
 */
const PDFJS_WASM_URL = `${import.meta.env.BASE_URL}pdfjs-wasm/`;

type PdfjsModule = typeof import("pdfjs-dist");

let pdfjsPromise: Promise<PdfjsModule> | null = null;

async function getPdfjs(): Promise<PdfjsModule> {
  if (!pdfjsPromise) {
    pdfjsPromise = import("pdfjs-dist").then((mod) => {
      mod.GlobalWorkerOptions.workerSrc = workerUrl;
      return mod;
    });
  }
  return pdfjsPromise;
}

/** Carrega o documento a partir de uma URL (ex.: `convertFileSrc(path)`). */
async function loadDocument(url: string) {
  const pdfjs = await getPdfjs();
  const res = await fetch(url);
  if (!res.ok) {
    throw new Error(`não foi possível ler o arquivo PDF (HTTP ${res.status})`);
  }
  const buf = await res.arrayBuffer();
  return pdfjs.getDocument({ data: new Uint8Array(buf), wasmUrl: PDFJS_WASM_URL })
    .promise;
}

export interface PdfInfo {
  pageCount: number;
  /** Tem texto embutido (PDF digital) → dá pra extrair sem OCR. */
  hasTextLayer: boolean;
  title?: string;
  author?: string;
  producer?: string;
}

/** Item de texto do pdf.js que nos interessa (os demais campos são ignorados). */
interface TextItemLike {
  str?: string;
  hasEOL?: boolean;
}

/** Reconstrói texto a partir dos itens, respeitando quebras de linha (`hasEOL`). */
function itemsToText(items: ReadonlyArray<unknown>): string {
  let out = "";
  for (const raw of items) {
    const it = raw as TextItemLike;
    if (typeof it.str === "string") out += it.str;
    if (it.hasEOL) out += "\n";
  }
  return out.replace(/[ \t]+\n/g, "\n").replace(/\n{3,}/g, "\n\n").trim();
}

/**
 * Inspeciona o PDF: nº de páginas, metadados e se há camada de texto.
 * A detecção de texto sonda as 2 primeiras páginas (heurística barata).
 */
export async function getPdfInfo(url: string): Promise<PdfInfo> {
  const pdf = await loadDocument(url);
  try {
    const pageCount = pdf.numPages;
    const probe = Math.min(2, pageCount);
    let textChars = 0;
    for (let i = 1; i <= probe; i++) {
      const page = await pdf.getPage(i);
      const tc = await page.getTextContent();
      textChars += itemsToText(tc.items).replace(/\s+/g, "").length;
      page.cleanup();
    }
    // Limiar conservador: ~20 caracteres "úteis" por página sondada.
    const hasTextLayer = textChars >= 20 * probe;

    let title: string | undefined;
    let author: string | undefined;
    let producer: string | undefined;
    try {
      const meta = await pdf.getMetadata();
      const info = meta.info as Record<string, unknown> | undefined;
      const pick = (v: unknown) =>
        typeof v === "string" && v.trim() ? v.trim() : undefined;
      if (info) {
        title = pick(info.Title);
        author = pick(info.Author);
        producer = pick(info.Producer);
      }
    } catch {
      /* metadados ausentes não são erro */
    }

    return { pageCount, hasTextLayer, title, author, producer };
  } finally {
    await pdf.loadingTask.destroy();
  }
}

export interface PdfPageText {
  page: number;
  text: string;
}

/** Extrai o texto embutido de TODAS as páginas (PDF digital). */
export async function extractPdfText(url: string): Promise<PdfPageText[]> {
  const pdf = await loadDocument(url);
  try {
    const out: PdfPageText[] = [];
    for (let i = 1; i <= pdf.numPages; i++) {
      const page = await pdf.getPage(i);
      const tc = await page.getTextContent();
      out.push({ page: i, text: itemsToText(tc.items) });
      page.cleanup();
    }
    return out;
  } finally {
    await pdf.loadingTask.destroy();
  }
}

/**
 * Renderiza uma página do PDF e devolve a **data URL** PNG completa — para
 * exibir num `<img>` no visualizador. `scale` ~1.5–2 basta para tela (o zoom
 * por CSS amplia depois sem re-renderizar).
 */
export async function renderPdfPageToDataUrl(
  url: string,
  pageNumber: number,
  scale = 1.8,
): Promise<string> {
  const pdf = await loadDocument(url);
  try {
    const page = await pdf.getPage(pageNumber);
    const viewport = page.getViewport({ scale });
    const canvas = document.createElement("canvas");
    canvas.width = Math.ceil(viewport.width);
    canvas.height = Math.ceil(viewport.height);
    const ctx = canvas.getContext("2d");
    if (!ctx) throw new Error("canvas 2D indisponível neste ambiente");
    await page.render({ canvas, canvasContext: ctx, viewport }).promise;
    const dataUrl = canvas.toDataURL("image/png");
    page.cleanup();
    return dataUrl;
  } finally {
    await pdf.loadingTask.destroy();
  }
}

/**
 * Renderiza uma página em PNG base64 (sem o prefixo data:) para o OCR.
 * `scale` ~2.5 dá boa resolução para o RapidOCR sem estourar memória.
 */
export async function renderPdfPageToPngBase64(
  url: string,
  pageNumber: number,
  scale = 2.5,
): Promise<string> {
  const dataUrl = await renderPdfPageToDataUrl(url, pageNumber, scale);
  return dataUrl.split(",")[1] ?? "";
}
