/**
 * laudoExport — helper compartilhado para exportar o laudo em PDF/HTML/DOCX.
 *
 * K — Centraliza a lógica que estava espalhada entre o `ExportMenu` e
 * outros pontos (tipo o botão SIGDOC do toolbar, ou o passo 1 da
 * assinatura SIGDOCS). Sempre que possível, abre o Explorer na pasta
 * do arquivo gerado.
 *
 * Não inclui UI/toast — quem chama é responsável por feedback. Aqui
 * só roda o pipeline.
 */

import { commands } from "@core/commands";
import {
  collectEvidencePaths,
  LAUDO_FIELDS,
  loadBrandingAssets,
  loadEvidenceAssets,
  normalizeEvidenceSrcsForSave,
  renderSicroDocToHtml,
  resolveAllFields,
  resolvePageNumber,
  type SicroDoc,
  type SicroDocPageNumber,
} from "../document-engine";
import type { Export } from "@domain/export";

/**
 * Resolve TODOS os campos automáticos (`{numero_laudo}` etc.) para os valores
 * reais — usando metadata (override local) + occurrence (fonte do caso) +
 * catálogo. É o MESMO resolver do editor/preview, então o .docx/PDF-Word sai
 * idêntico ao que o perito vê na tela. Campos vazios (incl. page/pages, que só
 * o Word numera) ficam de fora do mapa → o backend mantém a pílula/campo
 * nativo. Passado ao backend nos exports DOCX e PDF-LibreOffice.
 */
function resolveFieldValuesForExport(
  doc: SicroDoc,
  occurrence: Record<string, unknown> | null,
): Record<string, string> {
  const resolved = resolveAllFields(
    {
      metadata: (doc.metadata ?? {}) as Record<string, unknown>,
      occurrence,
    },
    LAUDO_FIELDS,
  );
  const out: Record<string, string> = {};
  for (const [key, value] of resolved) {
    const v = value.trim();
    if (v) out[key] = v;
  }
  return out;
}

export type ExportTarget = "pdf" | "html" | "docx" | "pdf_lo" | "pdf_a";

export interface ExportResult {
  export: Export;
  /** Caminho absoluto resolvido (sempre presente, ainda que o backend
   *  só conheça `relative_path`). Pode ser usado no `revealPathInExplorer`. */
  absolute_path: string;
}

/**
 * Exporta o laudo para o formato pedido. Quando `revealAfter=true`
 * (default), abre o Explorer apontando pro arquivo gerado.
 */
