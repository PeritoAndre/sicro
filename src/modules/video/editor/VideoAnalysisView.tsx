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

import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { ArrowLeft, AlertTriangle, Repeat, X } from "lucide-react";
import { Button } from "@components/Button/Button";
import { toSicroError } from "@core/errors";
import { useShortcuts } from "@core/useShortcuts";
import {
  selectActiveOccurrence,
  selectActiveWorkspacePath,
  useWorkspaceStore,
} from "@stores/workspaceStore";
import { useVideoStore } from "../store/videoStore";
import { VideoPlayerPanel, type PlayerController } from "./VideoPlayerPanel";
import { VideoTimeline } from "./VideoTimeline";
import { VideoEventPanel } from "./VideoEventPanel";
import { VideoMetadataPanel } from "./VideoMetadataPanel";
import { VideoStoryboardPanel } from "./VideoStoryboardPanel";
import { SpeedPanel } from "./speed/SpeedPanel";
import { MeasurePanel } from "./measure/MeasurePanel";
import {
  estimateFrameIndex,
  formatDuration,
  parseTimeInput,
  parseWarnings,
  probeStartTime,
} from "./format";
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
  const controllerRef = useRef<PlayerController | null>(null);

  // Autor (perito) do contexto do app — usado nas calibrações/cálculos de
  // velocidade. Nunca vazio: cai para "Perito" se a ocorrência não listar.
  const author = useMemo(() => {
    const peritos = occurrence?.peritos ?? [];
    return peritos.length > 0 ? peritos.join(", ") : "Perito";
  }, [occurrence]);

  const handleRegisterController = useCallback((c: PlayerController) => {
    controllerRef.current = c;
  }, []);
  /** Tempo exato do player (o estado `currentTime` pode estar um tique atrás). */
  const nowTime = () => controllerRef.current?.getTime() ?? currentTime;
  const flash = (msg: string, ms = 2500) => {
    setFeedback(msg);
    setTimeout(() => setFeedback(null), ms);
  };

  const probeWarnings = useMemo(
    () => (bundle ? parseWarnings(bundle.media.warnings_json) : []),
    [bundle],
  );

  // Tela cheia do reprodutor (F / duplo clique / botão): o painel inteiro —
  // vídeo, controles, linha do tempo e "Coletar frame" — vai para a tela
  // cheia pela Fullscreen API; os atalhos seguem valendo (ouvem a janela).
  // Se o WebView recusar, cai num modo "expandido" que cobre a janela.
  const playerPaneRef = useRef<HTMLDivElement | null>(null);
  const [fullscreen, setFullscreen] = useState(false);
  const [expanded, setExpanded] = useState(false);

  useEffect(() => {
    const onChange = () =>
      setFullscreen(
        playerPaneRef.current != null &&
          document.fullscreenElement === playerPaneRef.current,
      );
    document.addEventListener("fullscreenchange", onChange);
    return () => {
      document.removeEventListener("fullscreenchange", onChange);
      if (document.fullscreenElement) void document.exitFullscreen().catch(() => {});
    };
  }, []);

  const exitFullscreen = useCallback(() => {
    setExpanded(false);
    if (document.fullscreenElement) void document.exitFullscreen().catch(() => {});
  }, []);

  const toggleFullscreen = useCallback(() => {
    if (fullscreen || expanded) {
      exitFullscreen();
      return;
    }
    const el = playerPaneRef.current;
    if (!el) return;
    if (typeof el.requestFullscreen !== "function") {
      setExpanded(true);
      return;
    }
    el.requestFullscreen().catch(() => setExpanded(true));
  }, [fullscreen, expanded, exitFullscreen]);

  // O modo expandido sai com Esc (a tela cheia nativa já sai sozinha).
  useEffect(() => {
    if (!expanded) return;
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") setExpanded(false);
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [expanded]);

  // Trocar de aba sai da tela cheia (o painel do reprodutor fica oculto).
  useEffect(() => {
    if (mainTab !== "player") exitFullscreen();
  }, [mainTab, exitFullscreen]);

  useShortcuts(
    { "video.fullscreen": toggleFullscreen },
    { enabled: mainTab === "player" },
  );
  const bigScreen = fullscreen || expanded;

  const media0 = bundle?.media ?? null;
  const fpsDeclared = media0?.fps_declared ?? null;
  const startTime = useMemo(
    () => probeStartTime(media0?.raw_probe_json),
    [media0?.raw_probe_json],
  );

  // ---- trecho entrada/saída (I/O) + repetição ----------------------------
  const [markIn, setMarkIn] = useState<number | null>(null);
  const [markOut, setMarkOut] = useState<number | null>(null);
  const [loopOn, setLoopOn] = useState(false);
  const range = useMemo(
    () =>
      markIn != null && markOut != null && Math.abs(markOut - markIn) > 1e-3
        ? { a: Math.min(markIn, markOut), b: Math.max(markIn, markOut) }
        : null,
    [markIn, markOut],
  );
  const loop = loopOn && range ? range : null;
  // Fechou o trecho (entrada + saída) → já liga a repetição.
  const setMark = (which: "in" | "out") => {
    const t = nowTime();
    if (which === "in") setMarkIn(t);
    else setMarkOut(t);
    const other = which === "in" ? markOut : markIn;
    if (other != null && Math.abs(other - t) > 1e-3) setLoopOn(true);
    flash(`${which === "in" ? "Entrada" : "Saída"} do trecho em ${formatDuration(t)}.`);
  };
  const clearInOut = () => {
    setMarkIn(null);
    setMarkOut(null);
    setLoopOn(false);
  };

  // ---- ir para tempo / quadro (digitado) ---------------------------------
  const [gotoOpen, setGotoOpen] = useState(false);
  const [gotoText, setGotoText] = useState("");
  const [gotoError, setGotoError] = useState(false);
  const openGoto = () => {
    setGotoText(formatDuration(nowTime()));
    setGotoError(false);
    setGotoOpen(true);
  };
  const submitGoto = () => {
    const t = parseTimeInput(gotoText, fpsDeclared);
    if (t == null) {
      setGotoError(true);
      return;
    }
    controllerRef.current?.seek(t);
    setCurrentTime(t);
    setGotoOpen(false);
  };

  // ---- pular entre eventos -----------------------------------------------
  const jumpEvent = (dir: 1 | -1) => {
    const evs = [...(bundle?.events ?? [])].sort((x, y) => x.timestamp_s - y.timestamp_s);
    const now = nowTime();
    const target =
      dir > 0
        ? evs.find((e) => e.timestamp_s > now + 0.02)
        : [...evs].reverse().find((e) => e.timestamp_s < now - 0.02);
    if (!target) {
      flash(dir > 0 ? "Não há evento depois deste ponto." : "Não há evento antes deste ponto.");
      return;
    }
    setSelectedEventId(target.id);
    controllerRef.current?.seek(target.timestamp_s);
    setCurrentTime(target.timestamp_s);
  };

  useShortcuts(
    {
      "video.markIn": () => setMark("in"),
      "video.markOut": () => setMark("out"),
      "video.clearInOut": clearInOut,
      "video.gotoIn": () => range && controllerRef.current?.seek(range.a),
      "video.gotoOut": () => range && controllerRef.current?.seek(range.b),
      "video.toggleLoop": () => {
        if (!range) {
          flash("Marque a entrada (I) e a saída (O) do trecho primeiro.");
          return;
        }
        setLoopOn((on) => !on);
      },
      "video.prevEvent": () => jumpEvent(-1),
      "video.nextEvent": () => jumpEvent(1),
      "video.gotoTime": openGoto,
    },
    { enabled: mainTab === "player" },
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
    controllerRef.current?.seek(seconds);
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
            ref={playerPaneRef}
            className={`${styles.tabPane} ${expanded ? styles.tabPaneExpanded : ""}`}
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
              registerController={handleRegisterController}
              mediaKey={media.sha256}
              startTime={startTime}
              loop={loop}
              fullscreen={bigScreen}
              onToggleFullscreen={toggleFullscreen}
            />
            <VideoTimeline
              duration={effectiveDuration}
              fps={media.fps_declared}
              currentTime={currentTime}
              events={events}
              selectedEventId={selectedEventId}
              range={range}
              loopOn={loopOn}
              shortcutsEnabled={mainTab === "player"}
              onScrubStart={() => controllerRef.current?.scrubStart()}
              onScrub={(t) => {
                controllerRef.current?.scrub(t);
                setCurrentTime(t);
              }}
              onScrubEnd={(t) => {
                controllerRef.current?.scrubEnd(t);
                setCurrentTime(t);
              }}
              onSelectEvent={(id) => {
                setSelectedEventId(id);
                const ev = events.find((e) => e.id === id);
                if (ev) handleSeek(ev.timestamp_s);
              }}
            />
            <div className={styles.statusBar}>
              {gotoOpen ? (
                <input
                  autoFocus
                  className={`${styles.gotoInput} ${gotoError ? styles.gotoInputError : ""}`}
                  value={gotoText}
                  onChange={(e) => {
                    setGotoText(e.target.value);
                    setGotoError(false);
                  }}
                  onFocus={(e) => e.currentTarget.select()}
                  onKeyDown={(e) => {
                    if (e.key === "Enter") submitGoto();
                    if (e.key === "Escape") {
                      e.preventDefault();
                      setGotoOpen(false);
                    }
                  }}
                  onBlur={() => setGotoOpen(false)}
                  placeholder="00:12.480 ou #312"
                  title={
                    gotoError
                      ? "Não entendi — use 12.48, 00:12.480, 1:02:03 ou #312 (quadro)"
                      : "Enter vai · Esc cancela · tempo (00:12.480) ou quadro (#312)"
                  }
                />
              ) : (
                <button
                  type="button"
                  className={styles.timeBtn}
                  onClick={openGoto}
                  title="Ir para um tempo ou quadro (Ctrl+G)"
                >
                  tempo atual: <code>{formatDuration(currentTime)}</code>
                </button>
              )}
              <span title="Estimado: round(tempo × fps declarado) — mesma conta do storyboard">
                quadro ≈ <code>{estimateFrameIndex(currentTime, media.fps_declared) ?? "—"}</code>
              </span>
              <span>
                duração: <code>{formatDuration(effectiveDuration)}</code>
              </span>
              {range ? (
                <span className={styles.rangeInfo}>
                  trecho:{" "}
                  <button type="button" onClick={() => handleSeek(range.a)} title="Ir para a entrada (Shift+I)">
                    <code>{formatDuration(range.a)}</code>
                  </button>
                  →
                  <button type="button" onClick={() => handleSeek(range.b)} title="Ir para a saída (Shift+O)">
                    <code>{formatDuration(range.b)}</code>
                  </button>
                  <span className={styles.rangeLen}>({(range.b - range.a).toFixed(3)} s)</span>
                  <button
                    type="button"
                    className={`${styles.rangeBtn} ${loopOn ? styles.rangeBtnOn : ""}`}
                    onClick={() => setLoopOn((on) => !on)}
                    title={loopOn ? "Repetindo o trecho — clique para parar (Ctrl+L)" : "Repetir o trecho (Ctrl+L)"}
                  >
                    <Repeat size={12} />
                  </button>
                  <button
                    type="button"
                    className={styles.rangeBtn}
                    onClick={clearInOut}
                    title="Limpar entrada e saída (Alt+X)"
                  >
                    <X size={12} />
                  </button>
                </span>
              ) : markIn != null || markOut != null ? (
                <span className={styles.rangeInfo}>
                  {markIn != null ? `entrada ${formatDuration(markIn)} — falta a saída (O)` : `saída ${formatDuration(markOut ?? 0)} — falta a entrada (I)`}
                  <button type="button" className={styles.rangeBtn} onClick={clearInOut} title="Limpar (Alt+X)">
                    <X size={12} />
                  </button>
                </span>
              ) : null}
              <span>
                eventos: <code>{events.length}</code>
              </span>
              <span>
                storyboard: <code>{storyboard.length}</code>
              </span>
              {/* Em tela cheia o topo (onde o feedback aparece) fica escondido. */}
              {bigScreen && feedback && (
                <span className={styles.statusFeedback}>{feedback}</span>
              )}
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
