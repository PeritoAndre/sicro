/**
 * openInModule — abre uma prova da Central de Provas no seu módulo de origem,
 * com o item carregado. Seta o store do módulo (mesma ação que a lista do
 * módulo usa) e navega para a rota. Itens derivados (export) apontam para o
 * "pai" via `original_id`. Itens que vivem só como arquivo (foto, pacote,
 * export de laudo, frame) não têm módulo "casa" → `moduleTargetFor` = null.
 */
import type { NavigateFunction } from "react-router-dom";

import { useCroquiStore } from "@modules/croqui/store/croquiStore";
import { useImagemStore } from "@modules/imagem/store/imagemStore";
import { useLaudoStore } from "@modules/laudo/store/laudoStore";
import { useVideoStore } from "@modules/video/store/videoStore";
import { useDocumentsStore } from "@stores/documentsStore";
import type { EvidenceRegistryItem } from "@domain/evidence_registry";

export interface ModuleTarget {
  route: string;
  moduleLabel: string;
}

/** Módulo "casa" do item, ou null se ele só existe como arquivo. */
export function moduleTargetFor(item: EvidenceRegistryItem): ModuleTarget | null {
  switch (item.kind) {
    case "video":
      return { route: "/video", moduleLabel: "Vídeo" };
    case "laudo":
      return { route: "/laudo", moduleLabel: "Laudo" };
    case "croqui":
    case "croqui_export":
      return { route: "/croqui", moduleLabel: "Croqui" };
    case "image_analysis":
    case "image_export":
      return { route: "/imagem", moduleLabel: "Imagem" };
    case "audio":
      return { route: "/audio", moduleLabel: "Áudio" };
    case "document":
      return { route: "/documentoscopia", moduleLabel: "Documentoscopia" };
    default:
      // photo, storyboard_frame, laudo_export, imported_package, other
      return null;
  }
}

/** Id de origem a partir do id sintético "<kind>:<id>". */
function sourceId(item: EvidenceRegistryItem): string {
  const i = item.id.indexOf(":");
  return i >= 0 ? item.id.slice(i + 1) : item.id;
}

/**
 * Carrega o item no store do módulo e navega. Lança se o carregamento falhar
 * (o chamador trata e mostra feedback).
 */
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
    case "laudo":
      await useLaudoStore.getState().openLaudo(workspacePath, sourceId(item));
      navigate("/laudo");
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
    case "document":
      useDocumentsStore.getState().select(sourceId(item));
      navigate("/documentoscopia");
      return;
    default:
      return;
  }
}
