/**
 * SpeedPanel — aba "Velocidade" do editor de vídeo (modo manual).
 *
 * Opera sobre FRAMES COLETADOS (PNG do storyboard) — nunca sobre o vídeo ao
 * vivo — porque só o frame extraído pelo ffmpeg é frame-accurate e carrega
 * `actual_timestamp_s`. Fluxo em 2 passos:
 *
 *   1. Calibração (uma vez): escolha um frame, marque a referência métrica
 *      ('line' = 2 pontos + distância; 'plane' = 4 cantos + dimensões reais;
 *      'cross_ratio' = ≥3 pontos colineares + posição de cada um ao longo da
 *      via) → cria a homografia. Mostra o RMS de reprojeção como qualidade.
 *   2. Trajetória: percorra N frames coletados e marque a posição do veículo
 *      em cada (CONTATO PNEU-SOLO, para reduzir paralaxe). ≥3 habilita IC + MC.
 *
 * Tudo é decisão do perito (KNOWN_LIMITATIONS §13): sem tracking, sem
 * detecção. O Monte Carlo só roda se o perito informar as incertezas (σ).
 */

import { useEffect, useMemo, useState } from "react";
import { Button } from "@components/Button/Button";
import { toSicroError } from "@core/errors";
import type { VideoMedia, VideoStoryboardFrame } from "@domain/video";
import type {
  ComputeSpeedInput,
  ControlPoint,
  McSigmas,
  TrajectoryPoint,
  VideoSpeedCalibration,
} from "@domain/video_speed";
import { useVideoStore } from "../../store/videoStore";
import { formatDuration } from "../format";
import { FrameCanvas, type FrameMarker, type FramePoint } from "./FrameCanvas";
import { SpeedResultCard } from "./SpeedResultCard";
import { frameAssetSrc, frameTimestamp, hasActualTimestamp } from "./speedShared";
import styles from "./SpeedPanel.module.css";

type Step = "calibrate" | "mark";
/** Modo de calibração — exportado para reuso na aba "Medições". */
export type Method = "plane" | "line" | "cross_ratio";
/** Fonte da referência métrica — exportado para reuso na aba "Medições". */
export type RefSource = "campo" | "norma_viaria" | "entre_eixos";

/** Máximo de pontos de referência no modo razão cruzada (mínimo é 3). */
const MAX_CR_POINTS = 12;

interface Props {
  workspacePath: string;
  media: VideoMedia;
  frames: VideoStoryboardFrame[];
  /** Autor (perito) vindo do contexto do app — nunca vazio aqui. */
  author: string;
}

