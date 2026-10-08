/** Guia do modo Distância: Quadro → Referência (a mesma da Velocidade) → Medir. */

import { useEffect, useState } from "react";
import { Ruler, Timer } from "lucide-react";
import { Button } from "@components/Button/Button";
import { toSicroError } from "@core/errors";
import type { VideoMedia, VideoStoryboardFrame } from "@domain/video";
import { useVideoStore } from "../../store/videoStore";
import { formatDuration } from "../format";
import { frameTimestamp } from "../speed/speedShared";
import { fmt } from "./geo";
import { SIGMA_NITIDEZ, useMedirStore, type DistDraft } from "./medirStore";
import { ReferenceGuide } from "./ReferenceGuide";
import type { MedirActions } from "./SpeedGuide";
import { VideoHealth } from "./VideoHealth";
import styles from "./Medir.module.css";

const STEPS = ["Quadro", "Referência", "Medir"];

export function DistanceGuide({
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
  const d = useMedirStore((s) => s.dist);
  const patch = useMedirStore((s) => s.patchDist);
  const perito = useMedirStore((s) => s.perito);
  const setPerito = useMedirStore((s) => s.setPerito);
  const cals = useVideoStore((s) => s.speedCalibrations);
  const measurements = useVideoStore((s) => s.distanceMeasurements);
  const loadDistanceData = useVideoStore((s) => s.loadDistanceData);
  const createDistance = useVideoStore((s) => s.createDistanceMeasurement);
  const busy = useVideoStore((s) => s.isMutating);
  const [error, setError] = useState<string | null>(null);
  const [collecting, setCollecting] = useState(false);

  useEffect(() => {
    void loadDistanceData(workspacePath, media.sha256);
  }, [workspacePath, media.sha256, loadDistanceData]);

  const cal = cals[0] ?? null;
  const frame = frames.find((f) => f.id === d.frameId) ?? null;
  const done = [!!frame, !!cal, measurements.some((m) => m.calibration_id === cal?.id)];
  const go = (step: DistDraft["step"]) => {
    setError(null);
    patch({ step });
  };

  const markNow = async () => {
    setError(null);
    setCollecting(true);
    try {
      const t = actions.now();
      const f = await actions.collectAt(t, `Distância · ${formatDuration(t)}`);
      if (f) patch({ frameId: f.id, points: [] });
    } catch (e) {
      setError(toSicroError(e).message);
    } finally {
      setCollecting(false);
    }
  };

  const measure = async () => {
    setError(null);
    if (!cal) return setError("Falta a referência no chão.");
    if (d.points.length !== 2) return setError("Clique nas duas pontas.");
    const [a, b] = d.points as [{ x: number; y: number }, { x: number; y: number }];
    try {
      await createDistance(workspacePath, {
        calibration_id: cal.id,
        p1_px: a.x,
        p1_py: a.y,
        p2_px: b.x,
        p2_py: b.y,
        mc_n: 10000,
        mc_sigmas: { calibration_px: 1, world_m: 0, measure_px: SIGMA_NITIDEZ[d.nitidez] },
        author,
      });
      patch({ points: [] });
    } catch (e) {
      setError(toSicroError(e).message);
    }
  };

  let body: JSX.Element;
  if (d.step === 0) {
    body = (
      <>
        <h3 className={styles.title}>Em qual quadro está o que você quer medir?</h3>
        <p className={styles.text}>
          Pause o vídeo e toque em <b>Marcar este instante</b>, ou escolha um quadro já coletado.
        </p>
        <Button variant="primary" leftIcon={<Timer size={14} />} disabled={collecting} onClick={() => void markNow()}>
          Marcar este instante
        </Button>
        {frames.length > 0 && (
          <label className={styles.field}>
            Usar um quadro já coletado
            <select value={d.frameId ?? ""} onChange={(e) => patch({ frameId: e.target.value || null, points: [] })}>
              <option value="">Escolher…</option>
              {frames.map((f) => (
                <option key={f.id} value={f.id}>
                  {formatDuration(frameTimestamp(f))} — {f.title}
                </option>
              ))}
            </select>
          </label>
        )}
        {frame && (
          <>
            <div className={styles.check}>
              <i className={styles.ok}>✓</i>
              <span>Quadro de {formatDuration(frameTimestamp(frame))}</span>
            </div>
            <Button variant="primary" onClick={() => go(cal ? 2 : 1)}>
              {cal ? "Continuar: medir →" : "Continuar: referência no chão →"}
            </Button>
          </>
        )}
      </>
    );
  } else if (d.step === 1) {
    body = <ReferenceGuide workspacePath={workspacePath} media={media} author={author} onContinue={() => go(2)} continueLabel="Continuar: medir →" />;
  } else {
    const mine = measurements.filter((m) => m.calibration_id === cal?.id);
    const last = mine[0] ?? null;
    body =
      !frame || !cal ? (
        <>
          <h3 className={styles.title}>Falta pouco</h3>
          {!frame && (
            <div className={styles.empty}>
              <b>Escolha o quadro.</b>
              <Button variant="primary" size="sm" onClick={() => go(0)}>
                Ir para quadro
              </Button>
            </div>
          )}
          {!cal && (
            <div className={styles.empty}>
              <b>Faça a referência no chão.</b>
              <Button variant="primary" size="sm" onClick={() => go(1)}>
                Ir para referência
              </Button>
            </div>
          )}
        </>
      ) : (
        <>
          <h3 className={styles.title}>Clique nas duas pontas</h3>
          <p className={styles.text}>As duas pontas precisam estar no chão (no mesmo plano da referência).</p>
          <div className={styles.field}>
            Quão nítidas estão as pontas?
            <div className={styles.seg}>
              {([
                ["nitido", "Nítidas"],
                ["normal", "Normais"],
                ["borrado", "Borradas"],
              ] as const).map(([k, t]) => (
                <button key={k} type="button" className={d.nitidez === k ? styles.segOn : ""} onClick={() => patch({ nitidez: k })}>
                  {t}
                </button>
              ))}
            </div>
          </div>
          <div className={styles.row}>
            <Button variant="primary" leftIcon={<Ruler size={14} />} disabled={busy || d.points.length !== 2} onClick={() => void measure()}>
              Medir
            </Button>
            <Button size="sm" disabled={d.points.length === 0} onClick={() => patch({ points: [] })}>
              Limpar pontos
            </Button>
          </div>
          {last && (
            <>
              <p className={styles.big}>
                A distância é de
                <strong>{fmt(last.distance_m, 2)} m</strong>
              </p>
              {last.mc_p2_5_m != null && last.mc_p97_5_m != null && (
                <p className={styles.text}>
                  Muito provavelmente entre <b>{fmt(last.mc_p2_5_m, 2)} e {fmt(last.mc_p97_5_m, 2)} m</b>.
                </p>
              )}
              {perito && (
                <div className={styles.techBox}>
                  <div className={styles.kv}>
                    <span>p1 → p2</span>
                    <b>
                      ({fmt(last.p1_px, 0)}, {fmt(last.p1_py, 0)}) → ({fmt(last.p2_px, 0)}, {fmt(last.p2_py, 0)}) px
                    </b>
                    <span>Monte Carlo</span>
                    <b>{last.mc_n != null ? `N ${last.mc_n} · semente ${last.mc_seed} · mediana ${fmt(last.mc_median_m ?? 0, 3)} m` : "não executado"}</b>
                  </div>
                  {last.limitations.length > 0 && (
                    <ul style={{ margin: 0, paddingLeft: 16 }}>
                      {last.limitations.map((l, i) => (
                        <li key={i}>{l}</li>
                      ))}
                    </ul>
                  )}
                </div>
              )}
              {mine.length > 1 && (
                <div className={styles.chips}>
                  {mine.slice(1, 7).map((m) => (
                    <span key={m.id} className={styles.chip} style={{ paddingRight: 9 }}>
                      {fmt(m.distance_m, 2)} m
                    </span>
                  ))}
                </div>
              )}
            </>
          )}
        </>
      );
  }

  return (
    <div className={styles.guide}>
      <div className={styles.head}>
        <div className={styles.steps}>
          {STEPS.map((s, i) => (
            <button
              key={s}
              type="button"
              className={`${styles.stepBtn} ${d.step === i ? styles.stepOn : done[i] ? styles.stepDone : ""}`}
              aria-current={d.step === i ? "step" : undefined}
              onClick={() => go(i as DistDraft["step"])}
            >
              <b>{done[i] && d.step !== i ? "✓" : i + 1}</b>
              {s}
            </button>
          ))}
        </div>
      </div>
      {body}
      {error && <span className={styles.error}>{error}</span>}
      <VideoHealth warnings={warnings} perito={perito} />
      <label className={styles.perito}>
        <input type="checkbox" checked={perito} onChange={(e) => setPerito(e.target.checked)} />
        Detalhes de perito
      </label>
    </div>
  );
}
