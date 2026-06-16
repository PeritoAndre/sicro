/**
 * Helpers neutros de caminho de arquivo (sem dependência do editor de laudo).
 *
 * Viviam em `modules/laudo/document-engine/relative-src.ts` quando o editor
 * in-app existia; com a aposentadoria do editor (SICRO 3.0) `joinWorkspace`
 * continua sendo usado pelo módulo Imagem (round-trip de fotos), então foi
 * movido para cá — uma casa neutra em `core/`.
 */

/**
 * Junta o caminho absoluto do workspace com um caminho relativo de asset,
 * normalizando o separador para o do host.
 *
 * Cuidado importante: em Windows, misturar `\` e `/` num path absoluto quebra
 * o asset protocol do Tauri (`convertFileSrc` aceita mas a resolução do <img>
 * falha). Esta função normaliza tudo pro separador do host.
 */
export function joinWorkspace(workspacePath: string, rel: string): string {
  const isWin = workspacePath.includes("\\");
  const sep = isWin ? "\\" : "/";
  const trimmed = workspacePath.replace(/[\\/]+$/, "");
  const normRel = isWin ? rel.replace(/\//g, "\\") : rel.replace(/\\/g, "/");
  return `${trimmed}${sep}${normRel}`;
}
