/**
 * Guia do modo Velocidade: Momentos → Referência → Pneu → Resultado. Os passos nunca
 * travam; o cálculo é o mesmo do backend (regressão + Monte Carlo com σ da nitidez).
 */

import { useEffect, useMemo, useState } from "react";
import { Crosshair, Timer } from "lucide-react";
import { Button } from "@components/Button/Button";
import { toSicroError } from "@core/errors";
import type { VideoMedia, VideoStoryboardFrame } from "@domain/video";
import type { TrajectoryPoint, VideoSpeedCalculation, VideoSpeedCalibration } from "@domain/video_speed";
import { useVideoStore } from "../../store/videoStore";
import { formatDuration } from "../format";
import { frameTimestamp, hasActualTimestamp } from "../speed/speedShared";
import { fmt, num, serie } from "./geo";
import { SIGMA_NITIDEZ, useMedirStore, type SpeedDraft } from "./medirStore";
import { describeCalibration, ReferenceGuide } from "./ReferenceGuide";
import { VideoHealth } from "./VideoHealth";
import styles from "./Medir.module.css";

const STEPS = ["Momentos", "Referência", "Pneu", "Resultado"];
const SIGMA_CAL_PX = 1;
const SIGMA_TEMPO_S = 0.01;

export interface MedirActions {
  /** Tempo exato do reprodutor. */
  now: () => number;
  /** Coleta o quadro exato em `t` (ffmpeg) e devolve o quadro do storyboard. */
  collectAt: (t: number, title: string) => Promise<VideoStoryboardFrame | null>;
  seek: (t: number) => void;
  fps: number | null;
}

/** Entradas que, mudando, deixam o resultado velho. */
function signature(cal: VideoSpeedCalibration | null, sp: SpeedDraft, frames: VideoStoryboardFrame[]): string {
  const ms = frames.filter((f) => sp.moments.includes(f.id) && sp.marks[f.id]).map((f) => [f.id, sp.marks[f.id]]);
  return JSON.stringify([cal?.id ?? null, ms, sp.nitidez, sp.sigmaCal, sp.sigmaTempo, sp.sigmaMundo]);
}

