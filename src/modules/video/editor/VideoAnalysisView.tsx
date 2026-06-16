/**
 * VideoAnalysisView — orquestrador do editor de vídeo.
 *
 *   ┌────────────────────────────────────────────────────────────────┐
 *   │ Toolbar (voltar + arquivo)                                     │
 *   ├────────────────────────────┬───────────────────────────────────┤
 *   │  VideoPlayerPanel           │  VideoMetadataPanel              │
 *   │  (HTMLVideoElement)         │                                  │
 *   │                             │  VideoEventPanel                 │
 *   │  VideoTimeline              │                                  │
 *   │  Status (timestamp / dur)   │  VideoStoryboardPanel            │
 *   └────────────────────────────┴───────────────────────────────────┘
 *
 * O player local mantém o tempo corrente (`currentTime`). Quando o
 * usuário cria evento ou coleta frame, esse tempo é usado como
 * timestamp técnico.
 */

import { useCallback, useMemo, useRef, useState } from "react";
import { ArrowLeft, AlertTriangle } from "lucide-react";
import { Button } from "@components/Button/Button";
import { toSicroError } from "@core/errors";
import {
  selectActiveOccurrence,
  selectActiveWorkspacePath,
  useWorkspaceStore,
} from "@stores/workspaceStore";
import { useVideoStore } from "../store/videoStore";
import { VideoPlayerPanel } from "./VideoPlayerPanel";
import { VideoTimeline } from "./VideoTimeline";
import { VideoEventPanel } from "./VideoEventPanel";
import { VideoMetadataPanel } from "./VideoMetadataPanel";
import { VideoStoryboardPanel } from "./VideoStoryboardPanel";
import { SpeedPanel } from "./speed/SpeedPanel";
import { MeasurePanel } from "./measure/MeasurePanel";
import { formatDuration, parseWarnings } from "./format";
import styles from "./VideoAnalysisView.module.css";

