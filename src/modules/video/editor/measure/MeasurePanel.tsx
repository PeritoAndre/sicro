/**
 * MeasurePanel — aba "Medições" do editor de vídeo (distância por fotogrametria).
 *
 * Princípio de arquitetura: esta aba NÃO cria um conceito próprio de
 * calibração. Ela COMPARTILHA a calibração da aba Velocidade (a geometria da
 * cena, uma só) — seleciona uma já existente ou cria uma nova reusando o mesmo
 * `CalibrationControls`. Cada medição referencia a `calibration_id` escolhida.
 *
 * Fluxo em 2 passos sobre FRAMES COLETADOS:
 *   1. Calibração: escolha uma existente OU crie uma nova.
 *   2. Medição: marque EXATAMENTE 2 pontos (extremidades), SOBRE o plano
 *      calibrado / contato com o solo (paralaxe). Informe σ se quiser intervalo
 *      de incerteza — sem σ, sai só a distância pontual (KNOWN_LIMITATIONS §13).
 */

import { useEffect, useMemo, useState } from "react";
import { Button } from "@components/Button/Button";
import { toSicroError } from "@core/errors";
import type { VideoMedia, VideoStoryboardFrame } from "@domain/video";
import type { ControlPoint, VideoSpeedCalibration } from "@domain/video_speed";
import type { CreateDistanceMeasurementInput } from "@domain/video_distance";
import { useVideoStore } from "../../store/videoStore";
import { formatDuration } from "../format";
import { FrameCanvas, type FrameMarker, type FramePoint } from "../speed/FrameCanvas";
import {
  CalibrationControls,
  type Method,
  type RefSource,
} from "../speed/SpeedPanel";
import { frameAssetSrc, frameTimestamp } from "../speed/speedShared";
import { DistanceResultCard } from "./DistanceResultCard";
import styles from "../speed/SpeedPanel.module.css";
import mstyles from "./MeasurePanel.module.css";

type Step = "calibrate" | "measure";

const MAX_CR_POINTS = 12;

interface Props {
  workspacePath: string;
  media: VideoMedia;
  frames: VideoStoryboardFrame[];
  /** Autor (perito) vindo do contexto do app — nunca vazio aqui. */
  author: string;
}

