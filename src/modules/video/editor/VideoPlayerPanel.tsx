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
 * Só de tela (nada disso altera o arquivo nem os quadros coletados):
 *   roda do mouse / Ctrl+= / Ctrl+− / Ctrl+0  lupa (arrastar move a imagem)
 *   A  liga/desliga os ajustes de brilho/contraste/gama (comparar)
 *   Ctrl+M mudo · Ctrl+↑/↓ volume (lembrado entre sessões)
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
import { useMagnifier } from "./useMagnifier";
import { useContextMenu, type MenuItem } from "@components/ContextMenu/ContextMenu";
import {
  ADJUST_DEFAULT,
  GammaFilterDefs,
  ImageAdjustPanel,
  adjustFilter,
  isAdjusted,
  type Adjust,
} from "./ImageAdjust";
import { useShortcuts } from "@core/useShortcuts";
import {
  Keyboard,
  Maximize,
  Minimize,
  SlidersHorizontal,
  Volume1,
  Volume2,
  VolumeX,
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
  /** Itens do menu de botão direito que vêm de fora (coletar, evento, trecho…). */
  menuItems?: () => MenuItem[];
  /** O vídeo tem trilha de áudio? (sem áudio o controle de volume fica inativo) */
  hasAudio?: boolean;
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

// Volume/mudo e a legenda aberta/fechada: preferência de UI por máquina.
const AUDIO_KEY = "sicro.video.audio.v1";
// v2: a legenda passou a começar escondida (v1 guardava "1" para todo mundo).
const LEGEND_KEY = "sicro.video.legend.v2";
function loadAudioPrefs(): { volume: number; muted: boolean } {
  try {
    const o = JSON.parse(localStorage.getItem(AUDIO_KEY) ?? "null") as {
      volume?: number;
      muted?: boolean;
    } | null;
    const v = typeof o?.volume === "number" ? Math.min(1, Math.max(0, o.volume)) : 1;
    return { volume: v, muted: o?.muted === true };
  } catch {
    return { volume: 1, muted: false };
  }
}
function loadLegendOpen(): boolean {
  try {
    return localStorage.getItem(LEGEND_KEY) === "1";
  } catch {
    return false;
  }
}
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
  menuItems,
  hasAudio = true,
  fullscreen = false,
  onToggleFullscreen,
}: Props) {
  const videoRef = useRef<HTMLVideoElement | null>(null);
  const [isPlaying, setIsPlaying] = useState(false);
  const [isReversing, setIsReversing] = useState(false);
  const [playbackRate, setPlaybackRate] = useState(1);
  const [error, setError] = useState<string | null>(null);
  const [notice, setNotice] = useState<string | null>(null);
  const [adjust, setAdjust] = useState<Adjust>(ADJUST_DEFAULT);
  const [adjustOpen, setAdjustOpen] = useState(false);
  const [audio, setAudio] = useState(loadAudioPrefs);
  const [legendOpen, setLegendOpen] = useState(loadLegendOpen);

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

  const magnifier = useMagnifier(src);
  const menu = useContextMenu();

  // Ajustes de imagem valem para o vídeo aberto; outro vídeo começa no original.
  useEffect(() => {
    setAdjust(ADJUST_DEFAULT);
  }, [src]);

  // Pausado, o WebKitGTK não redesenha o quadro quando o <video> muda de
  // tamanho (linha do tempo mais alta, divisória, janela): a imagem fica
  // "rasgada". Parou de mudar de tamanho → força um quadro novo: vai meio
  // milissegundo adiante e volta ao instante exato (buscar no MESMO instante
  // o WebKit ignora). O tempo final é o de antes, sem arredondar nada.
  useEffect(() => {
    const v = videoRef.current;
    if (!v || typeof ResizeObserver === "undefined") return;
    let timer: number | undefined;
    let first = true;
    const ro = new ResizeObserver(() => {
      if (first) {
        first = false;
        return;
      }
      window.clearTimeout(timer);
      timer = window.setTimeout(() => {
        if (!v.paused || v.seeking || v.readyState < 2) return;
        const t0 = v.currentTime;
        const back = () => {
          v.removeEventListener("seeked", back);
          if (v.paused) v.currentTime = t0;
        };
        v.addEventListener("seeked", back);
        v.currentTime = t0 + 0.0005;
      }, 180);
    });
    ro.observe(v);
    return () => {
      window.clearTimeout(timer);
      ro.disconnect();
    };
  }, [src]);

  // Volume/mudo: aplica no <video> e lembra.
  useEffect(() => {
    const v = videoRef.current;
    if (v) {
      v.volume = audio.volume;
      v.muted = audio.muted;
    }
    try {
      localStorage.setItem(AUDIO_KEY, JSON.stringify(audio));
    } catch {
      /* sem localStorage — só não lembra */
    }
  }, [audio, src]);
  const changeVolume = (delta: number) => {
    if (!hasAudio) {
      setNotice("Este vídeo não tem trilha de áudio.");
      return;
    }
    setAudio((a) => {
      const volume = Math.round(Math.min(1, Math.max(0, a.volume + delta)) * 100) / 100;
      setNotice(`Volume ${Math.round(volume * 100)}%`);
      return { volume, muted: false };
    });
  };
  const toggleMute = () => {
    if (!hasAudio) {
      setNotice("Este vídeo não tem trilha de áudio.");
      return;
    }
    setAudio((a) => {
      setNotice(a.muted ? `Som ligado (${Math.round(a.volume * 100)}%)` : "Mudo");
      return { ...a, muted: !a.muted };
    });
  };

  useEffect(() => {
    try {
      localStorage.setItem(LEGEND_KEY, legendOpen ? "1" : "0");
    } catch {
      /* idem */
    }
  }, [legendOpen]);

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
      "video.magnifyIn": () => magnifier.zoomAt(1.5),
      "video.magnifyOut": () => magnifier.zoomAt(1 / 1.5),
      "video.magnifyReset": magnifier.reset,
      "video.adjustToggle": () => {
        if (!isAdjusted(adjust)) {
          setAdjustOpen(true); // nada a comparar ainda: abre o painel
          return;
        }
        setAdjust((a) => ({ ...a, on: !a.on }));
        setNotice(adjust.on ? "Original (ajustes desligados)" : "Ajustes de tela ligados");
      },
      "video.mute": toggleMute,
      "video.volumeUp": () => changeVolume(0.1),
      "video.volumeDown": () => changeVolume(-0.1),
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
      <div
        ref={magnifier.wrapRef}
        className={styles.videoWrap}
        style={{ cursor: magnifier.scale > 1 ? (magnifier.panning ? "grabbing" : "grab") : undefined }}
        {...magnifier.panHandlers}
        onContextMenu={(e) => {
          const v = videoRef.current;
          const playingNow = !!v && (!v.paused || revActiveRef.current);
          // Posição do clique AGORA (o evento do React não guarda o elemento depois).
          const r = e.currentTarget.getBoundingClientRect();
          const cx = e.clientX - r.left;
          const cy = e.clientY - r.top;
          const own: MenuItem[] = [
            { label: playingNow ? "Pausar" : "Tocar", shortcut: "Espaço", onSelect: () => void togglePlay() },
            ...(onToggleFullscreen
              ? [{ label: fullscreen ? "Sair da tela cheia" : "Tela cheia", shortcut: "F", onSelect: onToggleFullscreen }]
              : []),
            "separator",
            ...(magnifier.scale > 1
              ? [{ label: "Lupa: imagem inteira", shortcut: "Ctrl+0", onSelect: magnifier.reset }]
              : [{ label: "Lupa: aproximar aqui", shortcut: "roda", onSelect: () => magnifier.zoomAt(2, cx, cy) }]),
            { label: "Ajustes de tela…", onSelect: () => setAdjustOpen(true) },
            ...(isAdjusted(adjust)
              ? [{ label: adjust.on ? "Ver o original" : "Ligar os ajustes", shortcut: "A", onSelect: () => setAdjust((a) => ({ ...a, on: !a.on })) }]
              : []),
            ...(hasAudio
              ? [{ label: audio.muted ? "Ligar o som" : "Mudo", shortcut: "Ctrl+M", onSelect: toggleMute }]
              : []),
          ];
          menu.open(e, [...own, "separator", ...(menuItems?.() ?? [])]);
        }}
      >
        {menu.element}
        {notice && <div className={styles.notice}>{notice}</div>}
        {(magnifier.scale > 1 || (adjust.on && isAdjusted(adjust))) && (
          <div className={styles.viewChips}>
            {magnifier.scale > 1 && (
              <button
                type="button"
                className={styles.viewChip}
                onClick={magnifier.reset}
                title="Imagem inteira (Ctrl+0)"
              >
                Lupa {magnifier.scale.toFixed(1).replace(".", ",")}× · arraste para mover · ×
              </button>
            )}
            {adjust.on && isAdjusted(adjust) && (
              <button
                type="button"
                className={styles.viewChip}
                onClick={() => setAdjustOpen(true)}
                title="Ajustes só de tela — A compara com o original"
              >
                Ajuste de tela ativo (não altera o vídeo)
              </button>
            )}
          </div>
        )}
        {adjustOpen && (
          <ImageAdjustPanel
            value={adjust}
            onChange={setAdjust}
            onClose={() => setAdjustOpen(false)}
          />
        )}
        {adjust.gamma !== 1 && <GammaFilterDefs gamma={adjust.gamma} />}
        {src ? (
          <video
            ref={videoRef}
            src={src}
            className={styles.video}
            style={{
              transform: magnifier.transform,
              transformOrigin: "0 0",
              filter: adjustFilter(adjust),
            }}
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
        <div className={styles.volumeGroup}>
          <button
            type="button"
            disabled={!hasAudio}
            onClick={(e) => {
              e.currentTarget.blur();
              toggleMute();
            }}
            title={
              hasAudio
                ? audio.muted
                  ? "Ligar o som (Ctrl+M)"
                  : "Mudo (Ctrl+M) · volume: Ctrl+↑/↓"
                : "Este vídeo não tem trilha de áudio"
            }
          >
            {!hasAudio || audio.muted || audio.volume === 0 ? (
              <VolumeX size={14} />
            ) : audio.volume < 0.5 ? (
              <Volume1 size={14} />
            ) : (
              <Volume2 size={14} />
            )}
          </button>
          <input
            type="range"
            min={0}
            max={1}
            step={0.05}
            disabled={!hasAudio}
            value={audio.muted ? 0 : audio.volume}
            onChange={(e) => setAudio({ volume: Number(e.target.value), muted: false })}
            onKeyDown={(e) => e.preventDefault()}
            onPointerUp={(e) => e.currentTarget.blur()}
            title={hasAudio ? `Volume ${Math.round(audio.volume * 100)}%` : "Sem áudio"}
            className={styles.volumeSlider}
          />
        </div>
        <button
          type="button"
          className={adjustOpen || (adjust.on && isAdjusted(adjust)) ? styles.toggleOn : ""}
          onClick={(e) => {
            e.currentTarget.blur();
            setAdjustOpen((o) => !o);
          }}
          title="Brilho, contraste e gama — só na tela (A compara)"
        >
          <SlidersHorizontal size={14} />
        </button>
        <button
          type="button"
          className={legendOpen ? styles.toggleOn : ""}
          onClick={(e) => {
            e.currentTarget.blur();
            setLegendOpen((o) => !o);
          }}
          title={legendOpen ? "Esconder a lista de atalhos" : "Mostrar a lista de atalhos"}
        >
          <Keyboard size={14} />
        </button>
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
      {legendOpen && (
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
        <span>
          roda no vídeo / <kbd>Ctrl</kbd>+<kbd>=</kbd> lupa · <kbd>A</kbd> ajustes
        </span>
        <span>
          <kbd>M</kbd> evento · <kbd>Ctrl</kbd>+<kbd>Shift</kbd>+<kbd>C</kbd> copiar tempo
        </span>
        <span>
          <kbd>Ctrl</kbd>+<kbd>M</kbd> mudo · <kbd>Ctrl</kbd>+<kbd>↑</kbd>/<kbd>↓</kbd> volume
        </span>
        <span className={styles.shortcutStrong}>
          <kbd>Ctrl</kbd>+<kbd>1</kbd> coletar frame · <kbd>Ctrl</kbd>+<kbd>2</kbd> sequência
        </span>
      </div>
      )}
    </div>
  );
}
