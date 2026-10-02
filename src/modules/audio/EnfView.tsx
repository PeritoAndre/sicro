/**
 * ENF (frequência da rede gravada como zumbido): resumo, curva no tempo com
 * trechos sem ENF e variações bruscas, e comparação com uma referência da rede
 * (outro áudio do caso). Indício, não conclusão.
 */
import { useEffect, useRef, useState } from "react";
import { ClipboardCopy, GitCompareArrows, Loader2 } from "lucide-react";
import { commands } from "@core/commands";
import { toSicroError } from "@core/errors";
import type { EnfComparison, EnfResult } from "@domain/audio";
import styles from "./EnfView.module.css";

const MIN_SNR = 6;
const dec = (v: number, d: number) => v.toFixed(d).replace(".", ",");
const fmtT = (s: number) => {
  const h = Math.floor(s / 3600);
  const m = Math.floor((s % 3600) / 60);
  const r = Math.floor(s % 60);
  return h > 0
    ? `${h}:${String(m).padStart(2, "0")}:${String(r).padStart(2, "0")}`
    : `${m}:${String(r).padStart(2, "0")}`;
};

/** Largura real do elemento (gráficos sem texto esticado). */
function useWidth<T extends HTMLElement>(): [React.RefObject<T>, number] {
  const ref = useRef<T>(null);
  const [w, setW] = useState(900);
  useEffect(() => {
    const el = ref.current;
    if (!el) return;
    const ro = new ResizeObserver(() => setW(Math.max(300, Math.round(el.clientWidth))));
    ro.observe(el);
    return () => ro.disconnect();
  }, []);
  return [ref, w];
}

function enfToText(e: EnfResult, cmp?: EnfComparison | null, refName?: string): string {
  const L: string[] = [];
  L.push(
    `ENF: rede de ${e.nominal_hz} Hz${e.auto ? ` (detectada automaticamente: zumbido ${dec(e.score_60_db, 0)} dB em 60 Hz, ${dec(e.score_50_db, 0)} dB em 50 Hz)` : " (informada)"}.`,
  );
  L.push(
    `Harmônicos usados: ${e.harmonics.map((h) => `${h.k}º (${dec(h.snr_db, 0)} dB, peso ${dec(h.weight * 100, 0)}%)`).join(", ")}. Janela ${dec(e.window_s, 0)} s, passo ${dec(e.step_s, 0)} s.`,
  );
  L.push(
    `ENF confiável (SNR ≥ ${MIN_SNR} dB) em ${dec(e.confidence * 100, 0)}% do áudio. Média ${dec(e.mean_hz, 3)} Hz, desvio-padrão ${dec(e.std_hz, 4)} Hz, maior variação entre quadros ${dec(e.max_jump_hz, 4)} Hz.`,
  );
  L.push(
    e.jumps.length
      ? `Variações bruscas: ${e.jumps.map(([t, d]) => `${fmtT(t)} (${d > 0 ? "+" : ""}${dec(d, 3)} Hz)`).join(", ")}.`
      : "Sem variações bruscas na curva.",
  );
  if (e.gaps.length) L.push(`Trechos sem ENF confiável: ${e.gaps.map(([a, b]) => `${fmtT(a)}–${fmtT(b)}`).join(", ")}.`);
  const m = cmp?.matching;
  if (cmp && refName) {
    L.push(
      m
        ? `Comparação com a referência ${refName}: melhor encaixe a partir de ${fmtT(m.offset_s)} da referência, diferença média ${dec(m.mean_abs_diff_hz * 1000, 1)} mHz, correlação ${dec(m.correlation, 2)} (${m.frames_used} quadros); segundo melhor encaixe: diferença ${m.second_mean_abs_diff_hz != null ? `${dec(m.second_mean_abs_diff_hz * 1000, 1)} mHz` : "—"}.${m.parts.length ? ` Por partes: ${m.parts.map((p) => `${fmtT(p.start_s)}–${fmtT(p.end_s)} do áudio ↔ ${fmtT(p.offset_s + p.start_s)} da referência (r ${dec(p.correlation, 2)})`).join("; ")}${m.parts_disagree ? " — partes encaixam em pontos diferentes da referência (indício de montagem)" : ""}.` : ""}`
        : `Comparação com a referência ${refName}: ENF insuficiente para comparar.`,
    );
  }
  L.push("A ENF é indício técnico: variações e encaixes devem ser conferidos com o restante do exame.");
  return L.join("\n");
}

interface Props {
  workspacePath: string;
  audioId: string;
  enf: EnfResult;
  references: { id: string; filename: string; duration_s: number | null }[];
  onSeek?: (t: number) => void;
}

