/**
 * Orquestrador do editor de vídeo. Trilho de modos à esquerda (Assistir, Velocidade,
 * Distância); no centro o reprodutor ou o quadro exato; à direita metadados/eventos/
 * storyboard (Assistir) ou o guia do modo. O tempo do player é o timestamp técnico.
 */

import { registerOpenVideo } from "@modules/midia/midiaLink";
import { useImmersive } from "@stores/immersiveStore";
import { open as openFileDialog } from "@tauri-apps/plugin-dialog";
import { VideoTabs } from "./VideoTabs";
import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { ArrowLeft, Clock, Columns2, Copy, Repeat, Scissors, X } from "lucide-react";
import type {
  ClipMode,
  ExportClipResult,
  VideoClockCalibration,
  VideoMedia,
  VideoStoryboardFrame,
} from "@domain/video";
import type { MenuItem } from "@components/ContextMenu/ContextMenu";
import { commands } from "@core/commands";
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
import { ModeRail } from "./medir/ModeRail";
import { MedirStage } from "./medir/MedirStage";
import { SpeedGuide, type MedirActions } from "./medir/SpeedGuide";
import { DistanceGuide } from "./medir/DistanceGuide";
import { VideoHealth } from "./medir/VideoHealth";
import medirStyles from "./medir/Medir.module.css";
import { useMedirStore } from "./medir/medirStore";
import { ClockDialog, ExportClipDialog, SequenceDialog } from "./AnalysisDialogs";
import { StoryboardGallery } from "./StoryboardGallery";
import { MultiCamView } from "./MultiCamView";
import {
  cameraClockAt,
  estimateFrameIndex,
  formatClock,
  formatDuration,
  formatLaudoTime,
  parseDerivation,
  parseTimeInput,
  parseWarnings,
  probeHasAudio,
  probeStartTime,
} from "./format";
import styles from "./VideoAnalysisView.module.css";
import { askConfirm } from "@components/Dialog/ask";

