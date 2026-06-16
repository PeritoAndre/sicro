/**
 * Kit de gráficos em SVG puro — sem dependência externa, tema-aware (usa as
 * CSS vars do Design System), determinístico. Cada componente degrada para um
 * placeholder "sem dados" quando vazio.
 */

import type { ReactNode } from "react";
import { nf } from "../stats/format";
import type {
  CategorySlice,
  HistogramBin,
  KpiTone,
  TimePoint,
} from "../stats/model";
import { colorAt } from "./palette";
import styles from "./charts.module.css";

function fmtVal(v: number, unit?: string): string {
  const s = nf(v, Number.isInteger(v) ? 0 : 1);
  return unit ? `${s} ${unit}` : s;
}

function trunc(s: string, max: number): string {
  return s.length > max ? `${s.slice(0, max - 1)}…` : s;
}

// --------------------------------------------------------------------------

export function EmptyChart({ message = "Sem dados para exibir." }: { message?: string }) {
  return <div className={styles.empty}>{message}</div>;
}

export function ChartCard({
  title,
  subtitle,
  footnote,
  span,
  actions,
  children,
}: {
  title: string;
  subtitle?: string;
  footnote?: ReactNode;
  span?: "wide" | "full";
  actions?: ReactNode;
  children: ReactNode;
}) {
  const cls = [
    styles.card,
    span === "full" ? styles.cardFull : span === "wide" ? styles.cardWide : "",
  ]
    .filter(Boolean)
    .join(" ");
  return (
    <section className={cls}>
      <header className={styles.cardHead}>
        <div>
          <h3 className={styles.cardTitle}>{title}</h3>
          {subtitle && <p className={styles.cardSub}>{subtitle}</p>}
        </div>
        {actions && <div className={styles.cardActions}>{actions}</div>}
      </header>
      <div className={styles.cardBody}>{children}</div>
      {footnote && <p className={styles.cardFoot}>{footnote}</p>}
    </section>
  );
}

const TONE_CLASS: Record<KpiTone, string> = {
  default: "",
  accent: styles.kpiAccent ?? "",
  ok: styles.kpiOk ?? "",
  warn: styles.kpiWarn ?? "",
  crit: styles.kpiCrit ?? "",
};

export function KpiCard({
  label,
  value,
  sub,
  tone = "default",
}: {
  label: string;
  value: string;
  sub?: string;
  tone?: KpiTone;
}) {
  return (
    <div className={`${styles.kpi} ${TONE_CLASS[tone]}`}>
      <div className={styles.kpiValue}>{value}</div>
      <div className={styles.kpiLabel}>{label}</div>
      {sub && <div className={styles.kpiSub}>{sub}</div>}
    </div>
  );
}

// --------------------------------------------------------------------------
// Barras verticais (poucas categorias)

export function BarChart({
  data,
  unit,
  height = 200,
  color,
}: {
  data: CategorySlice[];
  unit?: string;
  height?: number;
  color?: string;
}) {
  if (!data.length) return <EmptyChart />;
  const n = data.length;
  const W = Math.max(320, n * 64);
  const H = height;
  const padL = 8;
  const padR = 8;
  const padT = 22;
  const padB = 42;
  const plotW = W - padL - padR;
  const plotH = H - padT - padB;
  const maxV = Math.max(...data.map((d) => d.value), 1);
  const slot = plotW / n;
  const barW = Math.min(48, slot * 0.62);
  const baseY = padT + plotH;
  return (
    <svg viewBox={`0 0 ${W} ${H}`} className={styles.svg} role="img">
      <line x1={padL} y1={baseY} x2={W - padR} y2={baseY} className={styles.axis} />
      {data.map((d, i) => {
        const cx = padL + slot * (i + 0.5);
        const barH = (d.value / maxV) * plotH;
        const y = baseY - barH;
        return (
          <g key={`${d.label}-${i}`}>
            <rect
              x={cx - barW / 2}
              y={y}
              width={barW}
              height={Math.max(0, barH)}
              rx={2}
              style={{ fill: color ?? colorAt(i) }}
            />
            <text x={cx} y={y - 5} className={styles.barValue} textAnchor="middle">
              {fmtVal(d.value, unit)}
            </text>
            <text x={cx} y={baseY + 15} className={styles.barLabel} textAnchor="middle">
              {trunc(d.label, 11)}
            </text>
          </g>
        );
      })}
    </svg>
  );
}

// --------------------------------------------------------------------------
// Barras horizontais (rankings / rótulos longos)

