/**
 * Palco dos modos Velocidade e Distância depois de escolhido o momento: o quadro exato
 * (PNG do ffmpeg) com as marcas, a grade de conferência e a tira de momentos.
 */

import { useMemo } from "react";
import type { VideoMedia, VideoStoryboardFrame } from "@domain/video";
import { useVideoStore } from "../../store/videoStore";
import { formatDuration } from "../format";
import { FrameCanvas, type FrameGuide, type FrameMarker } from "../speed/FrameCanvas";
import { frameAssetSrc, frameTimestamp } from "../speed/speedShared";
import { dicaClique, guideLines, pontosPedidos } from "./geo";
import { useMedirStore, type Pt } from "./medirStore";
import styles from "./Medir.module.css";

const GOLD = "#d7a84f";
const CYAN = "#22d3ee";

export function MedirStage({ workspacePath, media, frames }: { workspacePath: string; media: VideoMedia; frames: VideoStoryboardFrame[] }) {
  const modo = useMedirStore((s) => s.modo);
  const sp = useMedirStore((s) => s.speed);
  const d = useMedirStore((s) => s.dist);
  const ref = useMedirStore((s) => s.ref);
  const patchSpeed = useMedirStore((s) => s.patchSpeed);
  const patchDist = useMedirStore((s) => s.patchDist);
  const patchRef = useMedirStore((s) => s.patchRef);
  const cals = useVideoStore((s) => s.speedCalibrations);
  const measurements = useVideoStore((s) => s.distanceMeasurements);
  const cal = cals[0] ?? null;
  const editingRef = !cal || ref.editando;

  const moments = useMemo(
    () => frames.filter((f) => sp.moments.includes(f.id)).sort((a, b) => frameTimestamp(a) - frameTimestamp(b)),
    [frames, sp.moments],
  );
  const calGuides = useMemo(() => (cal ? guideLines(cal) : []), [cal]);
  const faint = (gs: { points: number[]; strong?: boolean }[]): FrameGuide[] =>
    gs.map((g) => ({ points: g.points, color: g.strong ? "rgba(215,168,79,0.35)" : "rgba(215,168,79,0.18)" }));
  const bright = (gs: { points: number[]; strong?: boolean }[]): FrameGuide[] =>
    gs.map((g) => ({ points: g.points, color: g.strong ? GOLD : "rgba(215,168,79,0.55)", width: g.strong ? 2 : 1 }));

  const isRefStep = (modo === "velocidade" && sp.step === 1) || (modo === "distancia" && d.step === 1);

  // Quadro mostrado
  let frame: VideoStoryboardFrame | null = null;
  if (modo === "distancia") frame = frames.find((f) => f.id === d.frameId) ?? null;
  else if (isRefStep) frame = frames.find((f) => f.id === ref.frameId) ?? moments[0] ?? frames[0] ?? null;
  else if (sp.step === 2) frame = (sp.focus ? moments.find((f) => f.id === sp.focus) : moments.find((f) => !sp.marks[f.id])) ?? moments[moments.length - 1] ?? null;
  else frame = [...moments].reverse().find((f) => sp.marks[f.id]) ?? moments[0] ?? null;

  let markers: FrameMarker[] = [];
  let guides: FrameGuide[] = [];
  let polyline: Pt[] | undefined;
  let closed = false;
  let hint: string | null = null;
  let onAddPoint: ((x: number, y: number) => void) | undefined;

  if (isRefStep) {
    if (editingRef) {
      const max = pontosPedidos(ref.kind).max;
      markers = ref.points.map((p, i) => ({ ...p, label: String(i + 1), color: GOLD }));
      polyline = ref.points;
      closed = ref.kind === "retangulo" && ref.points.length === 4;
      hint = dicaClique(ref);
      onAddPoint = (x, y) => {
        const cur = useMedirStore.getState().ref;
        if (cur.points.length >= max) return;
        // O rascunho fica preso ao quadro em que foi clicado.
        patchRef({ points: [...cur.points, { x, y }], frameId: frame?.id ?? cur.frameId });
      };
    } else {
      guides = bright(calGuides);
    }
  } else if (modo === "velocidade") {
    const path = moments.filter((f) => sp.marks[f.id]).map((f) => sp.marks[f.id]!);
    guides = [...faint(calGuides), ...(path.length > 1 ? [{ points: path.flatMap((p) => [p.x, p.y]), color: "rgba(34,211,238,0.75)", width: 2, dash: true }] : [])];
    markers = moments
      .filter((f) => sp.marks[f.id])
      .map((f) => ({ ...sp.marks[f.id]!, color: CYAN, faint: f.id !== frame?.id, label: f.id === frame?.id ? formatDuration(frameTimestamp(f)) : undefined }));
    if (sp.step === 2 && frame) {
      const target = frame;
      hint = sp.marks[target.id] ? "Clique de novo para corrigir este momento" : "Clique onde o pneu toca o chão";
      onAddPoint = (x, y) => {
        const cur = useMedirStore.getState().speed;
        patchSpeed({ marks: { ...cur.marks, [target.id]: { x, y } }, focus: null });
      };
    }
  } else if (modo === "distancia" && d.step === 2) {
    guides = faint(calGuides);
    markers = d.points.map((p, i) => ({ ...p, label: i === 0 ? "A" : "B", color: GOLD }));
    polyline = d.points;
    // Sem rascunho: a última medição continua desenhada, com o valor.
    const last = measurements.find((m) => m.calibration_id === cal?.id);
    if (d.points.length === 0 && last) {
      guides = [...guides, { points: [last.p1_px, last.p1_py, last.p2_px, last.p2_py], color: GOLD, width: 2 }];
      markers = [
        { x: last.p1_px, y: last.p1_py, color: GOLD },
        { x: last.p2_px, y: last.p2_py, color: GOLD, label: `${last.distance_m.toFixed(2).replace(".", ",")} m` },
      ];
    }
    hint = d.points.length === 0 ? "Clique na primeira ponta" : d.points.length === 1 ? "Clique na segunda ponta" : null;
    onAddPoint = (x, y) => {
      const pts = useMedirStore.getState().dist.points;
      patchDist({ points: pts.length >= 2 ? [{ x, y }] : [...pts, { x, y }] });
    };
  }

  const src = frame ? frameAssetSrc(workspacePath, frame) : null;
  const strip = modo === "velocidade" ? moments : frame ? [frame] : [];

  return (
    <div className={styles.stage}>
      <div className={styles.canvasBox}>
        <FrameCanvas
          src={src}
          naturalWidth={media.width}
          naturalHeight={media.height}
          markers={markers}
          polyline={polyline}
          closed={closed}
          guides={guides}
          hint={hint}
          lupa={!!onAddPoint}
          onAddPoint={onAddPoint}
        />
      </div>
      {strip.length > 0 && (
        <div className={styles.strip} role="list" aria-label="Momentos">
          {strip.map((f) => {
            const on = f.id === frame?.id;
            const ok = modo === "velocidade" && !!sp.marks[f.id];
            return (
              <button
                key={f.id}
                type="button"
                role="listitem"
                className={`${styles.thumb} ${on ? styles.thumbOn : ""} ${ok ? styles.thumbOk : ""}`}
                onClick={() => {
                  if (modo !== "velocidade") return;
                  if (isRefStep) patchRef({ frameId: f.id, points: [] });
                  else patchSpeed({ focus: f.id });
                }}
                title={f.title}
              >
                <img src={frameAssetSrc(workspacePath, f) ?? ""} alt="" loading="lazy" />
                <span>{formatDuration(frameTimestamp(f))}</span>
              </button>
            );
          })}
        </div>
      )}
    </div>
  );
}