export async function exportLaudo(
  target: ExportTarget,
  workspacePath: string,
  laudoId: string,
  doc: SicroDoc,
  occurrence: Record<string, unknown> | null,
  options: { revealAfter?: boolean } = {},
): Promise<ExportResult> {
  const revealAfter = options.revealAfter ?? true;

  // O alvo "pdf" = MELHOR PDF DISPONÍVEL: prefere LibreOffice (diagramação fiel
  // ao Word, paginação consistente) quando instalado; senão cai pro Edge (sem
  // dependência), pra fluxos automáticos (prep do SIGDOC, botão Exportar PDF do
  // editor) nunca quebrarem. O menu manual de exportação já pede LibreOffice
  // explícito (pdf_lo) — esta política só vale pro alvo genérico "pdf".
  let effectiveTarget: ExportTarget = target;
  if (target === "pdf") {
    try {
      const lo = await commands.getLibreofficeStatus();
      if (lo.installed) effectiveTarget = "pdf_lo";
    } catch {
      /* status indisponível → mantém Edge */
    }
  }

  let result: Export;
  if (effectiveTarget === "docx") {
    result = await commands.exportLaudoDocx(
      workspacePath,
      laudoId,
      resolveFieldValuesForExport(doc, occurrence),
    );
  } else if (effectiveTarget === "pdf_lo" || effectiveTarget === "pdf_a") {
    // PDF via LibreOffice: o backend gera o .docx e converte headless. Não
    // precisa do render HTML rico — a diagramação vem do LibreOffice.
    // `pdf_a` = mesmo pipeline, porém em PDF/A (arquivamento ISO 19005).
    result = await commands.exportLaudoPdfLibreoffice(
      workspacePath,
      laudoId,
      effectiveTarget === "pdf_a",
      resolveFieldValuesForExport(doc, occurrence),
    );
  } else {
    const branding = await loadBrandingAssets();
    const portableContent = normalizeEvidenceSrcsForSave(doc.content);
    const evidencePaths = collectEvidencePaths(portableContent);
    // W (fase 2b) — o brasão do CABEÇALHO e do RODAPÉ (importados do .docx)
    // também precisam virar data URI no export, senão somem no PDF/HTML
    // (o renderer inlina header/footer só se os assets foram pré-carregados).
    if (doc.header?.content) {
      const headerPortable = normalizeEvidenceSrcsForSave(doc.header.content);
      for (const p of collectEvidencePaths(headerPortable)) evidencePaths.add(p);
    }
    if (doc.footer?.content) {
      const footerPortable = normalizeEvidenceSrcsForSave(doc.footer.content);
      for (const p of collectEvidencePaths(footerPortable)) evidencePaths.add(p);
    }
    const evidenceAssets =
      evidencePaths.size > 0
        ? await loadEvidenceAssets(workspacePath, evidencePaths)
        : null;
    const html = renderSicroDocToHtml(
      { ...doc, content: portableContent },
      {
        fullDocument: true,
        occurrence,
        branding,
        evidenceAssets,
      },
    );
    // Numeração de página no PDF: counter(page) só funciona na margem @page,
    // que o `--print-to-pdf` (CLI) ignora. Se o laudo usa {page}/{pages},
    // mandamos o template "Folha X de Y" — o backend imprime via CDP
    // (Page.printToPDF, no CABEÇALHO) os placeholders pageNumber/totalPages do
    // Chromium (contador nativo, por página). Formato/fonte/tamanho/cor/
    // alinhamento vêm da config do laudo (Inspector › Página).
    const usesPageCounter = /"field"\s*:\s*"(?:page|pages)"/.test(
      JSON.stringify([doc.content, doc.header ?? null, doc.footer ?? null]),
    );
    const pageFooter =
      effectiveTarget === "pdf" && usesPageCounter
        ? buildPageNumberTemplate(resolvePageNumber(doc))
        : null;
    result =
      effectiveTarget === "pdf"
        ? await commands.exportLaudoPdf(workspacePath, laudoId, html, pageFooter)
        : await commands.exportLaudoHtml(workspacePath, laudoId, html);
  }

  // K — Caminho absoluto = workspace + relative_path. Backend retorna
  // só relative; juntamos aqui pra `revealPathInExplorer`.
  const absolute = joinPaths(workspacePath, result.relative_path);

  if (revealAfter) {
    try {
      await commands.revealPathInExplorer(absolute);
    } catch {
      /* non-fatal */
    }
  }

  return { export: result, absolute_path: absolute };
}

/**
 * Monta o `headerTemplate` do Chromium a partir da config de numeração. Os
 * tokens `{n}`/`{total}` viram os spans NATIVOS `pageNumber`/`totalPages` (o
 * Chromium preenche por página). O texto livre do usuário é escapado — só os
 * spans são HTML. Fonte/tamanho(pt)/cor/alinhamento vêm da config.
 */
function buildPageNumberTemplate(cfg: SicroDocPageNumber): string {
  const esc = (s: string): string =>
    s
      .replace(/&/g, "&amp;")
      .replace(/</g, "&lt;")
      .replace(/>/g, "&gt;")
      .replace(/"/g, "&quot;");
  const body = esc(cfg.format)
    .replace(/\{n\}/g, '<span class="pageNumber"></span>')
    .replace(/\{total\}/g, '<span class="totalPages"></span>');
  // Remove aspas/escape/; da família pra não quebrar o atributo style.
  const fam = cfg.font_family.replace(/['"\\;]/g, "");
  return (
    `<div style="width:100%;box-sizing:border-box;padding:0 1.5cm;` +
    `font-family:'${fam}',serif;font-size:${cfg.size_pt}pt;color:${cfg.color};` +
    `text-align:${cfg.align};">${body}</div>`
  );
}

/**
 * Junta workspace + relative path com o separador certo (\ no Windows,
 * / em macOS/Linux). Detecta plataforma via presença de \ no workspace.
 */
function joinPaths(workspace: string, relative: string): string {
  const sep = workspace.includes("\\") ? "\\" : "/";
  const ws = workspace.replace(/[\\/]+$/, "");
  const rel = relative.replace(/^[\\/]+/, "").replace(/\//g, sep);
  return `${ws}${sep}${rel}`;
}
