// @ts-nocheck -- mexe nos singletons estáticos do motor vendido (arcada).
/**
 * Os singletons estáticos do arcada (FloorPlan, TransformLayer, AddWallManager)
 * sobrevivem ao unmount do React; sem este reset, reabrir um croqui
 * reaproveitaria instâncias com display objects já destruídos.
 */
import { FloorPlan } from "./editor/editor/objects/FloorPlan";
import { TransformLayer } from "./editor/editor/objects/TransformControls/TransformLayer";
import { AddWallManager } from "./editor/editor/actions/AddWallManager";
import { disposeEvidenceLayer } from "./evidenceLayer";

export function disposePlantaEngine(): void {
  try {
    disposeEvidenceLayer();
  } catch {
    /* noop */
  }
  try {
    FloorPlan.instance = undefined;
  } catch {
    /* noop */
  }
  try {
    TransformLayer.instance = undefined;
  } catch {
    /* noop */
  }
  try {
    AddWallManager.instance = undefined;
  } catch {
    /* noop */
  }
}
