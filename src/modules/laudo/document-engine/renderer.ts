/**
 * `.sicrodoc` → HTML renderer.
 *
 * Uses TipTap's `generateHTML` with the same extension list as the editor,
 * then walks the resulting markup to apply pericial concerns that don't
 * belong to a single node:
 *   - figure numbering ("Figura 1", "Figura 2", "Croqui 1", ...);
 *   - figcaption slot replacement;
 *   - storyboard description slot replacement.
 *
 * For MVP 2, the renderer also wraps the document in the institutional chrome
 * (header / footer / side-mark / page numbering) defined by
 * `layout.institutional_template`. Chrome is configuration, NOT editable
 * content — it lives in the layout, never in `content`.
 *
 * The output is plain HTML suitable for `<iframe srcdoc>` previews or for
 * Edge `--print-to-pdf`.
 */

import { generateHTML } from "@tiptap/html";
import type { JSONContent } from "@tiptap/core";
import { laudoExtensions } from "./extensions";
// N10 — Cabeçalho dinâmico usa as MESMAS extensões reduzidas do editor
// interativo do header (instância TipTap dedicada em useHeaderEditor).
import { headerExtensions } from "./header-extensions";
import type { SicroDoc, SicroDocPageMargins } from "./schema";
import { numberFigures } from "./numbering";
import {
  inlineEvidenceAssets,
  type EvidenceAssetMap,
} from "./evidence-assets";
import {
  findInstitutionalTemplate,
  type InstitutionalTemplate,
} from "./institutional-templates";
// N — `resolveHeaderField` REMOVIDO do renderer: era usado apenas dentro do
// antigo `renderHeader()` que injetava a banda institucional hardcoded.
// O cabeçalho agora é dinâmico, lido de `doc.header.content` (N10).
import { resolveEffectiveMargins } from "./page-layout";
import type { BrandingAssets } from "./branding";
// Campos automáticos `{campo}` — resolvidos para o VALOR real na exportação
// (PDF/HTML), usando occurrence + metadata (com override local do laudo).
import { resolveFieldValue, type FieldResolveContext } from "./fields";
import { extractOutline, numberOutline } from "./sections";

