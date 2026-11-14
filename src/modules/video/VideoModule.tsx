/**
 * Módulo Vídeo (§8.1): o trilho de modos à esquerda vale na análise e na Edição; as duas ficam montadas, uma
 * escondida (`hidden`) e pausada. O id do vídeo aberto fica no `videoStore`: continua aberto ao ir para outra aba
 * e voltar; só fecha quando muda a ocorrência. A Edição monta na primeira vez que aparece e fica até trocar de caso.
 */

import { useCallback, useEffect, useRef, useState } from "react";
import { savePosition } from "./editor/resume";
import { audioToVideoTime, takeVideoHandoff } from "@modules/midia/midiaLink";
import { commands } from "@core/commands";
import { useShortcuts } from "@core/useShortcuts";
import { Clapperboard } from "lucide-react";
import {
  selectActiveOccurrence,
  selectActiveWorkspacePath,
  useWorkspaceStore,
} from "@stores/workspaceStore";
import { useImmersive } from "@stores/immersiveStore";
import { NoOccurrenceState } from "@components/NoOccurrenceState/NoOccurrenceState";
import { useDialogosAbertos } from "@components/Dialog/Dialog";
import { EdicaoRaiz } from "@modules/edicao/EdicaoRaiz";
import { useEdicaoStore } from "@modules/edicao/store/edicaoStore";
import { VideoListView } from "./VideoListView";
import { VideoAnalysisView } from "./editor/VideoAnalysisView";
import { ModeRail, type ModoTrilho } from "./editor/medir/ModeRail";
import { useMedirStore } from "./editor/medir/medirStore";
import { useVideoStore } from "./store/videoStore";
import styles from "./VideoModule.module.css";

/** Ocorrência em que o vídeo aberto foi aberto (sobrevive à troca de aba). */
let lastWorkspace: string | null | undefined;

// Caso trocado com o módulo desmontado (pela tela inicial): a Edição não volta na frente no caso novo.
useWorkspaceStore.subscribe((s, antes) => {
  if (s.activeWorkspacePath !== antes.activeWorkspacePath) useVideoStore.getState().mostrarEdicao(false);
});

/** Para o que tocava na parte que ficou escondida. */
export function pausarDentro(el: HTMLElement | null): void {
  el?.querySelectorAll<HTMLMediaElement>("video, audio").forEach((m) => {
    if (!m.paused) m.pause();
  });
}

export function VideoModule() {
  const occurrence = useWorkspaceStore(selectActiveOccurrence);
  const workspacePath = useWorkspaceStore(selectActiveWorkspacePath);
  const activeMediaId = useVideoStore((s) => s.activeMediaId);
  const closeMedia = useVideoStore((s) => s.closeMedia);
  const openMedia = useVideoStore((s) => s.openMedia);
  const edicao = useVideoStore((s) => s.edicao);
  const mostrarEdicao = useVideoStore((s) => s.mostrarEdicao);
  const modo = useMedirStore((s) => s.modo);
  const setModo = useMedirStore((s) => s.setModo);
  const semDialogo = useDialogosAbertos((s) => s.pilha.length === 0);
  const [montada, setMontada] = useState(edicao);
  const analiseRef = useRef<HTMLDivElement | null>(null);
  const edicaoRef = useRef<HTMLDivElement | null>(null);

  // Fecha o vídeo aberto e a Edição só quando a ocorrência muda.
  useEffect(() => {
    if (lastWorkspace !== workspacePath) {
      if (lastWorkspace !== undefined) {
        closeMedia();
        mostrarEdicao(false);
        setMontada(false);
        // o projeto do caso anterior sai (gravado antes pela guarda de navegação)
        const e = useEdicaoStore.getState();
        if (e.ws && e.ws !== workspacePath) void e.fechar().catch(() => {});
      }
      lastWorkspace = workspacePath;
    }
  }, [workspacePath, closeMedia, mostrarEdicao]);

  useEffect(() => {
    if (edicao) setMontada(true);
    // o lado que saiu da frente para de tocar
    pausarDentro(edicao ? analiseRef.current : edicaoRef.current);
  }, [edicao]);

  useImmersive(edicao || activeMediaId != null);

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
        mostrarEdicao(false);
        return openMedia(workspacePath, v.id);
      })
      .catch(() => {
        /* sem o vídeo, fica a lista */
      });
  }, [workspacePath, openMedia, mostrarEdicao]);

  const trocar = useCallback(
    (m: ModoTrilho) => {
      if (m === "edicao") return mostrarEdicao(true);
      mostrarEdicao(false);
      setModo(m);
    },
    [mostrarEdicao, setModo],
  );

  // Alt+1..6 quando a análise não está na frente (Edição ou lista); com vídeo aberto, a análise cuida deles.
  useShortcuts(
    {
      "video.mode.assistir": () => trocar("assistir"),
      "video.mode.velocidade": () => trocar("velocidade"),
      "video.mode.distancia": () => trocar("distancia"),
      "video.mode.pessoas": () => trocar("pessoas"),
      "video.mode.realce": () => trocar("realce"),
      "video.mode.edicao": () => trocar("edicao"),
    },
    { enabled: semDialogo && (edicao || activeMediaId == null) },
  );

  if (!workspacePath || !occurrence) {
    return (
      <NoOccurrenceState
        icon={<Clapperboard size={36} strokeWidth={1.5} />}
        moduleName="Vídeo"
      />
    );
  }

  return (
    <div className={styles.wrap}>
      <div className={styles.linha}>
        <ModeRail modo={edicao ? "edicao" : modo} onChange={trocar} />
        <div ref={analiseRef} className={styles.lado} hidden={edicao} data-lado="analise">
          {/* key: cada vídeo começa com o estado limpo (trecho, lupa, comparação…) */}
          {activeMediaId != null ? <VideoAnalysisView key={activeMediaId} visivel={!edicao} /> : <VideoListView />}
        </div>
        {montada && (
          <div ref={edicaoRef} className={styles.lado} hidden={!edicao} data-lado="edicao">
            <EdicaoRaiz visivel={edicao} />
          </div>
        )}
      </div>
    </div>
  );
}
