/**
 * VideoModule — Spike F shell.
 *
 * Same shape as the Croqui module: list vs editor. The active media id
 * lives in `videoStore`: o vídeo continua aberto ao ir para a aba Áudios (ou
 * outro módulo) e voltar; só fecha quando muda a ocorrência.
 */

import { useEffect } from "react";
import { savePosition } from "./editor/resume";
import { audioToVideoTime, takeVideoHandoff } from "@modules/midia/midiaLink";
import { commands } from "@core/commands";
import { Film } from "lucide-react";
import {
  selectActiveOccurrence,
  selectActiveWorkspacePath,
  useWorkspaceStore,
} from "@stores/workspaceStore";
import { NoOccurrenceState } from "@components/NoOccurrenceState/NoOccurrenceState";
import { VideoListView } from "./VideoListView";
import { VideoAnalysisView } from "./editor/VideoAnalysisView";
import { useVideoStore } from "./store/videoStore";
import styles from "./VideoModule.module.css";

/** Ocorrência em que o vídeo aberto foi aberto (sobrevive à troca de aba). */
let lastWorkspace: string | null | undefined;

export function VideoModule() {
  const occurrence = useWorkspaceStore(selectActiveOccurrence);
  const workspacePath = useWorkspaceStore(selectActiveWorkspacePath);
  const activeMediaId = useVideoStore((s) => s.activeMediaId);
  const closeMedia = useVideoStore((s) => s.closeMedia);
  const openMedia = useVideoStore((s) => s.openMedia);

  // Fecha o vídeo aberto só quando a ocorrência muda.
  useEffect(() => {
    if (lastWorkspace !== workspacePath) {
      if (lastWorkspace !== undefined) closeMedia();
      lastWorkspace = workspacePath;
    }
  }, [workspacePath, closeMedia]);

  // Veio da aba Áudios com o áudio de um vídeo: abre esse vídeo no mesmo instante.
  useEffect(() => {
    const h = takeVideoHandoff();
    if (!h || !workspacePath) return;
    void commands
      .listVideoMedia(workspacePath)
      .then((list) => {
        const v = list.find((m) => m.sha256 === h.videoSha256);
        if (!v) return;
        savePosition(v.sha256, audioToVideoTime(h.audioTime, v.raw_probe_json));
        return openMedia(workspacePath, v.id);
      })
      .catch(() => {
        /* sem o vídeo, fica a lista */
      });
  }, [workspacePath, openMedia]);

  if (!workspacePath || !occurrence) {
    return (
      <NoOccurrenceState
        icon={<Film size={36} strokeWidth={1.5} />}
        moduleName="Vídeo"
      />
    );
  }

  return (
    <div className={styles.wrap}>
      {/* key: cada vídeo começa com o estado limpo (trecho, lupa, comparação…) */}
      {activeMediaId == null ? <VideoListView /> : <VideoAnalysisView key={activeMediaId} />}
    </div>
  );
}
