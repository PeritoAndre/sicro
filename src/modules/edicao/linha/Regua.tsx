/** Régua do programa (MM:SS e frações pelo zoom) com os marcadores; clicar ou arrastar leva a cabeça. Dono: P4.1. */

import { memo, type PointerEvent as PE, type Ref } from "react";
import type { Id, Racional, Seg } from "@domain/edicao";
import type { MarcadorNaLinha } from "./modelo";
import { capturar } from "../palco/capturar";
import styles from "./Linha.module.css";

export interface ReguaProps {
  pxPorS: number;
  rolagem_s: number;
  largura: number;
  // CONTRATO: acréscimos do P4.1 (props do próprio componente).
  fps: Racional;
  /** Parte desenhada (px desde o começo do programa). */
  janela: readonly [number, number];
  marcadores: readonly MarcadorNaLinha[];
  /** Ponteiro na régua (slot); `soltou` no fim do arrasto. */
  onIr(q: number, soltou: boolean): void;
  onMarcador(id: Id, q: number): void;
  cabecaRef?: Ref<HTMLDivElement>;
  /** Largura em CSS (acompanha o zoom vivo pela variável `--px`). */
  larguraCss?: string;
}

/** Distância mínima entre dois rótulos (px): a 14 px/s, uma marca a cada 5 s (maquete). */
const ROTULO_PX = 60;
/** Traço menor só com esta folga (maquete: o de 1 s aparece acima de 20 px/s). */
const MENOR_PX = 20;
const PASSOS_S = [1, 2, 5, 10, 15, 30, 60, 120, 300, 600, 900, 1800, 3600, 7200];

/** Passo dos traços com rótulo e dos menores, em s: quadros (1, 2, 5, 10) até horas, o menor com rótulo a ≥ 60 px. */
export function passosDaRegua(pxPorS: number, fps: Racional): { maior: Seg; menor: Seg | null } {
  const quadro = fps.den / fps.num;
  const cands = [...[1, 2, 5, 10].map((n) => n * quadro).filter((s) => s < 1), ...PASSOS_S];
  const maior = cands.find((s) => s * pxPorS >= ROTULO_PX) ?? cands[cands.length - 1]!;
  const div = maior < 1 ? (maior / quadro >= 5 ? 5 : maior / quadro) : maior % 5 === 0 ? 5 : maior % 2 === 0 ? 2 : 4;
  const menor = maior / div;
  return { maior, menor: menor * pxPorS > MENOR_PX && menor >= quadro - 1e-9 ? menor : null };
}

const dois = (n: number) => String(n).padStart(2, "0");

/** "0:05", "1:20", "1:02:05" (≥ 1 h) e "0:05,250" quando o passo é menor que 1 s (maquete). */
export function textoDaRegua(t: Seg, ms: boolean): string {
  const total = Math.round(Math.max(0, t) * 1000);
  const s = Math.floor(total / 1000);
  const h = Math.floor(s / 3600);
  const base = h > 0 ? `${h}:${dois(Math.floor(s / 60) % 60)}:${dois(s % 60)}` : `${Math.floor(s / 60)}:${dois(s % 60)}`;
  return ms ? `${base},${String(total % 1000).padStart(3, "0")}` : base;
}

export const Regua = memo(function Regua(p: ReguaProps) {
  const { pxPorS, fps } = p;
  const { maior, menor } = passosDaRegua(pxPorS, fps);
  const [x0, x1] = p.janela;
  const tracos: { x: number; h: number; rot: string | null }[] = [];
  const passo = menor ?? maior;
  const k0 = Math.max(0, Math.floor(x0 / pxPorS / passo));
  const k1 = Math.ceil(x1 / pxPorS / passo);
  const porMaior = Math.round(maior / passo);
  for (let k = k0; k <= k1 && tracos.length < 1200; k++) {
    const t = k * passo;
    const ehMaior = k % porMaior === 0;
    tracos.push({ x: (t * fps.num) / fps.den, h: ehMaior ? 10 : 5, rot: ehMaior ? textoDaRegua(t, maior < 1) : null });
  }
  const slot = (clientX: number, el: HTMLElement) => {
    const r = el.getBoundingClientRect();
    // px por slot de agora: o zoom vivo pode estar à frente do desenhado
    const k = parseFloat(getComputedStyle(el).getPropertyValue("--px")) || (pxPorS * fps.den) / fps.num;
    return Math.max(0, Math.floor((clientX - r.left) / k));
  };
  const descer = (e: PE<HTMLDivElement>) => {
    if (e.button !== 0) return;
    e.preventDefault();
    const alvo = e.currentTarget;
    capturar(alvo, e.pointerId);
    p.onIr(slot(e.clientX, alvo), false);
    const mover = (ev: PointerEvent) => p.onIr(slot(ev.clientX, alvo), false);
    const soltar = (ev: PointerEvent) => {
      alvo.removeEventListener("pointermove", mover);
      alvo.removeEventListener("pointerup", soltar);
      alvo.removeEventListener("pointercancel", soltar);
      p.onIr(slot(ev.clientX, alvo), true);
    };
    alvo.addEventListener("pointermove", mover);
    alvo.addEventListener("pointerup", soltar);
    alvo.addEventListener("pointercancel", soltar);
  };
  return (
    <div className={styles.regua} style={{ width: p.larguraCss ?? p.largura }} onPointerDown={descer} data-regua="">
      {tracos.map((t) => (
        <span key={t.x}>
          <i className={styles.traco} style={{ left: `calc(var(--px) * ${t.x}px)`, height: t.h }} />
          {t.rot && (
            <span className={styles.tempo} style={{ left: `calc(var(--px) * ${t.x}px)` }}>
              {t.rot}
            </span>
          )}
        </span>
      ))}
      {p.marcadores.map((m) => (
        <span
          key={`${m.id}:${m.q}`}
          className={styles.marcador}
          style={{ left: `calc(var(--px) * ${m.q}px)`, background: m.cor }}
          title={m.rotulo}
          data-marcador={m.id}
          data-dado=""
          onPointerDown={(e) => {
            e.stopPropagation();
            p.onMarcador(m.id, m.q);
          }}
        />
      ))}
      <div ref={p.cabecaRef} className={styles.cabecaTopo} data-cabeca-topo="" />
    </div>
  );
});
