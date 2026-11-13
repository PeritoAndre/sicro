/** Traços e textos em px da folha, mas legíveis em qualquer folha: até 1000 px é 1; uma foto de drone de 5472 px, 5,5. */

const FOLHA_BASE_PX = 1000;

export function fatorDaFolha(canvas: { width_px: number; height_px: number }): number {
  const lado = Math.max(canvas.width_px, canvas.height_px);
  if (!Number.isFinite(lado) || lado <= 0) return 1;
  return Math.min(10, Math.max(1, lado / FOLHA_BASE_PX));
}
