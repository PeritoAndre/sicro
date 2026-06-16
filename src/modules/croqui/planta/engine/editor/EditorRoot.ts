// @ts-nocheck -- ponte do motor vendido (arcada). Vide planta/ATTRIBUTION.md.
/**
 * No arcada, `EditorRoot.tsx` (componente React) exportava `export let main: Main`
 * e o atribuía ao montar; vários módulos do motor (Floor, ViewportCoordinates)
 * importam esse `main` pra converter coordenadas. No SICRO a montagem é feita por
 * `mount.ts`, que chama `setMain(...)`. Mantemos só essa ponte (sem React/Mantine).
 */
import type { Main } from "./editor/Main";

export let main: Main;

export function setMain(m: Main): void {
  main = m;
}
