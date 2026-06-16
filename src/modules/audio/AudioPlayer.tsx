/**
 * AudioPlayer — player forense para o módulo Áudio (Camada 1).
 *
 * Forma de onda via Web Audio API (decodeAudioData → picos em canvas), sem
 * dependência externa. Transporte com seek preciso, velocidade, loop A‑B e
 * marcadores temporais PERSISTIDOS (timestamp + rótulo) no caso. Tudo
 * descritivo; nada interpreta o conteúdo.
 */

import {
  forwardRef,
  useCallback,
  useEffect,
  useImperativeHandle,
  useRef,
  useState,
} from "react";
import { Flag, Pause, Play, Repeat, RotateCcw, X } from "lucide-react";
import { commands } from "@core/commands";
import type { AudioMarker } from "@domain/audio";
import styles from "./AudioPlayer.module.css";

const RATES = [0.5, 0.75, 1, 1.5, 2];

export function fmtTime(t: number): string {
  if (!Number.isFinite(t) || t < 0) return "0:00.0";
  const m = Math.floor(t / 60);
  const s = Math.floor(t % 60);
  const d = Math.floor((t % 1) * 10);
  return `${m}:${s.toString().padStart(2, "0")}.${d}`;
}

interface Props {
  fileUrl: string;
  workspacePath: string;
  audioSha256: string;
  /** Chamado a cada atualização de tempo — sincroniza a degravação. */
  onTimeChange?: (t: number) => void;
}

/** Handle imperativo exposto via ref (usado pela tela de degravação). */
export interface AudioPlayerHandle {
  seekTo: (t: number) => void;
  togglePlay: () => void;
  getTime: () => number;
  /** Trecho A-B atual (ordenado), ou null se não definido. */
  getLoop: () => { a: number; b: number } | null;
}