export function VideoAnalysisView() {
  useImmersive();
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
  const mediaList = useVideoStore((s) => s.list);
  const openMedia = useVideoStore((s) => s.openMedia);
  const loadList = useVideoStore((s) => s.loadList);
  const registerMedia = useVideoStore((s) => s.registerMedia);
  /** Progresso ao registrar vídeos novos pelas abas ("Registrando 1 de 3…"). */
  const [adding, setAdding] = useState<string | null>(null);

  const [currentTime, setCurrentTime] = useState(0);
  const [duration, setDuration] = useState<number | null>(null);

  // Aba Áudios irmã: diz qual vídeo está aberto e em que instante (a barra do
  // topo leva isso ao trocar para Áudios).
  const timeNowRef = useRef(0);
  timeNowRef.current = currentTime;
  const openMediaRow = bundle?.media ?? null;
  useEffect(() => {
    const m = openMediaRow;
    if (!m) return;
    return registerOpenVideo(() => ({
      id: m.id,
      sha256: m.sha256,
      relativePath: m.relative_path,
      filename: m.filename,
      rawProbeJson: m.raw_probe_json,
      time: timeNowRef.current,
    }));
  }, [openMediaRow]);
  const [selectedEventId, setSelectedEventId] = useState<string | null>(null);
  const [feedback, setFeedback] = useState<string | null>(null);
  const modo = useMedirStore((s) => s.modo);
  const setModo = useMedirStore((s) => s.setModo);
  const speedStep = useMedirStore((s) => s.speed.step);
  const distStep = useMedirStore((s) => s.dist.step);
  const perito = useMedirStore((s) => s.perito);
  const bindMedia = useMedirStore((s) => s.bindMedia);
  // O reprodutor aparece em Assistir e no primeiro passo dos modos (escolher o momento).
  const showPlayer =
    modo === "assistir" || (modo === "velocidade" && speedStep === 0) || (modo === "distancia" && distStep === 0);
  useEffect(() => {
    if (openMediaRow) bindMedia(openMediaRow.sha256);
  }, [openMediaRow, bindMedia]);
  const controllerRef = useRef<PlayerController | null>(null);

  // Autor das calibrações/cálculos; nunca vazio (cai para "Perito").
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

  // Tela cheia: o painel inteiro (vídeo, controles, linha do tempo) vai pela
  // Fullscreen API; se o WebView recusar, cai num modo "expandido" que cobre a janela.
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

  // Sair do reprodutor sai da tela cheia (o painel fica oculto).
  useEffect(() => {
    if (!showPlayer) exitFullscreen();
  }, [showPlayer, exitFullscreen]);

  const bigScreen = fullscreen || expanded;

  // comparação de câmeras
  const [compareWith, setCompareWith] = useState<VideoMedia | null>(null);
  const [pickCompare, setPickCompare] = useState(false);
  // Galeria do storyboard e diálogo de trecho abertos: o teclado é deles.
  const [galleryIndex, setGalleryIndex] = useState<number | null>(null);
  const [clipOpen, setClipOpen] = useState(false);
  const playerKeys = showPlayer && compareWith == null && galleryIndex == null && !clipOpen;
  useShortcuts({ "video.fullscreen": toggleFullscreen }, { enabled: playerKeys });
  useShortcuts(
    {
      "video.mode.assistir": () => setModo("assistir"),
      "video.mode.velocidade": () => setModo("velocidade"),
      "video.mode.distancia": () => setModo("distancia"),
    },
    { enabled: compareWith == null && galleryIndex == null && !clipOpen },
  );

  // vídeos do caso: alternar e adicionar sem sair da análise
  const switchVideo = (id: string) => {
    if (!workspacePath || id === bundle?.media.id) return;
    exitFullscreen();
    void openMedia(workspacePath, id).catch((e) => flash(`Falha: ${toSicroError(e).message}`, 5000));
  };
  const stepVideo = (dir: 1 | -1) => {
    if (mediaList.length < 2 || !bundle) return;
    const i = mediaList.findIndex((m) => m.id === bundle.media.id);
    const next = mediaList[(i + dir + mediaList.length) % mediaList.length];
    if (next) switchVideo(next.id);
  };
  const addVideos = async () => {
    if (!workspacePath || adding) return;
    let picked: string | string[] | null;
    try {
      picked = await openFileDialog({
        multiple: true,
        title: "Adicionar vídeos ao caso",
        filters: [{ name: "Vídeos", extensions: ["mp4", "mov", "mkv", "avi", "webm", "m4v"] }],
      });
    } catch {
      return;
    }
    const files = Array.isArray(picked) ? picked : typeof picked === "string" ? [picked] : [];
    if (files.length === 0) return;
    let last: VideoMedia | null = null;
    const failed: string[] = [];
    for (const [i, file] of files.entries()) {
      setAdding(files.length > 1 ? `Registrando ${i + 1} de ${files.length}…` : "Registrando…");
      try {
        last = await registerMedia(workspacePath, file);
      } catch (e) {
        failed.push(`${file.split(/[\\/]/).pop()}: ${toSicroError(e).message}`);
      }
    }
    setAdding(null);
    if (failed.length) flash(`Não registrado — ${failed.join(" · ")}`, 7000);
    if (last) switchVideo(last.id);
  };
  useShortcuts(
    {
      "video.prevVideo": () => stepVideo(-1),
      "video.nextVideo": () => stepVideo(1),
      "video.addVideo": () => void addVideos(),
    },
    { enabled: playerKeys },
  );

  const media0 = bundle?.media ?? null;
  const fpsDeclared = media0?.fps_declared ?? null;
  const startTime = useMemo(
    () => probeStartTime(media0?.raw_probe_json),
    [media0?.raw_probe_json],
  );

  // trecho entrada/saída (I/O) + repetição
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
  const setMark = (which: "in" | "out") => setMarkAt(which, nowTime());
  const setMarkAt = (which: "in" | "out", t: number) => {
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

  // ir para tempo / quadro (digitado)
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

  // pular entre eventos
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
    { enabled: playerKeys },
  );

  // relógio da câmera
  const [clocks, setClocks] = useState<VideoClockCalibration[]>([]);
  const [clockOpen, setClockOpen] = useState(false);
  const [clockBusy, setClockBusy] = useState(false);
  useEffect(() => {
    if (!workspacePath) return;
    void commands
      .listVideoClocks(workspacePath)
      .then(setClocks)
      .catch(() => setClocks([]));
  }, [workspacePath, media0?.sha256]);
  const clock = clocks.find((c) => c.media_hash === media0?.sha256) ?? null;
  /** "câmera 03:36:08" do instante `t`, ou "" sem vínculo. */
  const clockText = (t: number, withMs = false) =>
    clock ? formatClock(cameraClockAt(t, clock), withMs) : "";
  const saveClock = async (v: {
    clockSeconds: number;
    clockLabel: string;
    clockDate: string | null;
    note: string;
  }) => {
    if (!workspacePath || !media0) return;
    setClockBusy(true);
    try {
      const t = nowTime();
      const saved = await commands.setVideoClock(workspacePath, {
        media_hash: media0.sha256,
        media_time_s: t,
        clock_seconds: v.clockSeconds,
        clock_date: v.clockDate,
        clock_label: v.clockLabel,
        note: v.note,
      });
      setClocks((cs) => [...cs.filter((c) => c.media_hash !== saved.media_hash), saved]);
      setClockOpen(false);
      flash(`Relógio vinculado: ${formatDuration(t)} do vídeo = ${formatClock(v.clockSeconds, false)} na câmera.`, 4000);
    } catch (err) {
      flash(`Falha: ${toSicroError(err).message}`, 5000);
    } finally {
      setClockBusy(false);
    }
  };
  const deleteClock = async () => {
    if (!workspacePath || !media0) return;
    setClockBusy(true);
    try {
      await commands.deleteVideoClock(workspacePath, media0.sha256);
      setClocks((cs) => cs.filter((c) => c.media_hash !== media0.sha256));
      setClockOpen(false);
      flash("Vínculo do relógio removido.");
    } catch (err) {
      flash(`Falha: ${toSicroError(err).message}`, 5000);
    } finally {
      setClockBusy(false);
    }
  };

  // evento com uma tecla e copiar o tempo
  const quickEvent = async () => {
    if (!workspacePath || !media0) return;
    const t = nowTime();
    try {
      const ev = await createEvent(workspacePath, {
        media_hash: media0.sha256,
        timestamp_s: t,
        category: "outro",
        title: `Marcador ${formatDuration(t)}${clock ? ` · câmera ${clockText(t)}` : ""}`,
      });
      setSelectedEventId(ev.id);
      flash(`Evento marcado em ${ev.timestamp_label} — renomeie no painel de eventos.`, 3500);
    } catch (err) {
      flash(`Falha: ${toSicroError(err).message}`, 5000);
    }
  };
  /** Texto no formato de laudo para o instante `t` (com relógio, se vinculado). */
  const timeText = (t: number) => {
    const idx = estimateFrameIndex(t, fpsDeclared);
    let text = `${formatLaudoTime(t)}${idx != null ? ` (quadro ≈ ${idx})` : ""}`;
    if (clock) {
      const cam = cameraClockAt(t, clock);
      let date = "";
      if (clock.clock_date) {
        const d = new Date(`${clock.clock_date}T00:00:00`);
        d.setDate(d.getDate() + Math.floor(cam / 86400));
        date = ` de ${d.toLocaleDateString("pt-BR")}`;
      }
      text += ` — relógio da câmera: ${formatClock(cam, false)}${date}`;
    }
    return text;
  };
  const copyText = async (text: string) => {
    try {
      await navigator.clipboard.writeText(text);
      flash(`Copiado: ${text}`, 3500);
    } catch {
      flash("Não foi possível copiar para a área de transferência.");
    }
  };
  const copyTime = () => copyText(timeText(nowTime()));
  const copyFrameTime = (f: VideoStoryboardFrame) =>
    copyText(timeText(f.actual_timestamp_s ?? f.requested_timestamp_s));

  // exportar trecho (cópia)
  const [clipBusy, setClipBusy] = useState(false);
  const [clipResult, setClipResult] = useState<ExportClipResult | null>(null);
  const exportClip = async (mode: ClipMode, includeAudio: boolean) => {
    if (!workspacePath || !media0 || !range) return;
    setClipBusy(true);
    try {
      const r = await commands.exportVideoClip(workspacePath, {
        media_hash: media0.sha256,
        start_s: range.a,
        end_s: range.b,
        mode,
        include_audio: includeAudio,
      });
      setClipResult(r);
      void loadList(workspacePath);
    } catch (err) {
      flash(`Falha ao exportar o trecho: ${toSicroError(err).message}`, 6000);
      setClipOpen(false);
    } finally {
      setClipBusy(false);
    }
  };
  const openClipDialog = () => {
    if (!range) {
      flash("Marque a entrada (I) e a saída (O) do trecho primeiro.");
      return;
    }
    exitFullscreen();
    setClipResult(null);
    setClipOpen(true);
  };

  // painel lateral redimensionável
  const [sideWidth, setSideWidth] = useState(() => {
    try {
      const n = Number(localStorage.getItem("sicro.video.sideWidth.v1"));
      return Number.isFinite(n) && n >= 260 ? n : 380;
    } catch {
      return 380;
    }
  });
  useEffect(() => {
    try {
      localStorage.setItem("sicro.video.sideWidth.v1", String(Math.round(sideWidth)));
    } catch {
      /* só não lembra */
    }
  }, [sideWidth]);
  const bodyRef = useRef<HTMLDivElement | null>(null);
  const startSideDrag = (e: React.PointerEvent<HTMLDivElement>) => {
    if (e.button !== 0) return;
    e.preventDefault();
    const el = e.currentTarget;
    el.setPointerCapture(e.pointerId);
    const move = (ev: PointerEvent) => {
      const r = bodyRef.current?.getBoundingClientRect();
      if (!r) return;
      setSideWidth(Math.min(r.width * 0.7, Math.max(260, r.right - ev.clientX)));
    };
    const up = () => {
      el.removeEventListener("pointermove", move);
      el.removeEventListener("pointerup", up);
      el.removeEventListener("pointercancel", up);
    };
    el.addEventListener("pointermove", move);
    el.addEventListener("pointerup", up);
    el.addEventListener("pointercancel", up);
  };

  // menus de botão direito
  const videoMenuItems = (): MenuItem[] => [
    { label: "Coletar frame", shortcut: "Ctrl+1", onSelect: () => void handleCollectFrame() },
    { label: "Coletar sequência…", shortcut: "Ctrl+2", onSelect: () => setSeqOpen(true) },
    { label: "Marcar evento aqui", shortcut: "M", onSelect: () => void quickEvent() },
    { label: "Copiar tempo (laudo)", shortcut: "Ctrl+Shift+C", onSelect: () => void copyTime() },
    "separator",
    { label: "Marcar entrada do trecho", shortcut: "I", onSelect: () => setMark("in") },
    { label: "Marcar saída do trecho", shortcut: "O", onSelect: () => setMark("out") },
    ...(range ? [{ label: "Exportar trecho…", onSelect: openClipDialog }] : []),
    "separator",
    { label: clock ? "Relógio da câmera…" : "Vincular relógio da câmera…", onSelect: () => setClockOpen(true) },
    { label: "Ir para tempo ou quadro…", shortcut: "Ctrl+G", onSelect: openGoto },
  ];
  const timelineMenuItems = (t: number): MenuItem[] => [
    { label: `Ir para ${formatDuration(t)}`, onSelect: () => handleSeek(t) },
    { label: "Marcar entrada aqui", onSelect: () => setMarkAt("in", t) },
    { label: "Marcar saída aqui", onSelect: () => setMarkAt("out", t) },
  ];

  // coletar sequência de quadros
  const [seqOpen, setSeqOpen] = useState(false);
  const [seqRun, setSeqRun] = useState<{ done: number; total: number } | null>(null);
  const seqCancelRef = useRef(false);
  const runSequence = async (count: number, stepFrames: number) => {
    if (!workspacePath || !media0) return;
    setSeqOpen(false);
    seqCancelRef.current = false;
    const t0 = nowTime();
    const frame = fpsDeclared && fpsDeclared > 0 ? 1 / fpsDeclared : 1 / 30;
    const end = media0.duration_s ?? Infinity;
    let done = 0;
    setSeqRun({ done: 0, total: count });
    try {
      for (let i = 0; i < count; i++) {
        if (seqCancelRef.current) break;
        const t = t0 + i * stepFrames * frame;
        if (t > end) break;
        await collectFrame(workspacePath, {
          media_hash: media0.sha256,
          timestamp_s: t,
          event_id: null,
          title: `Seq. ${i + 1}/${count} · ${formatDuration(t)}${clock ? ` · câmera ${clockText(t, true)}` : ""}`,
        });
        done = i + 1;
        setSeqRun({ done, total: count });
      }
      flash(
        seqCancelRef.current
          ? `Sequência interrompida: ${done} de ${count} quadros coletados.`
          : `${done} quadros coletados a partir de ${formatDuration(t0)}.`,
        4000,
      );
    } catch (err) {
      flash(`Falha no quadro ${done + 1}: ${toSicroError(err).message}`, 6000);
    } finally {
      setSeqRun(null);
    }
  };

  useShortcuts(
    {
      "video.quickEvent": () => void quickEvent(),
      "video.copyTime": () => void copyTime(),
      "video.collectSequence": () => {
        if (!seqRun) setSeqOpen(true);
      },
    },
    { enabled: playerKeys },
  );

  // Lista de vídeos da ocorrência (para escolher a 2ª câmera).
  useEffect(() => {
    if (workspacePath && mediaList.length === 0) void loadList(workspacePath);
  }, [workspacePath, mediaList.length, loadList]);
  const otherVideos = mediaList.filter((m) => m.sha256 !== media0?.sha256);
  // Este vídeo é um trecho exportado? (de onde veio)
  const derivation = parseDerivation(media0?.derivation_json);
  const sourceOfClip = derivation
    ? mediaList.find((m) => m.sha256 === derivation.source.sha256) ?? null
    : null;

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
    if (!(await askConfirm({ title: "Apagar evento", message: "A ação é permanente.", confirmLabel: "Apagar", danger: true }))) return;
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

  const medirActions: MedirActions = {
    now: nowTime,
    collectAt: async (t, title) => {
      const f = await collectFrame(workspacePath, { media_hash: media.sha256, timestamp_s: t, event_id: null, title });
      return f;
    },
    seek: (t) => {
      handleSeek(t);
      setCurrentTime(t);
    },
    fps: media.fps_declared,
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
        title:
          opts?.title ??
          `Frame ${formatDuration(currentTime)}${clock ? ` · câmera ${clockText(currentTime, true)}` : ""}`,
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
      !(await askConfirm({
        title: "Apagar o PNG do disco também?",
        message: "A ação não pode ser desfeita.",
        confirmLabel: "Apagar",
        danger: true,
      }))
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
        <VideoTabs
          workspacePath={workspacePath}
          videos={mediaList.length ? mediaList : [media]}
          activeId={media.id}
          adding={adding}
          onOpen={switchVideo}
          onAdd={() => void addVideos()}
          onCompare={(id) => {
            const m = mediaList.find((x) => x.id === id);
            if (!m) return;
            setPickCompare(false);
            exitFullscreen();
            controllerRef.current?.seek(nowTime());
            setCompareWith(m);
          }}
        />
        {feedback && <span className={styles.feedback}>{feedback}</span>}
        <div className={styles.compareBox}>
          {compareWith ? (
            <button type="button" className={styles.compareBtn} onClick={() => setCompareWith(null)}>
              <X size={13} /> Fechar comparação
            </button>
          ) : (
            <button
              type="button"
              className={styles.compareBtn}
              disabled={otherVideos.length === 0}
              onClick={() => setPickCompare((o) => !o)}
              title={
                otherVideos.length === 0
                  ? "Registre outro vídeo nesta ocorrência para comparar"
                  : "Ver duas câmeras lado a lado, sincronizadas"
              }
            >
              <Columns2 size={13} /> Comparar câmeras
            </button>
          )}
          {pickCompare && !compareWith && (
            <ul className={styles.comparePick}>
              {otherVideos.map((m) => (
                <li key={m.id}>
                  <button
                    type="button"
                    onClick={() => {
                      setPickCompare(false);
                      exitFullscreen();
                      controllerRef.current?.seek(nowTime()); // estabiliza o tempo atual
                      setCompareWith(m);
                    }}
                  >
                    {m.filename}
                    <span>
                      {m.duration_s != null ? formatDuration(m.duration_s) : "—"}
                      {clocks.some((c) => c.media_hash === m.sha256) ? " · relógio vinculado" : ""}
                    </span>
                  </button>
                </li>
              ))}
            </ul>
          )}
        </div>
      </header>
      {derivation && (
        <div className={styles.derivedBar}>
          <Scissors size={11} /> trecho de <strong>{derivation.source.filename}</strong> (
          {formatDuration(derivation.actual.start_s)} → {formatDuration(derivation.actual.end_s)},{" "}
          {derivation.mode === "copy" ? "sem recompressão" : "recomprimido"})
          {sourceOfClip && (
            <button
              type="button"
              onClick={() => workspacePath && void openMedia(workspacePath, sourceOfClip.id)}
              title="Abrir o vídeo de origem"
            >
              abrir origem
            </button>
          )}
        </div>
      )}

      {compareWith && workspacePath ? (
        <MultiCamView
          workspacePath={workspacePath}
          a={media}
          b={compareWith}
          clocks={clocks}
          initialTime={currentTime}
          onClose={() => setCompareWith(null)}
        />
      ) : (
      <div ref={bodyRef} className={styles.body}>
        <ModeRail modo={modo} onChange={setModo} />
        {galleryIndex != null && workspacePath && (
          <StoryboardGallery
            workspacePath={workspacePath}
            frames={storyboard}
            events={events}
            startIndex={galleryIndex}
            onClose={() => setGalleryIndex(null)}
            onGoto={(f) => handleSeek(f.actual_timestamp_s ?? f.requested_timestamp_s)}
            onCopyTime={(f) => void copyFrameTime(f)}
          />
        )}
        <main className={styles.main}>
          {/* O reprodutor fica sempre montado (só muda a visibilidade) para preservar o estado do player. */}
          <div
            ref={playerPaneRef}
            className={`${styles.tabPane} ${expanded ? styles.tabPaneExpanded : ""}`}
            style={{ display: showPlayer ? "flex" : "none" }}
          >
            {clockOpen && (
              <ClockDialog
                mediaTime={nowTime()}
                current={clock}
                busy={clockBusy}
                onSave={(v) => void saveClock(v)}
                onDelete={() => void deleteClock()}
                onClose={() => setClockOpen(false)}
              />
            )}
            {clipOpen && range && (
              <ExportClipDialog
                range={range}
                hasAudio={probeHasAudio(media.raw_probe_json)}
                busy={clipBusy}
                result={clipResult}
                onExport={(mode, audio) => void exportClip(mode, audio)}
                onOpenClip={() => {
                  const id = clipResult?.media.id;
                  setClipOpen(false);
                  if (id && workspacePath) void openMedia(workspacePath, id);
                }}
                onClose={() => setClipOpen(false)}
              />
            )}
            {seqOpen && (
              <SequenceDialog
                startTime={nowTime()}
                fps={fpsDeclared}
                duration={effectiveDuration}
                onStart={(n, k) => void runSequence(n, k)}
                onClose={() => setSeqOpen(false)}
              />
            )}
            <VideoPlayerPanel
              workspacePath={workspacePath}
              relativePath={media.relative_path}
              fps={media.fps_declared}
              active={playerKeys}
              onTimeUpdate={setCurrentTime}
              onDurationLoaded={setDuration}
              onCollectFrame={() => void handleCollectFrame()}
              registerController={handleRegisterController}
              mediaKey={media.sha256}
              startTime={startTime}
              loop={loop}
              hasAudio={probeHasAudio(media.raw_probe_json)}
              menuItems={videoMenuItems}
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
              shortcutsEnabled={playerKeys}
              contextItems={timelineMenuItems}
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
              <button
                type="button"
                className={styles.clockBtn}
                onClick={() => setClockOpen(true)}
                title={
                  clock
                    ? `Relógio da câmera (vínculo: ${formatDuration(clock.media_time_s)} = ${clock.clock_label}) — clique para refazer`
                    : "Vincular o relógio que a câmera mostra na imagem"
                }
              >
                <Clock size={11} />{" "}
                {clock ? (
                  <>
                    câmera <code>{clockText(currentTime, true)}</code>
                  </>
                ) : (
                  "relógio da câmera"
                )}
              </button>
              <button
                type="button"
                className={styles.clockBtn}
                onClick={() => void copyTime()}
                title="Copiar o tempo no formato de laudo (Ctrl+Shift+C)"
              >
                <Copy size={11} /> copiar
              </button>
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
                    className={styles.rangeBtn}
                    onClick={openClipDialog}
                    title="Exportar este trecho como vídeo novo (cópia; o original não muda)"
                  >
                    <Scissors size={12} />
                  </button>
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
              {seqRun ? (
                <span className={styles.seqRun} style={{ marginLeft: "auto" }}>
                  coletando {seqRun.done}/{seqRun.total}…
                  <button type="button" onClick={() => (seqCancelRef.current = true)}>
                    parar
                  </button>
                </span>
              ) : (
                <Button
                  variant="secondary"
                  onClick={() => setSeqOpen(true)}
                  style={{ marginLeft: "auto" }}
                  title="Coletar N quadros seguidos (Ctrl+2)"
                >
                  Sequência…
                </Button>
              )}
              <Button
                variant="primary"
                onClick={() => void handleCollectFrame()}
                disabled={seqRun != null}
              >
                Coletar frame atual
              </Button>
            </div>
          </div>

          {!showPlayer && <MedirStage workspacePath={workspacePath} media={media} frames={storyboard} />}
        </main>

        <div
          className={styles.splitter}
          onPointerDown={startSideDrag}
          onDoubleClick={() => setSideWidth(380)}
          title="Arraste para mudar a largura do painel · duplo clique volta ao padrão"
          role="separator"
          aria-orientation="vertical"
        />
        <aside className={styles.side} style={{ width: sideWidth }}>
          {modo === "velocidade" ? (
            <SpeedGuide
              workspacePath={workspacePath}
              media={media}
              author={author}
              frames={storyboard}
              warnings={[...probeWarnings, ...warningsFromLastAction]}
              actions={medirActions}
            />
          ) : modo === "distancia" ? (
            <DistanceGuide
              workspacePath={workspacePath}
              media={media}
              author={author}
              frames={storyboard}
              warnings={[...probeWarnings, ...warningsFromLastAction]}
              actions={medirActions}
            />
          ) : (
          <>
          {(probeWarnings.length > 0 || warningsFromLastAction.length > 0) && (
            <div className={medirStyles.healthWrap}>
              <VideoHealth warnings={[...probeWarnings, ...warningsFromLastAction]} perito={perito} />
            </div>
          )}
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
            onOpenGallery={(i) => setGalleryIndex(i)}
            onCopyTime={(f) => void copyFrameTime(f)}
          />
          </>
          )}
        </aside>
      </div>
      )}
    </div>
  );
}
