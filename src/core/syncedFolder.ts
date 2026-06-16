/**
 * Detecção (heurística) de pastas sincronizadas com a nuvem — OneDrive,
 * Dropbox, Google Drive. Um workspace `.sicro` "vivo" dentro de uma dessas
 * pastas é arriscado: o serviço de sync mexe no SQLite/arquivos no meio das
 * escritas e pode corromper. A redundância em nuvem deve ser feita por BACKUP
 * (`.sicrobackup` estático), não sincronizando os dados vivos.
 *
 * É só heurística por caminho (offline, sem rede) — boa o suficiente para
 * AVISAR o perito; nunca bloqueia.
 */

/** Serviços de sync reconhecidos pelo nome no caminho. */
const SYNC_PATTERNS: ReadonlyArray<{ service: string; re: RegExp }> = [
  { service: "OneDrive", re: /(^|[\\/])onedrive([\s._-][^\\/]*)?([\\/]|$)/i },
  { service: "Dropbox", re: /(^|[\\/])dropbox([\\/]|$)/i },
  {
    service: "Google Drive",
    re: /(^|[\\/])(google ?drive|my drive|meu drive)([\\/]|$)/i,
  },
];

/**
 * Retorna o nome do serviço de sync se o caminho parecer estar dentro de uma
 * pasta sincronizada; senão `null`.
 */
export function detectSyncedFolder(path: string | null | undefined): string | null {
  if (!path) return null;
  for (const { service, re } of SYNC_PATTERNS) {
    if (re.test(path)) return service;
  }
  return null;
}
