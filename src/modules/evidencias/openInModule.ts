/**
 * Abre uma prova no módulo de origem com o item carregado. Derivados (export)
 * apontam para o pai via `original_id`; item que só existe como arquivo → null.
 */
import type { NavigateFunction } from "react-router-dom";

import { useCroquiStore } from "@modules/croqui/store/croquiStore";
import { useImagemStore } from "@modules/imagem/store/imagemStore";
import { useVideoStore } from "@modules/video/store/videoStore";
import type { EvidenceRegistryItem } from "@domain/evidence_registry";

interface ModuleTarget {
  route: string;
  moduleLabel: string;
}

export function moduleTargetFor(item: EvidenceRegistryItem): ModuleTarget | null {
  switch (item.kind) {
    case "video":
      return { route: "/video", moduleLabel: "Vídeo" };
    case "croqui":
    case "croqui_export":
      return { route: "/croqui", moduleLabel: "Croqui" };
    case "image_analysis":
    case "image_export":
      return { route: "/imagem", moduleLabel: "Imagem" };
    case "audio":
      return { route: "/audio", moduleLabel: "Áudio" };
    default:
      // photo, storyboard_frame, laudo/documento (módulos que saíram no 4.0),
      // laudo_export, imported_package, other: só o arquivo
      return null;
  }
}

/** Id de origem a partir do id sintético "<kind>:<id>". */
function sourceId(item: EvidenceRegistryItem): string {
  const i = item.id.indexOf(":");
  return i >= 0 ? item.id.slice(i + 1) : item.id;
}

/** Lança se o carregamento falhar; o chamador mostra o feedback. */
export async function openInModule(
  item: EvidenceRegistryItem,
  workspacePath: string,
  navigate: NavigateFunction,
): Promise<void> {
  switch (item.kind) {
    case "video":
      await useVideoStore.getState().openMedia(workspacePath, sourceId(item));
      navigate("/video");
      return;
    case "croqui":
      await useCroquiStore.getState().openCroqui(workspacePath, sourceId(item));
      navigate("/croqui");
      return;
    case "croqui_export":
      if (item.original_id) {
        await useCroquiStore
          .getState()
          .openCroqui(workspacePath, item.original_id);
      }
      navigate("/croqui");
      return;
    case "image_analysis":
      await useImagemStore
        .getState()
        .openAnalysis(workspacePath, sourceId(item));
      navigate("/imagem");
      return;
    case "image_export":
      if (item.original_id) {
        await useImagemStore
          .getState()
          .openAnalysis(workspacePath, item.original_id);
      }
      navigate("/imagem");
      return;
    case "audio":
      // O módulo Áudio abre pela lista; sem ação de "abrir item" dedicada.
      navigate("/audio");
      return;
    default:
      return;
  }
}
