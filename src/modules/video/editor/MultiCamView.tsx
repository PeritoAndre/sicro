/**
 * MultiCamView — duas câmeras lado a lado.
 *
 * Cada lado é um player completo: sua linha do tempo (arrasto, zoom), quadro
 * a quadro, lupa, ajustes de tela, "Coletar frame" e a faixa do seu
 * storyboard. O lado ATIVO (o último clicado, com contorno) recebe o teclado.
 *
 * VINCULAR: pare as duas no mesmo acontecimento e clique em Vincular — o
 * SICRO guarda a diferença (tempo_B = tempo_A + deslocamento) e a partir daí
 * qualquer comando move as duas; coletar pega o PAR (um quadro de cada, no
 * storyboard de cada vídeo). "pelo relógio" vincula pelo relógio da câmera
 * quando os dois vídeos o têm. Desvincular solta para refinar.
 *
 * Tocando vinculadas, a outra é corrigida se desviar mais que ~3 quadros;
 * paradas, cada passo reposiciona a outra exatamente.
 */
import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import {
  Camera,
  Link2,
  Link2Off,
  Maximize,
  Minimize,
  Pause,
  Play,
  SlidersHorizontal,
  StepBack,
  StepForward,
  X,
} from "lucide-react";
import type {
  VideoClockCalibration,
  VideoEvent,
  VideoMedia,
  VideoStoryboardFrame,
} from "@domain/video";
import { commands } from "@core/commands";
import { mediaSrc } from "@core/mediaSrc";
import { toSicroError } from "@core/errors";
import { useShortcuts } from "@core/useShortcuts";
import { useContextMenu, type MenuItem } from "@components/ContextMenu/ContextMenu";
import { useVideoStore } from "../store/videoStore";
import { VideoTimeline } from "./VideoTimeline";
import { useMagnifier } from "./useMagnifier";
import {
  ADJUST_DEFAULT,
  GammaFilterDefs,
  ImageAdjustPanel,
  adjustFilter,
  isAdjusted,
  type Adjust,
} from "./ImageAdjust";
import { frameSrc } from "./VideoStoryboardPanel";
import {
  cameraClockAt,
  clockSyncOffset,
  formatClock,
  formatDuration,
  probeStartTime,
} from "./format";
import styles from "./MultiCamView.module.css";

const RATES = [0.1, 0.25, 0.5, 1, 2, 4, 8];
/** Desvio (s) tolerado no lado que segue antes de corrigir enquanto toca. */
const DRIFT_S = 0.12;

type Side = "a" | "b";

interface Props {
  workspacePath: string;
  a: VideoMedia;
  b: VideoMedia;
  clocks: VideoClockCalibration[];
  initialTime: number;
  onClose: () => void;
}

function srcOf(ws: string, rel: string): string {
  const sep = ws.includes("\\") ? "\\" : "/";
  return mediaSrc(`${ws}${sep}${rel.replace(/\//g, sep)}`);
}

/** Estado e comandos de UM lado (vídeo, tempo, lupa, ajustes). */
function useCam(media: VideoMedia, startAt: number) {
  const ref = useRef<HTMLVideoElement | null>(null);
  const [t, setT] = useState(startAt);
  const [dur, setDur] = useState(media.duration_s ?? 0);
  const [playing, setPlaying] = useState(false);
  const [adjust, setAdjust] = useState<Adjust>(ADJUST_DEFAULT);
  const [adjustOpen, setAdjustOpen] = useState(false);
  const magnifier = useMagnifier(media.id);
  const start = useMemo(() => probeStartTime(media.raw_probe_json), [media.raw_probe_json]);
  const frame = media.fps_declared && media.fps_declared > 0 ? 1 / media.fps_declared : 1 / 30;
  const startAtRef = useRef(startAt);

  useEffect(() => {
    const v = ref.current;
    if (!v) return;
    const onPlay = () => setPlaying(true);
    const onPause = () => {
      setPlaying(false);
      setT(v.currentTime);
    };
    const onSeeked = () => setT(v.currentTime);
    const onMeta = () => {
      setDur(v.duration);
      v.currentTime = Math.max(0, startAtRef.current);
    };
    v.addEventListener("play", onPlay);
    v.addEventListener("pause", onPause);
    v.addEventListener("seeked", onSeeked);
    v.addEventListener("loadedmetadata", onMeta);
    return () => {
      v.removeEventListener("play", onPlay);
      v.removeEventListener("pause", onPause);
      v.removeEventListener("seeked", onSeeked);
      v.removeEventListener("loadedmetadata", onMeta);
    };
  }, []);

  const time = useCallback(() => ref.current?.currentTime ?? 0, []);
  const seek = useCallback((x: number) => {
    const v = ref.current;
    if (!v) return;
    const d = Number.isFinite(v.duration) ? v.duration : x;
    v.currentTime = Math.max(0, Math.min(d, x));
    setT(v.currentTime);
  }, []);
  /** O instante `x` existe neste vídeo (entre o 1º quadro e o fim)? */
  const inRange = (x: number) => x >= start - 1e-3 && (!(dur > 0) || x <= dur);

  return {
    media, ref, t, setT, dur, playing, start, frame, time, seek, inRange,
    magnifier, adjust, setAdjust, adjustOpen, setAdjustOpen,
  };
}
type Cam = ReturnType<typeof useCam>;

