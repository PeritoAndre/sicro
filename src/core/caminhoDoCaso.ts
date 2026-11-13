/**
 * Caminhos relativos gravados em casos antigos (layout 1) → layout atual
 * (Exportações / Mídias / Sistema). Espelha `PREFIXOS_ANTIGOS` de
 * src-tauri/src/workspace/paths.rs; mantenha as duas tabelas iguais.
 */

const PREFIXOS_ANTIGOS: ReadonlyArray<readonly [string, string]> = [
  ["croquis/exports", "Exportações/Croquis"],
  ["croquis", "Sistema/croquis"],
  ["videos/trechos", "Exportações/Vídeos/Trechos"],
  ["videos/originais", "Mídias/Vídeos"],
  ["videos/media", "Mídias/Vídeos/media"],
  ["videos", "Sistema/videos"],
  ["imagens/originais", "Mídias/Imagens"],
  ["imagens/tratadas", "Exportações/Imagens/tratadas"],
  ["imagens/anotadas", "Exportações/Imagens/anotadas"],
  ["imagens/exports", "Exportações/Imagens"],
  ["imagens/relatorios", "Exportações/Imagens/relatorios"],
  ["imagens", "Sistema/imagens"],
  ["midias/anexos", "Mídias/Anexos"],
  ["midias", "Mídias"],
  ["media/photos", "Mídias/Fotos"],
  ["audio/originais", "Mídias/Áudios"],
  ["audio", "Sistema/audio"],
  ["exports", "Exportações/Laudos"],
  ["reports", "Exportações/Relatórios"],
  ["backups", "Exportações/Backups"],
  ["imports", "Sistema/imports"],
  ["cache", "Sistema/cache"],
  ["logs", "Sistema/logs"],
  ["dossie", "Sistema/dossie"],
  ["laudos", "Sistema/laudos"],
];

/** Relativo (com `/`) no layout atual; o que já está novo volta igual. */
export function relativoNovo(rel: string): string {
  const r = rel.replace(/\\/g, "/").replace(/^\.\//, "");
  const mont = /^videos\/montagens\/(.+)$/.exec(r);
  const arq = mont?.[1];
  if (arq) {
    const json = arq.endsWith(".json") && !arq.endsWith(".relatorio.json");
    return `${json ? "Sistema/videos/montagens" : "Exportações/Vídeos/Montagens"}/${arq}`;
  }
  for (const [antigo, novo] of PREFIXOS_ANTIGOS) {
    if (r === antigo) return novo;
    if (r.startsWith(`${antigo}/`)) return `${novo}${r.slice(antigo.length)}`;
  }
  return r;
}

/** Caminho absoluto de um relativo gravado (novo ou antigo), com o separador do SO. */
export function caminhoDoCaso(workspacePath: string, rel: string): string {
  const sep = workspacePath.includes("\\") ? "\\" : "/";
  const raiz = workspacePath.replace(/[\\/]+$/, "");
  const r = relativoNovo(rel).replace(/^\/+/, "");
  return `${raiz}${sep}${sep === "\\" ? r.replace(/\//g, "\\") : r}`;
}
