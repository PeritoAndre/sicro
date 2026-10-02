// @ts-nocheck -- ponte do motor vendido (arcada). Vide planta/ATTRIBUTION.md.
/**
 * No arcada, `main` era exportado pelo componente React EditorRoot e módulos do
 * motor o importam pra converter coordenadas. No SICRO, mount.ts chama
 * `setMain`; fica só esta ponte.
 */
import type { Main } from "./editor/Main";

export let main: Main;

export function setMain(m: Main): void {
  main = m;
}
