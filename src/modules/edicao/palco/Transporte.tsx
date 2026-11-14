/**
 * Transporte do palco no modo clipe (§8.2): a faixa é a mídia inteira; o clipe é a parte destacada, com as bordas
 * arrastáveis; o cursor pode sair do clipe (para aparar ou alargar). Tempo `HH:MM:SS,mmm` e o número do quadro
 * da tabela. As contas ficam em `faixa.ts`; aqui só px e eventos.
 */

import { useLayoutEffect, useRef, useState, type PointerEvent as PE } from "react";
import { Aperture, Check, ChevronFirst, ChevronLast, Clapperboard, Film, Loader2, Maximize2, Minimize2, MonitorPlay, Pause, Play, Scan, Square, StepBack, StepForward } from "lucide-react";
import type { Seg } from "@domain/edicao";
import { formatarTempo } from "@components/CampoTempo/tempoTexto";
import { tempoDoX, xDoTempo, type Escala } from "./faixa";
import type { Fiel } from "./quadroFiel";
import styles from "./Palco.module.css";
import { capturar } from "./capturar";

export type Fase = "inicio" | "move" | "fim";
export type EstadoRender = "nada" | "pedindo" | "tocando";

export interface TransporteProps {
  de: Seg;
  ate: Seg;
  /** Clipe em [início, fim); null = sem faixa (mídia sem tabela). */
  faixa: [Seg, Seg] | null;
  cursor: Seg;
  quadro: number | null;
  base: "fonte" | "programa";
  tocando: boolean;
  /** Bordas e I/O ligados (tabela confiável e mídia de vídeo). */
  bordas: boolean;
  fiel: Fiel;
  render: EstadoRender;
  expandido: boolean;
  desligado?: boolean;
  onBuscar(t: Seg, fase: Fase): void;
  onBorda(borda: "inicio" | "fim", t: Seg, fase: Fase): void;
  onTocar(): void;
  onPasso(d: 1 | -1): void;
  onInicio(): void;
  onFim(): void;
  onQuadroFinal(): void;
  onPreviaRender(): void;
  onAjustar(): void;
  onExpandir(): void;
}

function IconeEstado({ fiel }: { fiel: Fiel }) {
  if (fiel === "pedindo") return <Loader2 size={14} className={styles.gira} aria-hidden />;
  if (fiel !== "viva") return <Check size={14} aria-hidden />;
  return <Aperture size={14} aria-hidden />;
}