export function EnfView({ workspacePath, audioId, enf: e, references, onSeek }: Props) {
  const [refId, setRefId] = useState("");
  const [cmp, setCmp] = useState<EnfComparison | null>(null);
  const [busy, setBusy] = useState(false);
  const [err, setErr] = useState<string | null>(null);
  const [copied, setCopied] = useState(false);
  const refName = references.find((r) => r.id === refId)?.filename;

  const compare = async () => {
    if (!refId) return;
    setBusy(true);
    setErr(null);
    try {
      setCmp(await commands.audioEnfCompare(workspacePath, audioId, refId, e.nominal_hz));
    } catch (x) {
      setErr(toSicroError(x).message);
    } finally {
      setBusy(false);
    }
  };

  const conf = e.confidence;
  return (
    <div className={styles.box}>
      <div className={styles.head}>
        <strong>ENF</strong>
        <span>
          rede de <b>{e.nominal_hz} Hz</b>
          {e.auto ? " (automático)" : ""} · ENF confiável em{" "}
          <b className={conf < 0.5 ? styles.warn : undefined}>{Math.round(conf * 100)}%</b> do áudio
        </span>
        <button
          type="button"
          className={styles.copy}
          onClick={() => {
            void navigator.clipboard.writeText(enfToText(e, cmp, refName)).then(() => {
              setCopied(true);
              window.setTimeout(() => setCopied(false), 1500);
            });
          }}
        >
          <ClipboardCopy size={12} /> {copied ? "copiado!" : "Copiar como texto"}
        </button>
      </div>

      <dl className={styles.stats}>
        <dt>Zumbido</dt>
        <dd>
          60 Hz: {dec(e.score_60_db, 0)} dB · 50 Hz: {dec(e.score_50_db, 0)} dB
        </dd>
        <dt>Harmônicos</dt>
        <dd>
          {e.harmonics.map((h) => `${h.k}º ${dec(h.snr_db, 0)} dB (${Math.round(h.weight * 100)}%)`).join(" · ")}
        </dd>
        <dt>Média / desvio</dt>
        <dd>
          {dec(e.mean_hz, 3)} Hz · {dec(e.std_hz, 4)} Hz
        </dd>
        <dt>Variações bruscas</dt>
        <dd className={e.jumps.length ? styles.warn : undefined}>
          {e.jumps.length === 0
            ? "nenhuma"
            : e.jumps.map(([t, d], i) => (
                <button key={i} type="button" className={styles.time} onClick={() => onSeek?.(t)}>
                  {fmtT(t)} ({d > 0 ? "+" : ""}
                  {dec(d * 1000, 0)} mHz)
                </button>
              ))}
        </dd>
        {e.gaps.length > 0 && (
          <>
            <dt>Sem ENF</dt>
            <dd>
              {e.gaps.slice(0, 10).map(([a, b], i) => (
                <button key={i} type="button" className={styles.time} onClick={() => onSeek?.(a)}>
                  {fmtT(a)}–{fmtT(b)}
                </button>
              ))}
              {e.gaps.length > 10 ? " …" : ""}
            </dd>
          </>
        )}
      </dl>

      <EnfChart e={e} onSeek={onSeek} />

      {conf < 0.5 && (
        <p className={styles.note}>
          Pouco zumbido de rede neste áudio: a curva é pouco confiável (equipamento a bateria, filtro ou
          ambiente sem rede elétrica).
        </p>
      )}

      <div className={styles.cmpBar}>
        <GitCompareArrows size={13} aria-hidden />
        <span>Comparar com a referência da rede:</span>
        <select value={refId} onChange={(x) => setRefId(x.target.value)} disabled={busy}>
          <option value="">escolha um áudio do caso…</option>
          {references.map((r) => (
            <option key={r.id} value={r.id}>
              {r.filename}
              {r.duration_s ? ` (${fmtT(r.duration_s)})` : ""}
            </option>
          ))}
        </select>
        <button type="button" className={styles.cmpBtn} disabled={!refId || busy} onClick={() => void compare()}>
          {busy ? <Loader2 size={12} className="spin" /> : null} Comparar
        </button>
      </div>
      {err && <p className={styles.err}>{err}</p>}
      {cmp && <EnfMatchView cmp={cmp} refName={refName ?? "referência"} />}
    </div>
  );
}

