/**
 * Helpers puros do "quadro técnico" da Documentoscopia — o resumo curto
 * (texto) que o perito cola no laudo. Linguagem indiciária (§13): apoio
 * técnico-computacional; a conclusão é sempre humana.
 */
import {
  docTypeLabel,
  type DetectedField,
  type DocumentCaseFile,
  type OcrRun,
} from "@domain/documentoscopia";

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

/**
 * Monta o quadro técnico curto (proveniência + campos revisados + nota §13)
 * a partir do documento, dos campos extraídos e da última execução de OCR.
 */
export function buildTechnicalSummary(
  doc: DocumentCaseFile,
  fields: DetectedField[],
  latestRun: OcrRun | null | undefined,
): string {
  const lines = [
    `Documento: ${doc.title}`,
    `Arquivo: ${doc.original_filename}`,
    `Tipo: ${docTypeLabel(doc.doc_type)}`,
    `Hash SHA-256: ${doc.sha256}`,
    `Tamanho: ${prettyBytes(doc.size_bytes)}`,
    `Importado em: ${fmtDate(doc.created_at)}`,
    `Motor de OCR: ${
      latestRun ? `${latestRun.engine} ${latestRun.engine_version}`.trim() : "—"
    }`,
    "",
    "Campos extraídos (revisados pelo perito):",
    ...(fields.length
      ? fields.map(
          (f) => `  - ${f.field_name}: ${f.corrected_value || f.field_value}`,
        )
      : ["  (nenhum campo registrado)"]),
    "",
    "Observação: os resultados de OCR, extração de campos e detecção de layout",
    "constituem APOIO técnico-computacional. A interpretação dos achados e a",
    "conclusão documentoscópica dependem de avaliação humana pelo perito.",
  ];
  return lines.join("\n");
}
