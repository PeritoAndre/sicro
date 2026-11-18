/**
 * Laço da prévia viva (§7.2, §7.3): rAF → `relogio.q()` → `estadoEm` → `pinta`; prepara o segmento seguinte quando o
 * atual começa, troca o mestre na fronteira, corrige seguidores por seek acima de 100 ms. O palco usa o modo
 * "Programa" só com `PREVIA_VIVA_PRONTA` (regra 3 de contratos-fase4.md). Dono: P3.3 (casca da abertura).
 */

import type { ProjetoEdicao } from "@domain/edicao";
import type { PlanoPrevia } from "./plano";
import { criarRelogio, type Relogio } from "./relogio";

/** O pacote liga ao entregar. */
export const PREVIA_VIVA_PRONTA = false;

export interface MotorPrevia {
  readonly relogio: Relogio;
  /** Canvas de saída e o elemento que hospeda o pool. */
  ligar(canvas: HTMLCanvasElement, host: HTMLElement): void;
  desligar(): void;
  /** Plano novo (documento mudou): o relógio fica no mesmo slot. */
  plano(p: PlanoPrevia, doc: ProjetoEdicao): void;
}

export function criarMotor(): MotorPrevia {
  const relogio = criarRelogio({ fps: { num: 30, den: 1 }, total_q: 0 });
  return { relogio, ligar: () => {}, desligar: () => {}, plano: () => {} };
}