export function MeasurePanel({ workspacePath, media, frames, author }: Props) {
  const speedCalibrations = useVideoStore((s) => s.speedCalibrations);
  const distanceMeasurements = useVideoStore((s) => s.distanceMeasurements);
  const loadDistanceData = useVideoStore((s) => s.loadDistanceData);
  const createCalibration = useVideoStore((s) => s.createCalibration);
  const createDistanceMeasurement = useVideoStore((s) => s.createDistanceMeasurement);
  const isMutating = useVideoStore((s) => s.isMutating);

  const [step, setStep] = useState<Step>("calibrate");
  const [selectedCalibrationId, setSelectedCalibrationId] = useState<string | null>(
    null,
  );
  const [creatingNew, setCreatingNew] = useState(false);

  // Rascunho de calibração (reusa CalibrationControls).
  const [calMethod, setCalMethod] = useState<Method>("plane");
  const [refSource, setRefSource] = useState<RefSource>("campo");
  const [calFrameId, setCalFrameId] = useState<string | null>(null);
  const [calPoints, setCalPoints] = useState<FramePoint[]>([]);
  const [lineDistanceM, setLineDistanceM] = useState("");
  const [planeW, setPlaneW] = useState("");
  const [planeH, setPlaneH] = useState("");
  const [crPositions, setCrPositions] = useState<string[]>([]);

  // Marcação da medição (2 pontos num frame).
  const [measureFrameId, setMeasureFrameId] = useState<string | null>(null);
  const [measurePoints, setMeasurePoints] = useState<FramePoint[]>([]);

  // Incertezas (σ) — EM BRANCO por padrão (sem σ ⇒ só distância pontual).
  const [sigmaCalPx, setSigmaCalPx] = useState("");
  const [sigmaWorldM, setSigmaWorldM] = useState("");
  const [sigmaMeasurePx, setSigmaMeasurePx] = useState("");
  const [mcN, setMcN] = useState("10000");

  const [selectedMeasurementId, setSelectedMeasurementId] = useState<string | null>(
    null,
  );
  const [feedback, setFeedback] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);

  // Carrega medições + (re)carrega as calibrações compartilhadas da mídia.
  useEffect(() => {
    void loadDistanceData(workspacePath, media.sha256);
  }, [workspacePath, media.sha256, loadDistanceData]);

  // Seleciona a calibração mais recente e pula para a medição.
  useEffect(() => {
    if (selectedCalibrationId === null && speedCalibrations.length > 0) {
      setSelectedCalibrationId(speedCalibrations[0]!.id);
      setStep("measure");
    }
  }, [speedCalibrations, selectedCalibrationId]);

  useEffect(() => {
    if (frames.length === 0) return;
    const firstId = frames[0]!.id;
    setCalFrameId((id) => id ?? firstId);
    setMeasureFrameId((id) => id ?? firstId);
  }, [frames]);

  const requiredPts = calMethod === "plane" ? 4 : calMethod === "line" ? 2 : 3;
  const activeCalibration: VideoSpeedCalibration | null = useMemo(
    () => speedCalibrations.find((c) => c.id === selectedCalibrationId) ?? null,
    [speedCalibrations, selectedCalibrationId],
  );
  const calFrame = frames.find((f) => f.id === calFrameId) ?? null;
  const measureFrame = frames.find((f) => f.id === measureFrameId) ?? null;

  const result = useMemo(
    () =>
      distanceMeasurements.find((x) => x.id === selectedMeasurementId) ??
      distanceMeasurements[0] ??
      null,
    [distanceMeasurements, selectedMeasurementId],
  );

  // O editor de calibração aparece quando não há calibração OU o perito pediu.
  const showCalEditor =
    step === "calibrate" && (creatingNew || speedCalibrations.length === 0);

  // --- Calibração (rascunho) ---
  const onAddCalPoint = (x: number, y: number) => {
    if (calMethod === "cross_ratio") {
      setCalPoints((pts) => (pts.length >= MAX_CR_POINTS ? pts : [...pts, { x, y }]));
      setCrPositions((ds) => (ds.length >= MAX_CR_POINTS ? ds : [...ds, ""]));
      return;
    }
    setCalPoints((pts) => (pts.length >= requiredPts ? pts : [...pts, { x, y }]));
  };
  const setCrPosition = (i: number, v: string) => {
    setCrPositions((ds) => {
      const next = [...ds];
      next[i] = v;
      return next;
    });
  };
  const resetCalDraft = () => {
    setCalPoints([]);
    setCrPositions([]);
  };

  const handleSaveCalibration = async () => {
    setError(null);
    if (!calFrame) {
      setError("Selecione um frame coletado para calibrar.");
      return;
    }
    let control_points: ControlPoint[];
    if (calMethod === "plane") {
      if (calPoints.length !== 4) {
        setError("Marque exatamente 4 cantos no frame.");
        return;
      }
      const w = parseNum(planeW);
      const h = parseNum(planeH);
      if (!(w > 0) || !(h > 0)) {
        setError("Informe largura e altura reais do retângulo (> 0 m).");
        return;
      }
      control_points = calPoints.map((p, i) => {
        const world = planeWorld(i, w, h);
        return { px: p.x, py: p.y, world_x_m: world.x, world_y_m: world.y, label: corner(i) };
      });
    } else if (calMethod === "line") {
      if (calPoints.length !== 2) {
        setError("Marque exatamente 2 pontos no frame.");
        return;
      }
      const d = parseNum(lineDistanceM);
      if (!(d > 0)) {
        setError("Informe a distância real do segmento (> 0 m).");
        return;
      }
      const p0 = calPoints[0]!;
      const p1 = calPoints[1]!;
      control_points = [
        { px: p0.x, py: p0.y, world_x_m: 0, world_y_m: 0, label: "A" },
        { px: p1.x, py: p1.y, world_x_m: d, world_y_m: 0, label: "B" },
      ];
    } else {
      if (calPoints.length < 3) {
        setError("Marque pelo menos 3 pontos colineares ao longo da via.");
        return;
      }
      const positions = crPositions.slice(0, calPoints.length).map(parseNum);
      if (positions.length !== calPoints.length || positions.some((d) => !Number.isFinite(d))) {
        setError("Informe a posição (m) de cada ponto marcado.");
        return;
      }
      const sorted = [...positions].sort((a, b) => a - b);
      const allDistinct = sorted.every(
        (v, i) => i === 0 || Math.abs(v - sorted[i - 1]!) > 1e-9,
      );
      if (!allDistinct) {
        setError("As posições ao longo da via devem ser distintas.");
        return;
      }
      control_points = calPoints.map((p, i) => ({
        px: p.x,
        py: p.y,
        world_x_m: positions[i]!,
        world_y_m: 0,
        label: String(i + 1),
      }));
    }
    try {
      const cal = await createCalibration(workspacePath, {
        media_hash: media.sha256,
        method: calMethod,
        control_points,
        reference_source: refSource,
        author,
      });
      resetCalDraft();
      setCreatingNew(false);
      setSelectedCalibrationId(cal.id);
      setStep("measure");
      const rms = cal.residuals_px != null ? `${cal.residuals_px.toFixed(3)} m` : "—";
      setFeedback(`Calibração salva. RMS de reprojeção: ${rms}.`);
    } catch (err) {
      setError(toSicroError(err).message);
    }
  };

  // --- Medição ---
  const onAddMeasurePoint = (x: number, y: number) => {
    setMeasurePoints((pts) => (pts.length >= 2 ? pts : [...pts, { x, y }]));
  };

  const handleCompute = async () => {
    setError(null);
    if (!activeCalibration) {
      setError("Selecione ou crie uma calibração antes de medir.");
      return;
    }
    if (measurePoints.length !== 2) {
      setError("Marque exatamente 2 pontos (as duas extremidades da distância).");
      return;
    }
    // σ: o MC só roda se o perito informar ≥1 incerteza > 0. Sem σ, o backend
    // pula o MC e sai só a distância pontual — a UI não esconde isso.
    const sCal = parseNum(sigmaCalPx);
    const sWorld = parseNum(sigmaWorldM);
    const sMeas = parseNum(sigmaMeasurePx);
    const anyNonZero = [sCal, sWorld, sMeas].some((v) => Number.isFinite(v) && v > 0);
    const mc_sigmas = anyNonZero
      ? {
          calibration_px: clampNonNeg(sCal),
          world_m: clampNonNeg(sWorld),
          measure_px: clampNonNeg(sMeas),
        }
      : null;

    const p1 = measurePoints[0]!;
    const p2 = measurePoints[1]!;
    const input: CreateDistanceMeasurementInput = {
      calibration_id: activeCalibration.id,
      p1_px: p1.x,
      p1_py: p1.y,
      p2_px: p2.x,
      p2_py: p2.y,
      mc_n: parseInt(mcN, 10) || 10000,
      mc_sigmas,
      author,
    };
    try {
      const m = await createDistanceMeasurement(workspacePath, input);
      setSelectedMeasurementId(m.id);
      setFeedback(
        m.mc_seed != null
          ? "Medição concluída (com Monte Carlo)."
          : "Medição concluída — sem intervalo de incerteza (σ não informados).",
      );
    } catch (err) {
      setError(toSicroError(err).message);
    }
  };

  // --- Markers / canvas ---
  const calMarkers: FrameMarker[] = calPoints.map((p, i) => ({
    x: p.x,
    y: p.y,
    label: calMethod === "plane" ? corner(i) : String(i + 1),
    color: "#f59e0b",
  }));
  const measureMarkers: FrameMarker[] = measurePoints.map((p, i) => ({
    x: p.x,
    y: p.y,
    label: i === 0 ? "A" : "B",
    color: "#22d3ee",
  }));

  if (frames.length === 0) {
    return (
      <div className={styles.empty}>
        Nenhum frame coletado ainda. Volte ao <strong>Reprodutor</strong> e use{" "}
        <strong>Coletar frame atual</strong> num instante nítido. A medição de
        distância trabalha sobre frames coletados (frame-accurate).
      </div>
    );
  }

  const canvasInMeasure = step === "measure";
  const canvasSrc = canvasInMeasure
    ? measureFrame
      ? frameAssetSrc(workspacePath, measureFrame)
      : null
    : calFrame
      ? frameAssetSrc(workspacePath, calFrame)
      : null;

  return (
    <div className={styles.panel}>
      <div className={styles.steps}>
        <button
          type="button"
          className={step === "calibrate" ? styles.stepActive : styles.step}
          onClick={() => setStep("calibrate")}
        >
          1 · Calibração
        </button>
        <button
          type="button"
          className={step === "measure" ? styles.stepActive : styles.step}
          onClick={() => activeCalibration && setStep("measure")}
          disabled={!activeCalibration}
          title={
            activeCalibration
              ? "Marcar a distância entre 2 pontos"
              : "Selecione ou crie uma calibração primeiro"
          }
        >
          2 · Medição
        </button>
        {feedback && <span className={styles.feedback}>{feedback}</span>}
        {error && <span className={styles.error}>{error}</span>}
      </div>

      {step === "calibrate" && !showCalEditor && (
        <div className={mstyles.calPicker}>
          <label className={styles.field}>
            <span>Calibração da cena (compartilhada com a Velocidade)</span>
            <select
              value={selectedCalibrationId ?? ""}
              onChange={(e) => setSelectedCalibrationId(e.target.value)}
            >
              {speedCalibrations.map((c) => (
                <option key={c.id} value={c.id}>
                  {calMethodLabel(c.method)} · RMS{" "}
                  {c.residuals_px != null ? `${c.residuals_px.toFixed(3)} m` : "—"} ·{" "}
                  {c.reference_source}
                </option>
              ))}
            </select>
          </label>
          <div className={mstyles.calPickerActions}>
            <Button
              variant="primary"
              onClick={() => selectedCalibrationId && setStep("measure")}
              disabled={!selectedCalibrationId}
            >
              Usar esta calibração e medir
            </Button>
            <button
              type="button"
              className={styles.linkBtn}
              onClick={() => {
                resetCalDraft();
                setCreatingNew(true);
              }}
            >
              Criar nova calibração
            </button>
          </div>
          <p className={styles.note}>
            A medição <strong>não recalibra</strong> a cena — consome a mesma
            geometria da aba Velocidade. Uma calibração (1) serve a muitas
            medições (N).
          </p>
        </div>
      )}

      {(showCalEditor || step === "measure") && (
        <div className={styles.body}>
          <div className={styles.canvasCol}>
            <div className={styles.canvasStage}>
              <FrameCanvas
                src={canvasSrc}
                naturalWidth={media.width}
                naturalHeight={media.height}
                markers={canvasInMeasure ? measureMarkers : calMarkers}
                polyline={canvasInMeasure ? measurePoints : calPoints}
                closed={
                  !canvasInMeasure && calMethod === "plane" && calPoints.length === 4
                }
                onAddPoint={canvasInMeasure ? onAddMeasurePoint : onAddCalPoint}
              />
            </div>
            <p className={styles.canvasHint}>
              {canvasInMeasure
                ? "Marque os 2 EXTREMOS da distância SOBRE o plano calibrado (ex.: contato com o solo) para reduzir paralaxe. Ctrl+roda = zoom; arraste de 2 dedos = pan."
                : calMethod === "plane"
                  ? "Clique nos 4 cantos do retângulo de referência em sequência (ex.: sentido horário)."
                  : calMethod === "line"
                    ? "Clique nas 2 extremidades do segmento de comprimento conhecido."
                    : "Clique em ≥3 pontos COLINEARES ao longo da via e informe a posição (m) de cada um."}
            </p>
          </div>

          <aside className={styles.controls}>
            {canvasInMeasure ? (
              <MeasureControls
                activeCalibration={activeCalibration}
                frames={frames}
                measureFrameId={measureFrameId}
                setMeasureFrameId={setMeasureFrameId}
                pointCount={measurePoints.length}
                clearPoints={() => setMeasurePoints([])}
                onSwitchCalibration={() => {
                  setCreatingNew(false);
                  setStep("calibrate");
                }}
                sigmaCalPx={sigmaCalPx}
                setSigmaCalPx={setSigmaCalPx}
                sigmaWorldM={sigmaWorldM}
                setSigmaWorldM={setSigmaWorldM}
                sigmaMeasurePx={sigmaMeasurePx}
                setSigmaMeasurePx={setSigmaMeasurePx}
                mcN={mcN}
                setMcN={setMcN}
                onCompute={() => void handleCompute()}
                busy={isMutating}
              />
            ) : (
              <>
                {speedCalibrations.length > 0 && (
                  <button
                    type="button"
                    className={styles.linkBtn}
                    onClick={() => {
                      resetCalDraft();
                      setCreatingNew(false);
                    }}
                  >
                    ← Voltar e escolher uma calibração existente
                  </button>
                )}
                <CalibrationControls
                  activeCalibration={activeCalibration}
                  frames={frames}
                  calFrameId={calFrameId}
                  setCalFrameId={(id) => {
                    setCalFrameId(id);
                    resetCalDraft();
                  }}
                  calMethod={calMethod}
                  setCalMethod={(m) => {
                    setCalMethod(m);
                    resetCalDraft();
                  }}
                  refSource={refSource}
                  setRefSource={setRefSource}
                  calPoints={calPoints}
                  requiredPts={requiredPts}
                  clearPoints={resetCalDraft}
                  lineDistanceM={lineDistanceM}
                  setLineDistanceM={setLineDistanceM}
                  planeW={planeW}
                  setPlaneW={setPlaneW}
                  planeH={planeH}
                  setPlaneH={setPlaneH}
                  crPositions={crPositions}
                  setCrPosition={setCrPosition}
                  onSave={() => void handleSaveCalibration()}
                  busy={isMutating}
                />
              </>
            )}
          </aside>
        </div>
      )}

      {step === "measure" && result && (
        <div className={styles.resultWrap}>
          <DistanceResultCard m={result} />
        </div>
      )}

      {distanceMeasurements.length > 0 && (
        <div className={mstyles.measList}>
          <div className={mstyles.measListHead}>
            Medições desta mídia ({distanceMeasurements.length})
          </div>
          {distanceMeasurements.map((dm) => {
            const active = result?.id === dm.id;
            const interval =
              dm.mc_p2_5_m != null && dm.mc_p97_5_m != null
                ? `IC ${dm.mc_p2_5_m.toFixed(2).replace(".", ",")}–${dm.mc_p97_5_m
                    .toFixed(2)
                    .replace(".", ",")} m`
                : "pontual (sem intervalo)";
            return (
              <button
                key={dm.id}
                type="button"
                className={active ? mstyles.measRowActive : mstyles.measRow}
                onClick={() => {
                  setSelectedMeasurementId(dm.id);
                  setStep("measure");
                }}
              >
                <strong>{dm.distance_m.toFixed(2).replace(".", ",")} m</strong>
                <span>{interval}</span>
              </button>
            );
          })}
        </div>
      )}
    </div>
  );
}

