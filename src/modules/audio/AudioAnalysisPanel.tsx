/**
 * W12 (paridade Audacity) — Painel de ANÁLISE FORENSE de áudio.
 *
 * Três análises determinísticas (backend Rust puro: hound + rustfft), todas
 * de MEDIÇÃO — não alteram o áudio (§13):
 *   - Medições: pico/RMS (dBFS), offset DC, clipping, fator de crista.
 *   - Espectro (Welch FFT): pico de frequência + mini-gráfico log.
 *   - ENF (Electric Network Frequency): média/desvio + maior salto
 *     (descontinuidade = indício de edição/splice).
 *
 * Cada chamada é registrada no log de auditoria do áudio pelo backend.
 */

import { useState } from "react";
import { Activity, AudioWaveform, Gauge, Loader2, Zap } from "lucide-react";
import { commands } from "@core/commands";
import { toSicroError } from "@core/errors";
import type {
  AudioMeasurements,
  EnfResult,
  SpectrumResult,
} from "@domain/audio";

interface Props {
  workspacePath: string;
  audioId: string;
}

export function AudioAnalysisPanel({ workspacePath, audioId }: Props) {
  const [busy, setBusy] = useState<"" | "measure" | "spectrum" | "enf">("");
  const [error, setError] = useState<string | null>(null);
  const [measure, setMeasure] = useState<AudioMeasurements | null>(null);
  const [spectrum, setSpectrum] = useState<SpectrumResult | null>(null);
  const [enf, setEnf] = useState<EnfResult | null>(null);
  const [nominalHz, setNominalHz] = useState<50 | 60>(60);

  const run = async (
    which: "measure" | "spectrum" | "enf",
    fn: () => Promise<void>,
  ) => {
    setBusy(which);
    setError(null);
    try {
      await fn();
    } catch (e) {
      setError(toSicroError(e).message);
    } finally {
      setBusy("");
    }
  };

  const dbfs = (v: number) => (v <= -119 ? "−∞" : `${v.toFixed(1)} dBFS`);

  return (
    <section
      style={{
        border: "1px solid var(--border, #2a2f3a)",
        borderRadius: 8,
        padding: 12,
        display: "flex",
        flexDirection: "column",
        gap: 10,
      }}
    >
      <header style={{ display: "flex", alignItems: "center", gap: 8 }}>
        <Activity size={15} />
        <strong style={{ fontSize: 13 }}>Análise forense (medição)</strong>
      </header>

      <div style={{ display: "flex", flexWrap: "wrap", gap: 6 }}>
        <button
          type="button"
          disabled={!!busy}
          onClick={() =>
            run("measure", async () => {
              setMeasure(await commands.audioMeasure(workspacePath, audioId));
            })
          }
        >
          {busy === "measure" ? <Loader2 size={12} className="spin" /> : <Gauge size={12} />}{" "}
          Medições
        </button>
        <button
          type="button"
          disabled={!!busy}
          onClick={() =>
            run("spectrum", async () => {
              setSpectrum(await commands.audioSpectrum(workspacePath, audioId, 4096));
            })
          }
        >
          {busy === "spectrum" ? <Loader2 size={12} className="spin" /> : <AudioWaveform size={12} />}{" "}
          Espectro (FFT)
        </button>
        <button
          type="button"
          disabled={!!busy}
          onClick={() =>
            run("enf", async () => {
              setEnf(await commands.audioEnf(workspacePath, audioId, nominalHz));
            })
          }
        >
          {busy === "enf" ? <Loader2 size={12} className="spin" /> : <Zap size={12} />} ENF
        </button>
        <label style={{ display: "flex", alignItems: "center", gap: 4, fontSize: 11 }}>
          Rede:
          <select
            value={nominalHz}
            onChange={(e) => setNominalHz(Number(e.target.value) as 50 | 60)}
          >
            <option value={60}>60 Hz</option>
            <option value={50}>50 Hz</option>
          </select>
        </label>
      </div>

      {error && (
        <p style={{ color: "var(--danger, #f87171)", fontSize: 12, margin: 0 }}>{error}</p>
      )}

      {measure && (
        <div style={{ fontSize: 12 }}>
          <strong>Medições</strong>
          <table style={{ width: "100%", borderCollapse: "collapse" }}>
            <tbody>
              <Row k="Duração" v={`${measure.duration_s.toFixed(2)} s`} />
              <Row k="Taxa / canais" v={`${measure.sample_rate} Hz · ${measure.channels}ch`} />
              <Row k="Pico" v={dbfs(measure.peak_dbfs)} />
              <Row k="RMS" v={dbfs(measure.rms_dbfs)} />
              <Row k="Fator de crista" v={`${measure.crest_factor_db.toFixed(1)} dB`} />
              <Row k="Offset DC" v={`${measure.dc_offset_pct.toFixed(3)} %`} />
              <Row
                k="Clipping"
                v={`${measure.clipped_samples} amostras · ${measure.clipped_runs} trechos · ${measure.clipped_pct.toFixed(3)}%`}
                warn={measure.clipped_samples > 0}
              />
            </tbody>
          </table>
        </div>
      )}

      {spectrum && (
        <div style={{ fontSize: 12 }}>
          <strong>Espectro</strong> — pico em{" "}
          <b>{spectrum.peak_freq_hz.toFixed(1)} Hz</b> ({spectrum.peak_db.toFixed(1)} dB),
          FFT {spectrum.fft_size}, janela {spectrum.window}
          <SpectrumSparkline result={spectrum} />
        </div>
      )}

      {enf && (
        <div style={{ fontSize: 12 }}>
          <strong>ENF</strong> (nominal {enf.nominal_hz} Hz)
          <table style={{ width: "100%", borderCollapse: "collapse" }}>
            <tbody>
              <Row k="Média" v={`${enf.mean_hz.toFixed(3)} Hz`} />
              <Row k="Desvio-padrão" v={`${enf.std_hz.toFixed(4)} Hz`} />
              <Row
                k="Maior salto"
                v={`${enf.max_jump_hz.toFixed(4)} Hz`}
                warn={enf.max_jump_hz > 0.1}
              />
              <Row k="Quadros" v={`${enf.enf_hz.length}`} />
            </tbody>
          </table>
          <p style={{ fontSize: 10.5, color: "var(--text-secondary, #9aa4b2)", margin: "4px 0 0" }}>
            Saltos &gt; ~0,1 Hz sugerem descontinuidade (possível edição). Cruzamento
            com banco de dados da rede elétrica é etapa separada (fora do app).
          </p>
        </div>
      )}
    </section>
  );
}

