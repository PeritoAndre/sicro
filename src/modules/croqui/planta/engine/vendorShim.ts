/**
 * Shims das dependências do arcada que o SICRO não traz: @mantine/notifications
 * (feedback é do editor) e react-device-detect (desktop, nunca mobile).
 */

export const isMobile = false;

/** No-op: o feedback é do PlantaEditor. */
export function showNotification(_opts?: unknown): void {
  // intencionalmente vazio
}