export function SpeedPanel({ workspacePath, media, frames, author }: Props) {
  const speedCalibrations = useVideoStore((s) => s.speedCalibrations);
  const speedCalculations = useVideoStore((s) => s.speedCalculations);
  const loadSpeedData = useVideoStore((s) => s.loadSpeedData);
  const createCalibration = useVideoStore((s) => s.createCalibration);
  const computeSpeed = useVideoStore((s) => s.computeSpeed);
  const isMutating = useVideoStore((s) => s.isMutating);

  const [step, setStep] = useState<Step>("calibrate");
  const [activeCalibrationId, setActiveCalibrationId] = useState<string | null>(
    null,
  );

  // Calibração (rascunho).
  const [calMethod, setCalMethod] = useState<Method>("plane");
  const [refSource, setRefSource] = useState<RefSource>("campo");
  const [calFrameId, setCalFrameId] = useState<string | null>(null);
  const [calPoints, setCalPoints] = useState<FramePoint[]>([]);
  const [lineDistanceM, setLineDistanceM] = useState("");
  const [planeW, setPlaneW] = useState("");
  const [planeH, setPlaneH] = useState("");
  // Posições (m) ao longo da via, uma por ponto, no modo razão cruzada.
  const [crPositions, setCrPositions] = useState<string[]>([]);

  // Trajetória.
  const [currentFrameId, setCurrentFrameId] = useState<string | null>(null);
  const [trajByFrame, setTrajByFrame] = useState<Record<string, FramePoint>>({});

  // Incertezas (σ) — obrigatórias para o Monte Carlo.
  const [sigmaMarkPx, setSigmaMarkPx] = useState("2");
  const [sigmaCalPx, setSigmaCalPx] = useState("1");
  const [sigmaTimeS, setSigmaTimeS] = useState("0.01");
  const [sigmaWorldM, setSigmaWorldM] = useState("0");
  const [mcN, setMcN] = useState("10000");

  const [feedback, setFeedback] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);

  // Carrega calibrações + cálculos da mídia ao montar / trocar mídia.
  useEffect(() => {
    void loadSpeedData(workspacePath, media.sha256);
  }, [workspacePath, media.sha256, loadSpeedData]);

  // Seleciona a calibração mais recente e pula para a marcação.
  useEffect(() => {
    if (activeCalibrationId === null && speedCalibrations.length > 0) {
      setActiveCalibrationId(speedCalibrations[0]!.id);
      setStep("mark");
    }
  }, [speedCalibrations, activeCalibrationId]);

  // Default dos seletores de frame.
  useEffect(() => {
    if (frames.length === 0) return;
    const firstId = frames[0]!.id;
    setCalFrameId((id) => id ?? firstId);
    setCurrentFrameId((id) => id ?? firstId);
  }, [frames]);

  const activeCalibration: VideoSpeedCalibration | null = useMemo(
    () => speedCalibrations.find((c) => c.id === activeCalibrationId) ?? null,
    [speedCalibrations, activeCalibrationId],
  );

  const result = speedCalculations[0] ?? null;
  const calFrame = frames.find((f) => f.id === calFrameId) ?? null;
  const currentFrame = frames.find((f) => f.id === currentFrameId) ?? null;
  const requiredPts = calMethod === "plane" ? 4 : calMethod === "line" ? 2 : 3;
  const markedCount = Object.keys(trajByFrame).length;

  // --- Calibração ---

  const onAddCalPoint = (x: number, y: number) => {
    if (calMethod === "cross_ratio") {
      // Sem contagem fixa: ≥3 pontos colineares (limite alto p/ não travar).
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
        return {
          px: p.x,
          py: p.y,
          world_x_m: world.x,
          world_y_m: world.y,
          label: corner(i),
        };
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
      // cross_ratio: ≥3 pontos colineares; cada um com posição (m) ao longo
      // da via em world_x_m (world_y_m = 0).
      if (calPoints.length < 3) {
        setError("Marque pelo menos 3 pontos colineares ao longo da via.");
        return;
      }
      const positions = crPositions.slice(0, calPoints.length).map(parseNum);
      if (
        positions.length !== calPoints.length ||
        positions.some((d) => !Number.isFinite(d))
      ) {
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
      setActiveCalibrationId(cal.id);
      setCalPoints([]);
      setCrPositions([]);
      setStep("mark");
      const rms =
        cal.residuals_px != null ? `${cal.residuals_px.toFixed(3)} m` : "—";
      setFeedback(`Calibração salva. RMS de reprojeção: ${rms}.`);
    } catch (err) {
      setError(toSicroError(err).message);
    }
  };

  // --- Trajetória ---

  const onAddTrajPoint = (x: number, y: number) => {
    if (!currentFrameId) return;
    setTrajByFrame((m) => ({ ...m, [currentFrameId]: { x, y } }));
  };

  const handleCompute = async () => {
    setError(null);
    if (!activeCalibration) {
      setError("Crie/escolha uma calibração antes de calcular.");
      return;
    }
    const marked = frames
      .filter((f) => trajByFrame[f.id])
      .sort((a, b) => frameTimestamp(a) - frameTimestamp(b));
    if (marked.length < 2) {
      setError("Marque a posição do veículo em pelo menos 2 frames.");
      return;
    }
    const uPx = parseNum(sigmaMarkPx);
    const points: TrajectoryPoint[] = marked.map((f) => {
      const pt = trajByFrame[f.id]!;
      return {
        storyboard_frame_id: f.id,
        export_id: f.export_id,
        px: pt.x,
        py: pt.y,
        u_px: Number.isFinite(uPx) && uPx > 0 ? uPx : 0,
        actual_timestamp_s: frameTimestamp(f),
        delta_s: f.delta_s,
        manual: true,
      };
    });

    // σ: o MC só roda se o perito informar ≥1 incerteza > 0 (item 4).
    const sCal = parseNum(sigmaCalPx);
    const sMark = parseNum(sigmaMarkPx);
    const sTime = parseNum(sigmaTimeS);
    const sWorld = parseNum(sigmaWorldM);
    const anyNonZero = [sCal, sMark, sTime, sWorld].some(
      (v) => Number.isFinite(v) && v > 0,
    );
    const mc_sigmas: McSigmas | null = anyNonZero
      ? {
          calibration_px: clampNonNeg(sCal),
          world_m: clampNonNeg(sWorld),
          trajectory_px: clampNonNeg(sMark),
          time_s: clampNonNeg(sTime),
        }
      : null;

    const input: ComputeSpeedInput = {
      calibration_id: activeCalibration.id,
      points,
      mc_n: parseInt(mcN, 10) || 10000,
      mc_sigmas,
      confidence: 0.95,
      author,
    };
    try {
      const calc = await computeSpeed(workspacePath, input);
      setFeedback(
        calc.mc_seed != null
          ? "Cálculo concluído (com Monte Carlo)."
          : "Cálculo concluído — Monte Carlo não executado (σ não informados).",
      );
    } catch (err) {
      setError(toSicroError(err).message);
    }
  };

  // --- Markers para o canvas ---

  const canvasSrc =
    step === "calibrate"
      ? calFrame
        ? frameAssetSrc(workspacePath, calFrame)
        : null
      : currentFrame
        ? frameAssetSrc(workspacePath, currentFrame)
        : null;

  const calMarkers: FrameMarker[] = calPoints.map((p, i) => ({
    x: p.x,
    y: p.y,
    label: calMethod === "plane" ? corner(i) : String(i + 1),
    color: "#f59e0b",
  }));

  const trajMarkers: FrameMarker[] = useMemo(() => {
    if (step !== "mark" || !currentFrame) return [];
    const pt = trajByFrame[currentFrame.id];
    if (!pt) return [];
    return [
      {
        x: pt.x,
        y: pt.y,
        label: `t=${formatDuration(frameTimestamp(currentFrame))}`,
        color: "#22d3ee",
      },
    ];
  }, [step, currentFrame, trajByFrame]);

  if (frames.length === 0) {
    return (
      <div className={styles.empty}>
        Nenhum frame coletado ainda. Volte ao <strong>Reprodutor</strong> e use{" "}
        <strong>Coletar frame atual</strong> nos instantes em que o veículo
        aparece. O cálculo de velocidade trabalha sobre frames coletados
        (frame-accurate), não sobre o vídeo ao vivo.
      </div>
    );
  }

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
          className={step === "mark" ? styles.stepActive : styles.step}
          onClick={() => activeCalibration && setStep("mark")}
          disabled={!activeCalibration}
          title={
            activeCalibration
              ? "Marcar a trajetória do veículo"
              : "Crie uma calibração primeiro"
          }
        >
          2 · Trajetória &amp; cálculo
        </button>
        {feedback && <span className={styles.feedback}>{feedback}</span>}
        {error && <span className={styles.error}>{error}</span>}
      </div>

      <div className={styles.body}>
        <div className={styles.canvasCol}>
          <div className={styles.canvasStage}>
            <FrameCanvas
              src={canvasSrc}
              naturalWidth={media.width}
              naturalHeight={media.height}
              markers={step === "calibrate" ? calMarkers : trajMarkers}
              polyline={step === "calibrate" ? calPoints : undefined}
              closed={calMethod === "plane" && calPoints.length === 4}
              onAddPoint={step === "calibrate" ? onAddCalPoint : onAddTrajPoint}
            />
          </div>
          <p className={styles.canvasHint}>
            {step === "calibrate"
              ? calMethod === "plane"
                ? "Clique nos 4 cantos do retângulo de referência em sequência (ex.: sentido horário)."
                : calMethod === "line"
                  ? "Clique nas 2 extremidades do segmento de comprimento conhecido."
                  : "Clique em ≥3 pontos COLINEARES ao longo da via (ex.: marcas de faixa) e informe a posição (m) de cada um."
              : "Marque o CONTATO PNEU-SOLO do veículo (reduz erro de paralaxe). Ctrl+roda = zoom; arraste de 2 dedos = pan."}
          </p>
        </div>

        <aside className={styles.controls}>
          {step === "calibrate" ? (
            <CalibrationControls
              activeCalibration={activeCalibration}
              frames={frames}
              calFrameId={calFrameId}
              setCalFrameId={(id) => {
                setCalFrameId(id);
                setCalPoints([]);
                setCrPositions([]);
              }}
              calMethod={calMethod}
              setCalMethod={(m) => {
                setCalMethod(m);
                setCalPoints([]);
                setCrPositions([]);
              }}
              refSource={refSource}
              setRefSource={setRefSource}
              calPoints={calPoints}
              requiredPts={requiredPts}
              clearPoints={() => {
                setCalPoints([]);
                setCrPositions([]);
              }}
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
          ) : (
            <TrajectoryControls
              activeCalibration={activeCalibration}
              frames={frames}
              currentFrameId={currentFrameId}
              setCurrentFrameId={setCurrentFrameId}
              trajByFrame={trajByFrame}
              clearCurrent={() => {
                if (!currentFrameId) return;
                setTrajByFrame((m) => {
                  const next = { ...m };
                  delete next[currentFrameId];
                  return next;
                });
              }}
              markedCount={markedCount}
              sigmaMarkPx={sigmaMarkPx}
              setSigmaMarkPx={setSigmaMarkPx}
              sigmaCalPx={sigmaCalPx}
              setSigmaCalPx={setSigmaCalPx}
              sigmaTimeS={sigmaTimeS}
              setSigmaTimeS={setSigmaTimeS}
              sigmaWorldM={sigmaWorldM}
              setSigmaWorldM={setSigmaWorldM}
              mcN={mcN}
              setMcN={setMcN}
              onCompute={() => void handleCompute()}
              busy={isMutating}
            />
          )}
          {result && <SpeedResultCard calc={result} />}
        </aside>
      </div>
    </div>
  );
}

