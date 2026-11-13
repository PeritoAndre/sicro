/** Traços e textos em px da folha, mas com o mesmo tamanho no papel: a folha padrão (1600 px) é 1; uma foto de drone de 5472 px, 3,4. */

const FOLHA_BASE_PX = 1600;

export function fatorDaFolha(canvas: { width_px: number; height_px: number }): number {
  const lado = Math.max(canvas.width_px, canvas.height_px);
  if (!Number.isFinite(lado) || lado <= 0) return 1;
  return Math.min(8, Math.max(1, lado / FOLHA_BASE_PX));
}