/** Curva ENF: linha nos quadros confiáveis, faixas cinza sem ENF, saltos em vermelho. */
function EnfChart({ e, onSeek }: { e: EnfResult; onSeek?: (t: number) => void }) {
  const [box, W] = useWidth<HTMLDivElement>();
  const H = 170;
  const P = { l: 58, r: 10, t: 10, b: 22 };
  if (e.times_s.length < 2) return <div ref={box} />;
  const t0 = e.times_s[0]! - e.step_s / 2;
  const t1 = e.times_s[e.times_s.length - 1]! + e.step_s / 2;
  const ok = e.enf_hz.filter((_, i) => e.snr_db[i]! >= MIN_SNR);
  const lo0 = ok.length ? Math.min(...ok) : e.nominal_hz - 0.05;
  const hi0 = ok.length ? Math.max(...ok) : e.nominal_hz + 0.05;
  const pad = Math.max(0.005, (hi0 - lo0) * 0.15);
  const [lo, hi] = [lo0 - pad, hi0 + pad];
  const x = (t: number) => P.l + ((t - t0) / (t1 - t0)) * (W - P.l - P.r);
  const y = (f: number) => P.t + (1 - (f - lo) / (hi - lo)) * (H - P.t - P.b);
  // Linhas só entre quadros confiáveis seguidos.
  const paths: string[] = [];
  let cur = "";
  e.enf_hz.forEach((f, i) => {
    if (e.snr_db[i]! >= MIN_SNR) {
      cur += `${cur ? "L" : "M"}${x(e.times_s[i]!).toFixed(1)},${y(f).toFixed(1)}`;
    } else if (cur) {
      paths.push(cur);
      cur = "";
    }
  });
  if (cur) paths.push(cur);
  const yTicks = [lo0, (lo0 + hi0) / 2, hi0];
  const xTicks = Array.from({ length: 6 }, (_, i) => t0 + ((t1 - t0) * i) / 5);
  return (
    <div ref={box}>
    <svg
      className={styles.chart}
      viewBox={`0 0 ${W} ${H}`}
      onClick={(ev) => {
        const r = (ev.currentTarget as SVGSVGElement).getBoundingClientRect();
        const t = t0 + (((ev.clientX - r.left) / r.width) * W - P.l) / (W - P.l - P.r) * (t1 - t0);
        if (t >= t0 && t <= t1) onSeek?.(t);
      }}
    >
      {e.gaps.map(([a, b], i) => (
        <rect key={i} x={x(a)} y={P.t} width={Math.max(1, x(b) - x(a))} height={H - P.t - P.b} className={styles.gap} />
      ))}
      {yTicks.map((f, i) => (
        <g key={i}>
          <line x1={P.l} x2={W - P.r} y1={y(f)} y2={y(f)} className={styles.grid} />
          <text x={P.l - 4} y={y(f) + 3} textAnchor="end" className={styles.axis}>
            {dec(f, 3)}
          </text>
        </g>
      ))}
      {xTicks.map((t, i) => (
        <text key={i} x={x(t)} y={H - 6} textAnchor="middle" className={styles.axis}>
          {fmtT(t)}
        </text>
      ))}
      {paths.map((d, i) => (
        <path key={i} d={d} className={styles.line} />
      ))}
      {e.jumps.map(([t], i) => (
        <line key={i} x1={x(t)} x2={x(t)} y1={P.t} y2={H - P.b} className={styles.jump} />
      ))}
    </svg>
    </div>
  );
}

