// @ts-nocheck -- mexe nos singletons estáticos do motor vendido (arcada).
/**
 * O motor do arcada usa singletons estáticos (FloorPlan, TransformLayer,
 * AddWallManager) que sobrevivem entre mount/unmount do React. No arcada (SPA)
 * isso era ok; no SICRO o PlantaEditor monta/desmonta. Sem reset, reabrir um
 * croqui de planta reaproveitaria instâncias com display objects já destruídos.
 * `disposePlantaEngine()` zera os singletons pra um próximo mount começar limpo.
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