export function HBarChart({
  data,
  unit,
  maxRows = 14,
}: {
  data: CategorySlice[];
  unit?: string;
  maxRows?: number;
}) {
  if (!data.length) return <EmptyChart />;
  const rows = data.slice(0, maxRows);
  const n = rows.length;
  const rowH = 24;
  const gap = 6;
  const padTop = 4;
  const labelW = 130;
  const valueW = 58;
  const barAreaW = 340;
  const W = labelW + barAreaW + valueW;
  const H = padTop * 2 + n * rowH + (n - 1) * gap;
  const maxV = Math.max(...rows.map((d) => d.value), 1);
  return (
    <svg viewBox={`0 0 ${W} ${H}`} className={styles.svg} role="img">
      {rows.map((d, i) => {
        const y = padTop + i * (rowH + gap);
        const w = Math.max(1, (d.value / maxV) * barAreaW);
        return (
          <g key={`${d.label}-${i}`}>
            <text
              x={labelW - 8}
              y={y + rowH / 2}
              className={styles.hbarLabel}
              textAnchor="end"
              dominantBaseline="middle"
            >
              {trunc(d.label, 20)}
            </text>
            <rect
              x={labelW}
              y={y + 3}
              width={w}
              height={rowH - 6}
              rx={2}
              style={{ fill: colorAt(i) }}
            />
            <text
              x={labelW + w + 6}
              y={y + rowH / 2}
              className={styles.hbarValue}
              dominantBaseline="middle"
            >
              {fmtVal(d.value, unit)}
            </text>
          </g>
        );
      })}
    </svg>
  );
}

// --------------------------------------------------------------------------
// Rosca (donut) + legenda

function donutArc(
  cx: number,
  cy: number,
  R: number,
  r: number,
  a0: number,
  a1: number,
): string {
  const large = a1 - a0 > Math.PI ? 1 : 0;
  const x0 = cx + R * Math.cos(a0);
  const y0 = cy + R * Math.sin(a0);
  const x1 = cx + R * Math.cos(a1);
  const y1 = cy + R * Math.sin(a1);
  const xi1 = cx + r * Math.cos(a1);
  const yi1 = cy + r * Math.sin(a1);
  const xi0 = cx + r * Math.cos(a0);
  const yi0 = cy + r * Math.sin(a0);
  return `M ${x0} ${y0} A ${R} ${R} 0 ${large} 1 ${x1} ${y1} L ${xi1} ${yi1} A ${r} ${r} 0 ${large} 0 ${xi0} ${yi0} Z`;
}

export function DonutChart({
  data,
  centerLabel,
}: {
  data: CategorySlice[];
  centerLabel?: string;
}) {
  const slices = data.filter((d) => d.value > 0);
  const total = slices.reduce((s, d) => s + d.value, 0);
  if (!slices.length || total <= 0) return <EmptyChart />;
  const size = 180;
  const cx = size / 2;
  const cy = size / 2;
  const R = 80;
  const r = 50;
  const single = slices.length === 1;
  let a0 = -Math.PI / 2;
  const arcs = slices.map((d, i) => {
    const frac = d.value / total;
    const a1 = a0 + frac * Math.PI * 2;
    const path = donutArc(cx, cy, R, r, a0, a1);
    a0 = a1;
    return { path, color: colorAt(i), label: d.label, value: d.value, frac };
  });
  return (
    <div className={styles.donutWrap}>
      <svg viewBox={`0 0 ${size} ${size}`} className={styles.donutSvg} role="img">
        {single ? (
          <circle
            cx={cx}
            cy={cy}
            r={(R + r) / 2}
            fill="none"
            style={{ stroke: arcs[0]!.color }}
            strokeWidth={R - r}
          />
        ) : (
          arcs.map((arc, i) => (
            <path key={i} d={arc.path} style={{ fill: arc.color }} />
          ))
        )}
        <text x={cx} y={cy - 3} className={styles.donutTotal} textAnchor="middle">
          {nf(total)}
        </text>
        <text x={cx} y={cy + 13} className={styles.donutTotalLabel} textAnchor="middle">
          {centerLabel ?? "total"}
        </text>
      </svg>
      <ul className={styles.legend}>
        {arcs.map((arc, i) => (
          <li key={i}>
            <span className={styles.legendDot} style={{ background: arc.color }} />
            <span className={styles.legendLabel}>{arc.label}</span>
            <span className={styles.legendValue}>
              {nf(arc.value)} · {Math.round(arc.frac * 100)}%
            </span>
          </li>
        ))}
      </ul>
    </div>
  );
}

// --------------------------------------------------------------------------
// Série temporal (linha + área)

