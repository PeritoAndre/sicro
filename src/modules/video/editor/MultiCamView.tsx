/**
 * MultiCamView — duas câmeras lado a lado, sincronizadas.
 *
 * A (o vídeo aberto) é o mestre; B segue com um deslocamento:
 *   tempo_B = tempo_A + offset
 * O deslocamento se ajusta à mão (±1 quadro / ±1 s de B) ou sai do relógio
 * das câmeras, quando as duas têm o vínculo feito (item "relógio da câmera").
 * Tocando, B é corrigido se desviar mais que ~3 quadros; parado, cada passo
 * reposiciona B exatamente. Só visualização — a coleta de quadros e as
 * medições continuam no editor de cada vídeo.
 */
import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import {
  Link2,
  Maximize,
  Minimize,
  Pause,
  Play,
  SkipBack,
  SkipForward,
  StepBack,
  StepForward,
  X,
} from "lucide-react";
import type { VideoClockCalibration, VideoMedia } from "@domain/video";
import { mediaSrc } from "@core/mediaSrc";
import { useShortcuts } from "@core/useShortcuts";
import { VideoTimeline } from "./VideoTimeline";
import {
  cameraClockAt,
  clockSyncOffset,
  formatClock,
  formatDuration,
  probeStartTime,
} from "./format";
import styles from "./MultiCamView.module.css";

const RATES = [0.1, 0.25, 0.5, 1, 2, 4, 8];
/** Desvio (s) tolerado em B antes de corrigir enquanto toca. */
const DRIFT_S = 0.12;

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

