/**
 * VideoTimeline — régua técnica + playhead + marcadores de evento.
 *
 *   - Clicar OU ARRASTAR na régua posiciona o vídeo (a imagem acompanha).
 *   - Zoom: Ctrl + roda do mouse (no ponto do cursor), teclas = / − / 0 ou os
 *     botões. Aproximado, a roda sozinha rola a janela e aparece a faixa de
 *     visão geral (clique/arraste nela para mover a janela). No zoom máximo a
 *     régua marca cada quadro.
 *   - Trecho entrada/saída (I/O) aparece destacado; mais forte quando repete.
 *   - Tocando com zoom, a janela acompanha o playhead.
 *   - Altura ajustável: arrastar a borda de cima (duplo clique volta ao
 *     padrão). Lembrada por máquina e igual em todas as linhas do tempo.
 */
import { useCallback, useEffect, useMemo, useRef, useState, useSyncExternalStore } from "react";
import { Maximize2, ZoomIn, ZoomOut } from "lucide-react";
import type { VideoEvent } from "@domain/video";
import { useShortcuts } from "@core/useShortcuts";
import { useContextMenu, type MenuItem } from "@components/ContextMenu/ContextMenu";
import styles from "./VideoTimeline.module.css";

interface Props {
  duration: number;
  fps?: number | null;
  currentTime: number;
  events: VideoEvent[];
  selectedEventId: string | null;
  /** Trecho entrada/saída (já ordenado), ou null. */
  range?: { a: number; b: number } | null;
  loopOn?: boolean;
  /** Atalhos de zoom só valem com a aba Reprodutor visível. */
  shortcutsEnabled: boolean;
  onScrubStart: () => void;
  onScrub: (seconds: number) => void;
  onScrubEnd: (seconds: number) => void;
  onSelectEvent: (id: string) => void;
  /** Itens do botão direito para o instante `t` sob o cursor (ir para, marcar…). */
  contextItems?: (t: number) => MenuItem[];
}

interface View {
  start: number;
  end: number;
}

// ---- altura da régua (preferência compartilhada) -------------------------------

const HEIGHT_KEY = "sicro.video.timelineHeight.v1";
export const TIMELINE_MIN_H = 36;
export const TIMELINE_MAX_H = 240;
export const TIMELINE_DEFAULT_H = 60;
const clampH = (h: number) => Math.round(Math.min(TIMELINE_MAX_H, Math.max(TIMELINE_MIN_H, h)));
let railHeight = (() => {
  try {
    const v = Number(localStorage.getItem(HEIGHT_KEY));
    return v > 0 ? clampH(v) : TIMELINE_DEFAULT_H;
  } catch {
    return TIMELINE_DEFAULT_H;
  }
})();
const heightListeners = new Set<() => void>();
function setRailHeight(h: number, persist: boolean) {
  railHeight = clampH(h);
  heightListeners.forEach((f) => f());
  if (persist) {
    try {
      localStorage.setItem(HEIGHT_KEY, String(railHeight));
    } catch {
      /* só não lembra */
    }
  }
}
function useRailHeight(): number {
  return useSyncExternalStore(
    (f) => {
      heightListeners.add(f);
      return () => heightListeners.delete(f);
    },
    () => railHeight,
  );
}