const BASE_DOC_STYLES = `
  body { margin: 0; font-family: Arial, "Helvetica Neue", Helvetica, sans-serif; color: #111; }

  h1 { font-size: 18pt; text-align: center; }
  h2 { font-size: 14pt; }
  h3 { font-size: 12pt; }
  /* Entrelinha SIMPLES (≈ single do Word) + sem espaço entre parágrafos, pra o
   * PDF/HTML (Edge fallback) bater com o editor e com o DOCX/PDF-LibreOffice. */
  p  { font-size: 12pt; text-align: justify; line-height: 1.15; margin: 0; }
  /* Linha em branco (Enter sem texto) reserva UMA linha — igual ao editor e ao
   * Word. Sem isto, o <p></p> vazio colapsava a zero e a paginação encolhia. */
  p:empty { min-height: 1.15em; }
  ul, ol { padding-left: 1.5em; }

  figure[data-sicro-figure] {
    margin: 1.5em auto; text-align: center; page-break-inside: avoid;
  }
  figure[data-sicro-figure] img {
    max-width: 100%; height: auto; border: 1px solid #d8d8d8;
  }
  figure[data-sicro-figure] figcaption {
    font-size: 10pt; color: #444; margin-top: 0.4em; font-style: italic;
  }
  table[data-sicro-table] {
    border-collapse: collapse; width: 100%; margin: 1em 0; font-size: 11pt;
    /* F1.2 — resize de coluna serializa via <colgroup>; fixed respeita-o. */
    table-layout: fixed;
  }
  table[data-sicro-table] th,
  table[data-sicro-table] td {
    border: var(--sicro-table-border-width, 1px) solid
      var(--sicro-table-border-color, #444);
    padding: var(--sicro-table-cell-padding, 4px) 8px;
    vertical-align: top; box-sizing: border-box;
    /* Palavra maior que a coluna quebra DENTRO da célula (espelha o
       editor/clone) — sem isto vazava pra fora da tabela no PDF. */
    overflow-wrap: break-word; word-break: break-word;
  }
  /* Cabecalho (th): SEM negrito do navegador nem fundo cinza automatico —
     espelha styles.css. O peso da fonte e decisao do perito (marca bold
     explicita); o sombreado de celula e opcional e explicito (attr
     backgroundColor -> style inline emitido pelo renderHTML do cell). */
  table[data-sicro-table] th { font-weight: inherit; text-align: left; }
  /* F4 — Apresentação serializada via data-* (cor/espessura/padding/align/
     bordas) + legenda <caption> abaixo da tabela. Espelha styles.css. */
  table[data-border-style="none"] td,
  table[data-border-style="none"] th { border: none; }
  table[data-border-style="none"] {
    border: 1px solid var(--sicro-table-border-color, #444);
  }
  table[data-table-align="center"] { margin-left: auto; margin-right: auto; }
  table[data-table-align="right"] { margin-left: auto; margin-right: 0; }
  table[data-sicro-table] > caption {
    caption-side: bottom; font-size: 10pt; font-style: italic;
    color: #333; text-align: left; padding-top: 0.2em;
  }
  /* V2 — bloco de registro do cabeçalho: só o retângulo externo (sem
     divisórias internas), igual à tela. */
  table[data-sicro-header-table] {
    border: 1px solid #444; border-collapse: collapse;
    width: 100%; margin: 1em 0; font-size: 11pt;
  }
  table[data-sicro-header-table] th,
  table[data-sicro-header-table] td {
    border: none; padding: 4px 8px; vertical-align: top;
  }
  section[data-sicro-storyboard] {
    border: 1px dashed #c0c0c0; padding: 0.5em; margin: 1em 0;
    page-break-inside: avoid;
  }
  article[data-sicro-storyboard-item] {
    display: grid; grid-template-columns: 200px 1fr; gap: 12px;
    align-items: start; padding: 8px 0;
    border-bottom: 1px solid #eee;
    page-break-inside: avoid;
  }
  article[data-sicro-storyboard-item]:last-child { border-bottom: none; }
  article[data-sicro-storyboard-item] img {
    width: 100%; border: 1px solid #d8d8d8;
  }
  span[data-sicro-system-data] {
    background: rgba(230, 180, 80, 0.12);
    border-bottom: 1px dotted #b07c20;
    padding: 0 2px;
  }
  span[data-sicro-system-data][data-review-status="reviewed"] {
    background: rgba(53, 196, 122, 0.10);
    border-bottom-color: #1f7a44;
  }
  span[data-sicro-system-data][data-review-status="converted"] {
    background: transparent;
    border-bottom: none;
  }

  /* ---- MVP 2: institutional blocks ---- */
  section[data-sicro-quesito-list] {
    margin: 1em 0;
  }
  article[data-sicro-quesito-item] {
    margin: 0.8em 0 1em;
    page-break-inside: avoid;
  }
  div[data-sicro-quesito-question] {
    font-weight: 700;
    margin-bottom: 0.2em;
  }
  div[data-sicro-quesito-question]::before {
    content: "Quesito " counter(quesito) ": ";
    counter-increment: quesito;
  }
  section[data-sicro-quesito-list] {
    counter-reset: quesito;
  }
  div[data-sicro-quesito-answer]::before {
    content: "Resposta: ";
    font-weight: 700;
  }
  div[data-sicro-quesito-answer] {
    margin-left: 1em;
    text-align: justify;
  }

  section[data-sicro-signature] {
    margin: 2em 0 1em;
    text-align: center;
    page-break-inside: avoid;
  }
  section[data-sicro-signature] [data-sicro-signature-place] {
    margin-bottom: 2em;
    text-align: right;
  }
  section[data-sicro-signature] [data-sicro-signature-rule] {
    letter-spacing: 1px;
  }
  section[data-sicro-signature] [data-sicro-signature-name] {
    font-weight: 700;
    margin-top: 0.2em;
  }
  section[data-sicro-signature] [data-sicro-signature-role] {
    font-style: italic;
    font-size: 11pt;
  }

  /* ---- N10 — Cabecalho DINAMICO Word-style ----
   * header.sicro-doc-page-header empacota o conteudo de doc.header.content
   * serializado via generateHTML(headerExtensions()). A faixa fica colada
   * no topo da pagina (dentro da area de margem superior) e usa cascata
   * de tipografia identica ao body. Em PDF (Edge headless) a regra
   * @page running() abaixo repete esse mesmo header em todas as paginas
   * automaticamente — sem precisar replicar o markup. */
  header.sicro-doc-page-header {
    position: running(sicroHeader);
    width: 100%;
    height: var(--sicro-header-height, 2.5cm);
    /* Pós-laudo U — overflow visível pra TextBoxes/Figures dentro do
     * cabeçalho poderem extender pra fora da altura nominal (caso típico:
     * texto vertical lateral + logo no canto da página, ambos parte do
     * cabeçalho replicado). Mesmo do editor (.region overflow:visible). */
    overflow: visible;
    border-bottom: 0.5pt solid #999;
    padding: 0.1cm 0 0.15cm;
    font-family: Arial, "Helvetica Neue", Helvetica, sans-serif;
    font-size: 11pt;
    color: #111;
  }
  header.sicro-doc-page-header p {
    margin: 0 0 0.15em;
    line-height: 1.3;
    /* N18 — Parágrafos vazios (enters extras pra empurrar verticalmente
     * o conteúdo) precisam reservar altura. Sem isso, o PDF colapsa o
     * <p></p> a zero e o layout some. */
    min-height: 1.3em;
  }
  header.sicro-doc-page-header img.sicro-header-image,
  header.sicro-doc-page-header img {
    max-height: 90%;
    object-fit: contain;
    vertical-align: middle;
  }
  @page {
    @top-center {
      content: element(sicroHeader);
    }
  }

  /* ---- W (fase 2b) — Rodapé DINAMICO Word-style ----
   * Simétrico ao header: footer.sicro-doc-page-footer empacota
   * doc.footer.content (ex.: brasão da Polícia Científica importado do
   * .docx) serializado via generateHTML(headerExtensions()). Em motores
   * paged-media (Prince/WeasyPrint) a regra @page running() abaixo repete
   * em todas as páginas; no Edge/Chromium headless o position:running()
   * é ignorado e o rodapé fica no fluxo, ancorado ao fim do corpo. */
  footer.sicro-doc-page-footer {
    position: running(sicroFooter);
    width: 100%;
    min-height: var(--sicro-footer-height, 2cm);
    /* Igual ao header: overflow visível pra brasão/TextBox poderem
     * extender além da altura nominal. */
    overflow: visible;
    border-top: 0.5pt solid #999;
    padding: 0.15cm 0 0.1cm;
    margin-top: 1.5em;
    font-family: Arial, "Helvetica Neue", Helvetica, sans-serif;
    font-size: 11pt;
    color: #111;
    /* O brasão importado normalmente vem centralizado. */
    text-align: center;
  }
  footer.sicro-doc-page-footer p {
    margin: 0 0 0.15em;
    line-height: 1.3;
    min-height: 1.3em;
  }
  footer.sicro-doc-page-footer img.sicro-header-image,
  footer.sicro-doc-page-footer img {
    max-height: 95%;
    object-fit: contain;
    vertical-align: middle;
  }
  @page {
    @bottom-center {
      content: element(sicroFooter);
    }
  }

  footer.sicro-doc-footer {
    border-top: 1px solid #aaa;
    padding-top: 0.4em;
    margin-top: 2em;
    font-size: 9pt;
    color: #444;
    display: flex;
    justify-content: space-between;
    align-items: baseline;
  }

  aside.sicro-doc-side-mark {
    position: fixed;
    top: 0; bottom: 0; left: 0;
    width: 1.5cm;
    display: flex;
    align-items: center;
    justify-content: center;
    color: #888;
    font-size: 8pt;
    letter-spacing: 4px;
    pointer-events: none;
    z-index: 0;
  }
  aside.sicro-doc-side-mark .sicro-side-mark-text {
    transform: rotate(-90deg);
    white-space: nowrap;
    text-transform: uppercase;
  }

  /* Fórmula matemática — exibida via PNG renderizado (sicro-math-img). */
  .sicro-math-block {
    text-align: center;
    margin: 0.4em 0;
  }
  .sicro-math-inline {
    display: inline;
  }
  .sicro-math-img {
    max-width: 100%;
  }
  .sicro-math-inline .sicro-math-img {
    vertical-align: middle;
  }

`;

