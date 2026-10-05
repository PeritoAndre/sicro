/**
 * Reprodutor de vídeo (HTMLVideoElement no WebView): só visualização e
 * navegação. A verdade pericial (metadados, quadros exatos) vem do Rust+ffmpeg.
 */

import { useEffect, useRef, useState } from "react";
import { mediaSrc } from "@core/mediaSrc";
import { formatDuration } from "./format";
import { loadPosition, savePosition } from "./resume";
import { containGeom, useMagnifier } from "./useMagnifier";
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
  /** Taxa declarada; dimensiona o passo de um quadro (≈ 1/fps). */
  fps?: number | null;
  /** Atalhos só valem com a aba do reprodutor visível. */
  active: boolean;
  onTimeUpdate: (t: number) => void;
  onDurationLoaded: (d: number) => void;
  /** Ctrl+1 — coleta o quadro atual (o pai resolve via ffmpeg). */
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

// Volume/mudo e legenda aberta/fechada: preferência de UI por máquina.
const AUDIO_KEY = "sicro.video.audio.v1";
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
/** Seta segurada além disto (ms) deixa de ser passo de quadro e vira play. */
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

  // refs lidas pelo listener de teclado (ligado uma vez; evita closures velhas)
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
  const restoredRef = useRef(false);
  const scrubRef = useRef<{ wasPlaying: boolean; pending: number | null } | null>(null);
  const lastSaveRef = useRef(0);
  /** Último tempo conhecido — ao desmontar, o <video> já pode ter zerado. */
  const lastTimeRef = useRef(0);
  // ré sintetizada
  const revActiveRef = useRef(false);
  const revRafRef = useRef<number | null>(null);
  const revLastWallRef = useRef(0);
  // toque-vs-segurar das setas
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

  // WebKitGTK: pausado, o <video> não redesenha ao mudar de tamanho e a imagem
  // "rasga". Ao parar de redimensionar, vai 0,5 ms adiante e volta ao instante
  // exato (buscar no MESMO instante o WebKit ignora).
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

  // helpers imperativos: só leem refs e setters estáveis (capturados uma vez)
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
   * Ré sintetizada (Chromium ignora playbackRate negativo). Seek para trás cai
   * no keyframe anterior e decodifica adiante — pode demorar mais que um rAF;
   * reatribuir currentTime a cada tick congelaria a imagem. Por isso só emite
   * o próximo seek quando o anterior terminou, descontando o tempo real × rate.
   */
  const startReverse = () => {
    const v = videoRef.current;
    if (!v) return;
    if (revActiveRef.current) return;
    if (!v.paused) v.pause();
    const lp0 = loopRef.current;
    if (lp0 && (v.currentTime <= lp0.a + 1e-3 || v.currentTime > lp0.b + 1e-3)) {
      v.currentTime = lp0.b; // repetindo: ré começa do fim do trecho
    } else if (v.currentTime <= startTimeRef.current + 1e-3) {
      return;
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
      if (!vid.seeking) {
        const now = performance.now();
        const owed = (Math.max(0, now - revLastWallRef.current) / 1000) * rateRef.current;
        // só quando deve ao menos um quadro: cada seek cruza uma borda real
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
      /* erros de autoplay chegam pelo evento error */
    }
  };

  const togglePlay = async () => {
    const v = videoRef.current;
    if (!v) return;
    if (revActiveRef.current) {
      stopReverse(); // em ré, o toggle vira "parar"
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

  // arrastar a régua: um seek de cada vez; o pedido novo espera o `seeked` do anterior
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

  // atalhos discretos (escopo `video`); as setas ←/→ ficam no listener manual
  // abaixo por causa do gesto toque-vs-segurar
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

  // Ctrl+1 vale mesmo com foco em campo de texto.
  useShortcuts(
    {
      "video.collectFrame": () => onCollectFrameRef.current(),
    },
    { enabled: active, allowInInputs: true },
  );

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

  useEffect(() => {
    restoredRef.current = false;
    setNotice(null);
  }, [src]);

  useEffect(() => {
    if (!notice) return;
    const id = window.setTimeout(() => setNotice(null), 5000);
    return () => clearTimeout(id);
  }, [notice]);

  // `timeupdate` vem só ~4×/s: pouco para a régua com zoom e para repetir um
  // trecho curto. Tocando, um rAF publica o tempo (~30×/s), cumpre a repetição
  // e salva a posição. Rede de segurança: o WebKit/GStreamer já "travou
  // tocando" (play ativo, tempo parado) e só destravou ao mudar a velocidade —
  // se não anda por 1 s, reaplica a velocidade e reposiciona (no máx. a cada 2 s).
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
      v.playbackRate = rateRef.current; // mantém a velocidade escolhida ao recarregar
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

  // saiu da aba do reprodutor: para qualquer movimento
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

    // Só as setas ←/→ ficam aqui: toque-vs-segurar precisa de keyup +
    // temporizador, e `useShortcuts` só trata keydown.
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
      if (e.repeat) return; // segurar é o nosso timer, não o auto-repeat do SO
      if (pressedDirRef.current !== null) return; // uma direção por vez
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
        // foi segurar: para o movimento
        if (dir === 1) videoRef.current?.pause();
        else stopReverse();
        holdActiveRef.current = null;
      } else {
        // foi toque: um quadro
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
    // Ligado uma vez: os handlers só leem refs e setters estáveis.
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
        {src && magnifier.scale > 1 && (
          <PixelLoupe wrap={magnifier.el} view={magnifier.view} filter={adjustFilter(adjust)} />
        )}
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

/**
 * Com o pixel do vídeo a ≥ 3 px de tela, desenha a área visível do quadro sem interpolação
 * (vizinho mais próximo) por cima do <video>, para enxergar o pixel real. Só redesenha
 * quando muda o quadro, a lupa ou o tamanho.
 */
function PixelLoupe({
  wrap,
  view,
  filter,
}: {
  wrap: HTMLElement | null;
  view: { s: number; tx: number; ty: number };
  filter?: string;
}) {
  const ref = useRef<HTMLCanvasElement | null>(null);
  const [on, setOn] = useState(false);
  useEffect(() => {
    let raf = 0;
    let last = "";
    const draw = () => {
      raf = requestAnimationFrame(draw);
      const c = ref.current;
      const g = containGeom(wrap);
      if (!c || !g) return;
      const k = (view.s * g.cw) / g.vw; // px de tela por pixel do vídeo
      const active = k >= 3;
      setOn(active);
      if (!active) return;
      const key = `${g.video.currentTime}|${view.s}|${view.tx}|${view.ty}|${g.W}|${g.H}`;
      if (key === last) return;
      last = key;
      const dpr = window.devicePixelRatio || 1;
      if (c.width !== Math.round(g.W * dpr) || c.height !== Math.round(g.H * dpr)) {
        c.width = Math.round(g.W * dpr);
        c.height = Math.round(g.H * dpr);
      }
      const ctx = c.getContext("2d");
      if (!ctx) return;
      ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
      ctx.imageSmoothingEnabled = false;
      ctx.clearRect(0, 0, g.W, g.H);
      const toU = (X: number) => ((X - view.tx) / view.s - g.cx) * (g.vw / g.cw);
      const toV = (Y: number) => ((Y - view.ty) / view.s - g.cy) * (g.vh / g.ch);
      const u0 = Math.max(0, Math.floor(toU(0)));
      const v0 = Math.max(0, Math.floor(toV(0)));
      const u1 = Math.min(g.vw, Math.ceil(toU(g.W)));
      const v1 = Math.min(g.vh, Math.ceil(toV(g.H)));
      if (u1 <= u0 || v1 <= v0) return;
      const dx = (u0 * (g.cw / g.vw) + g.cx) * view.s + view.tx;
      const dy = (v0 * (g.ch / g.vh) + g.cy) * view.s + view.ty;
      try {
        ctx.drawImage(g.video, u0, v0, u1 - u0, v1 - v0, dx, dy, (u1 - u0) * k, (v1 - v0) * k);
      } catch {
        /* quadro ainda não decodificado */
      }
    };
    raf = requestAnimationFrame(draw);
    return () => cancelAnimationFrame(raf);
  }, [wrap, view]);
  return (
    <canvas
      ref={ref}
      aria-hidden
      style={{
        position: "absolute",
        inset: 0,
        width: "100%",
        height: "100%",
        pointerEvents: "none",
        zIndex: 1,
        visibility: on ? "visible" : "hidden",
        filter,
      }}
    />
  );
}