export function SpeedGuide({
  workspacePath,
  media,
  author,
  frames,
  warnings,
  actions,
}: {
  workspacePath: string;
  media: VideoMedia;
  author: string;
  frames: VideoStoryboardFrame[];
  warnings: string[];
  actions: MedirActions;
}) {
  const sp = useMedirStore((s) => s.speed);
  const patch = useMedirStore((s) => s.patchSpeed);
  const perito = useMedirStore((s) => s.perito);
  const setPerito = useMedirStore((s) => s.setPerito);
  const cals = useVideoStore((s) => s.speedCalibrations);
  const calcs = useVideoStore((s) => s.speedCalculations);
  const loadSpeedData = useVideoStore((s) => s.loadSpeedData);
  const computeSpeed = useVideoStore((s) => s.computeSpeed);
  const busy = useVideoStore((s) => s.isMutating);
  const [error, setError] = useState<string | null>(null);
  const [collecting, setCollecting] = useState<string | null>(null);

  useEffect(() => {
    void loadSpeedData(workspacePath, media.sha256);
  }, [workspacePath, media.sha256, loadSpeedData]);

  const cal = cals[0] ?? null;
  // Momentos que ainda existem no storyboard, em ordem de tempo.
  const moments = useMemo(
    () => frames.filter((f) => sp.moments.includes(f.id)).sort((a, b) => frameTimestamp(a) - frameTimestamp(b)),
    [frames, sp.moments],
  );
  const marked = moments.filter((f) => sp.marks[f.id]);
  const done = [moments.length >= 2, !!cal, moments.length >= 2 && marked.length === moments.length, !!calcs[0] && sp.calcSig === signature(cal, sp, frames)];
  const sig = signature(cal, sp, frames);
  const result = calcs[0] && sp.calcSig === sig ? calcs[0] : null;

  const go = (step: SpeedDraft["step"]) => {
    setError(null);
    patch({ step, focus: null });
  };

  const addMoment = (f: VideoStoryboardFrame | null) => {
    if (!f) return;
    const cur = useMedirStore.getState().speed.moments;
    if (!cur.includes(f.id)) patch({ moments: [...cur, f.id] });
  };

  const markNow = async () => {
    setError(null);
    const t = actions.now();
    setCollecting("Guardando o quadro exato…");
    try {
      addMoment(await actions.collectAt(t, `Velocidade · ${formatDuration(t)}`));
    } catch (e) {
      setError(toSicroError(e).message);
    } finally {
      setCollecting(null);
    }
  };

  const markSequence = async (count: number, every: number) => {
    setError(null);
    const t0 = actions.now();
    const dt = 1 / (actions.fps && actions.fps > 0 ? actions.fps : 30);
    try {
      for (let i = 0; i < count; i++) {
        const t = t0 + i * every * dt;
        if (media.duration_s != null && t > media.duration_s) break;
        setCollecting(`Guardando ${i + 1} de ${count}…`);
        addMoment(await actions.collectAt(t, `Velocidade · ${formatDuration(t)}`));
      }
    } catch (e) {
      setError(toSicroError(e).message);
    } finally {
      setCollecting(null);
    }
  };

  const compute = async () => {
    setError(null);
    if (!cal) return setError("Falta a referência no chão.");
    if (marked.length < 2) return setError("Marque o pneu em pelo menos 2 momentos.");
    const uPx = SIGMA_NITIDEZ[sp.nitidez];
    const points: TrajectoryPoint[] = marked.map((f) => ({
      storyboard_frame_id: f.id,
      export_id: f.export_id,
      px: sp.marks[f.id]!.x,
      py: sp.marks[f.id]!.y,
      u_px: uPx,
      actual_timestamp_s: frameTimestamp(f),
      delta_s: f.delta_s,
      manual: true,
    }));
    const or = (s: string, d: number) => (s.trim() && num(s) >= 0 ? num(s) : d);
    try {
      await computeSpeed(workspacePath, {
        calibration_id: cal.id,
        points,
        mc_n: 10000,
        mc_sigmas: {
          calibration_px: or(sp.sigmaCal, SIGMA_CAL_PX),
          world_m: or(sp.sigmaMundo, 0),
          trajectory_px: uPx,
          time_s: or(sp.sigmaTempo, SIGMA_TEMPO_S),
        },
        confidence: 0.95,
        author,
      });
      patch({ calcSig: sig, step: 3 });
    } catch (e) {
      setError(toSicroError(e).message);
    }
  };

  const stepper = (
    <div className={styles.head}>
      <div className={styles.steps}>
        {STEPS.map((s, i) => (
          <button
            key={s}
            type="button"
            className={`${styles.stepBtn} ${sp.step === i ? styles.stepOn : done[i] ? styles.stepDone : ""}`}
            aria-current={sp.step === i ? "step" : undefined}
            onClick={() => go(i as SpeedDraft["step"])}
          >
            <b>{done[i] && sp.step !== i ? "✓" : i + 1}</b>
            {s}
          </button>
        ))}
      </div>
    </div>
  );

  const peritoToggle = (
    <label className={styles.perito}>
      <input type="checkbox" checked={perito} onChange={(e) => setPerito(e.target.checked)} />
      Detalhes de perito
    </label>
  );

  let body: JSX.Element;
  if (sp.step === 0) {
    const others = frames.filter((f) => !sp.moments.includes(f.id));
    body = (
      <>
        <h3 className={styles.title}>Quando o veículo passou?</h3>
        <p className={styles.text}>
          Pause o vídeo num instante em que o veículo aparece e toque em <b>Marcar este instante</b>. Dois momentos já bastam; cinco ou mais deixam o
          resultado mais firme.
        </p>
        <div className={styles.row}>
          <Button variant="primary" leftIcon={<Timer size={14} />} disabled={!!collecting} onClick={() => void markNow()}>
            Marcar este instante
          </Button>
          <Button disabled={!!collecting} onClick={() => void markSequence(5, 3)} title="5 quadros a partir daqui, de 3 em 3">
            Marcar 5 seguidos
          </Button>
        </div>
        {collecting && <span className={styles.hint}>{collecting}</span>}
        {moments.length === 0 ? (
          <div className={styles.empty}>
            <span className={styles.emptyIco}>
              <Timer size={16} />
            </span>
            <b>Nenhum momento ainda</b>
            <span className={styles.text}>Cada momento marcado guarda o quadro exato do vídeo e aparece aqui.</span>
          </div>
        ) : (
          <div className={styles.chips}>
            {moments.map((f) => (
              <span key={f.id} className={styles.chip}>
                <button type="button" style={{ color: "inherit", padding: 0 }} onClick={() => actions.seek(frameTimestamp(f))} title="Ir para este instante">
                  {formatDuration(frameTimestamp(f))}
                </button>
                <button type="button" aria-label="Tirar do cálculo" title="Tirar do cálculo (o quadro continua no storyboard)" onClick={() => patch({ moments: sp.moments.filter((id) => id !== f.id) })}>
                  ×
                </button>
              </span>
            ))}
          </div>
        )}
        {others.length > 0 && (
          <label className={styles.field}>
            Usar um quadro já coletado
            <select value="" onChange={(e) => addMoment(frames.find((f) => f.id === e.target.value) ?? null)}>
              <option value="">Escolher…</option>
              {others.map((f) => (
                <option key={f.id} value={f.id}>
                  {formatDuration(frameTimestamp(f))} — {f.title}
                </option>
              ))}
            </select>
          </label>
        )}
        {moments.length >= 2 && (
          <Button variant="primary" onClick={() => go(1)}>
            Continuar: referência no chão →
          </Button>
        )}
        {perito && (
          <div className={styles.techBox}>
            <span>Cada momento é extraído pelo ffmpeg no pts do quadro mostrado (frame-accurate) e entra na galeria do storyboard.</span>
            <span>fps declarado: <b>{media.fps_declared != null ? fmt(media.fps_declared, 3) : "—"}</b></span>
          </div>
        )}
      </>
    );
  } else if (sp.step === 1) {
    body = <ReferenceGuide workspacePath={workspacePath} media={media} author={author} onContinue={() => go(2)} continueLabel="Continuar: marcar o pneu →" />;
  } else if (sp.step === 2) {
    const missing: [string, SpeedDraft["step"]][] = [];
    if (moments.length < 2) missing.push(["Marque pelo menos 2 momentos.", 0]);
    if (!cal) missing.push(["Faça a referência no chão.", 1]);
    const cur = sp.focus ? moments.find((f) => f.id === sp.focus) : moments.find((f) => !sp.marks[f.id]);
    body = missing.length ? (
      <>
        <h3 className={styles.title}>Falta pouco</h3>
        {missing.map(([t, s]) => (
          <div key={t} className={styles.empty}>
            <b>{t}</b>
            <Button variant="primary" size="sm" onClick={() => go(s)}>
              Ir para {STEPS[s]!.toLowerCase()}
            </Button>
          </div>
        ))}
      </>
    ) : (
      <>
        <h3 className={styles.title}>{cur ? "Onde o pneu toca o chão?" : "Tudo marcado"}</h3>
        <p className={styles.text}>
          {cur ? (
            <>
              Clique no ponto em que um <b>pneu</b> encosta no asfalto, sempre o mesmo pneu. O SICRO pula para o próximo momento sozinho.
            </>
          ) : (
            "Os pontos azuis formam o caminho do veículo. Para refazer um, toque no momento."
          )}
        </p>
        <div className={styles.chips}>
          {moments.map((f) => (
            <span key={f.id} className={`${styles.chip} ${sp.marks[f.id] ? styles.chipOk : ""} ${cur?.id === f.id ? styles.chipCur : ""}`}>
              <button type="button" style={{ color: "inherit", padding: 0 }} onClick={() => patch({ focus: f.id })}>
                {sp.marks[f.id] ? "✓" : "○"} {formatDuration(frameTimestamp(f))}
              </button>
            </span>
          ))}
        </div>
        <div className={styles.field}>
          Quão nítido está o pneu?
          <div className={styles.seg}>
            {([
              ["nitido", "Nítido"],
              ["normal", "Normal"],
              ["borrado", "Borrado"],
            ] as const).map(([k, t]) => (
              <button key={k} type="button" className={sp.nitidez === k ? styles.segOn : ""} onClick={() => patch({ nitidez: k })}>
                {t}
              </button>
            ))}
          </div>
        </div>
        <span className={styles.hint}>
          {marked.length} de {moments.length} marcados · a nitidez entra na faixa de incerteza
        </span>
        {perito && (
          <div className={styles.techBox}>
            <span>
              σ marcação = <b>{SIGMA_NITIDEZ[sp.nitidez]} px</b> (pela nitidez)
            </span>
            <div className={styles.sigmas}>
              <label className={styles.field}>
                σ calibração (px)
                <input value={sp.sigmaCal} placeholder={String(SIGMA_CAL_PX)} onChange={(e) => patch({ sigmaCal: e.target.value })} />
              </label>
              <label className={styles.field}>
                σ tempo (s)
                <input value={sp.sigmaTempo} placeholder={String(SIGMA_TEMPO_S)} onChange={(e) => patch({ sigmaTempo: e.target.value })} />
              </label>
              <label className={styles.field}>
                σ medida real (m)
                <input value={sp.sigmaMundo} placeholder="0" onChange={(e) => patch({ sigmaMundo: e.target.value })} />
              </label>
            </div>
            <span>Pneu-solo: o ponto fica no plano da referência e evita paralaxe.</span>
          </div>
        )}
        <Button variant="primary" leftIcon={<Crosshair size={14} />} disabled={busy || marked.length < 2} onClick={() => void compute()}>
          {busy ? "Calculando…" : "Ver a velocidade"}
        </Button>
      </>
    );
  } else {
    body = result && cal ? (
      <SpeedResult calc={result} cal={cal} frames={frames} perito={perito} />
    ) : (
      <>
        <h3 className={styles.title}>{calcs[0] ? "As marcas mudaram" : "Ainda sem resultado"}</h3>
        <div className={styles.empty}>
          <b>{calcs[0] ? "Recalcule para ver o número com as marcas atuais." : "O número aparece aqui."}</b>
          <span className={styles.text}>Precisa de 2 momentos, a referência no chão e o pneu marcado em cada momento.</span>
          {marked.length >= 2 && cal ? (
            <Button variant="primary" size="sm" disabled={busy} onClick={() => void compute()}>
              {calcs[0] ? "Recalcular" : "Calcular"}
            </Button>
          ) : (
            <Button variant="primary" size="sm" onClick={() => go(moments.length < 2 ? 0 : !cal ? 1 : 2)}>
              Continuar de onde parei
            </Button>
          )}
        </div>
      </>
    );
  }

  return (
    <div className={styles.guide}>
      {stepper}
      {body}
      {error && <span className={styles.error}>{error}</span>}
      <VideoHealth warnings={warnings} perito={perito} />
      {peritoToggle}
    </div>
  );
}