export const AudioPlayer = forwardRef<AudioPlayerHandle, Props>(function AudioPlayer(
  { fileUrl, workspacePath, audioSha256, onTimeChange },
  ref,
) {
  const audioRef = useRef<HTMLAudioElement>(null);
  const canvasRef = useRef<HTMLCanvasElement>(null);
  const [peaks, setPeaks] = useState<number[] | null>(null);
  const [wavError, setWavError] = useState(false);
  const [playing, setPlaying] = useState(false);
  const [time, setTime] = useState(0);
  const [duration, setDuration] = useState(0);
  const [rate, setRate] = useState(1);
  const [markers, setMarkers] = useState<AudioMarker[]>([]);
  const [loopA, setLoopA] = useState<number | null>(null);
  const [loopB, setLoopB] = useState<number | null>(null);

  // Expõe controle de tempo/transporte para a tela de degravação (sync).
  useImperativeHandle(
    ref,
    () => ({
      seekTo: (t: number) => {
        const a = audioRef.current;
        if (a && a.duration > 0) {
          a.currentTime = Math.max(0, Math.min(a.duration, t));
        }
      },
      togglePlay: () => {
        const a = audioRef.current;
        if (!a) return;
        if (a.paused) void a.play();
        else a.pause();
      },
      getTime: () => audioRef.current?.currentTime ?? 0,
      getLoop: () =>
        loopA != null && loopB != null
          ? { a: Math.min(loopA, loopB), b: Math.max(loopA, loopB) }
          : null,
    }),
    [loopA, loopB],
  );

  // Reset de transporte ao trocar de arquivo.
  useEffect(() => {
    setLoopA(null);
    setLoopB(null);
    setTime(0);
    setPlaying(false);
  }, [fileUrl]);

  // Carrega marcadores persistidos do caso (por hash do áudio).
  useEffect(() => {
    let cancelled = false;
    setMarkers([]);
    void commands
      .listAudioMarkers(workspacePath, audioSha256)
      .then((ms) => {
        if (!cancelled) setMarkers(ms);
      })
      .catch(() => {
        /* best-effort */
      });
    return () => {
      cancelled = true;
    };
  }, [workspacePath, audioSha256]);

  // Decodifica os picos da forma de onda (best-effort).
  useEffect(() => {
    let cancelled = false;
    setPeaks(null);
    setWavError(false);
    void (async () => {
      try {
        const resp = await fetch(fileUrl);
        const raw = await resp.arrayBuffer();
        const Ctx =
          window.AudioContext ||
          (window as unknown as { webkitAudioContext: typeof AudioContext })
            .webkitAudioContext;
        const ctx = new Ctx();
        const audioBuf = await ctx.decodeAudioData(raw);
        const ch = audioBuf.getChannelData(0);
        const N = 700;
        const block = Math.max(1, Math.floor(ch.length / N));
        const out: number[] = [];
        for (let i = 0; i < N; i++) {
          let peak = 0;
          const start = i * block;
          for (let j = 0; j < block && start + j < ch.length; j++) {
            const v = Math.abs(ch[start + j] ?? 0);
            if (v > peak) peak = v;
          }
          out.push(peak);
        }
        void ctx.close();
        if (!cancelled) setPeaks(out);
      } catch {
        if (!cancelled) setWavError(true);
      }
    })();
    return () => {
      cancelled = true;
    };
  }, [fileUrl]);

  useEffect(() => {
    if (audioRef.current) audioRef.current.playbackRate = rate;
  }, [rate]);

  // Desenha forma de onda + progresso + marcadores + região de loop.
  useEffect(() => {
    const canvas = canvasRef.current;
    if (!canvas || !peaks) return;
    const dpr = window.devicePixelRatio || 1;
    const w = canvas.clientWidth;
    const h = canvas.clientHeight;
    if (w === 0 || h === 0) return;
    canvas.width = Math.floor(w * dpr);
    canvas.height = Math.floor(h * dpr);
    const ctx = canvas.getContext("2d");
    if (!ctx) return;
    ctx.scale(dpr, dpr);
    ctx.clearRect(0, 0, w, h);

    const accent =
      getComputedStyle(canvas).getPropertyValue("--sicro-accent").trim() ||
      "#d7a84f";
    const mid = h / 2;
    const bw = w / peaks.length;
    const progressX = duration > 0 ? (time / duration) * w : 0;

    if (loopA != null && loopB != null && duration > 0) {
      const xa = (loopA / duration) * w;
      const xb = (loopB / duration) * w;
      ctx.fillStyle = "rgba(53, 196, 122, 0.15)";
      ctx.fillRect(Math.min(xa, xb), 0, Math.abs(xb - xa), h);
    }

    for (let i = 0; i < peaks.length; i++) {
      const p = peaks[i] ?? 0;
      const bh = Math.max(1, p * (h * 0.92));
      const x = i * bw;
      ctx.fillStyle = x <= progressX ? accent : "rgba(255,255,255,0.16)";
      ctx.fillRect(x, mid - bh / 2, Math.max(1, bw - 0.5), bh);
    }

    for (const m of markers) {
      const x = duration > 0 ? (m.t_seconds / duration) * w : 0;
      ctx.fillStyle = "#5aa9e6";
      ctx.fillRect(x - 0.75, 0, 1.5, h);
    }
  }, [peaks, time, duration, markers, loopA, loopB]);

  const togglePlay = useCallback(() => {
    const a = audioRef.current;
    if (!a) return;
    if (a.paused) void a.play();
    else a.pause();
  }, []);

  const seekTo = (t: number) => {
    const a = audioRef.current;
    if (a && duration > 0) a.currentTime = Math.max(0, Math.min(duration, t));
  };

  const onCanvasClick = (e: React.MouseEvent<HTMLCanvasElement>) => {
    const canvas = canvasRef.current;
    if (!canvas || duration <= 0) return;
    const rect = canvas.getBoundingClientRect();
    seekTo(((e.clientX - rect.left) / rect.width) * duration);
  };

  const addMarker = async () => {
    try {
      const m = await commands.addAudioMarker(
        workspacePath,
        audioSha256,
        time,
        `Marcador ${markers.length + 1}`,
      );
      setMarkers((ms) => [...ms, m].sort((a, b) => a.t_seconds - b.t_seconds));
    } catch {
      /* best-effort */
    }
  };

  const removeMarker = async (id: string) => {
    try {
      await commands.deleteAudioMarker(workspacePath, id);
      setMarkers((ms) => ms.filter((x) => x.id !== id));
    } catch {
      /* best-effort */
    }
  };

  const clearMarkers = async () => {
    const ids = markers.map((m) => m.id);
    await Promise.all(
      ids.map((id) => commands.deleteAudioMarker(workspacePath, id).catch(() => {})),
    );
    setMarkers([]);
  };

  return (
    <div className={styles.player}>
      <audio
        ref={audioRef}
        src={fileUrl}
        onLoadedMetadata={(e) => setDuration(e.currentTarget.duration || 0)}
        onTimeUpdate={(e) => {
          const t = e.currentTarget.currentTime;
          setTime(t);
          onTimeChange?.(t);
          if (loopA != null && loopB != null) {
            const hi = Math.max(loopA, loopB);
            const lo = Math.min(loopA, loopB);
            if (t >= hi) seekTo(lo);
          }
        }}
        onPlay={() => setPlaying(true)}
        onPause={() => setPlaying(false)}
        onEnded={() => setPlaying(false)}
      />

      <canvas
        ref={canvasRef}
        className={styles.wave}
        onClick={onCanvasClick}
        title="Clique para posicionar"
      />
      {wavError && (
        <p className={styles.waveHint}>
          Forma de onda indisponível para este arquivo (a reprodução continua
          funcionando).
        </p>
      )}

      <div className={styles.transport}>
        <button
          type="button"
          className={styles.playBtn}
          onClick={togglePlay}
          aria-label={playing ? "Pausar" : "Reproduzir"}
        >
          {playing ? <Pause size={16} /> : <Play size={16} />}
        </button>
        <span className={styles.clock}>
          {fmtTime(time)} <span className={styles.clockDim}>/ {fmtTime(duration)}</span>
        </span>

        <div className={styles.spacer} />

        <div className={styles.rateBox} title="Velocidade">
          {RATES.map((r) => (
            <button
              key={r}
              type="button"
              className={`${styles.rateBtn} ${rate === r ? styles.rateActive : ""}`}
              onClick={() => setRate(r)}
            >
              {r}×
            </button>
          ))}
        </div>

        <button
          type="button"
          className={styles.ctlBtn}
          onClick={() => void addMarker()}
          title="Marcar tempo atual"
        >
          <Flag size={13} /> Marcar
        </button>
        <button type="button" className={styles.ctlBtn} onClick={() => setLoopA(time)} title="Início do loop">
          A
        </button>
        <button type="button" className={styles.ctlBtn} onClick={() => setLoopB(time)} title="Fim do loop">
          B
        </button>
        {(loopA != null || loopB != null) && (
          <button
            type="button"
            className={styles.ctlBtn}
            onClick={() => {
              setLoopA(null);
              setLoopB(null);
            }}
            title="Limpar loop"
          >
            <Repeat size={13} /> <X size={11} />
          </button>
        )}
      </div>

      {markers.length > 0 && (
        <div className={styles.markers}>
          <div className={styles.markersHead}>
            <Flag size={12} /> Marcadores
            <button
              type="button"
              className={styles.clearMarkers}
              onClick={() => void clearMarkers()}
              title="Limpar marcadores"
            >
              <RotateCcw size={11} /> limpar
            </button>
          </div>
          <div className={styles.markerList}>
            {markers.map((m) => (
              <div key={m.id} className={styles.markerRow}>
                <button
                  type="button"
                  className={styles.markerSeek}
                  onClick={() => seekTo(m.t_seconds)}
                >
                  <span className={styles.markerTime}>{fmtTime(m.t_seconds)}</span>
                  <span className={styles.markerLabel}>{m.label}</span>
                </button>
                <button
                  type="button"
                  className={styles.markerDel}
                  onClick={() => void removeMarker(m.id)}
                  aria-label="Remover marcador"
                >
                  <X size={12} />
                </button>
              </div>
            ))}
          </div>
        </div>
      )}
    </div>
  );
});