// ===========================================================================
// Calibração — controles

export function CalibrationControls(props: {
  activeCalibration: VideoSpeedCalibration | null;
  frames: VideoStoryboardFrame[];
  calFrameId: string | null;
  setCalFrameId: (id: string) => void;
  calMethod: Method;
  setCalMethod: (m: Method) => void;
  refSource: RefSource;
  setRefSource: (r: RefSource) => void;
  calPoints: FramePoint[];
  requiredPts: number;
  clearPoints: () => void;
  lineDistanceM: string;
  setLineDistanceM: (s: string) => void;
  planeW: string;
  setPlaneW: (s: string) => void;
  planeH: string;
  setPlaneH: (s: string) => void;
  crPositions: string[];
  setCrPosition: (i: number, v: string) => void;
  onSave: () => void;
  busy: boolean;
}) {
  const {
    activeCalibration,
    frames,
    calFrameId,
    setCalFrameId,
    calMethod,
    setCalMethod,
    refSource,
    setRefSource,
    calPoints,
    requiredPts,
    clearPoints,
    lineDistanceM,
    setLineDistanceM,
    planeW,
    setPlaneW,
    planeH,
    setPlaneH,
    crPositions,
    setCrPosition,
    onSave,
    busy,
  } = props;

  const isCr = calMethod === "cross_ratio";
  const enoughPoints = isCr
    ? calPoints.length >= 3
    : calPoints.length === requiredPts;

  return (
    <>
      {activeCalibration && (
        <div className={styles.activeCal}>
          ✓ Calibração ativa: <strong>{activeCalibration.method}</strong> ·{" "}
          {activeCalibration.reference_source}
          {activeCalibration.residuals_px != null && (
            <> · RMS {activeCalibration.residuals_px.toFixed(3)} m</>
          )}
          <span className={styles.activeCalNote}>
            Refaça abaixo se quiser uma nova calibração.
          </span>
        </div>
      )}

      <label className={styles.field}>
        <span>Frame de calibração</span>
        <select
          value={calFrameId ?? ""}
          onChange={(e) => setCalFrameId(e.target.value)}
        >
          {frames.map((f) => (
            <option key={f.id} value={f.id}>
              {formatDuration(frameTimestamp(f))} — {f.title}
            </option>
          ))}
        </select>
      </label>

      <div className={styles.field}>
        <span>Método</span>
        <div className={styles.radioRow}>
          <label>
            <input
              type="radio"
              name="cal-method"
              checked={calMethod === "plane"}
              onChange={() => setCalMethod("plane")}
            />
            Plano (4 cantos)
          </label>
          <label>
            <input
              type="radio"
              name="cal-method"
              checked={calMethod === "line"}
              onChange={() => setCalMethod("line")}
            />
            Linha (2 pontos)
          </label>
          <label>
            <input
              type="radio"
              name="cal-method"
              checked={calMethod === "cross_ratio"}
              onChange={() => setCalMethod("cross_ratio")}
            />
            Razão cruzada (referências colineares ao longo da via)
          </label>
        </div>
      </div>

      {isCr && (
        <p className={styles.note}>
          Use quando as boas referências são <strong>colineares</strong> —
          marcas de faixa a distâncias conhecidas ao longo da via —, caso em que
          o modo "plano" (retângulo) não se aplica.
        </p>
      )}

      <label className={styles.field}>
        <span>Fonte da referência</span>
        <select
          value={refSource}
          onChange={(e) => setRefSource(e.target.value as RefSource)}
        >
          <option value="campo">Medida em campo</option>
          <option value="norma_viaria">Norma viária (presumida)</option>
          <option value="entre_eixos">Entre-eixos do veículo</option>
        </select>
      </label>

      <div className={styles.pointStatus}>
        Pontos:{" "}
        <strong>
          {isCr ? `${calPoints.length} (mín. 3)` : `${calPoints.length}/${requiredPts}`}
        </strong>
        <button
          type="button"
          className={styles.linkBtn}
          onClick={clearPoints}
          disabled={calPoints.length === 0}
        >
          Limpar
        </button>
      </div>

      {calMethod === "plane" ? (
        <div className={styles.dims}>
          <label className={styles.field}>
            <span>Largura real (m)</span>
            <input
              type="number"
              step="0.01"
              min="0"
              value={planeW}
              onChange={(e) => setPlaneW(e.target.value)}
              placeholder="ex.: 3.50"
            />
          </label>
          <label className={styles.field}>
            <span>Altura real (m)</span>
            <input
              type="number"
              step="0.01"
              min="0"
              value={planeH}
              onChange={(e) => setPlaneH(e.target.value)}
              placeholder="ex.: 6.00"
            />
          </label>
        </div>
      ) : calMethod === "line" ? (
        <label className={styles.field}>
          <span>Distância real (m)</span>
          <input
            type="number"
            step="0.01"
            min="0"
            value={lineDistanceM}
            onChange={(e) => setLineDistanceM(e.target.value)}
            placeholder="ex.: 10.00"
          />
        </label>
      ) : (
        <div className={styles.field}>
          <span>Posição de cada ponto ao longo da via (m)</span>
          {calPoints.length === 0 ? (
            <p className={styles.sigmaNote}>
              Marque ≥3 pontos colineares no frame; informe a distância de cada
              um (ex.: 0, 5, 10, 15 m).
            </p>
          ) : (
            <div className={styles.crPosList}>
              {calPoints.map((_, i) => (
                <div key={i} className={styles.crPosRow}>
                  <span className={styles.crPosLabel}>Ponto {i + 1}</span>
                  <input
                    type="number"
                    step="0.01"
                    value={crPositions[i] ?? ""}
                    onChange={(e) => setCrPosition(i, e.target.value)}
                    placeholder={`${i * 5}`}
                  />
                  <span className={styles.crPosUnit}>m</span>
                </div>
              ))}
            </div>
          )}
        </div>
      )}

      <Button variant="primary" onClick={onSave} disabled={busy || !enoughPoints}>
        Salvar calibração
      </Button>
    </>
  );
}