function pageStyles(
  template: InstitutionalTemplate,
  margins: SicroDocPageMargins,
  /** Quando o documento usa os campos {page}/{pages} mas o template não tem
   *  numeração de rodapé, injeta "Folha X de Y" na MARGEM @page (único lugar
   *  onde counter(page) funciona no PDF — no corpo ele resolve pra 0). */
  injectPageNumber = false,
): string {
  // F12.7 — Print/PDF polido:
  //   - @page com header/footer institucional em TODAS as páginas
  //     (não só na primeira, via @top-center e @bottom-*).
  //   - "Página N de M" no rodapé direito.
  //   - Title institucional no topo central (legível, fonte serifa).
  //   - first/named pages: ":first" tira o counter da página 1 quando
  //     não desejado (mantemos sempre — auditável).
  //   - widows/orphans = 3 para evitar linhas órfãs.
  const showFooter = template.footer.show_page_numbers;
  const brandTop = template.header.brand_lines[0] ?? "";
  // Documento em branco: sem brand line → NÃO emite @top-center (senão
  // o `content: ""` ainda desenharia a borda inferior, deixando uma
  // linha fantasma no topo das páginas 2+).
  const topCenter = brandTop
    ? `@top-center {
        content: "${escapeCssString(brandTop)}";
        font-family: Arial, "Helvetica Neue", Helvetica, sans-serif;
        font-size: 9pt;
        color: #555;
        padding-bottom: 0.2cm;
        border-bottom: 0.4pt solid #aaa;
        margin-bottom: 0.4cm;
      }`
    : "";
  return `
    @page {
      size: ${template.page.size} ${template.page.orientation};
      margin: ${margins.top} ${margins.right} ${margins.bottom} ${margins.left};

      ${topCenter}
      ${
        showFooter
          ? `@bottom-right {
               content: "Página " counter(page) " de " counter(pages);
               font-family: Arial, "Helvetica Neue", Helvetica, sans-serif;
               font-size: 9pt;
               color: #444;
             }
             @bottom-left {
               content: "${escapeCssString(template.footer.text)}";
               font-family: Arial, "Helvetica Neue", Helvetica, sans-serif;
               font-size: 8.5pt;
               color: #555;
             }`
          : ""
      }
      ${
        injectPageNumber && !showFooter
          ? `@bottom-center {
               content: "Folha " counter(page) " de " counter(pages);
               font-family: Arial, "Helvetica Neue", Helvetica, sans-serif;
               font-size: 9pt;
               color: #444;
             }`
          : ""
      }
    }

    /* F12.7 — Primeira página suprime o @top-center
       (o header completo institucional cobre o topo). */
    @page :first {
      @top-center {
        content: none;
      }
    }

    @media screen {
      body {
        padding: ${margins.top} ${margins.right} ${margins.bottom} ${margins.left};
        max-width: 21cm;
        margin: 0 auto;
        background: #fff;
      }
    }

    /* F12.7 — Controles de quebra para print/PDF. */
    @media print {
      body {
        background: #fff !important;
        color: #000 !important;
      }
      p, li {
        widows: 3;
        orphans: 3;
      }
      h1, h2, h3 {
        page-break-after: avoid;
      }
      h1 + p, h2 + p, h3 + p {
        page-break-before: avoid;
      }
      figure, table, section[data-sicro-quesito-item],
      section[data-sicro-signature], article[data-sicro-storyboard-item] {
        page-break-inside: avoid;
      }
      /* Header institucional só na primeira página visual
         (as outras usam @top-center via @page). */
      .sicro-screen-only,
      footer.sicro-doc-footer .sicro-screen-page {
        display: none !important;
      }
      /* Campos {{page}} / {{pages}}: durante a impressão, o ::after pega o
         counter(page) / counter(pages) do CSS Paged Media — equivale a
         { PAGE } e { NUMPAGES } do Word. Cada página exporta o valor certo. */
      .sicro-page-counter::after {
        content: attr(data-fallback);
        font: inherit;
        color: inherit;
      }
      .sicro-page-counter--current::after {
        content: counter(page);
      }
      .sicro-page-counter--total::after {
        content: counter(pages);
      }
    }

    /* Em tela (editor/preview HTML), os campos {{page}}/{{pages}} mostram
       um placeholder visual (— / —) — não há "página corrente" fora do PDF. */
    @media screen {
      .sicro-page-counter {
        display: inline-block;
        min-width: 1.5em;
        text-align: center;
        background: rgba(224, 163, 58, 0.12);
        border: 1px dashed rgba(224, 163, 58, 0.5);
        border-radius: 3px;
        padding: 0 4px;
        font-size: 0.9em;
        color: #b86f08;
      }
      .sicro-page-counter::after {
        content: attr(data-fallback);
      }
    }
  `;
}

