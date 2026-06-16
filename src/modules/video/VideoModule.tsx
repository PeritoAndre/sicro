/**
 * VideoModule — Spike F shell.
 *
 * Same shape as the Croqui module: list vs editor. The active media id
 * lives in `videoStore` so the user can navigate to Laudo / Dossiê /
 * Croqui and come back without losing state.
 */

import { useEffect } from "react";
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

export function VideoModule() {
  const occurrence = useWorkspaceStore(selectActiveOccurrence);
  const workspacePath = useWorkspaceStore(selectActiveWorkspacePath);
  const activeMediaId = useVideoStore((s) => s.activeMediaId);
  const closeMedia = useVideoStore((s) => s.closeMedia);

  useEffect(() => {
    return () => {
      closeMedia();
    };
  }, [workspacePath, closeMedia]);

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
      {activeMediaId == null ? <VideoListView /> : <VideoAnalysisView />}
    </div>
  );
}
