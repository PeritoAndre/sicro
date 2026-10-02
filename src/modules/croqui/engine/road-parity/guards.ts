/** Type guards dos objetos parity (via / rotatória), por `kind`. */

import type { SicroObject } from "../schema";
import { PARITY_ENGINE_TAG } from "./types";
import type {
  SicroParityObject,
  SicroRoadObject_parity,
  SicroRoundaboutObject_parity,
} from "./types";

export function isParityRoad(
  obj: SicroObject | { engine?: unknown; kind?: unknown },
): obj is SicroRoadObject_parity {
  return (
    typeof obj === "object" &&
    obj !== null &&
    (obj as { kind?: unknown }).kind === "road_parity"
  );
}

export function isParityRoundabout(
  obj: SicroObject | { engine?: unknown; kind?: unknown },
): obj is SicroRoundaboutObject_parity {
  return (
    typeof obj === "object" &&
    obj !== null &&
    (obj as { kind?: unknown }).kind === "roundabout_parity"
  );
}

export function isParityObject(
  obj: SicroObject | { engine?: unknown; kind?: unknown },
): obj is SicroParityObject {
  return isParityRoad(obj) || isParityRoundabout(obj);
}

// Evita warning de import não usado; a tag fica disponível para checagem futura.
void PARITY_ENGINE_TAG;