// ---------------------------------------------------------------------------
// Resultado

function SpeedResult({
  calc,
  cal,
  frames,
  perito,
}: {
  calc: VideoSpeedCalculation;
  cal: VideoSpeedCalibration;
  frames: VideoStoryboardFrame[];
  perito: boolean;
}) {
  const [copied, setCopied] = useState(false);
  const hasMc = calc.mc_p2_5_kmh != null && calc.mc_p97_5_kmh != null;
  const hasCi = calc.ci_low != null && calc.ci_high != null;
  const lo = hasMc ? calc.mc_p2_5_kmh! : hasCi ? calc.ci_low! : null;
  const hi = hasMc ? calc.mc_p97_5_kmh! : hasCi ? calc.ci_high! : null;
  const ser = serie(cal, calc.points);
  const n = calc.points.length;
  const r2 = calc.r_squared;
  const anyVfr = calc.points.some((p) => {
    const f = frames.find((x) => x.id === p.storyboard_frame_id);
    return f ? !hasActualTimestamp(f) : false;
  });

  const laudo = [
    `Velocidade estimada do veículo: ${fmt(calc.velocity_kmh, 1)} km/h`,
    hasMc ? `, faixa provável de ${fmt(calc.mc_p2_5_kmh!, 1)} a ${fmt(calc.mc_p97_5_kmh!, 1)} km/h (p2,5–p97,5 de ${calc.mc_n ?? 0} simulações Monte Carlo, semente ${calc.mc_seed})` : "",
    hasCi ? `; margem do ajuste por regressão de ${fmt(calc.ci_low!, 1)} a ${fmt(calc.ci_high!, 1)} km/h (IC 95%${r2 != null ? `, R² = ${fmt(r2, 4)}` : ""})` : "",
    `. Referência no chão: ${describeCalibration(cal).toLowerCase()} (${cal.reference_source === "campo" ? "medida no local" : cal.reference_source === "norma_viaria" ? "norma viária, presumida" : "entre-eixos do veículo"}). `,
    `Posição do veículo marcada no contato pneu-solo em ${n} quadros entre ${formatDuration(calc.points[0]!.actual_timestamp_s)} e ${formatDuration(calc.points[n - 1]!.actual_timestamp_s)}.`,
    calc.limitations.length ? ` Ressalvas: ${calc.limitations.join(" ")}` : "",
  ].join("");

  const copy = async () => {
    try {
      await navigator.clipboard.writeText(laudo);
      setCopied(true);
      setTimeout(() => setCopied(false), 2000);
    } catch {
      setCopied(false);
    }
  };

  return (
    <>
      <p className={styles.big}>
        O veículo passava a cerca de
        <strong>{fmt(calc.velocity_kmh, 0)} km/h</strong>
      </p>
      <p className={styles.text}>
        {hasMc ? (
          <>
            Muito provavelmente entre <b>{fmt(lo!, 0)} e {fmt(hi!, 0)} km/h</b>.
          </>
        ) : hasCi ? (
          <>
            Pela margem do ajuste, entre <b>{fmt(lo!, 0)} e {fmt(hi!, 0)} km/h</b>.
          </>
        ) : (
          "Com 2 momentos não há faixa de incerteza: marque 3 ou mais."
        )}
        {!hasMc && cal.method === "line" && n > 2 && " A faixa provável precisa de um retângulo ou de marcas em fila."}
      </p>
      <Band v={calc.velocity_kmh} lo={lo} hi={hi} />
      {ser && <Chart t={ser.t} s={ser.s} v={ser.v} b0={ser.b0} />}
      <span className={styles.hint}>Pontos na reta = velocidade constante; um ponto fora indica clique ruim, freada ou aceleração.</span>
      <div style={{ display: "grid", gap: 6 }}>
        <div className={styles.check}>
          <i className={n >= 3 ? styles.ok : styles.warn}>{n >= 3 ? "✓" : "!"}</i>
          <span>{n} momentos{n < 3 ? ": com 3 ou mais aparece a margem do ajuste." : "."}</span>
        </div>
        {r2 != null && (
          <div className={styles.check}>
            <i className={r2 > 0.99 ? styles.ok : styles.warn}>{r2 > 0.99 ? "✓" : "!"}</i>
            <span>{r2 > 0.99 ? "Pontos bem alinhados." : "Pontos fora da reta: confira as marcas ou se o veículo freou."}</span>
          </div>
        )}
        <div className={styles.check}>
          <i className={cal.reference_source === "campo" ? styles.ok : styles.warn}>{cal.reference_source === "campo" ? "✓" : "!"}</i>
          <span>{cal.reference_source === "campo" ? "Referência medida no local." : cal.reference_source === "norma_viaria" ? "Referência pela norma: vale conferir no local." : "Referência pelo entre-eixos: confirme o modelo."}</span>
        </div>
        {anyVfr && (
          <div className={styles.check}>
            <i className={styles.warn}>!</i>
            <span>Algum quadro ficou sem horário real; foi usado o horário pedido.</span>
          </div>
        )}
      </div>
      <details className={styles.how} open={perito}>
        <summary>Como chegamos aqui</summary>
        <p className={styles.text}>
          A referência no chão transforma cada clique em metros. A distância andada entre os momentos, dividida pelo tempo real entre os quadros, dá a
          velocidade. A faixa provável vem de repetir a conta milhares de vezes com pequenos erros de clique, de referência e de tempo.
        </p>
        <div className={styles.techBox}>
          <div className={styles.kv}>
            <span>velocidade</span>
            <b>
              {fmt(calc.velocity_kmh, 2)} km/h · {fmt(calc.velocity_kmh / 3.6, 3)} m/s
            </b>
            <span>vx · vy</span>
            <b>
              {fmt(calc.vx_m_per_s, 2)} · {fmt(calc.vy_m_per_s, 2)} m/s
            </b>
            <span>IC 95% (regressão)</span>
            <b>{hasCi ? `${fmt(calc.ci_low!, 1)} – ${fmt(calc.ci_high!, 1)} km/h` : "— (2 pontos)"}</b>
            <span>Monte Carlo p2,5–p97,5</span>
            <b>{hasMc ? `${fmt(calc.mc_p2_5_kmh!, 1)} – ${fmt(calc.mc_p97_5_kmh!, 1)} km/h · mediana ${fmt(calc.mc_median_kmh ?? 0, 1)}` : "não executado"}</b>
            {hasMc && (
              <>
                <span>simulações</span>
                <b>
                  {calc.mc_n} · semente {calc.mc_seed}
                  {calc.mc_failed ? ` · ${calc.mc_failed} descartadas` : ""}
                </b>
              </>
            )}
            {calc.mc_sigmas && (
              <>
                <span>σ</span>
                <b>
                  clique {calc.mc_sigmas.trajectory_px} px · ref. {calc.mc_sigmas.calibration_px} px · tempo {calc.mc_sigmas.time_s} s · medida {calc.mc_sigmas.world_m} m
                </b>
              </>
            )}
            <span>R² · EP</span>
            <b>
              {r2 != null ? fmt(r2, 5) : "—"} · {calc.se_m_per_s != null ? `${fmt(calc.se_m_per_s, 3)} m/s` : "—"}
            </b>
          </div>
          {ser && (
            <table className={styles.pts}>
              <thead>
                <tr>
                  <th>t (s)</th>
                  <th>px</th>
                  <th>py</th>
                  <th>s (m)</th>
                </tr>
              </thead>
              <tbody>
                {calc.points.map((p, i) => (
                  <tr key={i}>
                    <td>{fmt(p.actual_timestamp_s, 3)}</td>
                    <td>{fmt(p.px, 1)}</td>
                    <td>{fmt(p.py, 1)}</td>
                    <td>{fmt(ser.s[i]!, 2)}</td>
                  </tr>
                ))}
              </tbody>
            </table>
          )}
          {calc.limitations.length > 0 && (
            <>
              <span>Ressalvas técnicas</span>
              <ul style={{ margin: 0, paddingLeft: 16 }}>
                {calc.limitations.map((l, i) => (
                  <li key={i}>{l}</li>
                ))}
              </ul>
            </>
          )}
        </div>
      </details>
      <div className={styles.row}>
        <Button variant="primary" onClick={() => void copy()}>
          {copied ? "Copiado" : "Copiar texto para o laudo"}
        </Button>
      </div>
    </>
  );
}