function EnfMatchView({ cmp, refName }: { cmp: EnfComparison; refName: string }) {
  const [box, W] = useWidth<HTMLDivElement>();
  const m = cmp.matching;
  if (!m) {
    return (
      <div className={styles.note} ref={box}>
        Não deu para comparar: a referência precisa ser mais longa que o áudio e as duas precisam de ENF
        confiável em boa parte do tempo (referência {Math.round(cmp.reference.confidence * 100)}%, áudio{" "}
        {Math.round(cmp.question.confidence * 100)}%).
      </div>
    );
  }
  // Único: o 2º melhor lugar fica bem pior (≥ 2× a diferença do 1º).
  const unique = m.second_mean_abs_diff_hz == null || m.second_mean_abs_diff_hz >= 2 * m.mean_abs_diff_hz;
  const q = cmp.question;
  const r = cmp.reference;
  // Curvas alinhadas: o áudio sobre o trecho da referência no encaixe.
  const H = 140;
  const P = { l: 58, r: 10, t: 8, b: 20 };
  const pts = q.times_s.map((t, i) => ({ t, fq: q.enf_hz[i]!, okq: q.snr_db[i]! >= MIN_SNR }));
  const refAt = (t: number) => {
    const idx = Math.round((t + m.offset_s - r.times_s[0]!) / r.step_s);
    return idx >= 0 && idx < r.enf_hz.length && r.snr_db[idx]! >= MIN_SNR ? r.enf_hz[idx]! : null;
  };
  const vals = [...pts.filter((p) => p.okq).map((p) => p.fq), ...pts.map((p) => refAt(p.t)).filter((v): v is number => v != null)];
  const lo = Math.min(...vals) - 0.005;
  const hi = Math.max(...vals) + 0.005;
  const t0 = pts[0]!.t;
  const t1 = pts[pts.length - 1]!.t;
  const x = (t: number) => P.l + ((t - t0) / Math.max(1e-9, t1 - t0)) * (W - P.l - P.r);
  const y = (f: number) => P.t + (1 - (f - lo) / (hi - lo)) * (H - P.t - P.b);
  const line = (get: (p: (typeof pts)[number]) => number | null) => {
    let d = "";
    let open = false;
    for (const p of pts) {
      const v = get(p);
      if (v == null) {
        open = false;
        continue;
      }
      d += `${open ? "L" : "M"}${x(p.t).toFixed(1)},${y(v).toFixed(1)}`;
      open = true;
    }
    return d;
  };
  // Correlação por deslocamento.
  const cv = m.curve_corr.map((c) => c ?? NaN);
  const cmax = m.curve_offsets_s[m.curve_offsets_s.length - 1] || 1;
  const cx = (o: number) => P.l + (o / cmax) * (W - P.l - P.r);
  const cy = (c: number) => 8 + (1 - (c + 1) / 2) * 60;
  let cd = "";
  let copen = false;
  m.curve_offsets_s.forEach((o, i) => {
    const c = cv[i]!;
    if (Number.isNaN(c)) {
      copen = false;
      return;
    }
    cd += `${copen ? "L" : "M"}${cx(o).toFixed(1)},${cy(c).toFixed(1)}`;
    copen = true;
  });

  return (
    <div className={styles.match} ref={box}>
      <p className={unique && m.correlation >= 0.8 && !m.parts_disagree ? styles.good : styles.warn}>
        Melhor encaixe a partir de <b>{fmtT(m.offset_s)}</b> de {refName}: diferença média{" "}
        <b>{dec(m.mean_abs_diff_hz * 1000, 1)} mHz</b>, correlação <b>{dec(m.correlation, 2)}</b> ({m.frames_used}{" "}
        quadros). Segundo melhor lugar:{" "}
        {m.second_mean_abs_diff_hz != null ? `${dec(m.second_mean_abs_diff_hz * 1000, 1)} mHz` : "—"}
        {unique ? "" : " — encaixe pouco único, cautela"}.
      </p>
      {m.parts.length > 0 && (
        <div className={styles.parts}>
          <span>Por partes (~30 s):</span>
          {m.parts.map((p, i) => (
            <span key={i} className={p.correlation >= 0.8 ? undefined : styles.dimPart}>
              {fmtT(p.start_s)}–{fmtT(p.end_s)} ↔ {fmtT(p.offset_s + p.start_s)} (r {dec(p.correlation, 2)})
            </span>
          ))}
          {m.parts_disagree && (
            <b className={styles.warn}>
              partes do áudio encaixam em pontos DIFERENTES da referência — indício de montagem
            </b>
          )}
        </div>
      )}
      <svg className={styles.chart} viewBox={`0 0 ${W} ${H}`}>
        {[lo + 0.005, hi - 0.005].map((f, i) => (
          <text key={i} x={P.l - 4} y={y(f) + 3} textAnchor="end" className={styles.axis}>
            {dec(f, 3)}
          </text>
        ))}
        <path d={line((p) => refAt(p.t))} className={styles.refLine} />
        <path d={line((p) => (p.okq ? p.fq : null))} className={styles.line} />
        <text x={W - P.r} y={H - 5} textAnchor="end" className={styles.axis}>
          áudio (cor) sobre a referência (cinza), alinhados no encaixe
        </text>
      </svg>
      <svg className={styles.corr} viewBox={`0 0 ${W} 84`}>
        <line x1={P.l} x2={W - P.r} y1={cy(0)} y2={cy(0)} className={styles.grid} />
        <path d={cd} className={styles.corrLine} />
        <line x1={cx(m.offset_s - (r.times_s[0]! - q.times_s[0]!))} x2={cx(m.offset_s - (r.times_s[0]! - q.times_s[0]!))} y1={4} y2={72} className={styles.jump} />
        <text x={P.l - 4} y={cy(1) + 4} textAnchor="end" className={styles.axis}>
          r=1
        </text>
        <text x={W - P.r} y={82} textAnchor="end" className={styles.axis}>
          correlação ao longo da referência
        </text>
      </svg>
    </div>
  );
}