export function VideoTimeline({
  duration,
  fps,
  currentTime,
  events,
  selectedEventId,
  range,
  loopOn = false,
  shortcutsEnabled,
  onScrubStart,
  onScrub,
  onScrubEnd,
  onSelectEvent,
  contextItems,
}: Props) {
  const menu = useContextMenu();
  const railRef = useRef<HTMLDivElement | null>(null);
  const height = useRailHeight();
  // Régua alta: riscos, rótulos e marcadores crescem junto.
  const tall = height >= 90;
  const gripRef = useRef<{ y: number; h: number } | null>(null);
  const [railWidth, setRailWidth] = useState(800);
  const safeDuration = duration > 0 ? duration : 1;
  const [view, setView] = useState<View>({ start: 0, end: safeDuration });
  const viewRef = useRef(view);
  viewRef.current = view;
  const draggingRef = useRef(false);
  const pendingRef = useRef<number | null>(null);
  const rafRef = useRef<number | null>(null);

  // ~12 quadros é o zoom máximo (cada quadro vira um risco bem visível).
  const minSpan = Math.min(safeDuration, fps && fps > 0 ? 12 / fps : 0.4);
  const span = view.end - view.start;
  const zoomed = span < safeDuration - 1e-6;

  // Duração conhecida/alterada → mostra o vídeo inteiro.
  useEffect(() => {
    setView({ start: 0, end: safeDuration });
  }, [safeDuration]);

  useEffect(() => {
    const el = railRef.current;
    if (!el) return;
    const ro = new ResizeObserver(([entry]) => {
      if (entry) setRailWidth(Math.max(100, entry.contentRect.width));
    });
    ro.observe(el);
    return () => ro.disconnect();
  }, []);

  const clampView = useCallback(
    (start: number, s: number): View => {
      const w = Math.min(safeDuration, Math.max(minSpan, s));
      const st = Math.max(0, Math.min(safeDuration - w, start));
      return { start: st, end: st + w };
    },
    [safeDuration, minSpan],
  );

  /** Zoom por `factor` (< 1 aproxima) mantendo `anchor` no mesmo lugar da tela. */
  const zoomAround = useCallback(
    (factor: number, anchor: number) => {
      const v = viewRef.current;
      const s = v.end - v.start;
      const ns = Math.min(safeDuration, Math.max(minSpan, s * factor));
      const rel = s > 0 ? (anchor - v.start) / s : 0.5;
      setView(clampView(anchor - rel * ns, ns));
    },
    [safeDuration, minSpan, clampView],
  );

  const fit = useCallback(() => setView({ start: 0, end: safeDuration }), [safeDuration]);

  // Aproximar/afastar pelo teclado centraliza no playhead.
  const zoomKeys = (factor: number) => {
    const v = viewRef.current;
    const s = v.end - v.start;
    const ns = Math.min(safeDuration, Math.max(minSpan, s * factor));
    setView(clampView(currentTime - ns / 2, ns));
  };
  useShortcuts(
    {
      "video.timelineZoomIn": () => zoomKeys(0.5),
      "video.timelineZoomOut": () => zoomKeys(2),
      "video.timelineZoomFit": fit,
    },
    { enabled: shortcutsEnabled },
  );

  // A janela acompanha o playhead (tocando, pulando de evento, etc.).
  useEffect(() => {
    if (draggingRef.current) return;
    const v = viewRef.current;
    const s = v.end - v.start;
    if (s >= safeDuration - 1e-6) return;
    if (currentTime > v.end) setView(clampView(currentTime - s * 0.1, s));
    else if (currentTime < v.start) setView(clampView(currentTime - s * 0.9, s));
  }, [currentTime, safeDuration, clampView]);

  // Roda do mouse: Ctrl = zoom no cursor; sem Ctrl (com zoom) = rolar a janela.
  // Listener nativo não-passivo, para o preventDefault valer.
  useEffect(() => {
    const el = railRef.current;
    if (!el) return;
    const onWheel = (e: WheelEvent) => {
      const v = viewRef.current;
      const s = v.end - v.start;
      const rect = el.getBoundingClientRect();
      if (e.ctrlKey) {
        e.preventDefault();
        const anchor = v.start + ((e.clientX - rect.left) / rect.width) * s;
        zoomAround(e.deltaY > 0 ? 1.25 : 0.8, anchor);
        return;
      }
      if (s >= safeDuration - 1e-6) return; // sem zoom: deixa a página rolar
      e.preventDefault();
      const delta = Math.abs(e.deltaX) > Math.abs(e.deltaY) ? e.deltaX : e.deltaY;
      setView(clampView(v.start + (delta / rect.width) * s, s));
    };
    el.addEventListener("wheel", onWheel, { passive: false });
    return () => el.removeEventListener("wheel", onWheel);
  }, [zoomAround, clampView, safeDuration]);

  const timeAtX = (clientX: number): number => {
    const el = railRef.current;
    if (!el || duration <= 0) return 0;
    const rect = el.getBoundingClientRect();
    const ratio = Math.max(0, Math.min(1, (clientX - rect.left) / rect.width));
    const v = viewRef.current;
    return v.start + ratio * (v.end - v.start);
  };

  // Arrastar: um seek por quadro de animação (o player ainda junta os que
  // chegarem enquanto o anterior não terminou).
  const flushScrub = () => {
    rafRef.current = null;
    if (pendingRef.current != null) onScrub(pendingRef.current);
    pendingRef.current = null;
  };
  const handlePointerDown = (e: React.PointerEvent<HTMLDivElement>) => {
    if (e.button !== 0 || duration <= 0) return;
    // Sem isso, arrastar saindo da régua seleciona o texto da tela.
    e.preventDefault();
    e.currentTarget.focus({ preventScroll: true });
    e.currentTarget.setPointerCapture(e.pointerId);
    draggingRef.current = true;
    onScrubStart();
    onScrub(timeAtX(e.clientX));
  };
  const handlePointerMove = (e: React.PointerEvent<HTMLDivElement>) => {
    if (!draggingRef.current) return;
    pendingRef.current = timeAtX(e.clientX);
    if (rafRef.current == null) rafRef.current = requestAnimationFrame(flushScrub);
  };
  const handlePointerUp = (e: React.PointerEvent<HTMLDivElement>) => {
    if (!draggingRef.current) return;
    draggingRef.current = false;
    if (rafRef.current != null) cancelAnimationFrame(rafRef.current);
    rafRef.current = null;
    pendingRef.current = null;
    onScrubEnd(timeAtX(e.clientX));
  };

  // Faixa de visão geral: clicar/arrastar centraliza a janela ali.
  const overviewRef = useRef<HTMLDivElement | null>(null);
  const overviewDragRef = useRef(false);
  const centerViewAt = (clientX: number) => {
    const el = overviewRef.current;
    if (!el) return;
    const rect = el.getBoundingClientRect();
    const t = Math.max(0, Math.min(1, (clientX - rect.left) / rect.width)) * safeDuration;
    const v = viewRef.current;
    const s = v.end - v.start;
    setView(clampView(t - s / 2, s));
  };

  const ticks = useMemo(
    () => buildTicks(view.start, view.end, railWidth, fps ?? null),
    [view.start, view.end, railWidth, fps],
  );

  const pct = (t: number) => `${((t - view.start) / (span || 1)) * 100}%`;
  const inView = (t: number) => t >= view.start - 1e-9 && t <= view.end + 1e-9;
  const pctAll = (t: number) => `${(t / safeDuration) * 100}%`;

  return (
    <div className={`${styles.wrap} ${tall ? styles.tall : ""}`}>
      {menu.element}
      <div
        className={styles.grip}
        title="Arraste para cima ou para baixo para mudar a altura da linha do tempo · duplo clique: altura padrão"
        onPointerDown={(e) => {
          e.preventDefault();
          e.currentTarget.setPointerCapture(e.pointerId);
          gripRef.current = { y: e.clientY, h: height };
        }}
        onPointerMove={(e) => {
          const g = gripRef.current;
          if (!g) return;
          // Não deixa a régua passar de ~45% da janela (o vídeo precisa de espaço).
          const cap = Math.max(TIMELINE_MIN_H, window.innerHeight * 0.45);
          setRailHeight(Math.min(cap, g.h + (g.y - e.clientY)), false);
        }}
        onPointerUp={() => {
          if (!gripRef.current) return;
          gripRef.current = null;
          setRailHeight(railHeight, true);
        }}
        onPointerCancel={() => (gripRef.current = null)}
        onDoubleClick={() => setRailHeight(TIMELINE_DEFAULT_H, true)}
      />
      <div className={styles.toolbar}>
        {zoomed ? (
          <div
            ref={overviewRef}
            className={styles.overview}
            title="Visão geral — clique ou arraste para mover a janela"
            onPointerDown={(e) => {
              e.preventDefault();
              e.currentTarget.setPointerCapture(e.pointerId);
              overviewDragRef.current = true;
              centerViewAt(e.clientX);
            }}
            onPointerMove={(e) => overviewDragRef.current && centerViewAt(e.clientX)}
            onPointerUp={() => (overviewDragRef.current = false)}
            onPointerCancel={() => (overviewDragRef.current = false)}
          >
            {range && (
              <div
                className={styles.overviewRange}
                style={{ left: pctAll(range.a), width: pctAll(range.b - range.a) }}
              />
            )}
            {events.map((ev) => (
              <div key={ev.id} className={styles.overviewEvent} style={{ left: pctAll(ev.timestamp_s) }} />
            ))}
            <div
              className={styles.overviewWindow}
              style={{ left: pctAll(view.start), width: pctAll(span) }}
            />
            <div className={styles.overviewPlayhead} style={{ left: pctAll(currentTime) }} />
          </div>
        ) : (
          <span className={styles.hint}>Ctrl + roda do mouse (ou = / −) aproxima a linha do tempo</span>
        )}
        <span className={styles.spanLabel}>
          {zoomed ? `mostrando ${formatSpan(span)}` : "vídeo inteiro"}
        </span>
        <button type="button" onClick={() => zoomKeys(2)} disabled={!zoomed} title="Afastar (−)">
          <ZoomOut size={13} />
        </button>
        <button type="button" onClick={() => zoomKeys(0.5)} disabled={span <= minSpan + 1e-6} title="Aproximar (=)">
          <ZoomIn size={13} />
        </button>
        <button type="button" onClick={fit} disabled={!zoomed} title="Vídeo inteiro (0)">
          <Maximize2 size={13} />
        </button>
      </div>

      <div
        ref={railRef}
        className={styles.rail}
        style={{ height }}
        onPointerDown={handlePointerDown}
        onPointerMove={handlePointerMove}
        onPointerUp={handlePointerUp}
        onPointerCancel={handlePointerUp}
        onContextMenu={(e) => {
          const t = timeAtX(e.clientX);
          menu.open(e, [
            ...(contextItems?.(t) ?? []),
            "separator",
            { label: "Aproximar a linha do tempo aqui", shortcut: "Ctrl+roda", onSelect: () => zoomAround(0.5, t) },
            { label: "Linha do tempo inteira", shortcut: "0", disabled: !zoomed, onSelect: fit },
            "separator",
            { label: "Linha do tempo mais alta", onSelect: () => setRailHeight(height + 30, true), disabled: height >= TIMELINE_MAX_H },
            { label: "Linha do tempo mais baixa", onSelect: () => setRailHeight(height - 30, true), disabled: height <= TIMELINE_MIN_H },
            { label: "Altura padrão", onSelect: () => setRailHeight(TIMELINE_DEFAULT_H, true), disabled: height === TIMELINE_DEFAULT_H },
          ]);
        }}
        role="slider"
        aria-valuemin={0}
        aria-valuemax={duration}
        aria-valuenow={currentTime}
        tabIndex={0}
      >
        {range && range.b > view.start && range.a < view.end && (
          <div
            className={`${styles.range} ${loopOn ? styles.rangeLoop : ""}`}
            style={{
              left: pct(Math.max(range.a, view.start)),
              width: `${((Math.min(range.b, view.end) - Math.max(range.a, view.start)) / (span || 1)) * 100}%`,
            }}
            title={loopOn ? "Trecho em repetição" : "Trecho marcado (I/O)"}
          />
        )}
        {ticks.minor.map((t) => (
          <div key={`m${t}`} className={styles.tick} style={{ left: pct(t) }} />
        ))}
        {ticks.major.map((t) => (
          <div key={`M${t.t}`} className={styles.tickMajor} style={{ left: pct(t.t) }}>
            <span className={styles.tickLabel}>{t.label}</span>
          </div>
        ))}
        {events.filter((ev) => inView(ev.timestamp_s)).map((ev) => (
          <button
            key={ev.id}
            type="button"
            className={`${styles.marker} ${
              selectedEventId === ev.id ? styles.markerActive : ""
            } ${styles[`cat-${ev.category}`] ?? ""}`}
            style={{ left: pct(ev.timestamp_s) }}
            title={`${ev.timestamp_label} · ${ev.category} · ${ev.title}`}
            onPointerDown={(e) => e.stopPropagation()}
            onClick={(e) => {
              e.stopPropagation();
              onSelectEvent(ev.id);
            }}
          />
        ))}
        {inView(currentTime) && <div className={styles.playhead} style={{ left: pct(currentTime) }} />}
      </div>
    </div>
  );
}

