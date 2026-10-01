/**
 * Espectrograma interativo do áudio (tempo × frequência, intensidade em cor).
 *
 *   - Acompanha o player (linha branca) e um clique leva o player ao ponto.
 *   - Ctrl + roda do mouse: zoom no tempo, no ponto do cursor; roda sozinha ou
 *     arrastar: anda quando aproximado; duplo clique: áudio inteiro.
 *   - Shift + arrastar: marca o trecho A–B no player (para ouvir em loop,
 *     recortar ou usar como perfil de ruído no realce).
 *   - Escala log/linear, resolução da FFT, frequência máxima e contraste.
 *   - Passando o mouse: tempo, frequência e nível (dB) do ponto.
 *
 * A imagem vem do backend (Rust, FFT) só da janela visível — zoom e arraste
 * pedem de novo. Só visualização: não mexe no áudio.
 */
import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { Maximize2, ZoomIn, ZoomOut } from "lucide-react";
import { commands } from "@core/commands";
import { toSicroError } from "@core/errors";
import type { SpectroImage } from "@domain/audio";
import { buildTicks } from "@modules/video/editor/VideoTimeline";
import styles from "./AudioSpectrogram.module.css";

interface Props {
  workspacePath: string;
  audioId: string;
  duration: number;
  /** Instante atual do player (s). */
  time: number;
  onSeek: (t: number) => void;
  onSelect: (a: number, b: number) => void;
}

interface Settings {
  logFreq: boolean;
  fftSize: number;
  /** null = até a metade da taxa de amostragem. */
  fMax: number | null;
  /** Abaixo disto, preto (dB). */
  floorDb: number;
}

const KEY = "sicro.audio.spectro.v1";
const HEIGHT = 240;
const DEFAULTS: Settings = { logFreq: true, fftSize: 2048, fMax: null, floorDb: -90 };

function loadSettings(): Settings {
  try {
    return { ...DEFAULTS, ...(JSON.parse(localStorage.getItem(KEY) ?? "{}") as Partial<Settings>) };
  } catch {
    return DEFAULTS;
  }
}

/** Mapa de cor tipo "inferno" (escuro → roxo → laranja → amarelo). */
const LUT: Uint8ClampedArray = (() => {
  const stops: [number, [number, number, number]][] = [
    [0, [0, 0, 4]],
    [0.25, [66, 10, 104]],
    [0.5, [147, 38, 103]],
    [0.75, [221, 81, 58]],
    [0.9, [252, 165, 10]],
    [1, [252, 255, 164]],
  ];
  const lut = new Uint8ClampedArray(256 * 3);
  for (let i = 0; i < 256; i++) {
    const u = i / 255;
    let k = 0;
    while (k < stops.length - 2 && u > stops[k + 1]![0]) k++;
    const [u0, c0] = stops[k]!;
    const [u1, c1] = stops[k + 1]!;
    const f = (u - u0) / (u1 - u0 || 1);
    for (let j = 0; j < 3; j++) lut[i * 3 + j] = c0[j]! + (c1[j]! - c0[j]!) * f;
  }
  return lut;
})();

function decode(b64: string): Uint8Array {
  const bin = atob(b64);
  const out = new Uint8Array(bin.length);
  for (let i = 0; i < bin.length; i++) out[i] = bin.charCodeAt(i);
  return out;
}

const fmtHz = (f: number) => (f >= 1000 ? `${(f / 1000).toFixed(f >= 10000 ? 0 : 1)} kHz` : `${Math.round(f)} Hz`);
const fmtT = (s: number) => `${Math.floor(s / 60)}:${(s % 60).toFixed(2).padStart(5, "0")}`;

