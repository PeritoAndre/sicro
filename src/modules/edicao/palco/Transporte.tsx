/**
 * Transporte do palco no modo clipe (§8.2): a faixa é a mídia inteira; o clipe é a parte destacada, com as bordas
 * arrastáveis; o cursor pode sair do clipe (para aparar ou alargar). Tempo `HH:MM:SS,mmm` e o número do quadro
 * da tabela. Na base de fonte, a forma de onda da mídia (`edicao_onda`, P1.9) fica no fundo da pista. As contas
 * ficam em `faixa.ts`; aqui só px e eventos.
 */

import { useEffect, useLayoutEffect, useMemo, useRef, useState, type PointerEvent as PE } from "react";
import { Aperture, Check, ChevronFirst, ChevronLast, Clapperboard, Film, Loader2, Maximize2, Minimize2, MonitorPlay, Pause, Play, Scan, Square, StepBack, StepForward, TriangleAlert } from "lucide-react";
import type { Onda, Seg } from "@domain/edicao";
import { commands } from "@core/commands";
import { formatarTempo } from "@components/CampoTempo/tempoTexto";
import { useEdicaoStore } from "../store/edicaoStore";
import { chaveFundo, fundoPorChave, useFundoStore } from "../store/fundoStore";
import { acharClipe, acharMidia } from "../modelo/projeto";
import { midiaDoClipe } from "./cena";
import { tempoDoX, xDoTempo, type Escala } from "./faixa";
import type { Fiel } from "./quadroFiel";
import styles from "./Palco.module.css";
import { capturar } from "./capturar";

export type Fase = "inicio" | "move" | "fim";
/** "falhou": a última prévia do render não saiu (ícone até pedir de novo ou trocar de clipe). */
export type EstadoRender = "nada" | "pedindo" | "tocando" | "falhou";

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
  // CONTRATO: acréscimo (P1.9), opcional.
  /** SHA-256 da mídia da pista (onda). Sem a prop: a do clipe do palco (cursor de clipe ou foco). */
  sha256?: string | null;
}

/** Mínimo e máximo (s16) de uma coluna de px. */
export interface Coluna {
  x: number;
  min: number;
  max: number;
}

/** Colunas de px de `[de, ate)` em `largura` px: mínimo e máximo dos baldes que cada uma cobre (ao menos o balde em
 *  que ela cai). Coluna fora do som não entra. */
export function colunasDaOnda(o: Onda, de: Seg, ate: Seg, largura: number): Coluna[] {
  const n = Math.min(o.min.length, o.max.length);
  const w = Math.floor(largura);
  if (n === 0 || w <= 0 || !(ate > de) || !(o.por_s > 0)) return [];
  const balde = (t: Seg) => (t - o.inicio_s) * o.por_s;
  const saida: Coluna[] = [];
  for (let x = 0; x < w; x++) {
    const b0 = balde(de + ((ate - de) * x) / w);
    const b1 = balde(de + ((ate - de) * (x + 1)) / w);
    const i0 = Math.max(0, Math.floor(b0));
    const i1 = Math.min(n, Math.max(Math.ceil(b1), i0 + 1));
    if (b1 <= 0 || i0 >= n) continue;
    let mn = o.min[i0]!;
    let mx = o.max[i0]!;
    for (let i = i0 + 1; i < i1; i++) {
      if (o.min[i]! < mn) mn = o.min[i]!;
      if (o.max[i]! > mx) mx = o.max[i]!;
    }
    saida.push({ x, min: mn, max: mx });
  }
  return saida;
}

/** Caminho SVG (viewBox `0 0 largura 100`): um traço vertical por coluna, na escala do maior pico da onda (som baixo
 *  ainda aparece; abaixo de 512 é ruído e fica na escala de 512). */
export function caminhoDaOnda(colunas: Coluna[], pico: number): string {
  const escala = Math.max(512, pico);
  const y = (v: number) => 50 - (50 * Math.max(-escala, Math.min(escala, v))) / escala;
  let d = "";
  for (const c of colunas) {
    let a = y(c.max);
    let b = y(c.min);
    if (b - a < 1) {
      a -= 0.5;
      b += 0.5;
    }
    d += `M${c.x + 0.5} ${a.toFixed(1)}V${b.toFixed(1)}`;
  }
  return d;
}

/** Maior valor absoluto da onda. */
function picoDe(o: Onda): number {
  let p = 0;
  for (const v of o.max) if (v > p) p = v;
  for (const v of o.min) if (-v > p) p = -v;
  return p;
}

