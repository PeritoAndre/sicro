/**
 * Python Parity Engine — public API.
 *
 * Tudo que o resto do app deve importar de `road-parity/` passa por
 * este barrel. Os arquivos internos (`types.ts`, `factories.ts`,
 * `guards.ts`) são detalhe de implementação.
 *
 * **NÃO contém renderer ainda** — Fase H.1 só cria os tipos. O
 * renderer (`RoadRendererParity.tsx`) entra na Fase H.2.
 */

export * from "./types";
export * from "./guards";
export * from "./factories";
export * from "./geometry";
export * from "./clipping";
export * from "./osmAdapter";
export { RoadParityRenderer, type RoadParityRendererProps } from "./renderer";