/** Faixa provável sobre uma régua de km/h. */
function Band({ v, lo, hi }: { v: number; lo: number | null; hi: number | null }) {
  const max = Math.max(100, Math.ceil(((hi ?? v) * 1.25) / 20) * 20);
  const W = 300;
  const X = (k: number) => 10 + (k / max) * (W - 20);
  const ticks: number[] = [];
  for (let k = 0; k <= max; k += max > 200 ? 40 : 20) ticks.push(k);
  return (
    <svg className={styles.svg} viewBox={`0 0 ${W} 44`} role="img" aria-label="Faixa provável da velocidade">
      <rect x={10} y={13} width={W - 20} height={6} rx={3} fill="var(--sicro-surface-3)" />
      {lo != null && hi != null && <rect x={X(lo)} y={9} width={Math.max(3, X(hi) - X(lo))} height={14} rx={4} fill="var(--sicro-accent)" opacity={0.35} />}
      <rect x={X(v) - 1.5} y={5} width={3} height={22} fill="var(--sicro-accent)" />
      {ticks.map((k) => (
        <text key={k} x={X(k)} y={40} textAnchor="middle" fontSize={9.5} fill="var(--sicro-fg-dim)" fontFamily="var(--font-mono)">
          {k}
        </text>
      ))}
    </svg>
  );
}