/**
 * Expande o marcador do Sumário dinâmico (`<div data-dynamic-summary></div>`,
 * emitido vazio pelo renderHTML do nó) na lista REAL de títulos numerados.
 * No PDF do navegador isso garante o índice (sem números de página — limitação
 * do motor); no DOCX/PDF-LibreOffice o walker emite um TOC nativo com páginas.
 */
function resolveDynamicSummaryInHtml(
  html: string,
  content: JSONContent,
): string {
  if (!html.includes("data-dynamic-summary")) return html;
  const outline = numberOutline(extractOutline(content));
  const items = outline
    .map(
      (o) =>
        `<li class="sicro-dynamic-list-item sicro-dynamic-list-item--level-${o.level}">` +
        `${escapeHtml(`${o.numero ? o.numero + " " : ""}${o.text}`)}</li>`,
    )
    .join("");
  const body = outline.length
    ? `<ul class="sicro-dynamic-list-items">${items}</ul>`
    : `<div class="sicro-dynamic-list-placeholder">Nenhum título encontrado.</div>`;
  const replacement =
    `<div class="sicro-dynamic-list sicro-dynamic-summary">` +
    `<h2 class="sicro-dynamic-list-title">SUMÁRIO</h2>${body}</div>`;
  return html.replace(
    /<div\b[^>]*\bdata-dynamic-summary\b[^>]*>\s*<\/div>/gi,
    replacement,
  );
}