export function AudioSpectrogram({ workspacePath, audioId, duration, time, onSeek, onSelect }: Props) {
  const [settings, setSettings] = useState<Settings>(loadSettings);
  const [view, setView] = useState({ t0: 0, t1: Math.max(duration, 0.01) });
  const [img, setImg] = useState<{ meta: SpectroImage; data: Uint8Array } | null>(null);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [width, setWidth] = useState(800);
  const [hover, setHover] = useState<{ x: number; y: number } | null>(null);
  const [sel, setSel] = useState<{ a: number; b: number } | null>(null);
  const wrapRef = useRef<HTMLDivElement | null>(null);
  const canvasRef = useRef<HTMLCanvasElement | null>(null);
  const reqId = useRef(0);
  const drag = useRef<{ x0: number; t0: number; view: { t0: number; t1: number }; mode: "pan" | "select" | "click" } | null>(null);
  const viewRef = useRef(view);
  viewRef.current = view;

  useEffect(() => {
    try {
      localStorage.setItem(KEY, JSON.stringify(settings));
    } catch {
      /* só não lembra */
    }
  }, [settings]);

  // Áudio novo: volta ao inteiro.
  useEffect(() => {
    setView({ t0: 0, t1: Math.max(duration, 0.01) });
    setImg(null);
    setSel(null);
  }, [audioId, duration]);

  useEffect(() => {
    const el = wrapRef.current;
    if (!el) return;
    const ro = new ResizeObserver(() => setWidth(Math.max(200, Math.round(el.clientWidth))));
    ro.observe(el);
    return () => ro.disconnect();
  }, []);

  // Pede a imagem da janela visível (com folga para o zoom/arraste não pedir a cada pixel).
  useEffect(() => {
    if (!(duration > 0)) return;
    const id = ++reqId.current;
    setLoading(true);
    const timer = window.setTimeout(() => {
      commands
        .audioSpectrogramData(workspacePath, audioId, {
          t0: view.t0,
          t1: view.t1,
          width: Math.min(2048, width),
          height: HEIGHT,
          fftSize: settings.fftSize,
          logFreq: settings.logFreq,
          fMax: settings.fMax,
        })
        .then((meta) => {
          if (id !== reqId.current) return;
          setImg({ meta, data: decode(meta.data_b64) });
          setError(null);
        })
        .catch((e) => id === reqId.current && setError(toSicroError(e).message))
        .finally(() => id === reqId.current && setLoading(false));
    }, 110);
    return () => window.clearTimeout(timer);
  }, [workspacePath, audioId, duration, view.t0, view.t1, width, settings.fftSize, settings.logFreq, settings.fMax]);

  // Desenha a imagem com o contraste escolhido (sem pedir de novo ao backend).
  useEffect(() => {
    const cv = canvasRef.current;
    if (!cv || !img) return;
    const { width: w, height: h } = img.meta;
    cv.width = w;
    cv.height = h;
    const ctx = cv.getContext("2d");
    if (!ctx) return;
    const out = ctx.createImageData(w, h);
    // u8 → dB: v/255·120 − 120. Faixa visível: floorDb … 0 dB.
    const lo = ((settings.floorDb + 120) / 120) * 255;
    const scale = 255 / Math.max(1, 255 - lo);
    for (let i = 0; i < w * h; i++) {
      const v = Math.max(0, Math.min(255, Math.round((img.data[i]! - lo) * scale)));
      out.data[i * 4] = LUT[v * 3]!;
      out.data[i * 4 + 1] = LUT[v * 3 + 1]!;
      out.data[i * 4 + 2] = LUT[v * 3 + 2]!;
      out.data[i * 4 + 3] = 255;
    }
    ctx.putImageData(out, 0, 0);
  }, [img, settings.floorDb]);

  // Tocando com zoom: a janela acompanha o player.
  useEffect(() => {
    const v = viewRef.current;
    const span = v.t1 - v.t0;
    if (span >= duration - 1e-6) return;
    if (time > v.t1 || time < v.t0) {
      const t0 = Math.max(0, Math.min(duration - span, time - span * 0.1));
      setView({ t0, t1: t0 + span });
    }
  }, [time, duration]);

  const meta = img?.meta;
  const span = view.t1 - view.t0;
  const xToT = useCallback(
    (clientX: number) => {
      const r = wrapRef.current?.getBoundingClientRect();
      if (!r) return 0;
      return view.t0 + ((clientX - r.left) / r.width) * span;
    },
    [view.t0, span],
  );
  const yToF = (y: number): number | null => {
    if (!meta) return null;
    const u = 1 - y / HEIGHT;
    return meta.log_freq ? meta.f_min * Math.pow(meta.f_max / meta.f_min, u) : meta.f_min + (meta.f_max - meta.f_min) * u;
  };
  const fToY = (f: number): number | null => {
    if (!meta || f < meta.f_min || f > meta.f_max) return null;
    const u = meta.log_freq ? Math.log(f / meta.f_min) / Math.log(meta.f_max / meta.f_min) : (f - meta.f_min) / (meta.f_max - meta.f_min);
    return (1 - u) * HEIGHT;
  };

  const zoom = (factor: number, center?: number) => {
    const c = center ?? (view.t0 + view.t1) / 2;
    const minSpan = Math.min(duration, 0.05);
    const ns = Math.max(minSpan, Math.min(duration, span * factor));
    const t0 = Math.max(0, Math.min(duration - ns, c - (c - view.t0) * (ns / span)));
    setView({ t0, t1: t0 + ns });
  };
  const fit = () => setView({ t0: 0, t1: duration });

  const onWheel = (e: React.WheelEvent) => {
    if (e.ctrlKey) {
      e.preventDefault();
      zoom(e.deltaY > 0 ? 1.4 : 1 / 1.4, xToT(e.clientX));
    } else if (span < duration - 1e-6) {
      const d = (e.deltaY + e.deltaX) / 600 * span;
      const t0 = Math.max(0, Math.min(duration - span, view.t0 + d));
      setView({ t0, t1: t0 + span });
    }
  };

  const onPointerDown = (e: React.PointerEvent) => {
    if (e.button !== 0) return;
    e.preventDefault();
    e.currentTarget.setPointerCapture(e.pointerId);
    drag.current = { x0: e.clientX, t0: xToT(e.clientX), view, mode: e.shiftKey ? "select" : "click" };
  };
  const onPointerMove = (e: React.PointerEvent) => {
    const r = wrapRef.current?.getBoundingClientRect();
    if (r) setHover({ x: e.clientX - r.left, y: e.clientY - r.top });
    const d = drag.current;
    if (!d) return;
    if (d.mode === "click" && Math.abs(e.clientX - d.x0) > 4) {
      d.mode = span < duration - 1e-6 ? "pan" : "click";
    }
    if (d.mode === "pan" && r) {
      const dt = ((e.clientX - d.x0) / r.width) * (d.view.t1 - d.view.t0);
      const s = d.view.t1 - d.view.t0;
      const t0 = Math.max(0, Math.min(duration - s, d.view.t0 - dt));
      setView({ t0, t1: t0 + s });
    } else if (d.mode === "select") {
      const t = Math.max(0, Math.min(duration, xToT(e.clientX)));
      setSel({ a: Math.min(d.t0, t), b: Math.max(d.t0, t) });
    }
  };
  const onPointerUp = (e: React.PointerEvent) => {
    const d = drag.current;
    drag.current = null;
    if (!d) return;
    if (d.mode === "click") onSeek(Math.max(0, Math.min(duration, xToT(e.clientX))));
    if (d.mode === "select" && sel && sel.b - sel.a > 0.02) onSelect(sel.a, sel.b);
  };

  const ticks = useMemo(() => buildTicks(view.t0, view.t1, width, null), [view.t0, view.t1, width]);
  const freqTicks = useMemo(() => {
    if (!meta) return [];
    const cands = meta.log_freq
      ? [50, 100, 200, 500, 1000, 2000, 5000, 10000, 20000]
      : Array.from({ length: 12 }, (_, i) => (i + 1) * (meta.f_max > 12000 ? 2000 : 1000));
    return cands.filter((f) => f > meta.f_min && f < meta.f_max);
  }, [meta]);

  const hoverInfo = (() => {
    if (!hover || !img) return null;
    const t = view.t0 + (hover.x / width) * span;
    const f = yToF(hover.y);
    const col = Math.max(0, Math.min(img.meta.width - 1, Math.floor((hover.x / width) * img.meta.width)));
    const row = Math.max(0, Math.min(img.meta.height - 1, Math.floor((hover.y / HEIGHT) * img.meta.height)));
    const v = img.data[row * img.meta.width + col] ?? 0;
    return { t, f, db: (v / 255) * 120 - 120 };
  })();

  const xOf = (t: number) => ((t - view.t0) / span) * 100;
  const set = (patch: Partial<Settings>) => setSettings((s) => ({ ...s, ...patch }));

  return (
    <section className={styles.box} data-no-magnify>
      <div className={styles.bar}>
        <strong className={styles.title}>Espectrograma</strong>
        <label>
          Escala
          <select value={settings.logFreq ? "log" : "lin"} onChange={(e) => set({ logFreq: e.target.value === "log" })}>
            <option value="log">logarítmica</option>
            <option value="lin">linear</option>
          </select>
        </label>
        <label title="Janelas maiores separam melhor as frequências; menores, os instantes">
          Resolução
          <select value={settings.fftSize} onChange={(e) => set({ fftSize: Number(e.target.value) })}>
            <option value={1024}>1024 (tempo)</option>
            <option value={2048}>2048</option>
            <option value={4096}>4096</option>
            <option value={8192}>8192 (frequência)</option>
          </select>
        </label>
        <label>
          Até
          <select value={settings.fMax ?? 0} onChange={(e) => set({ fMax: Number(e.target.value) || null })}>
            <option value={4000}>4 kHz (voz)</option>
            <option value={8000}>8 kHz</option>
            <option value={0}>tudo</option>
          </select>
        </label>
        <label title="Abaixo deste nível fica preto — sobe para ver detalhe fraco, desce para limpar o fundo">
          Contraste
          <input
            type="range"
            min={-120}
            max={-30}
            step={5}
            value={settings.floorDb}
            onChange={(e) => set({ floorDb: Number(e.target.value) })}
            onPointerUp={(e) => e.currentTarget.blur()}
          />
          <code>{settings.floorDb} dB</code>
        </label>
        <span className={styles.spacer} />
        <span className={styles.readout}>
          {hoverInfo
            ? `${fmtT(hoverInfo.t)} · ${hoverInfo.f != null ? fmtHz(hoverInfo.f) : "—"} · ${hoverInfo.db.toFixed(0)} dB`
            : span < duration - 1e-6
              ? `${fmtT(view.t0)} – ${fmtT(view.t1)}`
              : "áudio inteiro"}
        </span>
        <button type="button" onClick={() => zoom(1.6)} disabled={span >= duration - 1e-6} title="Afastar">
          <ZoomOut size={13} />
        </button>
        <button type="button" onClick={() => zoom(1 / 1.6, time)} title="Aproximar no ponto do player">
          <ZoomIn size={13} />
        </button>
        <button type="button" onClick={fit} disabled={span >= duration - 1e-6} title="Áudio inteiro (duplo clique)">
          <Maximize2 size={13} />
        </button>
      </div>
      <div className={styles.plot}>
        <div className={styles.fAxis}>
          {freqTicks.map((f) => {
            const y = fToY(f);
            return y == null ? null : (
              <span key={f} style={{ top: y }}>
                {fmtHz(f)}
              </span>
            );
          })}
        </div>
        <div
          ref={wrapRef}
          className={styles.canvasWrap}
          style={{ height: HEIGHT }}
          onWheel={onWheel}
          onPointerDown={onPointerDown}
          onPointerMove={onPointerMove}
          onPointerUp={onPointerUp}
          onPointerLeave={() => setHover(null)}
          onDoubleClick={fit}
          title="Clique: levar o player · Ctrl+roda: zoom · arrastar: andar · Shift+arrastar: marcar A–B"
        >
          <canvas ref={canvasRef} className={styles.canvas} />
          {freqTicks.map((f) => {
            const y = fToY(f);
            return y == null ? null : <div key={f} className={styles.fLine} style={{ top: y }} />;
          })}
          {sel && sel.b > view.t0 && sel.a < view.t1 && (
            <div
              className={styles.sel}
              style={{ left: `${Math.max(0, xOf(sel.a))}%`, width: `${Math.min(100, xOf(sel.b)) - Math.max(0, xOf(sel.a))}%` }}
            />
          )}
          {time >= view.t0 && time <= view.t1 && <div className={styles.playhead} style={{ left: `${xOf(time)}%` }} />}
          {hover && <div className={styles.cross} style={{ left: hover.x, top: hover.y }} />}
          {(loading || error) && <span className={error ? styles.err : styles.loading}>{error ?? "calculando…"}</span>}
        </div>
      </div>
      <div className={styles.tAxis}>
        {ticks.major.map((t) => (
          <span key={t.t} style={{ left: `${xOf(t.t)}%` }}>
            {t.label}
          </span>
        ))}
      </div>
    </section>
  );
}