export function LineChart({
  data,
  unit,
  area = true,
}: {
  data: TimePoint[];
  unit?: string;
  area?: boolean;
}) {
  if (!data.length) return <EmptyChart />;
  const W = 480;
  const H = 200;
  const padL = 12;
  const padR = 12;
  const padT = 16;
  const padB = 28;
  const plotW = W - padL - padR;
  const plotH = H - padT - padB;
  const n = data.length;
  const maxV = Math.max(...data.map((d) => d.value), 1);
  const baseY = padT + plotH;
  const xAt = (i: number) => (n === 1 ? padL + plotW / 2 : padL + (i / (n - 1)) * plotW);
  const yAt = (v: number) => baseY - (v / maxV) * plotH;
  const pts = data.map((d, i) => `${xAt(i)},${yAt(d.value)}`);
  const linePath = `M ${pts.join(" L ")}`;
  const areaPath = `M ${xAt(0)},${baseY} L ${pts.join(" L ")} L ${xAt(n - 1)},${baseY} Z`;
  const step = Math.max(1, Math.ceil(n / 6));
  return (
    <svg viewBox={`0 0 ${W} ${H}`} className={styles.svg} role="img">
      <line x1={padL} y1={baseY} x2={W - padR} y2={baseY} className={styles.axis} />
      <text x={padL} y={padT - 3} className={styles.axisLabel}>
        {`máx ${fmtVal(maxV, unit)}`}
      </text>
      {area && n > 1 && <path d={areaPath} className={styles.areaFill} />}
      {n > 1 && <path d={linePath} className={styles.line} />}
      {data.map((d, i) =>
        n <= 40 ? (
          <circle key={`p${i}`} cx={xAt(i)} cy={yAt(d.value)} r={2.5} className={styles.dot} />
        ) : null,
      )}
      {data.map((d, i) =>
        i % step === 0 || i === n - 1 ? (
          <text key={`l${i}`} x={xAt(i)} y={H - 9} className={styles.axisLabel} textAnchor="middle">
            {d.label}
          </text>
        ) : null,
      )}
    </svg>
  );
}

// --------------------------------------------------------------------------
// Histograma

export function Histogram({
  bins,
  unit,
}: {
  bins: HistogramBin[];
  unit?: string;
}) {
  if (!bins.length) return <EmptyChart />;
  const n = bins.length;
  const W = Math.max(360, n * 46);
  const H = 200;
  const padL = 10;
  const padR = 10;
  const padT = 20;
  const padB = 40;
  const plotW = W - padL - padR;
  const plotH = H - padT - padB;
  const maxC = Math.max(...bins.map((b) => b.count), 1);
  const slot = plotW / n;
  const baseY = padT + plotH;
  const step = Math.max(1, Math.ceil(n / 6));
  return (
    <svg viewBox={`0 0 ${W} ${H}`} className={styles.svg} role="img">
      <line x1={padL} y1={baseY} x2={W - padR} y2={baseY} className={styles.axis} />
      {unit && (
        <text x={W - padR} y={padT - 6} textAnchor="end" className={styles.axisLabel}>
          {unit}
        </text>
      )}
      {bins.map((b, i) => {
        const barH = (b.count / maxC) * plotH;
        const x = padL + i * slot;
        const y = baseY - barH;
        return (
          <g key={i}>
            <rect
              x={x + 1}
              y={y}
              width={Math.max(1, slot - 2)}
              height={Math.max(0, barH)}
              style={{ fill: "var(--sicro-accent)" }}
            />
            {b.count > 0 && (
              <text x={x + slot / 2} y={y - 4} className={styles.barValue} textAnchor="middle">
                {b.count}
              </text>
            )}
            {(i % step === 0 || i === n - 1) && (
              <text
                x={x + slot / 2}
                y={baseY + 14}
                className={styles.binLabel}
                textAnchor="middle"
              >
                {b.label}
              </text>
            )}
          </g>
        );
      })}
    </svg>
  );
}

// --------------------------------------------------------------------------
// Barra de progresso

export function ProgressBar({
  value,
  label,
  tone = "accent",
}: {
  value: number;
  label?: string;
  tone?: KpiTone;
}) {
  const p = Math.max(0, Math.min(100, value));
  const fillColor =
    tone === "ok"
      ? "var(--sicro-success)"
      : tone === "warn"
        ? "var(--sicro-warning)"
        : tone === "crit"
          ? "var(--sicro-danger)"
          : "var(--sicro-accent)";
  return (
    <div className={styles.progressWrap}>
      {label && (
        <div className={styles.progressLabel}>
          <span>{label}</span>
          <span>{Math.round(p)}%</span>
        </div>
      )}
      <div className={styles.progressTrack}>
        <div className={styles.progressFill} style={{ width: `${p}%`, background: fillColor }} />
      </div>
    </div>
  );
}

// --------------------------------------------------------------------------
// Tabela

export function StatTable({
  columns,
  rows,
  emptyMessage = "Sem registros.",
}: {
  columns: string[];
  rows: (string | number)[][];
  emptyMessage?: string;
}) {
  if (!rows.length) return <EmptyChart message={emptyMessage} />;
  return (
    <div className={styles.tableWrap}>
      <table className={styles.table}>
        <thead>
          <tr>
            {columns.map((c, i) => (
              <th key={i}>{c}</th>
            ))}
          </tr>
        </thead>
        <tbody>
          {rows.map((r, ri) => (
            <tr key={ri}>
              {r.map((c, ci) => (
                <td key={ci}>{c}</td>
              ))}
            </tr>
          ))}
        </tbody>
      </table>
    </div>
  );
}