export function VideoAnalysisView() {
  const workspacePath = useWorkspaceStore(selectActiveWorkspacePath);
  const occurrence = useWorkspaceStore(selectActiveOccurrence);
  const bundle = useVideoStore((s) => s.bundle);
  const closeMedia = useVideoStore((s) => s.closeMedia);
  const createEvent = useVideoStore((s) => s.createEvent);
  const updateEvent = useVideoStore((s) => s.updateEvent);
  const deleteEvent = useVideoStore((s) => s.deleteEvent);
  const collectFrame = useVideoStore((s) => s.collectFrame);
  const deleteStoryboardFrame = useVideoStore((s) => s.deleteStoryboardFrame);
  const warningsFromLastAction = useVideoStore((s) => s.warningsFromLastAction);
  const clearWarnings = useVideoStore((s) => s.clearWarnings);

  const [currentTime, setCurrentTime] = useState(0);
  const [duration, setDuration] = useState<number | null>(null);
  const [selectedEventId, setSelectedEventId] = useState<string | null>(null);
  const [feedback, setFeedback] = useState<string | null>(null);
  const [mainTab, setMainTab] = useState<"player" | "speed" | "measure">(
    "player",
  );
  const seekRef = useRef<((seconds: number) => void) | null>(null);

  // Autor (perito) do contexto do app — usado nas calibrações/cálculos de
  // velocidade. Nunca vazio: cai para "Perito" se a ocorrência não listar.
  const author = useMemo(() => {
    const peritos = occurrence?.peritos ?? [];
    return peritos.length > 0 ? peritos.join(", ") : "Perito";
  }, [occurrence]);

  const handlePlayerSeekRef = useCallback((fn: (s: number) => void) => {
    seekRef.current = fn;
  }, []);

  const probeWarnings = useMemo(
    () => (bundle ? parseWarnings(bundle.media.warnings_json) : []),
    [bundle],
  );

  if (!workspacePath || !bundle) {
    return <div className={styles.empty}>Sem mídia aberta.</div>;
  }

  const { media, events, storyboard } = bundle;

  const handleCreateEvent = async (category: string, title: string) => {
    if (!workspacePath) return;
    try {
      const ev = await createEvent(workspacePath, {
        media_hash: media.sha256,
        timestamp_s: currentTime,
        category,
        title,
      });
      setSelectedEventId(ev.id);
      setFeedback(`Evento criado em ${ev.timestamp_label}.`);
      setTimeout(() => setFeedback(null), 2500);
    } catch (err) {
      setFeedback(`Falha: ${toSicroError(err).message}`);
    }
  };

  const handleUpdateEvent = async (
    eventId: string,
    patch: { title?: string; description?: string; category?: string; reviewed?: boolean },
  ) => {
    if (!workspacePath) return;
    try {
      await updateEvent(workspacePath, eventId, patch);
    } catch (err) {
      setFeedback(`Falha: ${toSicroError(err).message}`);
    }
  };

  const handleDeleteEvent = async (eventId: string) => {
    if (!workspacePath) return;
    if (!window.confirm("Apagar este evento? A ação é permanente.")) return;
    try {
      await deleteEvent(workspacePath, eventId);
      if (selectedEventId === eventId) setSelectedEventId(null);
    } catch (err) {
      setFeedback(`Falha: ${toSicroError(err).message}`);
    }
  };

  const handleAdjustEventToCurrent = async (eventId: string) => {
    if (!workspacePath) return;
    try {
      await updateEvent(workspacePath, eventId, { timestamp_s: currentTime });
      setFeedback(`Evento movido para ${formatDuration(currentTime)}.`);
      setTimeout(() => setFeedback(null), 2500);
    } catch (err) {
      setFeedback(`Falha: ${toSicroError(err).message}`);
    }
  };

  const handleSeek = (seconds: number) => {
    seekRef.current?.(seconds);
  };

  const handleCollectFrame = async (opts?: {
    title?: string;
    eventId?: string | null;
  }) => {
    if (!workspacePath) return;
    try {
      await collectFrame(workspacePath, {
        media_hash: media.sha256,
        timestamp_s: currentTime,
        event_id: opts?.eventId ?? null,
        title: opts?.title ?? `Frame ${formatDuration(currentTime)}`,
      });
      setFeedback(`Frame coletado em ${formatDuration(currentTime)}.`);
      setTimeout(() => {
        setFeedback(null);
        clearWarnings();
      }, 6000);
    } catch (err) {
      setFeedback(`Falha: ${toSicroError(err).message}`);
    }
  };

  const handleDeleteFrame = async (frameId: string, deletePng: boolean) => {
    if (!workspacePath) return;
    if (
      deletePng &&
      !window.confirm("Apagar PNG do disco também? A ação não pode ser desfeita.")
    ) {
      return;
    }
    try {
      await deleteStoryboardFrame(workspacePath, frameId, deletePng);
    } catch (err) {
      setFeedback(`Falha: ${toSicroError(err).message}`);
    }
  };

  const effectiveDuration = duration ?? media.duration_s ?? 0;

  return (
    <div className={styles.wrap}>
      <header className={styles.topBar}>
        <button
          type="button"
          className={styles.backBtn}
          onClick={closeMedia}
          title="Voltar para a lista de vídeos"
        >
          <ArrowLeft size={14} /> Voltar
        </button>
        <div className={styles.titleBlock}>
          <strong>{media.filename}</strong>
          <span className={styles.meta}>
            {media.codec ?? "codec —"} ·{" "}
            {media.width && media.height
              ? `${media.width}×${media.height}`
              : "—"}{" "}
            ·{" "}
            {media.fps_declared
              ? `${media.fps_declared.toFixed(2)} fps`
              : "fps —"}{" "}
            · SHA <code>{media.sha256.slice(0, 12)}…</code>
          </span>
        </div>
        {feedback && <span className={styles.feedback}>{feedback}</span>}
      </header>

      {(probeWarnings.length > 0 || warningsFromLastAction.length > 0) && (
        <div className={styles.warningBanner}>
          <AlertTriangle size={14} />
          <div>
            {probeWarnings.map((w, i) => (
              <div key={`p-${i}`}>{w}</div>
            ))}
            {warningsFromLastAction.map((w, i) => (
              <div key={`a-${i}`}>{w}</div>
            ))}
          </div>
        </div>
      )}

      <div className={styles.body}>
        <main className={styles.main}>
          <div className={styles.mainTabs}>
            <button
              type="button"
              className={mainTab === "player" ? styles.mainTabActive : styles.mainTab}
              onClick={() => setMainTab("player")}
            >
              Reprodutor
            </button>
            <button
              type="button"
              className={mainTab === "speed" ? styles.mainTabActive : styles.mainTab}
              onClick={() => setMainTab("speed")}
              title="Calculador de velocidade (sobre frames coletados)"
            >
              Velocidade
            </button>
            <button
              type="button"
              className={mainTab === "measure" ? styles.mainTabActive : styles.mainTab}
              onClick={() => setMainTab("measure")}
              title="Medição de distância (compartilha a calibração)"
            >
              Medições
            </button>
          </div>

          {/* Reprodutor e Velocidade ficam ambos MONTADOS; alternamos só a
              visibilidade para preservar a marcação em andamento e o estado
              do player ao trocar de aba. */}
          <div
            className={styles.tabPane}
            style={{ display: mainTab === "player" ? "flex" : "none" }}
          >
            <VideoPlayerPanel
              workspacePath={workspacePath}
              relativePath={media.relative_path}
              fps={media.fps_declared}
              active={mainTab === "player"}
              onTimeUpdate={setCurrentTime}
              onDurationLoaded={setDuration}
              onCollectFrame={() => void handleCollectFrame()}
              registerSeek={handlePlayerSeekRef}
            />
            <VideoTimeline
              duration={effectiveDuration}
              currentTime={currentTime}
              events={events}
              selectedEventId={selectedEventId}
              onSeek={handleSeek}
              onSelectEvent={(id) => {
                setSelectedEventId(id);
                const ev = events.find((e) => e.id === id);
                if (ev) handleSeek(ev.timestamp_s);
              }}
            />
            <div className={styles.statusBar}>
              <span>
                tempo atual:{" "}
                <code>{formatDuration(currentTime)}</code>
              </span>
              <span>
                duração: <code>{formatDuration(effectiveDuration)}</code>
              </span>
              <span>
                eventos: <code>{events.length}</code>
              </span>
              <span>
                storyboard: <code>{storyboard.length}</code>
              </span>
              <Button
                variant="primary"
                onClick={() => void handleCollectFrame()}
                style={{ marginLeft: "auto" }}
              >
                Coletar frame atual
              </Button>
            </div>
          </div>

          <div
            className={styles.tabPaneScroll}
            style={{ display: mainTab === "speed" ? "flex" : "none" }}
          >
            <SpeedPanel
              workspacePath={workspacePath}
              media={media}
              frames={storyboard}
              author={author}
            />
          </div>

          {/* Medições e Velocidade compartilham a calibração; ambos os painéis
              ficam MONTADOS (só alternamos a visibilidade) para preservar
              marcações em andamento ao trocar de aba. */}
          <div
            className={styles.tabPaneScroll}
            style={{ display: mainTab === "measure" ? "flex" : "none" }}
          >
            <MeasurePanel
              workspacePath={workspacePath}
              media={media}
              frames={storyboard}
              author={author}
            />
          </div>
        </main>

        <aside className={styles.side}>
          <VideoMetadataPanel media={media} warnings={probeWarnings} />
          <VideoEventPanel
            events={events}
            currentTime={currentTime}
            selectedEventId={selectedEventId}
            onSelect={(id) => {
              setSelectedEventId(id);
              const ev = events.find((e) => e.id === id);
              if (ev) handleSeek(ev.timestamp_s);
            }}
            onCreate={handleCreateEvent}
            onUpdate={handleUpdateEvent}
            onDelete={handleDeleteEvent}
            onAdjustToCurrent={handleAdjustEventToCurrent}
            onCollectFrameForEvent={(eventId, title) =>
              void handleCollectFrame({ title, eventId })
            }
          />
          <VideoStoryboardPanel
            workspacePath={workspacePath}
            frames={storyboard}
            events={events}
            onSelectFrame={(f) => handleSeek(f.requested_timestamp_s)}
            onDelete={handleDeleteFrame}
          />
        </aside>
      </div>
    </div>
  );
}
