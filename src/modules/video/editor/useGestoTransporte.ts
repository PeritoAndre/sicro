/**
 * Gesto das setas comum à análise e à Edição (§8.5): ←/→ toque = um quadro, segurar (`SEGURAR_MS`) toca ou dá ré e
 * soltar para; Shift+←/→ = ±1 s (repete com a tecla); nada com campo editável em foco, com Ctrl/Alt/Meta nem com
 * `ativo` falso. Usado pelo `VideoPlayerPanel`; o P1.0 liga no transporte da Edição ao fechar a fase (O3).
 * Fica fora do `useShortcuts`: toque × segurar precisa de keyup e temporizador.
 */

import { useEffect, useRef } from "react";

/** Seta segurada além disto (ms) deixa de ser passo de quadro e vira movimento. */
export const SEGURAR_MS = 300;

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

function digitando(): boolean {
  const el = document.activeElement as HTMLElement | null;
  if (!el) return false;
  const tag = el.tagName;
  return tag === "INPUT" || tag === "TEXTAREA" || tag === "SELECT" || el.isContentEditable === true;
}

const direcao = (key: string): 1 | -1 | null => (key === "ArrowRight" ? 1 : key === "ArrowLeft" ? -1 : null);

export function useGestoTransporte(acoes: AcoesDoGesto, ativo: boolean): void {
  const acoesRef = useRef(acoes);
  acoesRef.current = acoes;
  const ativoRef = useRef(ativo);
  // seta apertada, temporizador do segurar e o movimento em curso
  const g = useRef<{ dir: 1 | -1 | null; timer: number | null; movendo: 1 | -1 | null }>({ dir: null, timer: null, movendo: null });

  /** Esquece a tecla apertada; se estava movendo, para. */
  const soltar = () => {
    const s = g.current;
    if (s.timer !== null) window.clearTimeout(s.timer);
    const movendo = s.movendo;
    g.current = { dir: null, timer: null, movendo: null };
    if (movendo !== null) acoesRef.current.parar();
  };

  useEffect(() => {
    ativoRef.current = ativo;
    if (!ativo) soltar();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [ativo]);

  useEffect(() => {
    const baixo = (e: KeyboardEvent) => {
      if (!ativoRef.current || digitando()) return;
      if (e.ctrlKey || e.metaKey || e.altKey) return;
      const d = direcao(e.key);
      if (d === null) return;
      e.preventDefault();
      if (e.shiftKey) {
        acoesRef.current.saltar(d);
        return;
      }
      // segurar é o temporizador, não a repetição do sistema; uma direção por vez
      if (e.repeat || g.current.dir !== null) return;
      g.current.dir = d;
      g.current.timer = window.setTimeout(() => {
        g.current.timer = null;
        g.current.movendo = d;
        acoesRef.current.mover(d);
      }, SEGURAR_MS);
    };
    const cima = (e: KeyboardEvent) => {
      const d = direcao(e.key);
      if (d === null || g.current.dir !== d) return;
      const tocou = g.current.movendo === null;
      soltar();
      if (tocou) acoesRef.current.passo(d);
    };
    // janela perdeu o foco com a seta apertada: o keyup não vem
    const fora = () => soltar();
    window.addEventListener("keydown", baixo);
    window.addEventListener("keyup", cima);
    window.addEventListener("blur", fora);
    return () => {
      window.removeEventListener("keydown", baixo);
      window.removeEventListener("keyup", cima);
      window.removeEventListener("blur", fora);
      soltar();
    };
    // ligado uma vez: só lê refs
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);
}
