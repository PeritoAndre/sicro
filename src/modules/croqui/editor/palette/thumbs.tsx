/** Miniaturas da prateleira: arte real dos veículos e pessoas; vestígios e elementos pelo código do canvas. */

import { useMemo } from "react";
import { Layer, Rect, Shape, Stage } from "react-konva";
import {
  FIXTURE_SPECS,
  makeFixture,
  makePerson,
  type PersonPosicao,
  makeTrace,
  makeVehicle,
  type FixtureSubtype,
  type TraceSubtype,
  type VehicleBodyType,
} from "../../engine";
import { PARITY_STYLE_DEFAULT } from "../../engine/road-parity";
import { drawTrace } from "../traceDraw";
import { drawFixture } from "../fixtureDraw";
import { drawPerson } from "../personDraw";
import { getVehicleArtSvg } from "../../engine/vehicleArt";
import styles from "./Palette.module.css";

const svgUrl = (svg: string) => `data:image/svg+xml;charset=utf-8,${encodeURIComponent(svg)}`;

/** Veículo deitado (frente para a direita, como no croqui); sem arte, retângulo na cor do preset. */
export function VehicleThumb({ body }: { body: VehicleBodyType }) {
  const { svg, color } = useMemo(() => {
    const preset = makeVehicle({ x: 0, y: 0 }, "", body, null);
    return { svg: getVehicleArtSvg(body, preset.color), color: preset.color ?? "#3b82f6" };
  }, [body]);
  if (!svg) {
    return (
      <svg viewBox="0 0 64 40" className={styles.thumbSvg} aria-hidden>
        <rect x="6" y="9" width="52" height="22" rx="6" fill={color} />
        <rect x="22" y="12" width="12" height="16" rx="2" fill="rgba(255,255,255,0.45)" />
      </svg>
    );
  }
  return <img src={svgUrl(svg)} alt="" className={styles.thumbImgRot} draggable={false} />;
}


/** Amostra de cada vestígio na miniatura: px/m, pontas (px) e ajustes. */
const TRACE_THUMBS: Record<TraceSubtype, { ppm: number; a: [number, number]; b: [number, number]; bendM?: number; params?: Record<string, number | string | boolean> }> = {
  frenagem: { ppm: 9, a: [5, 20], b: [59, 20] },
  derrapagem: { ppm: 8, a: [5, 15], b: [59, 22], bendM: -0.9, params: { abre: 0.9 } },
  arrasto: { ppm: 10, a: [7, 20], b: [57, 20] },
  sulcagem: { ppm: 30, a: [10, 20], b: [56, 20] },
  ranhura: { ppm: 13, a: [7, 20], b: [57, 20] },
  fluido: { ppm: 14, a: [5, 17], b: [40, 21] },
  fragmentos: { ppm: 24, a: [8, 20], b: [58, 20], params: { dens: 30 } },
  colisao: { ppm: 24, a: [32, 20], b: [32, 20] },
};

/** O vestígio desenhado pelo mesmo código do canvas, sobre asfalto. */
export function TraceThumb({ subtype }: { subtype: TraceSubtype }) {
  const t = TRACE_THUMBS[subtype];
  const obj = useMemo(() => {
    const o = makeTrace(subtype, { x: t.a[0], y: t.a[1] }, { x: t.b[0], y: t.b[1] }, t.ppm);
    o.seed = 7;
    if (t.bendM !== undefined) o.bend = t.bendM * t.ppm;
    if (t.params) o.params = { ...o.params, ...t.params };
    return o;
  }, [subtype, t]);
  return (
    <Stage width={64} height={40} listening={false} className={styles.thumbStage}>
      <Layer listening={false}>
        <Rect x={0} y={4} width={64} height={32} fill="#e6e6e6" />
        <Shape
          sceneFunc={(kctx) => {
            drawTrace((kctx as unknown as { _context: CanvasRenderingContext2D })._context, obj, {
              ppm: t.ppm,
              tema: "tecnico",
              finish: "textura",
              asfalto: "#e6e6e6",
            });
          }}
        />
      </Layer>
    </Stage>
  );
}

