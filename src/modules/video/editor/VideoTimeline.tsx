/**
 * VideoTimeline — régua técnica + playhead + marcadores de evento.
 * Click na régua → seek. Click no marcador → seleciona/navega.
 */

import { useMemo, useRef } from "react";
import type { VideoEvent } from "@domain/video";
import styles from "./VideoTimeline.module.css";

interface Props {
  duration: number;
  currentTime: number;
  events: VideoEvent[];
  selectedEventId: string | null;
  onSeek: (seconds: number) => void;
  onSelectEvent: (id: string) => void;
}

export function VideoTimeline({
  duration,
  currentTime,
  events,
  selectedEventId,
  onSeek,
  onSelectEvent,
}: Props) {
  const railRef = useRef<HTMLDivElement | null>(null);

  const safeDuration = duration > 0 ? duration : 1;
  const playheadLeft = useMemo(() => {
    const pct = Math.max(0, Math.min(1, currentTime / safeDuration));
    return `${pct * 100}%`;
  }, [currentTime, safeDuration]);

  const handleRailClick = (e: React.MouseEvent<HTMLDivElement>) => {
    if (!railRef.current || duration <= 0) return;
    const rect = railRef.current.getBoundingClientRect();
    const ratio = Math.max(0, Math.min(1, (e.clientX - rect.left) / rect.width));
    onSeek(ratio * duration);
  };

  // Tick marks every "nice" interval — 1s, 5s, 10s, 30s, 60s, ... depending on length.
  const ticks = useMemo(() => generateTicks(duration), [duration]);

  return (
    <div className={styles.wrap}>
      <div
        ref={railRef}
        className={styles.rail}
        onClick={handleRailClick}
        role="slider"
        aria-valuemin={0}
        aria-valuemax={duration}
        aria-valuenow={currentTime}
        tabIndex={0}
      >
        {ticks.map((t) => (
          <div
            key={t.s}
            className={t.major ? styles.tickMajor : styles.tick}
            style={{ left: `${(t.s / safeDuration) * 100}%` }}
          >
            {t.major && <span className={styles.tickLabel}>{formatShort(t.s)}</span>}
          </div>
        ))}
        {events.map((ev) => (
          <button
            key={ev.id}
            type="button"
            className={`${styles.marker} ${
              selectedEventId === ev.id ? styles.markerActive : ""
            } ${styles[`cat-${ev.category}`] ?? ""}`}
            style={{ left: `${(ev.timestamp_s / safeDuration) * 100}%` }}
            title={`${ev.timestamp_label} · ${ev.category} · ${ev.title}`}
            onClick={(e) => {
              e.stopPropagation();
              onSelectEvent(ev.id);
            }}
          />
        ))}
        <div className={styles.playhead} style={{ left: playheadLeft }} />
      </div>
    </div>
  );
}

interface Tick {
  s: number;
  major: boolean;
}

function generateTicks(duration: number): Tick[] {
  if (duration <= 0) return [];
  const step = pickStep(duration);
  const majorEvery = step * 5;
  const ticks: Tick[] = [];
  for (let s = 0; s <= duration + 0.001; s += step) {
    const major = Math.abs((s % majorEvery) - 0) < 0.0001 || s === 0;
    ticks.push({ s, major });
  }
  return ticks;
}

function pickStep(duration: number): number {
  // Aim for ~50 minor ticks across the rail.
  const target = duration / 50;
  const choices = [0.5, 1, 2, 5, 10, 30, 60, 120, 300, 600];
  for (const c of choices) {
    if (c >= target) return c;
  }
  return 600;
}

function formatShort(s: number): string {
  if (s < 60) return `${Math.round(s)}s`;
  const m = Math.floor(s / 60);
  const r = Math.round(s - m * 60);
  return `${m}:${r.toString().padStart(2, "0")}`;
}