// ===========================================================================
// Trajetória — controles

function TrajectoryControls(props: {
  activeCalibration: VideoSpeedCalibration | null;
  frames: VideoStoryboardFrame[];
  currentFrameId: string | null;
  setCurrentFrameId: (id: string) => void;
  trajByFrame: Record<string, FramePoint>;
  clearCurrent: () => void;
  markedCount: number;
  sigmaMarkPx: string;
  setSigmaMarkPx: (s: string) => void;
  sigmaCalPx: string;
  setSigmaCalPx: (s: string) => void;
  sigmaTimeS: string;
  setSigmaTimeS: (s: string) => void;
  sigmaWorldM: string;
  setSigmaWorldM: (s: string) => void;
  mcN: string;
  setMcN: (s: string) => void;
  onCompute: () => void;
  busy: boolean;
}) {
  const {
    activeCalibration,
    frames,
    currentFrameId,
    setCurrentFrameId,
    trajByFrame,
    clearCurrent,
    markedCount,
    sigmaMarkPx,
    setSigmaMarkPx,
    sigmaCalPx,
    setSigmaCalPx,
    sigmaTimeS,
    setSigmaTimeS,
    sigmaWorldM,
    setSigmaWorldM,
    mcN,
    setMcN,
    onCompute,
    busy,
  } = props;

  const anyVfr = frames.some((f) => !hasActualTimestamp(f));

  return (
    <>
      {activeCalibration && (
        <div className={styles.activeCal}>
          Calibração: <strong>{activeCalibration.method}</strong>
          {activeCalibration.residuals_px != null && (
            <> · RMS {activeCalibration.residuals_px.toFixed(3)} m</>
          )}
        </div>
      )}

      <div className={styles.field}>
        <span>
          Frames coletados — marque o veículo em cada (
          <strong>{markedCount}</strong> marcados)
        </span>
        <div className={styles.frameStrip}>
          {frames.map((f) => {
            const marked = !!trajByFrame[f.id];
            const active = f.id === currentFrameId;
            return (
              <button
                key={f.id}
                type="button"
                className={[
                  styles.frameChip,
                  active ? styles.frameChipActive : "",
                  marked ? styles.frameChipMarked : "",
                ]
                  .filter(Boolean)
                  .join(" ")}
                onClick={() => setCurrentFrameId(f.id)}
                title={f.title}
              >
                {marked ? "● " : "○ "}
                {formatDuration(frameTimestamp(f))}
              </button>
            );
          })}
        </div>
        {currentFrameId && trajByFrame[currentFrameId] && (
          <button type="button" className={styles.linkBtn} onClick={clearCurrent}>
            Desmarcar este frame
          </button>
        )}
      </div>

      <div className={styles.note}>
        Mínimo 2 pontos (velocidade média). <strong>3+</strong> habilita
        intervalo de confiança e Monte Carlo.
      </div>

      <fieldset className={styles.sigmas}>
        <legend>Incertezas (σ) — necessárias para o Monte Carlo</legend>
        <label className={styles.field}>
          <span>σ marcação (px)</span>
          <input
            type="number"
            step="0.1"
            min="0"
            value={sigmaMarkPx}
            onChange={(e) => setSigmaMarkPx(e.target.value)}
          />
        </label>
        <label className={styles.field}>
          <span>σ calibração (px)</span>
          <input
            type="number"
            step="0.1"
            min="0"
            value={sigmaCalPx}
            onChange={(e) => setSigmaCalPx(e.target.value)}
          />
        </label>
        <label className={styles.field}>
          <span>σ temporal (s)</span>
          <input
            type="number"
            step="0.001"
            min="0"
            value={sigmaTimeS}
            onChange={(e) => setSigmaTimeS(e.target.value)}
          />
        </label>
        <label className={styles.field}>
          <span>σ dimensão real (m) — opcional</span>
          <input
            type="number"
            step="0.01"
            min="0"
            value={sigmaWorldM}
            onChange={(e) => setSigmaWorldM(e.target.value)}
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
          Sem σ informado, o resultado sai <strong>sem Monte Carlo</strong> — só
          com o IC do ajuste — e a ressalva fica registrada.
        </p>
      </fieldset>

      {anyVfr && (
        <div className={styles.note}>
          Atenção: há frame(s) sem <code>actual_timestamp_s</code> confiável; o
          tempo solicitado é usado como aproximação.
        </div>
      )}

      <Button
        variant="primary"
        onClick={onCompute}
        disabled={busy || markedCount < 2 || !activeCalibration}
      >
        Calcular velocidade
      </Button>
    </>
  );
}

// ===========================================================================
// helpers

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
