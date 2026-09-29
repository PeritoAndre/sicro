/**
 * VideoPlayerPanel — HTMLVideoElement embedded in the Tauri WebView.
 *
 * The asset is served via Tauri's asset protocol (`convertFileSrc`) — on
 * Linux via file:// instead, see `@core/mediaSrc`. Why
 * HTMLVideoElement is the right starting point in 2026:
 *   - Chromium (the WebView) plays H.264/AAC inside MP4/MOV out of the
 *     box on Windows;
 *   - the perito's most common footage is MP4 from phones / dashcams;
 *   - the API is well-known (`currentTime`, `play()`, `pause()`,
 *     `requestVideoFrameCallback`);
 *   - everything we don't trust the player for (metadata, frame
 *     extraction) goes through Rust+ffmpeg, as the lab decided.
 *
 * Keyboard (active only while the "Reprodutor" tab is visible and no text
 * field is focused):
 *   →/←  tap = ±1 frame · hold = reproduz à frente / em ré (até o início)
 *   ,/.  frame anterior / próximo            Shift+→/←  ±1 s
 *   Espaço/K  play-pause   Home/End  1º quadro / fim
 *   J/L  ré / frente — repetir acelera 1× → 2× → 4× → 8× (como nos editores)
 *   ↑/↓  velocidade (0,1× … 8×)   Ctrl+1  coletar frame
 *   F / duplo clique no vídeo  tela cheia (quem executa é o VideoAnalysisView)
 *
 * Também: repete o trecho entrada/saída (`loop`), publica o tempo ~30×/s
 * enquanto toca (a linha do tempo com zoom precisa), junta os seeks de quem
 * arrasta a régua e lembra a posição de cada vídeo (abre no 1º quadro real).
 *
 * Reverse playback is synthesized with requestAnimationFrame (Chromium
 * ignores a negative playbackRate), so it is an *approximation* for visual
 * scrubbing — the pericial truth (exact frames/instants) always comes from
 * ffmpeg via "Coletar frame".
 *
 * Known limits surfaced in `docs/archive/SPIKE_F_VIDEO_ENGINE_RELATORIO.md`:
 * AVI / MKV with unusual codecs may NOT play. The status bar shows the
 * player's `error` event with a clear message if that happens.
 */

import { useEffect, useRef, useState } from "react";
import { mediaSrc } from "@core/mediaSrc";
import { formatDuration } from "./format";
import { loadPosition, savePosition } from "./resume";
import { useShortcuts } from "@core/useShortcuts";
import {
  Maximize,
  Minimize,
  Pause,
  Play,
  Rewind,
  SkipBack,
  SkipForward,
  StepBack,
  StepForward,
  AlertTriangle,
} from "lucide-react";
import styles from "./VideoPlayerPanel.module.css";

interface Props {
  workspacePath: string;
  relativePath: string;
  /** Declared frame rate, used to size a frame step (≈ 1/fps). */
  fps?: number | null;
  /** Only handle keyboard shortcuts while the player tab is visible. */
  active: boolean;
  onTimeUpdate: (t: number) => void;
  onDurationLoaded: (d: number) => void;
  /** Ctrl+1 — collect the current frame (resolved upstream via ffmpeg). */
  onCollectFrame: () => void;
  /** Entrega ao pai os comandos imperativos (seek, arrastar, tempo atual). */
  registerController: (c: PlayerController) => void;
  /** SHA-256 da mídia — chave do "continuar de onde parou". */
  mediaKey: string;
  /** Instante do 1º quadro (vídeo recortado pode começar depois do 0:00). */
  startTime?: number;
  /** Trecho a repetir (já ordenado), ou null. */
  loop?: { a: number; b: number } | null;
  /** Tela cheia do reprodutor — estado e alternância vêm do VideoAnalysisView. */
  fullscreen?: boolean;
  onToggleFullscreen?: () => void;
}

export interface PlayerController {
  seek: (seconds: number) => void;
  /** Arrastar na régua: pausa (e lembra se tocava), segue o dedo, retoma. */
  scrubStart: () => void;
  scrub: (seconds: number) => void;
  scrubEnd: (seconds: number) => void;
  getTime: () => number;
}

