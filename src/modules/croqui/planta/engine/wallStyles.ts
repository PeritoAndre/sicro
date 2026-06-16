// @ts-nocheck -- ponte SICRO ↔ motor arcada. Estilo ("skin") por parede.
/**
 * Mapa de ESTILO por parede, fora do serializer do arcada (que não guarda
 * atributo por-parede). A chave é o par de IDs de nós (ordenado), estável entre
 * save/load. O `Wall.drawLine` lê este mapa pra decidir a aparência: parede
 * normal, muro, cerca de madeira/arame ou calçada. O SICRO popula/persiste via
 * `SicroPlantaDoc.wallStyles`.
 */
export const wallStyleMap = new Map<string, string>();

/**
 * Offset (em coords LOCAIS da parede: x ao longo, y perpendicular) do rótulo de
 * cota arrastado pelo perito. Sobrevive ao redraw (Wall.drawLine lê daqui) e é
 * persistido em SicroPlantaDoc.labelOffsets. Chave = par de nós (wallStyleKey).
 */
export const labelOffsetMap = new Map<string, { x: number; y: number }>();

/** Chave canônica (par de nós ordenado) para um segmento de parede. */
export function wallStyleKey(a: number, b: number): string {
  return a < b ? `${a}-${b}` : `${b}-${a}`;
}
