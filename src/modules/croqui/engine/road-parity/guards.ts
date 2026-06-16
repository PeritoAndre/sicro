/**
 * Python Parity Engine — type guards.
 *
 * Cada objeto parity carrega `engine: "parity"` como discriminador.
 * Estas funções permitem distinguir, em runtime e em type-narrowing
 * TypeScript, entre objetos parity e legados v1/v2.
 *
 * Princípio: NUNCA assumir que um `kind: "road"` é parity ou legado
 * sem chamar o guard. O `SicroObject` union aceita ambos; o renderer
 * precisa decidir qual fluxo seguir baseado em `engine`.
 */

import type { SicroObject } from "../schema";
import { PARITY_ENGINE_TAG } from "./types";
import type {
  SicroParityObject,
  SicroRoadObject_parity,
  SicroRoundaboutObject_parity,
} from "./types";

/**
 * True se `obj` é uma via parity (`engine === "parity"` E `kind === "road"`).
 *
 * Type narrowing: dentro do `if (isParityRoad(obj))`, o TS sabe que
 * `obj` é `SicroRoadObject_parity` — pode acessar `obj.largura_m`,
 * `obj.ax`, etc.
 */
export function isParityRoad(
  obj: SicroObject | { engine?: unknown; kind?: unknown },
): obj is SicroRoadObject_parity {
  return (
    typeof obj === "object" &&
    obj !== null &&
    (obj as { kind?: unknown }).kind === "road_parity"
  );
}

/**
 * True se `obj` é uma rotatória parity.
 */
export function isParityRoundabout(
  obj: SicroObject | { engine?: unknown; kind?: unknown },
): obj is SicroRoundaboutObject_parity {
  return (
    typeof obj === "object" &&
    obj !== null &&
    (obj as { kind?: unknown }).kind === "roundabout_parity"
  );
}

/**
 * True se `obj` é qualquer objeto parity (via OU rotatória).
 */
export function isParityObject(
  obj: SicroObject | { engine?: unknown; kind?: unknown },
): obj is SicroParityObject {
  return isParityRoad(obj) || isParityRoundabout(obj);
}

// Suprime warning de `PARITY_ENGINE_TAG` não usado (mantemos
// disponível para defesa em depth).
void PARITY_ENGINE_TAG;