const PLAYBACK_RATES = [0.1, 0.25, 0.5, 1, 2, 4, 8];
/** J/L repetidos: 1× → 2× → 4× → 8× (fica em 8×). */
const nextShuttle = (r: number) => (r < 1 ? 1 : Math.min(8, r * 2));
/** Hold longer than this (ms) and an arrow switches from frame-step to play. */
const HOLD_MS = 300;
const DEFAULT_FPS = 30;

export function VideoPlayerPanel({
  workspacePath,
  relativePath,
  fps,
  active,
  onTimeUpdate,
  onDurationLoaded,
  onCollectFrame,
  registerController,
  mediaKey,
  startTime = 0,
  loop = null,
  fullscreen = false,
  onToggleFullscreen,
}: Props) {
  const videoRef = useRef<HTMLVideoElement | null>(null);
  const [isPlaying, setIsPlaying] = useState(false);
  const [isReversing, setIsReversing] = useState(false);
  const [playbackRate, setPlaybackRate] = useState(1);
  const [error, setError] = useState<string | null>(null);
  const [notice, setNotice] = useState<string | null>(null);

  // --- refs read by the single bound key listener (avoid stale closures) ---
  const activeRef = useRef(active);
  const fpsRef = useRef(fps ?? null);
  const rateRef = useRef(1);
  const onCollectFrameRef = useRef(onCollectFrame);
  const onTimeUpdateRef = useRef(onTimeUpdate);
  onTimeUpdateRef.current = onTimeUpdate;
  const loopRef = useRef(loop);
  loopRef.current = loop;
  const startTimeRef = useRef(startTime);
  startTimeRef.current = startTime;
  const mediaKeyRef = useRef(mediaKey);
  mediaKeyRef.current = mediaKey;
  // posição restaurada uma vez por mídia; arrastar a régua
  const restoredRef = useRef(false);
  const scrubRef = useRef<{ wasPlaying: boolean; pending: number | null } | null>(null);
  const lastSaveRef = useRef(0);
  /** Último tempo conhecido — ao desmontar, o <video> já pode ter zerado. */
  const lastTimeRef = useRef(0);
  // reverse-playback state — paced by the decoder (one seek at a time)
  const revActiveRef = useRef(false);
  const revRafRef = useRef<number | null>(null);
  const revLastWallRef = useRef(0);
  // tap-vs-hold state for the arrow keys
  const pressedDirRef = useRef<null | 1 | -1>(null);
  const holdTimerRef = useRef<number | null>(null);
  const holdActiveRef = useRef<null | 1 | -1>(null);

  useEffect(() => {
    fpsRef.current = fps ?? null;
  }, [fps]);
  useEffect(() => {
    onCollectFrameRef.current = onCollectFrame;
  }, [onCollectFrame]);

  const src = (() => {
    try {
      const sep = workspacePath.includes("\\") ? "\\" : "/";
      const abs = `${workspacePath}${sep}${relativePath.replace(/\//g, sep)}`;
      return mediaSrc(abs);
    } catch {
      return null;
    }
  })();

  // ---- imperative helpers (only read refs + stable setState; safe to
  // capture once inside the key listener) --------------------------------
  const frameDur = () => {
    const f = fpsRef.current;
    return f && f > 0 ? 1 / f : 1 / DEFAULT_FPS;
  };

  const seekTo = (seconds: number) => {
    const v = videoRef.current;
    if (!v) return;
    v.currentTime = Math.max(0, Math.min(seconds, v.duration || seconds));
  };

  const seekBy = (delta: number) => {
    const v = videoRef.current;
    if (!v) return;
    v.currentTime = Math.max(0, Math.min(v.duration || 0, v.currentTime + delta));
  };

  const stopReverse = () => {
    revActiveRef.current = false;
    if (revRafRef.current != null) {
      cancelAnimationFrame(revRafRef.current);
      revRafRef.current = null;
    }
    setIsReversing(false);
  };

  /**
   * Reverse is synthesized by walking `currentTime` backward. The catch:
   * a backward seek to a non-keyframe position forces the decoder to jump
   * to the previous keyframe and decode forward, which can take far longer
   * than one animation frame. A naive 60 Hz rAF loop that re-assigns
   * `currentTime` every tick would keep superseding the still-pending seek,
   * so the painted frame FREEZES until `currentTime` re-enters a
   * keyframe-dense region (exactly the "stuck until ~3 s" symptom).
   *
   * Fix: a persistent rAF that only issues the next backward seek when the
   * previous one finished (`!video.seeking`), and that accumulates elapsed
   * wall-time so it steps back by the time actually elapsed × rate. This
   * holds a true ~1× average where the decoder keeps up and degrades to
   * coarser-but-always-moving steps where seeks are slow — never frozen,
   * never faster than real time. MAX_REVERSE_STEP_S caps a jump after a
   * long stall (e.g. the tab was backgrounded).
   */
  const startReverse = () => {
    const v = videoRef.current;
    if (!v) return;
    if (revActiveRef.current) return; // already reversing
    if (!v.paused) v.pause();
    const lp0 = loopRef.current;
    if (lp0 && (v.currentTime <= lp0.a + 1e-3 || v.currentTime > lp0.b + 1e-3)) {
      v.currentTime = lp0.b; // repetindo: ré começa do fim do trecho
    } else if (v.currentTime <= startTimeRef.current + 1e-3) {
      return; // already at the first frame
    }
    revActiveRef.current = true;
    setIsReversing(true);
    revLastWallRef.current = performance.now();

    const MAX_REVERSE_STEP_S = 0.5;

    const loop = () => {
      if (!revActiveRef.current) {
        revRafRef.current = null;
        return;
      }
      const vid = videoRef.current;
      if (!vid) {
        stopReverse();
        return;
      }
      // Only advance once the previous backward seek has actually landed.
      if (!vid.seeking) {
        const now = performance.now();
        const owed = (Math.max(0, now - revLastWallRef.current) / 1000) * rateRef.current;
        // Wait until at least one frame is owed, so each seek crosses a real
        // frame boundary; the wall clock keeps accumulating until then.
        if (owed >= frameDur()) {
          revLastWallRef.current = now;
          const step = Math.min(owed, MAX_REVERSE_STEP_S);
          const next = vid.currentTime - step;
          const lp = loopRef.current;
          if (lp && next <= lp.a) {
            vid.currentTime = lp.b; // trecho em repetição: volta ao fim
          } else if (next <= startTimeRef.current) {
            vid.currentTime = startTimeRef.current;
            stopReverse();
            return;
          } else {
            vid.currentTime = next;
          }
        }
      }
      revRafRef.current = requestAnimationFrame(loop);
    };
    revRafRef.current = requestAnimationFrame(loop);
  };

  const playForward = async () => {
    const v = videoRef.current;
    if (!v) return;
    stopReverse();
    const lp = loopRef.current;
    if (lp && (v.currentTime < lp.a - 1e-3 || v.currentTime >= lp.b - 1e-3)) {
      v.currentTime = lp.a; // repetindo: começa do início do trecho
    }
    try {
      await v.play();
    } catch {
      /* autoplay/permission errors surface via the error event */
    }
  };

  const togglePlay = async () => {
    const v = videoRef.current;
    if (!v) return;
    if (revActiveRef.current) {
      stopReverse(); // reversing → treat the toggle as "stop"
      return;
    }
    if (v.paused) await playForward();
    else v.pause();
  };

  const frameStep = (direction: 1 | -1) => {
    const v = videoRef.current;
    if (!v) return;
    stopReverse();
    if (!v.paused) v.pause();
    seekBy(direction * frameDur());
  };

  const applyRate = (r: number) => {
    const v = videoRef.current;
    if (v) v.playbackRate = r;
    rateRef.current = r;
    setPlaybackRate(r);
  };

  const cycleRate = (dir: 1 | -1) => {
    const idx = PLAYBACK_RATES.indexOf(rateRef.current);
    const ni = Math.max(
      0,
      Math.min(
        PLAYBACK_RATES.length - 1,
        (idx < 0 ? PLAYBACK_RATES.indexOf(1) : idx) + dir,
      ),
    );
    applyRate(PLAYBACK_RATES[ni]!);
  };

  // J/L no estilo dos editores: parado → 1×; repetir na mesma direção dobra
  // até 8×; a direção oposta recomeça em 1×. K/Espaço pausa.
  const shuttleForward = () => {
    const v = videoRef.current;
    if (!v) return;
    if (revActiveRef.current || v.paused) {
      stopReverse();
      applyRate(1);
      void playForward();
      return;
    }
    applyRate(nextShuttle(rateRef.current));
  };
  const shuttleReverse = () => {
    if (revActiveRef.current) {
      applyRate(nextShuttle(rateRef.current));
      return;
    }
    applyRate(1);
    startReverse();
  };

  // ---- arrastar a régua: um seek de cada vez; o último pedido espera o
  // anterior terminar (evento `seeked`) em vez de empilhar -----------------
  const scrubStart = () => {
    const v = videoRef.current;
    if (!v) return;
    stopReverse();
    scrubRef.current = { wasPlaying: !v.paused, pending: null };
    if (!v.paused) v.pause();
  };
  const scrub = (t: number) => {
    const v = videoRef.current;
    if (!v) return;
    if (v.seeking && scrubRef.current) {
      scrubRef.current.pending = t;
      return;
    }
    seekTo(t);
  };
  const scrubEnd = (t: number) => {
    const s = scrubRef.current;
    scrubRef.current = null;
    seekTo(t);
    if (s?.wasPlaying) void playForward();
  };

  // ---- atalhos discretos do reprodutor (customizáveis, escopo `video`) ---
  //
  // Só disparam enquanto a aba "Reprodutor" está visível (`enabled: active`).
  // O guard padrão de inputs evita que Espaço/K/J/L atrapalhem a digitação
  // nos painéis laterais (título de evento, etc.). As setas ←/→ ficam no
  // listener manual acima (gesto toque-vs-segurar).
  useShortcuts(
    {
      "video.playPause": () => void togglePlay(),
      "video.playPauseK": () => void togglePlay(),
      "video.reverse": shuttleReverse,
      "video.forward": shuttleForward,
      "video.prevFrame": () => frameStep(-1),
      "video.nextFrame": () => frameStep(1),
      "video.seekStart": () => {
        stopReverse();
        seekTo(startTimeRef.current); // 1º quadro real
      },
      "video.seekEnd": () => {
        stopReverse();
        if (videoRef.current) seekTo(videoRef.current.duration || 0);
      },
      "video.speedUp": () => cycleRate(1),
      "video.speedDown": () => cycleRate(-1),
    },
    { enabled: active },
  );

  // Coletar frame (Ctrl+1) — chord deliberado: vale mesmo com foco em campo.
  useShortcuts(
    {
      "video.collectFrame": () => onCollectFrameRef.current(),
    },
    { enabled: active, allowInInputs: true },
  );

  // ---- wiring: expose the controller to the parent ----------------------
  useEffect(() => {
    registerController({
      seek: (seconds: number) => {
        stopReverse();
        seekTo(seconds);
      },
      scrubStart,
      scrub,
      scrubEnd,
      getTime: () => videoRef.current?.currentTime ?? 0,
    });
    // Handlers só leem refs — a primeira closure continua correta.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [registerController]);

  // ---- nova mídia: restaurar a posição de novo ---------------------------
  useEffect(() => {
    restoredRef.current = false;
    setNotice(null);
  }, [src]);

  // Aviso curto sobre o vídeo (retomada / 1º quadro) — some sozinho.
  useEffect(() => {
    if (!notice) return;
    const id = window.setTimeout(() => setNotice(null), 5000);
    return () => clearTimeout(id);
  }, [notice]);

  // ---- relógio fino + repetição do trecho enquanto toca -------------------
  // `timeupdate` só vem ~4×/s: pouco para a régua com zoom e para repetir um
  // trecho curto sem passar do ponto. Enquanto toca, um rAF publica o tempo
  // (~30×/s), cumpre a repetição e salva a posição de tempos em tempos.
  //
  // Rede de segurança: numa sessão o WebKit/GStreamer "travou tocando" (play
  // ativo, tempo parado) e só destravou ao mudar a velocidade. Se o tempo não
  // anda por 1 s tocando (sem seek pendente), reaplica a velocidade e
  // reposiciona no mesmo ponto — no máximo a cada 2 s.
  useEffect(() => {
    if (!isPlaying) return;
    let raf = 0;
    let lastPub = 0;
    let stallT = -1;
    let stallSince = performance.now();
    let lastNudge = 0;
    const tick = (now: number) => {
      const v = videoRef.current;
      if (v && !v.paused) {
        if (v.seeking || v.readyState < 2 || Math.abs(v.currentTime - stallT) > 1e-4) {
          stallT = v.currentTime;
          stallSince = now;
        } else if (now - stallSince > 1000 && now - lastNudge > 2000) {
          lastNudge = now;
          stallSince = now;
          const r = v.playbackRate;
          v.playbackRate = r === 1 ? 0.999 : 1;
          v.playbackRate = r;
          v.currentTime = v.currentTime;
        }
        const lp = loopRef.current;
        if (lp && v.currentTime >= lp.b) v.currentTime = lp.a;
        lastTimeRef.current = v.currentTime;
        if (now - lastPub >= 33) {
          lastPub = now;
          onTimeUpdateRef.current(v.currentTime);
        }
        if (now - lastSaveRef.current >= 2000) {
          lastSaveRef.current = now;
          savePosition(mediaKeyRef.current, v.currentTime);
        }
      }
      raf = requestAnimationFrame(tick);
    };
    raf = requestAnimationFrame(tick);
    return () => cancelAnimationFrame(raf);
  }, [isPlaying]);

  // Ao sair do vídeo (voltar / trocar de mídia), guarda onde parou — com a
  // chave DESTE vídeo (a ref já pode apontar para o próximo).
  useEffect(() => {
    const key = mediaKey;
    return () => {
      if (restoredRef.current) savePosition(key, lastTimeRef.current);
    };
  }, [src, mediaKey]);

  // ---- media element event listeners ------------------------------------
  useEffect(() => {
    const v = videoRef.current;
    if (!v) return;
    const onPlay = () => setIsPlaying(true);
    const onPause = () => {
      setIsPlaying(false);
      if (restoredRef.current) savePosition(mediaKeyRef.current, v.currentTime);
    };
    const onTime = () => {
      lastTimeRef.current = v.currentTime;
      onTimeUpdate(v.currentTime);
    };
    const onSeeked = () => {
      lastTimeRef.current = v.currentTime;
      onTimeUpdate(v.currentTime);
      // arrastando: aplica o último pedido que chegou durante este seek
      const s = scrubRef.current;
      if (s && s.pending != null) {
        const p = s.pending;
        s.pending = null;
        seekTo(p);
        return;
      }
      // parado e navegando quadro a quadro: salva (no máx. 2×/s)
      const now = performance.now();
      if (!s && restoredRef.current && now - lastSaveRef.current >= 500) {
        lastSaveRef.current = now;
        savePosition(mediaKeyRef.current, v.currentTime);
      }
    };
    const onMeta = () => {
      onDurationLoaded(v.duration);
      v.playbackRate = rateRef.current; // keep the chosen rate across loads
      if (restoredRef.current) return;
      restoredRef.current = true;
      // Continua de onde parou; senão abre no 1º quadro (se houver vazio antes).
      const st = startTimeRef.current;
      const saved = loadPosition(mediaKeyRef.current);
      const dur = Number.isFinite(v.duration) ? v.duration : Infinity;
      if (saved != null && saved > st + 0.5 && saved < dur - 0.5) {
        v.currentTime = saved;
        setNotice(`Continuando de ${formatDuration(saved)} · Home volta ao 1º quadro`);
      } else if (st > 0.05) {
        v.currentTime = st;
        setNotice(
          `Este vídeo começa em ${formatDuration(st)} — antes disso não há quadros (Home volta aqui).`,
        );
      }
    };
    const onErr = () => {
      const code = v.error?.code;
      setError(
        code != null
          ? `Falha ao reproduzir (MediaError code ${code}). Codec pode não ser suportado pelo WebView.`
          : "Falha ao reproduzir o vídeo.",
      );
    };
    v.addEventListener("play", onPlay);
    v.addEventListener("pause", onPause);
    v.addEventListener("timeupdate", onTime);
    v.addEventListener("seeked", onSeeked);
    v.addEventListener("loadedmetadata", onMeta);
    v.addEventListener("error", onErr);
    return () => {
      v.removeEventListener("play", onPlay);
      v.removeEventListener("pause", onPause);
      v.removeEventListener("timeupdate", onTime);
      v.removeEventListener("seeked", onSeeked);
      v.removeEventListener("loadedmetadata", onMeta);
      v.removeEventListener("error", onErr);
    };
  }, [onTimeUpdate, onDurationLoaded]);

  // ---- track tab visibility; stop motion when we leave the player tab ----
  useEffect(() => {
    activeRef.current = active;
    if (!active) {
      if (holdTimerRef.current != null) {
        clearTimeout(holdTimerRef.current);
        holdTimerRef.current = null;
      }
      pressedDirRef.current = null;
      holdActiveRef.current = null;
      stopReverse();
    }
  }, [active]);

  // ---- keyboard shortcuts (bound once) ----------------------------------
  useEffect(() => {
    const isTypingTarget = () => {
      const el = document.activeElement as HTMLElement | null;
      if (!el) return false;
      const tag = el.tagName;
      return (
        tag === "INPUT" ||
        tag === "TEXTAREA" ||
        tag === "SELECT" ||
        el.isContentEditable
      );
    };

    // Apenas as SETAS ←/→ continuam no listener manual: o gesto de
    // toque-vs-segurar (com keyup + temporizador) não cabe no modelo
    // customizável (só keydown). Todos os demais atalhos discretos
    // (Espaço/K, J/L, , / ., Home/End, ↑/↓, Ctrl+1) são resolvidos via
    // `useShortcuts` — veja `usePlayerShortcut*` mais abaixo.
    const onKeyDown = (e: KeyboardEvent) => {
      if (!activeRef.current) return;
      if (isTypingTarget()) return;
      if (e.ctrlKey || e.metaKey || e.altKey) return;
      if (e.key !== "ArrowRight" && e.key !== "ArrowLeft") return;

      e.preventDefault();
      const dir: 1 | -1 = e.key === "ArrowRight" ? 1 : -1;
      if (e.shiftKey) {
        seekBy(dir * 1); // ±1 s
        return;
      }
      if (e.repeat) return; // hold handled by our timer, not OS auto-repeat
      if (pressedDirRef.current !== null) return; // one direction at a time
      pressedDirRef.current = dir;
      holdTimerRef.current = window.setTimeout(() => {
        holdActiveRef.current = dir;
        if (dir === 1) void playForward();
        else startReverse();
      }, HOLD_MS);
    };

    const onKeyUp = (e: KeyboardEvent) => {
      if (e.key !== "ArrowRight" && e.key !== "ArrowLeft") return;
      const dir: 1 | -1 = e.key === "ArrowRight" ? 1 : -1;
      if (pressedDirRef.current !== dir) return;
      pressedDirRef.current = null;
      if (holdTimerRef.current != null) {
        clearTimeout(holdTimerRef.current);
        holdTimerRef.current = null;
      }
      if (holdActiveRef.current === dir) {
        // it was a hold → stop the continuous motion
        if (dir === 1) videoRef.current?.pause();
        else stopReverse();
        holdActiveRef.current = null;
      } else {
        // it was a tap → single frame step
        frameStep(dir);
      }
    };

    window.addEventListener("keydown", onKeyDown);
    window.addEventListener("keyup", onKeyUp);
    return () => {
      window.removeEventListener("keydown", onKeyDown);
      window.removeEventListener("keyup", onKeyUp);
      if (holdTimerRef.current != null) clearTimeout(holdTimerRef.current);
      if (revRafRef.current != null) cancelAnimationFrame(revRafRef.current);
    };
    // Bound once: every handler reads refs / stable setters, so the
    // first-render closures stay correct.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  const showPause = isPlaying || isReversing;

  return (
    <div className={styles.panel}>
      {error && (
        <div className={styles.errorBanner}>
          <AlertTriangle size={14} /> {error}
        </div>
      )}
      <div className={styles.videoWrap}>
        {notice && <div className={styles.notice}>{notice}</div>}
        {src ? (
          <video
            ref={videoRef}
            src={src}
            className={styles.video}
            controls={false}
            preload="metadata"
            onDoubleClick={onToggleFullscreen}
          />
        ) : (
          <div className={styles.placeholder}>
            Não foi possível resolver o caminho do vídeo.
          </div>
        )}
      </div>
      <div className={styles.controls}>
        <button type="button" onClick={() => seekBy(-5)} title="-5 s (Shift+← = -1 s)">
          <SkipBack size={14} />
        </button>
        <button
          type="button"
          onClick={() => frameStep(-1)}
          title="Frame anterior (← toque, ou , )"
        >
          <StepBack size={14} />
        </button>
        <button
          type="button"
          className={isReversing ? styles.reverseActive : ""}
          onClick={() => (isReversing ? stopReverse() : startReverse())}
          title="Reproduzir em ré (J — repetir acelera; ou segurar ←)"
        >
          <Rewind size={14} />
        </button>
        <button
          type="button"
          className={styles.primary}
          onClick={() => void togglePlay()}
          title={showPause ? "Pausar (Espaço / K)" : "Reproduzir (Espaço / K)"}
        >
          {showPause ? <Pause size={16} /> : <Play size={16} />}
        </button>
        <button
          type="button"
          onClick={() => frameStep(1)}
          title="Próximo frame (→ toque, ou . )"
        >
          <StepForward size={14} />
        </button>
        <button type="button" onClick={() => seekBy(5)} title="+5 s (Shift+→ = +1 s)">
          <SkipForward size={14} />
        </button>
        <div className={styles.rateGroup} role="radiogroup" aria-label="Velocidade">
          {PLAYBACK_RATES.map((r) => (
            <button
              key={r}
              type="button"
              className={`${styles.rate} ${playbackRate === r ? styles.rateActive : ""}`}
              onClick={() => applyRate(r)}
              title={`${String(r).replace(".", ",")}× (↑/↓ muda · J/L repetidos aceleram)`}
            >
              {String(r).replace(".", ",")}×
            </button>
          ))}
        </div>
        {onToggleFullscreen && (
          <button
            type="button"
            onClick={(e) => {
              // Tira o foco: senão o Espaço "clicaria" este botão em vez de dar play.
              e.currentTarget.blur();
              onToggleFullscreen();
            }}
            title={
              fullscreen
                ? "Sair da tela cheia (F ou Esc)"
                : "Tela cheia (F ou duplo clique no vídeo)"
            }
          >
            {fullscreen ? <Minimize size={14} /> : <Maximize size={14} />}
          </button>
        )}
      </div>
      <div className={styles.shortcuts}>
        <span>
          <kbd>→</kbd>/<kbd>←</kbd> quadro · segurar = play/ré
        </span>
        <span>
          <kbd>,</kbd>/<kbd>.</kbd> quadro · <kbd>Shift</kbd>+<kbd>→</kbd>/<kbd>←</kbd> ±1 s
        </span>
        <span>
          <kbd>Espaço</kbd>/<kbd>K</kbd> play · <kbd>J</kbd>/<kbd>L</kbd> ré/frente (repetir: até 8×)
        </span>
        <span>
          <kbd>↑</kbd>/<kbd>↓</kbd> velocidade · <kbd>Home</kbd>/<kbd>End</kbd>
        </span>
        <span>
          <kbd>I</kbd>/<kbd>O</kbd> trecho · <kbd>Ctrl</kbd>+<kbd>L</kbd> repetir
        </span>
        <span>
          <kbd>Shift</kbd>+<kbd>↑</kbd>/<kbd>↓</kbd> eventos · <kbd>Ctrl</kbd>+<kbd>G</kbd> ir para
        </span>
        <span>
          <kbd>=</kbd>/<kbd>−</kbd>/<kbd>0</kbd> zoom da linha
        </span>
        <span>
          <kbd>F</kbd> tela cheia
        </span>
        <span className={styles.shortcutStrong}>
          <kbd>Ctrl</kbd>+<kbd>1</kbd> coletar frame
        </span>
      </div>
    </div>
  );
}
