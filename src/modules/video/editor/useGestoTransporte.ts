/**
 * Gesto das setas comum à análise e à Edição (§8.5): ←/→ toque = um quadro, segurar (`SEGURAR_MS`) toca ou dá ré e
 * soltar para; Shift+←/→ = ±1 s; nada com campo editável em foco nem com `ativo` falso. O P1.10 tira o gesto de
 * `VideoPlayerPanel.tsx` para cá; o P1.0 liga no transporte da Edição ao fechar a fase (O3). Fase 0: casca.
 */

/** Quem usa o gesto diz o que cada movimento faz no seu reprodutor. */
export interface AcoesDoGesto {
  /** Um quadro para a frente (1) ou para trás (−1). */
  passo(d: 1 | -1): void;
  /** Segurou: toca para a frente (1) ou dá ré (−1) até `parar`. */
  mover(d: 1 | -1): void;
  parar(): void;
  /** Shift+seta: ±1 s. */
  saltar(segundos: number): void;
}

export function useGestoTransporte(acoes: AcoesDoGesto, ativo: boolean): void {
  void acoes;
  void ativo;
}