/**
 * Substitui as pílulas de campo (`<span data-field="KEY">{KEY}</span>`) pelo
 * VALOR resolvido (occurrence + metadata + override local). Sem esta etapa o
 * PDF/HTML sairia com os literais `{numero_laudo}` etc. (a resolução dinâmica
 * só acontecia no editor, via NodeView). `page`/`pages` viram vazio aqui — a
 * numeração real é injetada na margem @page (counter(page) só funciona lá).
 * Campo conhecido mas sem valor permanece como `{KEY}` — sinaliza pendência.
 */
function resolveDataFieldsInHtml(
  html: string,
  ctx: FieldResolveContext,
): string {
  if (!html) return html;
  return html.replace(
    /<span\b[^>]*\bdata-field=["']([^"']+)["'][^>]*>[\s\S]*?<\/span>/gi,
    (_match, key: string) => {
      // page/pages: a numeração real vai pra margem @page (counter(page) só
      // funciona lá). No corpo, resolvemos pra vazio — nada de "0 0".
      if (key === "page" || key === "pages") return "";
      const value = (resolveFieldValue(key, ctx) ?? "").trim();
      return value ? escapeHtml(value) : `{${escapeHtml(key)}}`;
    },
  );
}

/** Escapa string para uso seguro dentro de `content: "..."` em CSS. */
function escapeCssString(s: string): string {
  return s.replace(/\\/g, "\\\\").replace(/"/g, '\\"');
}

export interface RenderOptions {
  /** Wrap the output in a full <html> document with print-ready styles. */
  fullDocument?: boolean;
  /** Inject figure numbering. Default true. */
  numbering?: boolean;
  /** Occurrence values to fill the institutional header. When omitted, the
   *  header still renders but with empty values (collapsed). */
  occurrence?: Record<string, unknown> | null;
  /** Pre-loaded data-URIs for the institutional coats of arms. Required for
   *  the PDF pipeline (Edge headless reads the HTML from a temp file and
   *  cannot resolve `/branding/...` paths). Optional — without it the
   *  header renders without images. */
  branding?: BrandingAssets | null;
  /** MVP 4: pre-loaded evidence asset bytes (relative_path → data URI).
   *  When provided, figures/storyboard items whose `relative_path` matches
   *  have their `src` replaced before TipTap renders. */
  evidenceAssets?: EvidenceAssetMap | null;
}

export function renderSicroDocToHtml(
  doc: SicroDoc,
  options: RenderOptions = {},
): string {
  const numbering = options.numbering ?? true;
  const fullDocument = options.fullDocument ?? false;

  let content: JSONContent = numbering
    ? numberFigures(doc.content)
    : (doc.content as JSONContent);
  if (options.evidenceAssets) {
    // Inline data URIs so the HTML/PDF pipeline doesn't need
    // workspace-local file access.
    content = inlineEvidenceAssets(content, options.evidenceAssets);
  }
  // Contexto de resolução dos campos {campo}: occurrence + metadata (com
  // override local do laudo). Vale também no modo !fullDocument (preview HTML).
  const fieldCtx: FieldResolveContext = {
    metadata: (doc.metadata ?? {}) as Record<string, unknown>,
    occurrence: options.occurrence ?? null,
  };

  const innerHtml = resolveDynamicSummaryInHtml(
    resolveDataFieldsInHtml(generateHTML(content, laudoExtensions()), fieldCtx),
    content,
  );

  if (!fullDocument) {
    return innerHtml;
  }

  const template = findInstitutionalTemplate(
    doc.layout?.institutional_template,
  );
  const occurrence = options.occurrence ?? null;
  const metadata = (doc.metadata ?? {}) as Record<string, unknown>;
  const branding = options.branding ?? null;
  // Effective margins = doc.layout.page.margins (per-laudo override) →
  // template.page.margins → SICRO defaults. Same resolver used by the editor
  // so the on-screen sheet, PDF, and HTML preview agree pixel-for-pixel.
  const margins = resolveEffectiveMargins(doc, template);

  // page/pages NÃO resolvem inline no corpo do PDF (counter(page) só vale na
  // margem @page — no corpo dá 0). Se o documento usa esses campos, injetamos
  // "Folha X de Y" na margem inferior (onde funciona). Detecção textual no doc.
  const usesPageCounter = /"field"\s*:\s*"(?:page|pages)"/.test(
    JSON.stringify([doc.content, doc.header ?? null, doc.footer ?? null]),
  );

  // N10 — Cabeçalho DINÂMICO: serializa `doc.header.content` (ProseMirror
  // JSON) usando `headerExtensions()` — mesma máquina que o editor usa.
  // Só emite quando `doc.header.enabled === true`. Os params legados
  // (template/metadata/occurrence/branding) continuam consumidos pra
  // não quebrar a assinatura pública de `renderSicroDocToHtml` enquanto
  // a migração N12 não promove o conteúdo do `institutional_template`
  // pra dentro de `doc.header.content`.
  void template;
  void metadata;
  void occurrence;
  void branding;
  // O brasão/figuras do cabeçalho e rodapé também precisam ter seus
  // `relative_path` inlinados como data URI (igual ao body), senão o
  // <iframe srcdoc> / Edge headless não consegue carregar tauri://… e a
  // imagem some no PDF. Passamos o asset map pros renderizadores.
  const headerHtml = resolveDataFieldsInHtml(
    renderDynamicHeader(doc, options.evidenceAssets ?? null),
    fieldCtx,
  );
  const footerHtml = renderFooter(template);
  // W (fase 2b) — Rodapé DINÂMICO (brasão PC + textos do Word). Separado
  // do `footerHtml` institucional (sentença + nº de página): este carrega
  // o conteúdo real de `doc.footer.content`. Os campos {{page}}/{{pages}} que
  // o perito inserir aqui são substituídos pelos counters do CSS (que viram
  // o número correto de CADA página na hora do export PDF).
  const dynamicFooterHtml = resolveDataFieldsInHtml(
    renderDynamicFooter(doc, options.evidenceAssets ?? null),
    fieldCtx,
  );
  const sideMarkHtml = renderSideMark(template);

  return `<!doctype html>
<html lang="pt-BR">
  <head>
    <meta charset="utf-8" />
    <title>${escapeHtml(doc.title)}</title>
    <style>${pageStyles(template, margins, usesPageCounter)}${BASE_DOC_STYLES}</style>
  </head>
  <body>
    ${sideMarkHtml}
    ${headerHtml}
    <main class="sicro-doc-body">${innerHtml}</main>
    ${footerHtml}
    ${dynamicFooterHtml}
  </body>
</html>`;
}

/**
 * N10 — Cabeçalho dinâmico para export HTML/PDF.
 *
 * Lê `doc.header.content` (ProseMirror JSON) e serializa via
 * `generateHTML(headerExtensions())` — exatamente a mesma cadeia que o
 * editor interativo usa, então o WYSIWYG é fiel.
 *
 * Para PDF (Edge headless) este HTML será REPETIDO em todas as páginas
 * via `@page { @top-center { content: element(sicroHeader) } }` —
 * regra CSS está no `pageStyles` (vide renderer.ts:pageStyles). Para
 * HTML preview (uma única página contínua), o header aparece uma vez
 * no topo do `<body>`.
 *
 * Retorna string vazia quando `header.enabled === false` ou conteúdo
 * vazio — não polui o output.
 */
function renderDynamicHeader(
  doc: SicroDoc,
  evidenceAssets?: EvidenceAssetMap | null,
): string {
  const header = doc.header;
  if (!header || !header.enabled) return "";
  const heightCm =
    doc.layout?.header_height_cm ??
    /* fallback ao default da schema se ausente */ 2.5;

  // Heuristic: emite a faixa SOMENTE quando há conteúdo "real" no header
  // (texto não-vazio OU pelo menos uma imagem). Empty stubs (single
  // empty paragraph, parágrafos só com whitespace) NÃO geram faixa pra
  // evitar uma banda em branco no PDF. Checagem é feita na árvore
  // ProseMirror direto — mais robusto que comparar strings HTML.
  if (!hasMeaningfulHeaderContent(header.content)) return "";

  // Inline data URIs (brasão importado etc.) quando o caller pré-carregou
  // os assets — mesmo tratamento do body em `renderSicroDocToHtml`.
  const content = evidenceAssets
    ? inlineEvidenceAssets(header.content, evidenceAssets)
    : header.content;

  let inner = "";
  try {
    inner = generateHTML(content, headerExtensions());
  } catch {
    inner = "";
  }
  if (!inner.trim()) return "";
  return `<header class="sicro-doc-page-header" style="--sicro-header-height:${heightCm.toFixed(2)}cm">${inner}</header>`;
}

/**
 * W (fase 2b) — Rodapé dinâmico para export HTML/PDF.
 *
 * Espelho exato do `renderDynamicHeader`: lê `doc.footer.content`
 * (ProseMirror JSON) e serializa via `generateHTML(headerExtensions())`
 * — o rodapé reaproveita o mesmo subset de extensões do cabeçalho
 * (vide `useFooterEditor`), então o WYSIWYG é fiel ao editor.
 *
 * Caso típico: o brasão da Polícia Científica que o importador de .docx
 * traz da seção `<w:ftr>` do Word. Em paged-media repete via
 * `@page { @bottom-center { content: element(sicroFooter) } }`; no Edge
 * headless fica ancorado ao fim do corpo (uma vez).
 *
 * Retorna string vazia quando `footer.enabled === false` ou conteúdo
 * vazio — não polui o output (mesma heurística do header).
 */
function renderDynamicFooter(
  doc: SicroDoc,
  evidenceAssets?: EvidenceAssetMap | null,
): string {
  const footer = doc.footer;
  if (!footer || !footer.enabled) return "";
  const heightCm =
    doc.layout?.footer_height_cm ??
    /* fallback ao default da schema se ausente */ 2;

  if (!hasMeaningfulHeaderContent(footer.content)) return "";

  const content = evidenceAssets
    ? inlineEvidenceAssets(footer.content, evidenceAssets)
    : footer.content;

  let inner = "";
  try {
    inner = generateHTML(content, headerExtensions());
  } catch {
    inner = "";
  }
  if (!inner.trim()) return "";
  return `<footer class="sicro-doc-page-footer" style="--sicro-footer-height:${heightCm.toFixed(2)}cm">${inner}</footer>`;
}

/** True se a árvore ProseMirror contém pelo menos um text node não-vazio
 *  ou um nó visual (image, figure, textBox). Aceita qualquer profundidade.
 *
 *  W (fase 2b) — `figure` e `textBox` ENTRAM aqui: o brasão importado do
 *  .docx vira um nó `figure` (não `image`), então sem isto um cabeçalho /
 *  rodapé contendo SÓ o brasão era tratado como "vazio" e não era emitido
 *  no PDF/HTML — o brasão sumia no export. */
function hasMeaningfulHeaderContent(node: JSONContent | undefined): boolean {
  if (!node) return false;
  if (node.type === "text") {
    return typeof node.text === "string" && node.text.trim().length > 0;
  }
  if (node.type === "image" || node.type === "figure" || node.type === "textBox")
    return true;
  const children = node.content;
  if (!Array.isArray(children)) return false;
  return children.some((c) => hasMeaningfulHeaderContent(c));
}

// N — `renderHeader()` (versão antiga, institucional hardcoded) REMOVIDO em N3.
//   - lia `template.header.brand_lines`, `subtitle`, `metadata_fields`
//   - chamava `resolveHeaderField()` para preencher campos dinâmicos
//   - emitia `<img class="sicro-brand-img-pca/estado">` dos `branding` assets
//   - retornava `<header class="sicro-doc-header">…</header>` hardcoded
// Em N10 será reintroduzida como `renderDynamicHeader(header, layout)` que
// serializa `header.content` (ProseMirror) via `generateHTML(laudoExtensions)`
// — mesmo pipeline do body, com altura controlada por `layout.header_height_cm`.

function renderFooter(template: InstitutionalTemplate): string {
  // Documento em branco / template sem rodapé institucional: não emite
  // rodapé nenhum (nem "Folha 1"). Só renderiza quando há texto de rodapé
  // OU numeração de página configurada no template.
  if (!template.footer.text && !template.footer.show_page_numbers) {
    return "";
  }
  // The "Folha X de Y" is provided by `@page @bottom-right` for the PDF.
  // The HTML footer carries only the institutional sentence + a screen-only
  // "Folha 1" placeholder so the on-screen render doesn't look broken.
  return `<footer class="sicro-doc-footer">
    <span>${escapeHtml(template.footer.text)}</span>
    <span class="sicro-screen-page">Folha 1</span>
  </footer>`;
}

function renderSideMark(template: InstitutionalTemplate): string {
  // N18 — Side mark vertical desativado. Antes emitia o `<aside>` com o
  // texto "POLÍCIA CIENTÍFICA DO ESTADO DO AMAPÁ" pela esquerda. A info
  // institucional agora vive no cabeçalho dinâmico (`doc.header.content`),
  // editado pelo próprio usuário. A função é mantida pra evitar
  // refatoração maior no call site mas retorna string vazia.
  void template;
  return "";
}

function escapeHtml(s: string): string {
  return s
    .replace(/&/g, "&amp;")
    .replace(/</g, "&lt;")
    .replace(/>/g, "&gt;")
    .replace(/"/g, "&quot;");
}