export function MultiCamView({ workspacePath, a, b, clocks, initialTime, onClose }: Props) {
  const vaRef = useRef<HTMLVideoElement | null>(null);
  const vbRef = useRef<HTMLVideoElement | null>(null);
  const wrapRef = useRef<HTMLDivElement | null>(null);
  const clockA = clocks.find((c) => c.media_hash === a.sha256) ?? null;
  const clockB = clocks.find((c) => c.media_hash === b.sha256) ?? null;
  const bothClocks = clockA != null && clockB != null;

  const [offset, setOffset] = useState(() => (clockA && clockB ? clockSyncOffset(clockA, clockB) : 0));
  const offsetRef = useRef(offset);
  offsetRef.current = offset;
  const [tA, setTA] = useState(initialTime);
  const [tB, setTB] = useState(initialTime + offset);
  const [durA, setDurA] = useState(a.duration_s ?? 0);
  const [durB, setDurB] = useState(b.duration_s ?? 0);
  const [playing, setPlaying] = useState(false);
  const [rate, setRate] = useState(1);
  const [fullscreen, setFullscreen] = useState(false);
  const startB = useMemo(() => probeStartTime(b.raw_probe_json), [b.raw_probe_json]);
  const frameA = a.fps_declared && a.fps_declared > 0 ? 1 / a.fps_declared : 1 / 30;
  const frameB = b.fps_declared && b.fps_declared > 0 ? 1 / b.fps_declared : 1 / 30;
  const bOut = tA + offset < startB - 1e-3 || (durB > 0 && tA + offset > durB);

  /** Reposiciona B no ponto que corresponde a A. */
  const syncB = useCallback(() => {
    const va = vaRef.current;
    const vb = vbRef.current;
    if (!va || !vb) return;
    const target = va.currentTime + offsetRef.current;
    const dur = Number.isFinite(vb.duration) ? vb.duration : Infinity;
    vb.currentTime = Math.max(0, Math.min(dur, target));
  }, []);

  const seekA = useCallback(
    (t: number) => {
      const va = vaRef.current;
      if (!va) return;
      const dur = Number.isFinite(va.duration) ? va.duration : t;
      va.currentTime = Math.max(0, Math.min(dur, t));
      setTA(va.currentTime);
      syncB();
    },
    [syncB],
  );

  const pauseBoth = () => {
    vaRef.current?.pause();
    vbRef.current?.pause();
  };
  const togglePlay = async () => {
    const va = vaRef.current;
    const vb = vbRef.current;
    if (!va || !vb) return;
    if (!va.paused) {
      pauseBoth();
      return;
    }
    syncB();
    try {
      await Promise.all([va.play(), bOut ? Promise.resolve() : vb.play()]);
    } catch {
      /* erro de mídia aparece no próprio vídeo */
    }
  };
  const step = (dir: 1 | -1) => {
    pauseBoth();
    seekA((vaRef.current?.currentTime ?? 0) + dir * frameA);
  };
  const applyRate = (r: number) => {
    setRate(r);
    if (vaRef.current) vaRef.current.playbackRate = r;
    if (vbRef.current) vbRef.current.playbackRate = r;
  };
  const cycleRate = (dir: 1 | -1) => {
    const i = RATES.indexOf(rate);
    applyRate(RATES[Math.max(0, Math.min(RATES.length - 1, (i < 0 ? 3 : i) + dir))]!);
  };
  const nudge = (delta: number) => {
    setOffset((o) => {
      const n = Math.round((o + delta) * 1e6) / 1e6;
      offsetRef.current = n;
      return n;
    });
    if (vaRef.current?.paused) syncB();
  };

  // Estado de play/tempo de A (mestre) + B acompanhando enquanto toca.
  useEffect(() => {
    const va = vaRef.current;
    if (!va) return;
    const onPlay = () => setPlaying(true);
    const onPause = () => {
      setPlaying(false);
      vbRef.current?.pause();
      setTA(va.currentTime);
      syncB();
    };
    const onSeeked = () => setTA(va.currentTime);
    const onMeta = () => {
      setDurA(va.duration);
      va.currentTime = initialTime;
    };
    va.addEventListener("play", onPlay);
    va.addEventListener("pause", onPause);
    va.addEventListener("seeked", onSeeked);
    va.addEventListener("loadedmetadata", onMeta);
    return () => {
      va.removeEventListener("play", onPlay);
      va.removeEventListener("pause", onPause);
      va.removeEventListener("seeked", onSeeked);
      va.removeEventListener("loadedmetadata", onMeta);
    };
    // initialTime só vale na abertura.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [syncB]);

  useEffect(() => {
    const vb = vbRef.current;
    if (!vb) return;
    const onMeta = () => {
      setDurB(vb.duration);
      syncB();
    };
    const onSeeked = () => setTB(vb.currentTime);
    vb.addEventListener("loadedmetadata", onMeta);
    vb.addEventListener("seeked", onSeeked);
    return () => {
      vb.removeEventListener("loadedmetadata", onMeta);
      vb.removeEventListener("seeked", onSeeked);
    };
  }, [syncB]);

  useEffect(() => {
    if (!playing) return;
    let raf = 0;
    let last = 0;
    const tick = (now: number) => {
      const va = vaRef.current;
      const vb = vbRef.current;
      if (va && vb && !va.paused) {
        const target = va.currentTime + offsetRef.current;
        const dur = Number.isFinite(vb.duration) ? vb.duration : Infinity;
        const inRange = target >= startB - 1e-3 && target <= dur;
        if (!inRange) {
          if (!vb.paused) vb.pause();
        } else {
          if (!vb.seeking && Math.abs(vb.currentTime - target) > DRIFT_S) vb.currentTime = target;
          if (vb.paused) void vb.play().catch(() => {});
        }
        if (now - last > 33) {
          last = now;
          setTA(va.currentTime);
          setTB(vb.currentTime);
        }
      }
      raf = requestAnimationFrame(tick);
    };
    raf = requestAnimationFrame(tick);
    return () => cancelAnimationFrame(raf);
  }, [playing, startB]);

  // Tela cheia do comparador inteiro.
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

  useShortcuts({
    "video.playPause": () => void togglePlay(),
    "video.playPauseK": () => void togglePlay(),
    "video.prevFrame": () => step(-1),
    "video.nextFrame": () => step(1),
    "video.speedUp": () => cycleRate(1),
    "video.speedDown": () => cycleRate(-1),
    "video.seekStart": () => seekA(probeStartTime(a.raw_probe_json)),
    "video.fullscreen": toggleFullscreen,
  });

  // Setas: toque = ±1 quadro; Shift = ±1 s (sem o "segurar" do player simples).
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.key !== "ArrowLeft" && e.key !== "ArrowRight") return;
      if (e.ctrlKey || e.altKey || e.metaKey) return;
      const el = document.activeElement as HTMLElement | null;
      if (el && (el.tagName === "INPUT" || el.tagName === "TEXTAREA" || el.isContentEditable)) return;
      e.preventDefault();
      const dir = e.key === "ArrowRight" ? 1 : -1;
      if (e.shiftKey) {
        pauseBoth();
        seekA((vaRef.current?.currentTime ?? 0) + dir);
      } else step(dir);
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  });

  const camLabel = (cal: VideoClockCalibration | null, t: number) =>
    cal ? `câmera ${formatClock(cameraClockAt(t, cal))}` : "sem relógio vinculado";
  const clockDelta =
    bothClocks && !bOut
      ? cameraClockAt(tB, clockB!) - cameraClockAt(tA, clockA!)
      : null;

  return (
    <div ref={wrapRef} className={styles.wrap}>
      <div className={styles.grid}>
        <figure className={styles.cam}>
          <div className={styles.videoBox}>
            <video
              ref={vaRef}
              src={srcOf(workspacePath, a.relative_path)}
              className={styles.video}
              preload="auto"
              muted
              onDoubleClick={toggleFullscreen}
            />
          </div>
          <figcaption>
            <strong>A · {a.filename}</strong>
            <span>
              <code>{formatDuration(tA)}</code> · {camLabel(clockA, tA)}
            </span>
          </figcaption>
        </figure>
        <figure className={styles.cam}>
          <div className={styles.videoBox}>
            <video
              ref={vbRef}
              src={srcOf(workspacePath, b.relative_path)}
              className={styles.video}
              preload="auto"
              muted
              onDoubleClick={toggleFullscreen}
            />
            {bOut && <div className={styles.out}>Fora do vídeo B neste instante</div>}
          </div>
          <figcaption>
            <strong>B · {b.filename}</strong>
            <span>
              <code>{bOut ? "—" : formatDuration(tB)}</code> · {bOut ? "—" : camLabel(clockB, tB)}
            </span>
          </figcaption>
        </figure>
      </div>

      <div className={styles.controls}>
        <button type="button" onClick={() => seekA(tA - 5)} title="−5 s">
          <SkipBack size={14} />
        </button>
        <button type="button" onClick={() => step(-1)} title="Quadro anterior (← ou ,)">
          <StepBack size={14} />
        </button>
        <button
          type="button"
          className={styles.primary}
          onClick={(e) => {
            e.currentTarget.blur();
            void togglePlay();
          }}
          title="Reproduzir / pausar as duas (Espaço)"
        >
          {playing ? <Pause size={16} /> : <Play size={16} />}
        </button>
        <button type="button" onClick={() => step(1)} title="Próximo quadro (→ ou .)">
          <StepForward size={14} />
        </button>
        <button type="button" onClick={() => seekA(tA + 5)} title="+5 s">
          <SkipForward size={14} />
        </button>
        <div className={styles.rates}>
          {RATES.map((r) => (
            <button
              key={r}
              type="button"
              className={rate === r ? styles.rateOn : ""}
              onClick={() => applyRate(r)}
            >
              {String(r).replace(".", ",")}×
            </button>
          ))}
        </div>

        <div className={styles.sync} title="Sincronia: tempo de B = tempo de A + deslocamento">
          <Link2 size={13} />
          <span>
            B = A {offset >= 0 ? "+" : "−"} <code>{Math.abs(offset).toFixed(3)} s</code>
          </span>
          <button type="button" onClick={() => nudge(-1)} title="B −1 s">
            −1 s
          </button>
          <button type="button" onClick={() => nudge(-frameB)} title="B −1 quadro">
            −1 q
          </button>
          <button type="button" onClick={() => nudge(frameB)} title="B +1 quadro">
            +1 q
          </button>
          <button type="button" onClick={() => nudge(1)} title="B +1 s">
            +1 s
          </button>
          <button
            type="button"
            disabled={!bothClocks}
            onClick={() => {
              const o = clockSyncOffset(clockA!, clockB!);
              offsetRef.current = o;
              setOffset(o);
              syncB();
            }}
            title={
              bothClocks
                ? "Alinhar pelo relógio das duas câmeras"
                : "Vincule o relógio da câmera nos dois vídeos para usar"
            }
          >
            pelo relógio
          </button>
          <button
            type="button"
            onClick={() => {
              offsetRef.current = 0;
              setOffset(0);
              syncB();
            }}
            title="Zerar o deslocamento"
          >
            zerar
          </button>
        </div>

        <button type="button" onClick={toggleFullscreen} title="Tela cheia (F)">
          {fullscreen ? <Minimize size={14} /> : <Maximize size={14} />}
        </button>
        <button type="button" onClick={onClose} title="Fechar a comparação">
          <X size={14} />
        </button>
      </div>

      <VideoTimeline
        duration={durA || a.duration_s || 0}
        fps={a.fps_declared}
        currentTime={tA}
        events={[]}
        selectedEventId={null}
        shortcutsEnabled
        onScrubStart={pauseBoth}
        onScrub={seekA}
        onScrubEnd={seekA}
        onSelectEvent={() => {}}
      />

      <div className={styles.status}>
        <span>
          A <code>{formatDuration(tA)}</code>
        </span>
        <span>
          B <code>{bOut ? "fora" : formatDuration(tB)}</code>
        </span>
        {clockDelta != null && (
          <span title="Diferença entre os relógios das duas câmeras no quadro mostrado">
            Δ relógio (B − A) <code>{clockDelta >= 0 ? "+" : "−"}{Math.abs(clockDelta).toFixed(3)} s</code>
          </span>
        )}
        <span className={styles.hint}>
          Ajuste a sincronia até o mesmo acontecimento aparecer nas duas (±1 quadro), ou use o
          relógio das câmeras.
        </span>
      </div>
    </div>
  );
}
