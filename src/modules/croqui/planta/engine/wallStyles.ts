// @ts-nocheck -- ponte SICRO ↔ motor arcada.
/**
 * Estilo ("skin") por parede, fora do serializer do arcada (que não guarda
 * atributo por parede). Chave = par de IDs de nós ordenado, estável entre
 * save/load; Wall.drawLine lê o mapa. Persistido em SicroPlantaDoc.wallStyles.
 */
export const wallStyleMap = new Map<string, string>();

/** Offset do rótulo de cota arrastado (coords locais da parede: x ao longo,
 *  y perpendicular); Wall.drawLine lê daqui. Persistido em labelOffsets. */
export const labelOffsetMap = new Map<string, { x: number; y: number }>();

/** Chave canônica (par de nós ordenado) para um segmento de parede. */
export function wallStyleKey(a: number, b: number): string {
  return a < b ? `${a}-${b}` : `${b}-${a}`;
}