export function MultiCamView({ workspacePath, a, b, clocks, initialTime, onClose }: Props) {
  const clockA = clocks.find((c) => c.media_hash === a.sha256) ?? null;
  const clockB = clocks.find((c) => c.media_hash === b.sha256) ?? null;
  const bothClocks = clockA != null && clockB != null;
  // Com os dois relógios vinculados, já abre sincronizado pelo horário.
  const initialOffset = clockA && clockB ? clockSyncOffset(clockA, clockB) : null;

  const camA = useCam(a, initialTime);
  const camB = useCam(b, initialOffset != null ? initialTime + initialOffset : probeStartTime(b.raw_probe_json));
  const cam = (s: Side) => (s === "a" ? camA : camB);
  const other = (s: Side): Side => (s === "a" ? "b" : "a");

  const [active, setActive] = useState<Side>("a");
  const [linked, setLinked] = useState(initialOffset != null);
  const [offset, setOffset] = useState(initialOffset ?? 0); // tempo_B − tempo_A
  const offsetRef = useRef(offset);
  offsetRef.current = offset;
  const linkedRef = useRef(linked);
  linkedRef.current = linked;
  const [rate, setRate] = useState(1);
  const [fullscreen, setFullscreen] = useState(false);
  const [feedback, setFeedback] = useState<string | null>(null);
  const wrapRef = useRef<HTMLDivElement | null>(null);
  const flash = (m: string, ms = 3000) => {
    setFeedback(m);
    window.setTimeout(() => setFeedback(null), ms);
  };

  // ---- storyboard/eventos de cada lado -------------------------------------
  const storeBundle = useVideoStore((s) => s.bundle);
  const collectFrameA = useVideoStore((s) => s.collectFrame);
  const [bFrames, setBFrames] = useState<VideoStoryboardFrame[]>([]);
  const [bEvents, setBEvents] = useState<VideoEvent[]>([]);
  useEffect(() => {
    void commands
      .openVideoMedia(workspacePath, b.id)
      .then((bundle) => {
        setBFrames(bundle.storyboard);
        setBEvents(bundle.events);
      })
      .catch(() => {});
  }, [workspacePath, b.id]);
  const frames = (s: Side) => (s === "a" ? storeBundle?.storyboard ?? [] : bFrames);
  const events = (s: Side) => (s === "a" ? storeBundle?.events ?? [] : bEvents);
  const clockOf = (s: Side) => (s === "a" ? clockA : clockB);

  // ---- tempo equivalente no outro lado ---------------------------------------
  const counterpart = (s: Side, t: number) => (s === "a" ? t + offsetRef.current : t - offsetRef.current);
  const syncOther = (s: Side) => {
    if (!linkedRef.current) return;
    const o = cam(other(s));
    o.seek(counterpart(s, cam(s).time()));
  };

  const seekSide = (s: Side, t: number) => {
    cam(s).seek(t);
    if (linkedRef.current) cam(other(s)).seek(counterpart(s, t));
  };
  const pauseAll = () => {
    camA.ref.current?.pause();
    camB.ref.current?.pause();
  };
  const togglePlay = async (s: Side = active) => {
    const c = cam(s);
    const v = c.ref.current;
    if (!v) return;
    if (!v.paused) {
      if (linkedRef.current) pauseAll();
      else v.pause();
      return;
    }
    const plays: Promise<void>[] = [v.play()];
    if (linkedRef.current) {
      const o = cam(other(s));
      const target = counterpart(s, v.currentTime);
      o.seek(target);
      if (o.inRange(target) && o.ref.current) plays.push(o.ref.current.play());
    }
    await Promise.all(plays).catch(() => {});
  };
  const step = (dir: 1 | -1, s: Side = active) => {
    const c = cam(s);
    if (linkedRef.current) pauseAll();
    else c.ref.current?.pause();
    seekSide(s, c.time() + dir * c.frame);
  };
  const applyRate = (r: number) => {
    setRate(r);
    if (camA.ref.current) camA.ref.current.playbackRate = r;
    if (camB.ref.current) camB.ref.current.playbackRate = r;
  };
  const cycleRate = (dir: 1 | -1) => {
    const i = RATES.indexOf(rate);
    applyRate(RATES[Math.max(0, Math.min(RATES.length - 1, (i < 0 ? 3 : i) + dir))]!);
  };

  // ---- vincular ------------------------------------------------------------------
  const link = () => {
    const o = camB.time() - camA.time();
    offsetRef.current = o;
    setOffset(o);
    setLinked(true);
    flash(`Vinculadas: B = A ${o >= 0 ? "+" : "−"} ${Math.abs(o).toFixed(3)} s`);
  };
  const unlink = () => {
    setLinked(false);
    flash("Desvinculadas — cada câmera anda sozinha.");
  };
  const linkByClock = () => {
    if (!clockA || !clockB) return;
    const o = clockSyncOffset(clockA, clockB);
    offsetRef.current = o;
    setOffset(o);
    setLinked(true);
    linkedRef.current = true;
    syncOther(active);
    flash("Vinculadas pelo relógio das câmeras.");
  };
  /** Ajuste fino com as duas vinculadas: move B em relação a A. */
  const nudge = (delta: number) => {
    const o = Math.round((offsetRef.current + delta) * 1e6) / 1e6;
    offsetRef.current = o;
    setOffset(o);
    camB.seek(camA.time() + o);
  };

  // ---- relógio fino + correção de deriva --------------------------------------
  const anyPlaying = camA.playing || camB.playing;
  useEffect(() => {
    if (!anyPlaying) return;
    let raf = 0;
    let last = 0;
    const tick = (now: number) => {
      const va = camA.ref.current;
      const vb = camB.ref.current;
      if (va && vb) {
        if (linkedRef.current) {
          const master = active === "a" ? va : vb;
          const slave = active === "a" ? camB : camA;
          const sv = slave.ref.current!;
          if (!master.paused) {
            const target = counterpart(active, master.currentTime);
            if (!slave.inRange(target)) {
              if (!sv.paused) sv.pause();
            } else {
              if (!sv.seeking && Math.abs(sv.currentTime - target) > DRIFT_S) sv.currentTime = target;
              if (sv.paused) void sv.play().catch(() => {});
            }
          }
        }
        if (now - last > 33) {
          last = now;
          camA.setT(va.currentTime);
          camB.setT(vb.currentTime);
        }
      }
      raf = requestAnimationFrame(tick);
    };
    raf = requestAnimationFrame(tick);
    return () => cancelAnimationFrame(raf);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [anyPlaying, active]);

  // ---- coletar ---------------------------------------------------------------------
  const titleFor = (s: Side, t: number, pair: boolean) => {
    const c = clockOf(s);
    const cam_ = c ? ` · câmera ${formatClock(cameraClockAt(t, c), true)}` : "";
    return `${pair ? "Par · " : ""}${s.toUpperCase()} ${formatDuration(t)}${cam_}`;
  };
  const collectOne = async (s: Side, pair: boolean) => {
    const c = cam(s);
    const t = c.time();
    const input = { media_hash: c.media.sha256, timestamp_s: t, event_id: null, title: titleFor(s, t, pair) };
    if (s === "a") {
      await collectFrameA(workspacePath, input);
    } else {
      const r = await commands.collectVideoFrame(workspacePath, input);
      setBFrames((fs) =>
        [...fs, r.storyboard_frame].sort((x, y) => x.requested_timestamp_s - y.requested_timestamp_s),
      );
    }
  };
  const collect = async (s: Side = active) => {
    try {
      if (linkedRef.current) {
        pauseAll();
        await collectOne("a", true);
        if (camB.inRange(camB.time())) await collectOne("b", true);
        flash("Par coletado — um quadro no storyboard de cada vídeo.");
      } else {
        await collectOne(s, false);
        flash(`Quadro de ${s.toUpperCase()} coletado.`);
      }
    } catch (err) {
      flash(`Falha ao coletar: ${toSicroError(err).message}`, 6000);
    }
  };

  // ---- tela cheia do comparador ----------------------------------------------
  useEffect(() => {
    const onChange = () => setFullscreen(document.fullscreenElement === wrapRef.current);
    document.addEventListener("fullscreenchange", onChange);
    return () => {
      document.removeEventListener("fullscreenchange", onChange);
      if (document.fullscreenElement) void document.exitFullscreen().catch(() => {});
    };
  }, []);
  const toggleFullscreen = () => {
    if (document.fullscreenElement) void document.exitFullscreen().catch(() => {});
    else void wrapRef.current?.requestFullscreen?.().catch(() => {});
  };

  // ---- teclado (vale para o lado ativo; vinculadas, para as duas) ------------
  useShortcuts({
    "video.playPause": () => void togglePlay(),
    "video.playPauseK": () => void togglePlay(),
    "video.forward": () => void togglePlay(),
    "video.prevFrame": () => step(-1),
    "video.nextFrame": () => step(1),
    "video.speedUp": () => cycleRate(1),
    "video.speedDown": () => cycleRate(-1),
    "video.seekStart": () => seekSide(active, cam(active).start),
    "video.fullscreen": toggleFullscreen,
    "video.magnifyIn": () => cam(active).magnifier.zoomAt(1.5),
    "video.magnifyOut": () => cam(active).magnifier.zoomAt(1 / 1.5),
    "video.magnifyReset": () => cam(active).magnifier.reset(),
    "video.adjustToggle": () => {
      const c = cam(active);
      if (!isAdjusted(c.adjust)) c.setAdjustOpen(true);
      else c.setAdjust((x) => ({ ...x, on: !x.on }));
    },
  });
  useShortcuts({ "video.collectFrame": () => void collect() }, { allowInInputs: true });

  // Setas: toque = ±1 quadro; Shift = ±1 s; Tab troca o lado ativo.
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      const el = document.activeElement as HTMLElement | null;
      if (el && (el.tagName === "INPUT" || el.tagName === "TEXTAREA" || el.isContentEditable)) return;
      if (e.key === "Tab" && !e.ctrlKey && !e.altKey) {
        e.preventDefault();
        setActive((s) => other(s));
        return;
      }
      if (e.key !== "ArrowLeft" && e.key !== "ArrowRight") return;
      if (e.ctrlKey || e.altKey || e.metaKey) return;
      e.preventDefault();
      const dir = e.key === "ArrowRight" ? 1 : -1;
      if (e.shiftKey) {
        if (linkedRef.current) pauseAll();
        else cam(active).ref.current?.pause();
        seekSide(active, cam(active).time() + dir);
      } else step(dir);
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  });

  const menu = useContextMenu();
  const paneMenu = (e: React.MouseEvent, s: Side) => {
    setActive(s);
    const c = cam(s);
    const r = e.currentTarget.getBoundingClientRect();
    const cx = e.clientX - r.left;
    const cy = e.clientY - r.top;
    const items: MenuItem[] = [
      { label: c.playing ? "Pausar" : "Tocar", shortcut: "Espaço", onSelect: () => void togglePlay(s) },
      {
        label: linked ? "Coletar o par (A e B)" : `Coletar frame de ${s.toUpperCase()}`,
        shortcut: "Ctrl+1",
        onSelect: () => void collect(s),
      },
      "separator",
      linked
        ? { label: "Desvincular as câmeras", onSelect: unlink }
        : { label: "Vincular aqui (mesmo instante nas duas)", onSelect: link },
      ...(bothClocks ? [{ label: "Vincular pelo relógio", onSelect: linkByClock }] : []),
      "separator",
      c.magnifier.scale > 1
        ? { label: "Lupa: imagem inteira", shortcut: "Ctrl+0", onSelect: c.magnifier.reset }
        : { label: "Lupa: aproximar aqui", shortcut: "roda", onSelect: () => c.magnifier.zoomAt(2, cx, cy) },
      { label: "Ajustes de tela…", onSelect: () => c.setAdjustOpen(true) },
      "separator",
      { label: fullscreen ? "Sair da tela cheia" : "Tela cheia", shortcut: "F", onSelect: toggleFullscreen },
    ];
    menu.open(e, items);
  };

  const renderPane = (s: Side, c: Cam) => {
    const clock = clockOf(s);
    const outOfRange = linked && s !== active && !c.inRange(counterpart(active, cam(active).t));
    const fs = frames(s);
    return (
      <section
        className={`${styles.pane} ${active === s ? styles.paneActive : ""}`}
        onPointerDownCapture={() => setActive(s)}
      >
        <header className={styles.paneHead}>
          <strong>
            {s.toUpperCase()} · {c.media.filename}
          </strong>
          <span>
            <code>{formatDuration(c.t)}</code>
            {clock ? <> · câmera <code>{formatClock(cameraClockAt(c.t, clock), true)}</code></> : " · sem relógio vinculado"}
          </span>
        </header>
        <div
          ref={c.magnifier.wrapRef}
          className={styles.videoBox}
          style={{ cursor: c.magnifier.scale > 1 ? (c.magnifier.panning ? "grabbing" : "grab") : undefined }}
          {...c.magnifier.panHandlers}
          onContextMenu={(e) => paneMenu(e, s)}
        >
          {c.adjust.gamma !== 1 && <GammaFilterDefs gamma={c.adjust.gamma} id={`sicro-gamma-${s}`} />}
          <video
            ref={c.ref}
            src={srcOf(workspacePath, c.media.relative_path)}
            className={styles.video}
            style={{
              transform: c.magnifier.transform,
              transformOrigin: "0 0",
              filter: adjustFilter(c.adjust, `sicro-gamma-${s}`),
            }}
            preload="auto"
            muted
            onDoubleClick={toggleFullscreen}
          />
          {(c.magnifier.scale > 1 || (c.adjust.on && isAdjusted(c.adjust))) && (
            <div className={styles.chips}>
              {c.magnifier.scale > 1 && (
                <button type="button" onClick={c.magnifier.reset}>
                  Lupa {c.magnifier.scale.toFixed(1).replace(".", ",")}× · ×
                </button>
              )}
              {c.adjust.on && isAdjusted(c.adjust) && (
                <button type="button" onClick={() => c.setAdjustOpen(true)}>
                  Ajuste de tela ativo
                </button>
              )}
            </div>
          )}
          {c.adjustOpen && (
            <ImageAdjustPanel value={c.adjust} onChange={c.setAdjust} onClose={() => c.setAdjustOpen(false)} />
          )}
          {outOfRange && <div className={styles.out}>Fora deste vídeo neste instante</div>}
        </div>
        <div className={styles.paneControls}>
          <button type="button" onClick={() => step(-1, s)} title="Quadro anterior (← ou ,)">
            <StepBack size={14} />
          </button>
          <button
            type="button"
            className={styles.play}
            onClick={(e) => {
              e.currentTarget.blur();
              void togglePlay(s);
            }}
            title={linked ? "Tocar / pausar as duas (Espaço)" : `Tocar / pausar ${s.toUpperCase()} (Espaço)`}
          >
            {c.playing ? <Pause size={15} /> : <Play size={15} />}
          </button>
          <button type="button" onClick={() => step(1, s)} title="Próximo quadro (→ ou .)">
            <StepForward size={14} />
          </button>
          <button
            type="button"
            className={c.adjustOpen || (c.adjust.on && isAdjusted(c.adjust)) ? styles.on : ""}
            onClick={() => c.setAdjustOpen((o) => !o)}
            title="Brilho, contraste e gama — só na tela (A compara)"
          >
            <SlidersHorizontal size={14} />
          </button>
          <span className={styles.spacer} />
          <button
            type="button"
            className={styles.collect}
            onClick={() => void collect(s)}
            title={linked ? "Coletar o par — um quadro de cada vídeo (Ctrl+1)" : `Coletar este quadro de ${s.toUpperCase()} (Ctrl+1)`}
          >
            <Camera size={13} /> {linked ? "Coletar par" : "Coletar frame"}
          </button>
        </div>
        <VideoTimeline
          duration={c.dur || c.media.duration_s || 0}
          fps={c.media.fps_declared}
          currentTime={c.t}
          events={events(s)}
          selectedEventId={null}
          shortcutsEnabled={active === s}
          onScrubStart={() => {
            if (linkedRef.current) pauseAll();
            else c.ref.current?.pause();
          }}
          onScrub={(t) => seekSide(s, t)}
          onScrubEnd={(t) => seekSide(s, t)}
          onSelectEvent={(id) => {
            const ev = events(s).find((x) => x.id === id);
            if (ev) seekSide(s, ev.timestamp_s);
          }}
          contextItems={(t) => [{ label: `Ir para ${formatDuration(t)}`, onSelect: () => seekSide(s, t) }]}
        />
        <div className={styles.strip} title="Storyboard deste vídeo — clique para ir ao quadro">
          {fs.length === 0 ? (
            <span className={styles.stripEmpty}>Nenhum quadro coletado deste vídeo.</span>
          ) : (
            fs.map((f) => {
              const src = frameSrc(workspacePath, f);
              return (
                <button
                  key={f.id}
                  type="button"
                  className={styles.stripItem}
                  onClick={() => seekSide(s, f.actual_timestamp_s ?? f.requested_timestamp_s)}
                  title={`${f.title} · ${formatDuration(f.actual_timestamp_s ?? f.requested_timestamp_s)}`}
                >
                  {src && <img src={src} alt="" loading="lazy" />}
                </button>
              );
            })
          )}
        </div>
      </section>
    );
  };

  return (
    <div ref={wrapRef} className={styles.wrap}>
      {menu.element}
      <div className={styles.toolbar}>
        {linked ? (
          <>
            <button type="button" className={styles.linkOn} onClick={unlink} title="Desvincular: cada câmera anda sozinha">
              <Link2 size={14} /> Vinculadas
            </button>
            <span className={styles.offset} title="tempo de B = tempo de A + deslocamento">
              B = A {offset >= 0 ? "+" : "−"} <code>{Math.abs(offset).toFixed(3)} s</code>
            </span>
            <button type="button" onClick={() => nudge(-1)} title="Ajuste fino: B −1 s">−1 s</button>
            <button type="button" onClick={() => nudge(-camB.frame)} title="Ajuste fino: B −1 quadro">−1 q</button>
            <button type="button" onClick={() => nudge(camB.frame)} title="Ajuste fino: B +1 quadro">+1 q</button>
            <button type="button" onClick={() => nudge(1)} title="Ajuste fino: B +1 s">+1 s</button>
          </>
        ) : (
          <>
            <button
              type="button"
              className={styles.linkBtn}
              onClick={link}
              title="Deixe as duas paradas no mesmo acontecimento e clique: o SICRO guarda a diferença"
            >
              <Link2Off size={14} /> Vincular
            </button>
            <span className={styles.hint}>
              Ache o mesmo acontecimento em cada câmera (clique numa para ativá-la; Tab troca) e vincule.
            </span>
          </>
        )}
        <button
          type="button"
          disabled={!bothClocks}
          onClick={linkByClock}
          title={bothClocks ? "Vincular pelo relógio das duas câmeras" : "Vincule o relógio da câmera nos dois vídeos para usar"}
        >
          pelo relógio
        </button>
        <span className={styles.spacer} />
        {feedback && <span className={styles.feedback}>{feedback}</span>}
        <div className={styles.rates}>
          {RATES.map((r) => (
            <button key={r} type="button" className={rate === r ? styles.rateOn : ""} onClick={() => applyRate(r)}>
              {String(r).replace(".", ",")}×
            </button>
          ))}
        </div>
        <button type="button" onClick={toggleFullscreen} title="Tela cheia (F)">
          {fullscreen ? <Minimize size={14} /> : <Maximize size={14} />}
        </button>
        <button type="button" onClick={onClose} title="Fechar a comparação">
          <X size={14} />
        </button>
      </div>
      <div className={styles.grid}>
        {renderPane("a", camA)}
        {renderPane("b", camB)}
      </div>
    </div>
  );
}
