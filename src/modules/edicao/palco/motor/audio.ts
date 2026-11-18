/**
 * Som da prévia viva (§7.2, decisão do orquestrador na rodada 2 do P3.3): o som sai pelo PRÓPRIO elemento, com o
 * ganho no `volume` dele (0..1; acima de 1 a prévia fica em 1, o render é a verdade, D10). Medido no app (WebKitGTK):
 * o caminho `MediaElementSource → Web Audio` chega 90 a 160 ms depois da imagem e o `currentTime` do elemento ligado
 * corre ~1 s à frente; pelo elemento, som e imagem ficam juntos. O Web Audio fica só para o apito do "Silenciar
 * trecho" (modo tom), um oscilador de 1 kHz atrás de uma porta.
 * Medido no app (PipeWire): o `volume` do elemento é o volume do fluxo no servidor de som, e o WirePlumber guarda o
 * último valor por aplicativo e dá a todo fluxo novo do SICRO (a análise, o modo clipe). Por isso: elemento parado
 * volta a 1 (o silêncio é a pausa), e o elemento que começa a tocar recebe o volume de novo (`forcar`).
 */

// CONTRATO: troca (P3.3, rodada 2): sem `ligar`/`ganho`/`barra`/`saida`; o volume vai ao elemento.
export interface MisturaAudio {
  /** Volume do elemento: `v` × mestre, limitado a 0..1; `forcar` = escreve mesmo igual (fluxo novo herda o guardado). */
  volume(el: HTMLMediaElement, v: number, forcar?: boolean): void;
  /** Elemento parado: volume de volta a 1 (o servidor de som guarda o último valor do aplicativo). */
  repouso(el: HTMLMediaElement): void;
  mestre(v: number): void;
  /** Porta do apito (0 ou o nível dele). */
  apito(v: number): void;
  retomar(): Promise<void>;
  fechar(): void;
  /** Contexto do apito; null = sem Web Audio (o apito fica mudo). */
  readonly contexto: AudioContext | null;
}

type ComWebkit = typeof globalThis & { webkitAudioContext?: typeof AudioContext };

/** Frequência do apito (o `TOM` do `motor/audio.rs`). */
export const APITO_HZ = 1000;
/** Rampa da porta do apito (s): sem estalo. */
const RAMPA_S = 0.012;
/** Diferença de volume que vale escrever no elemento. */
const PASSO_VOLUME = 1e-3;

const limitar = (v: number) => (Number.isFinite(v) ? Math.min(Math.max(v, 0), 1) : 0);

export function criarMistura(): MisturaAudio {
  let mestre = 1;
  // o contexto nasce só quando o apito toca a primeira vez
  let ctx: AudioContext | null = null;
  let porta: GainNode | null = null;
  let osc: OscillatorNode | null = null;
  const contexto = (): AudioContext | null => {
    if (ctx) return ctx;
    const C = globalThis.AudioContext ?? (globalThis as ComWebkit).webkitAudioContext;
    if (!C) return null;
    try {
      ctx = new C();
      porta = ctx.createGain();
      porta.gain.value = 0;
      osc = ctx.createOscillator();
      osc.frequency.value = APITO_HZ;
      osc.connect(porta);
      porta.connect(ctx.destination);
      osc.start();
    } catch {
      ctx = null;
    }
    return ctx;
  };
  return {
    get contexto() {
      return ctx;
    },
    volume: (el, v, forcar = false) => {
      const alvo = limitar(v * mestre);
      // o elemento só repassa ao fluxo o que muda: para forçar, um valor vizinho antes
      if (forcar) el.volume = alvo >= 0.5 ? alvo - PASSO_VOLUME : alvo + PASSO_VOLUME;
      if (forcar || Math.abs(el.volume - alvo) > PASSO_VOLUME || (alvo === 0 && el.volume !== 0)) el.volume = alvo;
    },
    repouso: (el) => {
      if (el.paused && el.volume !== 1) el.volume = 1;
    },
    mestre: (v) => {
      mestre = Math.max(0, Number.isFinite(v) ? v : 0);
    },
    apito: (v) => {
      const nivel = limitar(v * mestre);
      if (nivel <= 0 && !ctx) return;
      const c = contexto();
      if (!c || !porta) return;
      if (c.state === "suspended") void c.resume().catch(() => {});
      // instante 0 = agora (o passado vale o tempo atual do contexto)
      porta.gain.cancelScheduledValues(0);
      porta.gain.setTargetAtTime(nivel, 0, RAMPA_S / 3);
    },
    retomar: async () => {
      if (ctx?.state === "suspended") await ctx.resume().catch(() => {});
    },
    fechar: () => {
      if (ctx) void ctx.close().catch(() => {});
      ctx = null;
    },
  };
}