function Row({ k, v, warn }: { k: string; v: string; warn?: boolean }) {
  return (
    <tr>
      <td style={{ padding: "2px 6px 2px 0", color: "var(--text-secondary, #9aa4b2)" }}>{k}</td>
      <td
        style={{
          padding: "2px 0",
          textAlign: "right",
          fontVariantNumeric: "tabular-nums",
          color: warn ? "var(--danger, #f87171)" : "inherit",
          fontWeight: warn ? 600 : 400,
        }}
      >
        {v}
      </td>
    </tr>
  );
}

/** Mini-gráfico do espectro (log-frequência no X, dB no Y) em SVG puro. */
function SpectrumSparkline({ result }: { result: SpectrumResult }) {
  const w = 280;
  const h = 70;
  const { freqs_hz, mag_db } = result;
  if (freqs_hz.length < 2) return null;
  const fMin = 20;
  const fMax = result.sample_rate / 2;
  const dbMin = -120;
  const dbMax = 0;
  const logF = (f: number) => Math.log10(Math.max(f, fMin));
  const x = (f: number) =>
    ((logF(f) - logF(fMin)) / (logF(fMax) - logF(fMin))) * w;
  const y = (db: number) =>
    h - ((Math.max(db, dbMin) - dbMin) / (dbMax - dbMin)) * h;
  let d = "";
  for (let i = 0; i < freqs_hz.length; i++) {
    const fx = freqs_hz[i] ?? 0;
    if (fx < fMin) continue;
    d += `${d ? "L" : "M"}${x(fx).toFixed(1)},${y(mag_db[i] ?? dbMin).toFixed(1)} `;
  }
  return (
    <svg
      width={w}
      height={h}
      style={{ display: "block", marginTop: 4, background: "rgba(127,127,127,0.06)", borderRadius: 4 }}
      role="img"
      aria-label="Espectro de frequências"
    >
      <path d={d} fill="none" stroke="var(--accent, #4ade80)" strokeWidth={1} />
    </svg>
  );
}
