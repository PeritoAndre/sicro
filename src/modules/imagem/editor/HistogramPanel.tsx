/**
 * HistogramPanel — visualização de histograma RGB + luminância.
 *
 * G12.12 — Painel que mostra os 256 bins de cada canal sobrepostos
 * num gráfico SVG, mais estatísticas (média / desvio / dinâmica
 * mínima/máxima). Útil para diagnóstico de exposição, sub/super-
 * exposição, e ajuste de levels.
 */

import { useEffect, useMemo, useState } from "react";
import { Loader2, RefreshCw } from "lucide-react";
import { commands } from "@core/commands";
import type { ImageHistogram } from "@domain/image_analysis";
import styles from "./HistogramPanel.module.css";

interface Props {
  workspacePath: string;
  relativePath: string;
}

export function HistogramPanel({ workspacePath, relativePath }: Props) {
  const [data, setData] = useState<ImageHistogram | null>(null);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const load = async () => {
    setLoading(true);
    setError(null);
    try {
      const h = await commands.computeImageHistogram(
        workspacePath,
        relativePath,
      );
      setData(h);
    } catch (e) {
      setError(String((e as Error)?.message ?? e));
    } finally {
      setLoading(false);
    }
  };

  useEffect(() => {
    void load();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [workspacePath, relativePath]);

  return (
    <div className={styles.panel}>
      <header className={styles.head}>
        <strong>Histograma</strong>
        <button
          type="button"
          onClick={() => void load()}
          disabled={loading}
          className={styles.refreshBtn}
          aria-label="Recalcular"
        >
          {loading ? (
            <Loader2 size={12} className={styles.spin} />
          ) : (
            <RefreshCw size={12} />
          )}
        </button>
      </header>
      {error && <div className={styles.error}>{error}</div>}
      {!data && !error && !loading && (
        <p className={styles.empty}>Aguardando dados…</p>
      )}
      {data && <HistogramChart data={data} />}
      {data && <HistogramStatsTable data={data} />}
    </div>
  );
}

function HistogramChart({ data }: { data: ImageHistogram }) {
  const { red, green, blue, luminance } = data;
  const maxVal = useMemo(() => {
    let m = 0;
    for (let i = 0; i < 256; i++) {
      const r = red[i] ?? 0;
      const g = green[i] ?? 0;
      const b = blue[i] ?? 0;
      const l = luminance[i] ?? 0;
      if (r > m) m = r;
      if (g > m) m = g;
      if (b > m) m = b;
      if (l > m) m = l;
    }
    return Math.max(1, m);
  }, [data]);

  const w = 256;
  const h = 100;
  const polyPath = (vals: number[]) => {
    const pts: string[] = [];
    pts.push(`M0,${h}`);
    for (let i = 0; i < 256; i++) {
      const v = vals[i] ?? 0;
      const y = h - (v / maxVal) * h;
      pts.push(`L${i},${y.toFixed(2)}`);
    }
    pts.push(`L255,${h}Z`);
    return pts.join(" ");
  };

  return (
    <svg
      className={styles.chart}
      viewBox={`0 0 ${w} ${h}`}
      preserveAspectRatio="none"
      role="img"
      aria-label="Gráfico de histograma RGB e luminância"
    >
      <rect x={0} y={0} width={w} height={h} fill="#0b0d11" />
      <path d={polyPath(luminance)} fill="rgba(255,255,255,0.22)" />
      <path d={polyPath(red)} fill="rgba(239,68,68,0.55)" />
      <path d={polyPath(green)} fill="rgba(34,197,94,0.55)" />
      <path d={polyPath(blue)} fill="rgba(59,130,246,0.55)" />
      {/* mid line */}
      <line
        x1={128}
        y1={0}
        x2={128}
        y2={h}
        stroke="rgba(255,255,255,0.1)"
        strokeDasharray="2 2"
      />
    </svg>
  );
}

function HistogramStatsTable({ data }: { data: ImageHistogram }) {
  const s = data.stats;
  return (
    <table className={styles.stats}>
      <thead>
        <tr>
          <th>Canal</th>
          <th>Média</th>
          <th>Desvio</th>
        </tr>
      </thead>
      <tbody>
        <tr>
          <td>
            <span className={styles.dotR} /> R
          </td>
          <td>{s.mean_r.toFixed(1)}</td>
          <td>{s.stddev_r.toFixed(1)}</td>
        </tr>
        <tr>
          <td>
            <span className={styles.dotG} /> G
          </td>
          <td>{s.mean_g.toFixed(1)}</td>
          <td>{s.stddev_g.toFixed(1)}</td>
        </tr>
        <tr>
          <td>
            <span className={styles.dotB} /> B
          </td>
          <td>{s.mean_b.toFixed(1)}</td>
          <td>{s.stddev_b.toFixed(1)}</td>
        </tr>
        <tr>
          <td>
            <span className={styles.dotL} /> Lum
          </td>
          <td>{s.mean_lum.toFixed(1)}</td>
          <td>{s.stddev_lum.toFixed(1)}</td>
        </tr>
      </tbody>
      <tfoot>
        <tr>
          <td colSpan={3} className={styles.foot}>
            Dinâmica Lum: <strong>{s.min_lum}</strong>–
            <strong>{s.max_lum}</strong> · {s.total_pixels.toLocaleString("pt-BR")} px
          </td>
        </tr>
      </tfoot>
    </table>
  );
}