// ---- régua ------------------------------------------------------------------

const NICE_STEPS = [
  0.01, 0.02, 0.05, 0.1, 0.2, 0.5, 1, 2, 5, 10, 15, 30, 60, 120, 300, 600, 900, 1800, 3600,
];

export interface Ticks {
  minor: number[];
  major: { t: number; label: string }[];
}

/**
 * Riscos da régua para a janela [start, end] numa régua de `widthPx`:
 * menores com ≥ 8 px entre si (no zoom máximo, um por quadro — 1/fps) e
 * maiores rotulados com ≥ 80 px.
 */
export function buildTicks(start: number, end: number, widthPx: number, fps: number | null): Ticks {
  const span = end - start;
  if (!(span > 0) || !(widthPx > 0)) return { minor: [], major: [] };
  const frame = fps && fps > 0 ? 1 / fps : null;
  // Abaixo de 1 quadro não há o que marcar num vídeo.
  const cands = [...NICE_STEPS, ...(frame ? [frame] : [])]
    .filter((c) => !frame || c >= frame - 1e-9)
    .sort((a, b) => a - b);
  const minor = cands.find((c) => (span / c) * 8 <= widthPx) ?? cands[cands.length - 1]!;
  // Com risco por quadro, os rótulos caem em múltiplos exatos de quadro
  // (a 25 fps: 0,2 s = 5 quadros; 0,1 s não é quadro nenhum).
  const frameGrid = frame != null && Math.abs(minor - frame) < 1e-9;
  const isFrameMultiple = (c: number) =>
    frame != null && Math.abs(c / frame - Math.round(c / frame)) < 1e-6;
  const majorCands = frameGrid
    ? [
        ...NICE_STEPS.filter(isFrameMultiple),
        ...[5, 10, 25, 50, 100].map((k) => k * frame!),
      ].sort((a, b) => a - b)
    : NICE_STEPS;
  const major =
    majorCands.find((c) => c >= minor * 2 - 1e-9 && (span / c) * 80 <= widthPx) ??
    majorCands[majorCands.length - 1]!;
  const at = (step: number) => {
    const out: number[] = [];
    const k0 = Math.ceil(start / step - 1e-9);
    const k1 = Math.floor(end / step + 1e-9);
    for (let k = k0; k <= k1 && out.length < 2000; k++) out.push(Number((k * step).toFixed(6)));
    return out;
  };
  const majors = at(major);
  const majorSet = new Set(majors.map((t) => t.toFixed(4)));
  return {
    minor: at(minor).filter((t) => !majorSet.has(t.toFixed(4))),
    major: majors.map((t) => ({ t, label: formatTick(t, major) })),
  };
}

function formatTick(t: number, step: number): string {
  const decimals = step < 0.1 ? 2 : step < 1 ? 1 : 0;
  if (t < 60) return `${t.toFixed(decimals)}s`;
  const m = Math.floor(t / 60);
  const s = t - m * 60;
  const ss = s.toFixed(decimals).padStart(decimals ? 3 + decimals : 2, "0");
  return `${m}:${ss}`;
}

function formatSpan(s: number): string {
  if (s < 1) return `${Math.round(s * 1000)} ms`;
  if (s < 60) return `${s.toFixed(s < 10 ? 2 : 1)} s`;
  const m = Math.floor(s / 60);
  return `${m} min ${Math.round(s - m * 60)} s`;
}