/** Posição × tempo com a reta ajustada. */
function Chart({ t, s, v, b0 }: { t: number[]; s: number[]; v: number; b0: number }) {
  const W = 300;
  const H = 150;
  const l = 32;
  const b = 22;
  const t0 = t[0]!;
  const t1 = t[t.length - 1]!;
  const smax = Math.max(...s, 0.1) * 1.08;
  const smin = Math.min(...s, 0);
  const X = (x: number) => l + ((x - t0) / (t1 - t0 || 1)) * (W - l - 8);
  const Y = (y: number) => H - b - ((y - smin) / (smax - smin || 1)) * (H - b - 10);
  const grid = [0, 0.25, 0.5, 0.75, 1].map((k) => smin + (smax - smin) * k);
  return (
    <svg className={styles.svg} viewBox={`0 0 ${W} ${H}`} role="img" aria-label="Posição do veículo pelo tempo">
      {grid.map((g, i) => (
        <g key={i}>
          <line x1={l} x2={W - 6} y1={Y(g)} y2={Y(g)} stroke="var(--sicro-divider)" />
          <text x={l - 5} y={Y(g) + 3} textAnchor="end" fontSize={9} fill="var(--sicro-fg-dim)" fontFamily="var(--font-mono)">
            {fmt(g, 0)}
          </text>
        </g>
      ))}
      <text x={4} y={10} fontSize={9} fill="var(--sicro-fg-dim)" fontFamily="var(--font-mono)">
        m
      </text>
      <text x={X(t0)} y={H - 6} textAnchor="start" fontSize={9} fill="var(--sicro-fg-dim)" fontFamily="var(--font-mono)">
        {fmt(t0, 2)} s
      </text>
      <text x={X(t1)} y={H - 6} textAnchor="end" fontSize={9} fill="var(--sicro-fg-dim)" fontFamily="var(--font-mono)">
        {fmt(t1, 2)} s
      </text>
      <line x1={X(t0)} y1={Y(b0 + v * t0)} x2={X(t1)} y2={Y(b0 + v * t1)} stroke="var(--sicro-accent)" strokeWidth={2} />
      {t.map((x, i) => (
        <circle key={i} cx={X(x)} cy={Y(s[i]!)} r={3.6} fill="var(--sicro-fg)" />
      ))}
    </svg>
  );
}
