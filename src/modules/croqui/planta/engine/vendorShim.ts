/**
 * Shims para dependências do arcada que NÃO trazemos pro SICRO.
 *
 * O motor (fork Apache-2.0, vide ATTRIBUTION.md) usava @mantine/notifications,
 * file-saver e react-device-detect. No SICRO:
 *  - notificações → nosso próprio feedback (no editor), aqui é no-op;
 *  - file-saver → persistência é via Tauri (.sicroplanta), aqui é no-op;
 *  - react-device-detect → app desktop, isMobile é sempre false.
 */

/** App desktop (Tauri) — nunca mobile. */
export const isMobile = false;

/** No-op: o SICRO usa seu próprio sistema de feedback no PlantaEditor. */
export function showNotification(_opts?: unknown): void {
  // intencionalmente vazio
}

/** No-op: salvar/baixar é responsabilidade do PlantaEditor (Tauri). */
export function saveAs(_blob: Blob, _filename?: string): void {
  // intencionalmente vazio
}