// ===========================================================================
// Controles da medição (frame + σ + calcular)

function MeasureControls(props: {
  activeCalibration: VideoSpeedCalibration | null;
  frames: VideoStoryboardFrame[];
  measureFrameId: string | null;
  setMeasureFrameId: (id: string) => void;
  pointCount: number;
  clearPoints: () => void;
  onSwitchCalibration: () => void;
  sigmaCalPx: string;
  setSigmaCalPx: (s: string) => void;
  sigmaWorldM: string;
  setSigmaWorldM: (s: string) => void;
  sigmaMeasurePx: string;
  setSigmaMeasurePx: (s: string) => void;
  mcN: string;
  setMcN: (s: string) => void;
  onCompute: () => void;
  busy: boolean;
}) {
  const {
    activeCalibration,
    frames,
    measureFrameId,
    setMeasureFrameId,
    pointCount,
    clearPoints,
    onSwitchCalibration,
    sigmaCalPx,
    setSigmaCalPx,
    sigmaWorldM,
    setSigmaWorldM,
    sigmaMeasurePx,
    setSigmaMeasurePx,
    mcN,
    setMcN,
    onCompute,
    busy,
  } = props;

  return (
    <>
      {activeCalibration && (
        <div className={styles.activeCal}>
          Calibração: <strong>{calMethodLabel(activeCalibration.method)}</strong>
          {activeCalibration.residuals_px != null && (
            <> · RMS {activeCalibration.residuals_px.toFixed(3)} m</>
          )}
          <button
            type="button"
            className={styles.linkBtn}
            onClick={onSwitchCalibration}
            style={{ display: "block", marginTop: 4 }}
          >
            Trocar calibração
          </button>
        </div>
      )}

      <label className={styles.field}>
        <span>Frame da medição</span>
        <select
          value={measureFrameId ?? ""}
          onChange={(e) => setMeasureFrameId(e.target.value)}
        >
          {frames.map((f) => (
            <option key={f.id} value={f.id}>
              {formatDuration(frameTimestamp(f))} — {f.title}
            </option>
          ))}
        </select>
      </label>

      <div className={styles.pointStatus}>
        Pontos: <strong>{pointCount}/2</strong>
        <button
          type="button"
          className={styles.linkBtn}
          onClick={clearPoints}
          disabled={pointCount === 0}
        >
          Limpar
        </button>
      </div>

      <fieldset className={styles.sigmas}>
        <legend>Incertezas (σ) — opcionais, habilitam o intervalo</legend>
        <label className={styles.field}>
          <span>σ calibração (px)</span>
          <input
            type="number"
            step="0.1"
            min="0"
            value={sigmaCalPx}
            placeholder="em branco = sem MC"
            onChange={(e) => setSigmaCalPx(e.target.value)}
          />
        </label>
        <label className={styles.field}>
          <span>σ dimensão real (m)</span>
          <input
            type="number"
            step="0.01"
            min="0"
            value={sigmaWorldM}
            placeholder="ex.: 0.02"
            onChange={(e) => setSigmaWorldM(e.target.value)}
          />
        </label>
        <label className={styles.field}>
          <span>σ marcação dos 2 pontos (px)</span>
          <input
            type="number"
            step="0.1"
            min="0"
            value={sigmaMeasurePx}
            placeholder="ex.: 1.0"
            onChange={(e) => setSigmaMeasurePx(e.target.value)}
          />
        </label>
        <label className={styles.field}>
          <span>Iterações Monte Carlo</span>
          <input
            type="number"
            step="1000"
            min="10"
            value={mcN}
            onChange={(e) => setMcN(e.target.value)}
          />
        </label>
        <p className={styles.sigmaNote}>
          Sem σ informado, o resultado sai <strong>só com a distância pontual</strong>
          {" "}— distância de 2 pontos não tem IC de regressão; a única incerteza é
          o Monte Carlo, e a ressalva fica registrada.
        </p>
      </fieldset>

      <Button
        variant="primary"
        onClick={onCompute}
        disabled={busy || pointCount !== 2 || !activeCalibration}
      >
        Calcular distância
      </Button>
    </>
  );
}

// ===========================================================================
// helpers

function calMethodLabel(method: string): string {
  switch (method) {
    case "plane":
      return "Plano (4 cantos)";
    case "line":
      return "Linha (2 pontos)";
    case "cross_ratio":
      return "Razão cruzada";
    default:
      return method;
  }
}

function parseNum(s: string): number {
  const n = Number(s.replace(",", "."));
  return Number.isFinite(n) ? n : NaN;
}

function clampNonNeg(v: number): number {
  return Number.isFinite(v) && v > 0 ? v : 0;
}

/** Coordenada de mundo de cada canto do retângulo, na ordem de clique. */
function planeWorld(i: number, w: number, h: number): { x: number; y: number } {
  switch (i) {
    case 0:
      return { x: 0, y: 0 };
    case 1:
      return { x: w, y: 0 };
    case 2:
      return { x: w, y: h };
    default:
      return { x: 0, y: h };
  }
}

function corner(i: number): string {
  return String.fromCharCode(65 + i); // A, B, C, D
}
