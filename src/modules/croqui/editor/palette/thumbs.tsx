/** Miniaturas da prateleira: arte real dos veículos e pessoas; glifo do canvas para marcadores. */

import { useMemo } from "react";
import { Group, Layer, Stage } from "react-konva";
import { makeMarker, makeVehicle, type MarkerSubtype, type VehicleBodyType } from "../../engine";
import { getPessoaArt, getVehicleArtSvg } from "../../engine/vehicleArt";
import { MarkerGlyph } from "../CanvasStage";
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

export function PessoaThumb({ subtype }: { subtype: string }) {
  const art = getPessoaArt(subtype);
  if (!art) return null;
  return <img src={svgUrl(art.svg)} alt="" className={styles.thumbImgRot} draggable={false} />;
}

/** O mesmo glifo que o canvas desenha, num Stage de 64×40. */
export function MarkerThumb({ subtype }: { subtype: MarkerSubtype }) {
  const m = useMemo(() => makeMarker({ x: 0, y: 0 }, subtype, undefined, null), [subtype]);
  const s = Math.min(34 / m.size, 1.5);
  return (
    <Stage width={64} height={40} listening={false} className={styles.thumbStage}>
      <Layer listening={false}>
        <Group x={32} y={20} scaleX={s} scaleY={s}>
          <MarkerGlyph obj={{ ...m, x: 0, y: 0 }} selected={false} />
        </Group>
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
