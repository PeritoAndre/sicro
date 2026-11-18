/**
 * Som da prévia viva (§7.2): `MediaElementSource → Gain` por item → barras "original" e "narracao" → ganho mestre →
 * limitador. Só `GainNode` (volume > 1 no elemento dá erro). `retomar()` no primeiro clique em tocar.
 * Dono: P3.3 (casca da abertura, contratos-fase4.md B8).
 */

export type Barra = "original" | "narracao";

export interface MisturaAudio {
  /** Liga o elemento ANTES de ele receber o `src` (§7.2); null = sem Web Audio (prévia muda). */
  ligar(el: HTMLMediaElement, barra: Barra): GainNode | null;
  ganho(no: GainNode, v: number): void;
  mestre(v: number): void;
  retomar(): Promise<void>;
  fechar(): void;
}

/** null = o motor do navegador não tem Web Audio (a prévia fica muda, com o ícone de estado). */
export function criarMistura(): MisturaAudio | null {
  return null;
}