/** Ondas já recebidas, por caso e mídia (poucas: uma hora de som são 720 mil números). */
const recebidas = new Map<string, Onda>();
const MAX_RECEBIDAS = 4;

async function pedirOnda(ws: string, sha: string): Promise<Onda | null> {
  try {
    return (await commands.edicaoOnda(ws, sha)) ?? null;
  } catch {
    return null;
  }
}

/** Onda da mídia: a guardada, ou pedida ao backend (que a pede à fila de fundo e responde null); pronta a tarefa
 *  `onda:<sha256>`, pede de novo. Falha não repete sozinha. */
export function useOnda(ws: string | null, sha: string | null): Onda | null {
  const pronta = useFundoStore((s) => {
    if (!sha) return null;
    const t = fundoPorChave(s.tarefas, chaveFundo("onda", sha));
    return t?.estado.tipo === "pronta" ? t.id : null;
  });
  const chave = ws && sha ? `${ws}\n${sha}` : null;
  const [onda, setOnda] = useState<{ chave: string; onda: Onda } | null>(null);
  useEffect(() => {
    if (!chave || !ws || !sha) return;
    const guardada = recebidas.get(chave);
    if (guardada) {
      setOnda({ chave, onda: guardada });
      return;
    }
    let vivo = true;
    void pedirOnda(ws, sha).then((o) => {
      if (!o) return;
      recebidas.delete(chave);
      recebidas.set(chave, o);
      while (recebidas.size > MAX_RECEBIDAS) recebidas.delete(recebidas.keys().next().value!);
      if (vivo) setOnda({ chave, onda: o });
    });
    return () => {
      vivo = false;
    };
  }, [chave, ws, sha, pronta]);
  return onda && onda.chave === chave ? onda.onda : null;
}

/** SHA-256 da mídia do clipe que o palco mostra (a mesma regra do `Palco`). */
function useShaDoPalco(): string | null {
  return useEdicaoStore((s) => {
    const id = s.cursor.modo === "clipe" ? s.cursor.clipe : s.selecao.foco?.k === "clipe" ? s.selecao.foco.id : null;
    const achado = s.doc && id ? acharClipe(s.doc, id) : null;
    const midia = achado?.onde === "principal" ? midiaDoClipe(achado.clipe) : null;
    return (s.doc && midia ? acharMidia(s.doc, midia)?.sha256 : null) ?? null;
  });
}

function OndaDaPista({ onda, de, ate, largura }: { onda: Onda; de: Seg; ate: Seg; largura: number }) {
  const pico = useMemo(() => picoDe(onda), [onda]);
  const d = useMemo(() => caminhoDaOnda(colunasDaOnda(onda, de, ate, largura), pico), [onda, de, ate, largura, pico]);
  if (!d) return null;
  return (
    <svg
      viewBox={`0 0 ${Math.max(1, Math.floor(largura))} 100`}
      preserveAspectRatio="none"
      aria-hidden
      data-onda=""
      style={{ position: "absolute", inset: 0, width: "100%", height: "100%", pointerEvents: "none" }}
    >
      <path d={d} stroke="var(--sicro-fg-faint)" strokeWidth={1} vectorEffect="non-scaling-stroke" fill="none" />
    </svg>
  );
}

function IconeEstado({ fiel }: { fiel: Fiel }) {
  if (fiel === "pedindo") return <Loader2 size={14} className={styles.gira} aria-hidden />;
  if (fiel !== "viva") return <Check size={14} aria-hidden />;
  return <Aperture size={14} aria-hidden />;
}

export function Transporte(p: TransporteProps) {
  const ws = useEdicaoStore((s) => s.ws);
  const doPalco = useShaDoPalco();
  const onda = useOnda(ws, p.base === "fonte" ? (p.sha256 === undefined ? doPalco : p.sha256) : null);
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
        {onda && p.base === "fonte" && <OndaDaPista onda={onda} de={p.de} ate={p.ate} largura={largura} />}
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
          <button type="button" className={`${styles.mini} ${p.render === "pedindo" || p.render === "tocando" ? styles.ativo : ""}`} onClick={p.onPreviaRender} disabled={p.desligado} title="Prévia do render" data-acao="previa-render">
            {p.render === "pedindo" ? (
              <Loader2 size={14} className={styles.gira} aria-hidden />
            ) : p.render === "tocando" ? (
              <Square size={14} aria-hidden />
            ) : p.render === "falhou" ? (
              <TriangleAlert size={14} className={styles.falhou} aria-hidden data-render-falhou="" />
            ) : (
              <MonitorPlay size={14} aria-hidden />
            )}
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