export function Transporte(p: TransporteProps) {
  const pista = useRef<HTMLDivElement | null>(null);
  const [largura, setLargura] = useState(0);
  useLayoutEffect(() => {
    const el = pista.current;
    if (!el) return;
    const medir = () => setLargura(el.clientWidth);
    medir();
    const ro = new ResizeObserver(medir);
    ro.observe(el);
    return () => ro.disconnect();
  }, []);
  const escala: Escala = { de: p.de, ate: p.ate, largura };
  const tDe = (e: PE) => tempoDoX(escala, e.clientX - (pista.current?.getBoundingClientRect().left ?? 0));
  const pct = (t: Seg) => (largura > 0 ? `${(xDoTempo(escala, t) / largura) * 100}%` : "0%");

  const arrastar = (f: (t: Seg, fase: Fase) => void) => (e: PE<HTMLDivElement>) => {
    if (e.button !== 0 || p.desligado) return;
    e.preventDefault();
    e.stopPropagation();
    const alvo = e.currentTarget;
    capturar(alvo, e.pointerId);
    f(tDe(e), "inicio");
    const mover = (ev: PointerEvent) => f(tDe(ev as unknown as PE), "move");
    const soltar = (ev: PointerEvent) => {
      alvo.removeEventListener("pointermove", mover);
      alvo.removeEventListener("pointerup", soltar);
      alvo.removeEventListener("pointercancel", soltar);
      f(tDe(ev as unknown as PE), "fim");
    };
    alvo.addEventListener("pointermove", mover);
    alvo.addEventListener("pointerup", soltar);
    alvo.addEventListener("pointercancel", soltar);
  };

  const IconeBase = p.base === "fonte" ? Film : Clapperboard;
  return (
    <div className={styles.transporte}>
      <div ref={pista} className={styles.pista} onPointerDown={arrastar(p.onBuscar)} data-pista="">
        {p.faixa && (
          <div className={styles.faixa} style={{ left: pct(p.faixa[0]), width: `calc(${pct(p.faixa[1])} - ${pct(p.faixa[0])})` }} data-faixa="">
            {p.bordas && (
              <>
                <div className={`${styles.borda} ${styles.bordaIni}`} onPointerDown={arrastar((t, f) => p.onBorda("inicio", t, f))} data-borda="inicio" />
                <div className={`${styles.borda} ${styles.bordaFim}`} onPointerDown={arrastar((t, f) => p.onBorda("fim", t, f))} data-borda="fim" />
              </>
            )}
          </div>
        )}
        <div className={styles.cursor} style={{ left: pct(p.cursor) }} data-cursor="" />
      </div>
      <div className={styles.controles}>
        <div className={styles.grupo}>
          <button type="button" className={styles.icone} title="Início do clipe" aria-label="Início do clipe" onClick={p.onInicio} disabled={p.desligado}>
            <ChevronFirst size={16} />
          </button>
          <button type="button" className={styles.icone} title="Quadro anterior" aria-label="Quadro anterior" onClick={() => p.onPasso(-1)} disabled={p.desligado}>
            <StepBack size={16} />
          </button>
          <button type="button" className={`${styles.icone} ${styles.tocar}`} title="Reproduzir" aria-label="Reproduzir" aria-pressed={p.tocando} onClick={p.onTocar} disabled={p.desligado}>
            {p.tocando ? <Pause size={16} /> : <Play size={16} />}
          </button>
          <button type="button" className={styles.icone} title="Próximo quadro" aria-label="Próximo quadro" onClick={() => p.onPasso(1)} disabled={p.desligado}>
            <StepForward size={16} />
          </button>
          <button type="button" className={styles.icone} title="Fim do clipe" aria-label="Fim do clipe" onClick={p.onFim} disabled={p.desligado}>
            <ChevronLast size={16} />
          </button>
        </div>
        <span className={styles.tempo} data-tempo="">
          <IconeBase size={13} aria-hidden className={styles.base} />
          <span>{formatarTempo(p.cursor)}</span>
          {p.quadro !== null && <span className={styles.quadroNum}>quadro {p.quadro}</span>}
        </span>
        <div className={`${styles.grupo} ${styles.direita}`}>
          <button type="button" className={`${styles.mini} ${p.fiel !== "viva" ? styles.ativo : ""}`} onClick={p.onQuadroFinal} disabled={p.desligado} title="Quadro final" data-acao="quadro-final">
            <IconeEstado fiel={p.fiel} />
            <span className={styles.rotuloMini}>Quadro final</span>
          </button>
          <button type="button" className={`${styles.mini} ${p.render !== "nada" ? styles.ativo : ""}`} onClick={p.onPreviaRender} disabled={p.desligado} title="Prévia do render" data-acao="previa-render">
            {p.render === "pedindo" ? <Loader2 size={14} className={styles.gira} aria-hidden /> : p.render === "tocando" ? <Square size={14} aria-hidden /> : <MonitorPlay size={14} aria-hidden />}
            <span className={styles.rotuloMini}>Prévia do render</span>
          </button>
          <button type="button" className={styles.icone} title="Ajustar à tela" aria-label="Ajustar à tela" onClick={p.onAjustar}>
            <Scan size={16} />
          </button>
          <button type="button" className={styles.icone} title="Expandir" aria-label="Expandir" aria-pressed={p.expandido} onClick={p.onExpandir}>
            {p.expandido ? <Minimize2 size={16} /> : <Maximize2 size={16} />}
          </button>
        </div>
      </div>
    </div>
  );
}
