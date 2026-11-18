/**
 * Som da prévia viva (§7.2): `MediaElementSource → Gain` por item → barras "original" e "narracao" → ganho mestre →
 * limitador. Só `GainNode` (volume > 1 no elemento dá erro). `retomar()` no primeiro clique em tocar. O apito do
 * "Silenciar trecho" (modo tom) é um oscilador de 1 kHz atrás de uma porta.
 */

export type Barra = "original" | "narracao";

export interface MisturaAudio {
  /** Liga o elemento ANTES de ele receber o `src` (§7.2); null = sem Web Audio (prévia muda). */
  ligar(el: HTMLMediaElement, barra: Barra): GainNode | null;
  ganho(no: GainNode, v: number): void;
  mestre(v: number): void;
  retomar(): Promise<void>;
  fechar(): void;
  // CONTRATO: acréscimos (P3.3)
  /** Ganho da barra (abaixar o original sob a narração): vai a `v` com a constante de tempo `tau` (s). */
  barra(b: Barra, v: number, tau: number): void;
  /** Porta do apito (0 ou o ganho dele). */
  apito(v: number): void;
  /** O nó antes do destino (a bancada mede o RMS por um `AnalyserNode` ligado aqui). */
  readonly saida: AudioNode;
  readonly contexto: AudioContext;
}

type ComWebkit = typeof globalThis & { webkitAudioContext?: typeof AudioContext };

/** Frequência e nível do apito (o tom do "Silenciar trecho"). */
export const APITO_HZ = 1000;
/** Ganho mudado em rampa curta (s): sem estalo. */
const RAMPA_S = 0.012;

/** null = o motor do navegador não tem Web Audio (a prévia fica muda, com o ícone de estado). */
export function criarMistura(): MisturaAudio | null {
  const C = globalThis.AudioContext ?? (globalThis as ComWebkit).webkitAudioContext;
  if (!C) return null;
  let ctx: AudioContext;
  try {
    ctx = new C();
  } catch {
    return null;
  }
  const mestre = ctx.createGain();
  const limitador = ctx.createDynamicsCompressor();
  // limitador: só pega o que passaria de −1 dBFS
  limitador.threshold.value = -1;
  limitador.knee.value = 0;
  limitador.ratio.value = 20;
  limitador.attack.value = 0.002;
  limitador.release.value = 0.1;
  mestre.connect(limitador);
  limitador.connect(ctx.destination);
  const barras: Record<Barra, GainNode> = { original: ctx.createGain(), narracao: ctx.createGain() };
  barras.original.connect(mestre);
  barras.narracao.connect(mestre);
  const porta = ctx.createGain();
  porta.gain.value = 0;
  const osc = ctx.createOscillator();
  osc.frequency.value = APITO_HZ;
  osc.connect(porta);
  porta.connect(mestre);
  let oscIniciado = false;
  const ligados = new WeakSet<HTMLMediaElement>();
  // instante 0 = agora (o passado vale o tempo atual do contexto): curva exponencial curta, sem estalo
  const rampa = (p: AudioParam, v: number) => {
    p.cancelScheduledValues(0);
    p.setTargetAtTime(v, 0, RAMPA_S / 3);
  };
  return {
    saida: limitador,
    contexto: ctx,
    ligar: (el, barra) => {
      // um elemento só tem uma fonte na vida (criar outra dá erro)
      if (ligados.has(el)) return null;
      try {
        const src = ctx.createMediaElementSource(el);
        const g = ctx.createGain();
        g.gain.value = 0;
        src.connect(g);
        g.connect(barras[barra]);
        ligados.add(el);
        return g;
      } catch {
        return null;
      }
    },
    ganho: (no, v) => {
      const alvo = Number.isFinite(v) ? Math.max(0, v) : 0;
      if (Math.abs(no.gain.value - alvo) > 1e-4) rampa(no.gain, alvo);
    },
    mestre: (v) => rampa(mestre.gain, Math.max(0, v)),
    barra: (b, v, tau) => {
      const p = barras[b].gain;
      p.cancelScheduledValues(0);
      p.setTargetAtTime(Math.max(0, v), 0, Math.max(tau, 0.001));
    },
    apito: (v) => {
      if (!oscIniciado && v > 0) {
        osc.start();
        oscIniciado = true;
      }
      rampa(porta.gain, Math.max(0, v));
    },
    retomar: async () => {
      if (ctx.state === "suspended") await ctx.resume().catch(() => {});
    },
    fechar: () => {
      void ctx.close().catch(() => {});
    },
  };
}