/** Amostra de cada elemento na miniatura: px/m, pontas (px) e ajustes. */
const FIXTURE_THUMBS: Record<FixtureSubtype, { ppm: number; a: [number, number]; b: [number, number]; bendM?: number; params?: Record<string, number | string | boolean> }> = {
  placa: { ppm: 14, a: [46, 20], b: [60, 20] },
  semaforo: { ppm: 14, a: [12, 33], b: [42, 15] },
  faixa_pedestre: { ppm: 5, a: [6, 20], b: [58, 20] },
  retencao: { ppm: 8, a: [32, 5], b: [32, 35] },
  lombada: { ppm: 7, a: [32, 2], b: [32, 38] },
  area_conflito: { ppm: 3, a: [32, 5], b: [32, 35], params: { largura: 13, passo: 2.2 } },
  seta: { ppm: 8, a: [34, 20], b: [50, 20] },
  poste: { ppm: 14, a: [22, 26], b: [46, 13], params: { luminaria: true } },
  arvore: { ppm: 6, a: [32, 20], b: [32, 20] },
  hidrante: { ppm: 30, a: [32, 20], b: [44, 20] },
  abrigo: { ppm: 7, a: [32, 20], b: [32, 30] },
  barreira: { ppm: 8, a: [6, 25], b: [58, 25], bendM: -0.9 },
  obstaculo: { ppm: 10, a: [32, 20], b: [44, 20] },
  camera: { ppm: 3.2, a: [8, 20], b: [60, 20] },
};

/** O elemento desenhado pelo mesmo código do canvas; sinalização sobre asfalto. */
export function FixtureThumb({ subtype }: { subtype: FixtureSubtype }) {
  const t = FIXTURE_THUMBS[subtype];
  const obj = useMemo(() => {
    const o = makeFixture(subtype, { x: t.a[0], y: t.a[1] }, { x: t.b[0], y: t.b[1] }, t.ppm);
    o.seed = 40;
    if (t.bendM !== undefined) o.bend = t.bendM * t.ppm;
    if (t.params) o.params = { ...o.params, ...t.params };
    return o;
  }, [subtype, t]);
  const asfalto = FIXTURE_SPECS[subtype].grupo === "sinalizacao" && subtype !== "placa" && subtype !== "semaforo";
  return (
    <Stage width={64} height={40} listening={false} className={styles.thumbStage}>
      <Layer listening={false}>
        {asfalto && <Rect x={0} y={2} width={64} height={36} fill="#e6e6e6" />}
        <Shape
          sceneFunc={(kctx) => {
            drawFixture((kctx as unknown as { _context: CanvasRenderingContext2D })._context, obj, {
              ppm: t.ppm,
              style: PARITY_STYLE_DEFAULT,
            });
          }}
        />
      </Layer>
    </Stage>
  );
}

/** Pessoa deitada na horizontal (cabeça à direita) ou em pé vista de cima. */
export function PersonThumb({ posicao }: { posicao: PersonPosicao }) {
  const obj = useMemo(() => {
    const empe = posicao === "empe";
    const o = makePerson(posicao, { x: empe ? 32 : 30, y: 20 });
    o.rotation = 90;
    return o;
  }, [posicao]);
  const ppm = posicao === "empe" ? 62 : 27;
  return (
    <Stage width={64} height={40} listening={false} className={styles.thumbStage}>
      <Layer listening={false}>
        <Rect x={0} y={0} width={64} height={40} fill="#e6e6e6" />
        <Shape
          sceneFunc={(kctx) => {
            drawPerson((kctx as unknown as { _context: CanvasRenderingContext2D })._context, obj, { ppm, tema: "tecnico", asfalto: "#e6e6e6", zoom: 1 });
          }}
        />
      </Layer>
    </Stage>
  );
}

/** Esboço vetorial (vias, linhas, cota, texto). */
export function SvgThumb({ children }: { children: React.ReactNode }) {
  return (
    <svg viewBox="0 0 64 40" className={styles.thumbSvg} aria-hidden>
      {children}
    </svg>
  );
}

export const roadThumb = (w: number, marks: React.ReactNode, fill = "#e6e6e6") => (
  <SvgThumb>
    <rect x="0" y={20 - w / 2} width="64" height={w} fill={fill} />
    <path d={`M0 ${20 - w / 2}H64M0 ${20 + w / 2}H64`} stroke="#111" strokeWidth="1.5" />
    {marks}
  </SvgThumb>
);
