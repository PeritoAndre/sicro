/**
 * Base das medições do Diagnóstico de vídeo (arquitetura §7.7): o que elas usam do navegador, atrás de uma
 * interface só, para o vitest trocar relógio, `<video>` e canvas por falsos (`falsos.ts`).
 * Lógica das páginas `p5/page/bench.js` e `ce/wk/page/{tempo3,pool8}.js` do dossiê.
 */

export interface Ambiente {
  /** `performance.now()`. */
  agora(): number;
  /** Próximo `requestAnimationFrame`. Lança `Cancelado` se o diagnóstico foi cancelado. */
  quadro(): Promise<number>;
  dormir(ms: number): Promise<void>;
  /** `<video>` novo, já preso ao palco (2×2 px, opacidade ≈ 0: o WebKit pode pausar vídeo fora da área visível). */
  video(): HTMLVideoElement;
  /** Canvas auxiliar, fora do DOM. */
  canvas(w: number, h: number): HTMLCanvasElement;
  /** Canvas visível das medições que desenham na tela. */
  palco: HTMLCanvasElement;
  /** Nome da mídia de teste → `src`. */
  url(nome: string): string;
  /** A página ficou oculta desde a chamada anterior (ou está oculta agora)? rAF e decodificação param. */
  oculta?(): boolean;
}

export class Cancelado extends Error {
  constructor() {
    super("cancelado");
    this.name = "Cancelado";
  }
}

export type Estatistica = { n: number; min: number; p50: number; p95: number; max: number; media: number };

/** `stats()` do bench.js (duas casas). */
export function stats(a: number[]): Estatistica | null {
  if (!a.length) return null;
  const s = [...a].sort((x, y) => x - y);
  const q = (p: number) => s[Math.min(s.length - 1, Math.floor(p * s.length))] ?? 0;
  const r2 = (v: number) => Math.round(v * 100) / 100;
  return {
    n: s.length,
    min: r2(s[0] ?? 0),
    p50: r2(q(0.5)),
    p95: r2(q(0.95)),
    max: r2(s[s.length - 1] ?? 0),
    media: r2(s.reduce((x, y) => x + y, 0) / s.length),
  };
}

/** Evento ou tempo esgotado, sem lançar (`once` do tempo3 e do pool8). */
export function esperar(amb: Ambiente, el: EventTarget, ev: string, ms: number): Promise<"ok" | "timeout"> {
  let h: (() => void) | null = null;
  const evento = new Promise<"ok">((res) => {
    h = () => res("ok");
    el.addEventListener(ev, h, { once: true });
  });
  return Promise.race([evento, amb.dormir(ms).then(() => "timeout" as const)]).finally(() => {
    if (h) el.removeEventListener(ev, h);
  });
}

/** Evento ou erro "timeout <ev>" (`once` do bench.js). */
export async function esperarOuFalhar(amb: Ambiente, el: EventTarget, ev: string, ms = 15000): Promise<void> {
  if ((await esperar(amb, el, ev, ms)) === "timeout") throw new Error("timeout " + ev);
}

/** Teto de espera do `play()`: sem ele um elemento parado prende o diagnóstico inteiro. */
export const PRAZO_PLAY_MS = 10000;

/** `p` ou erro "timeout <rotulo>" depois de `ms`. */
export function comPrazo<T>(amb: Ambiente, p: Promise<T>, ms: number, rotulo: string): Promise<T> {
  return Promise.race([
    p,
    amb.dormir(ms).then((): never => {
      throw new Error("timeout " + rotulo);
    }),
  ]);
}

/** `seekTo` do bench.js. */
export async function buscar(amb: Ambiente, v: HTMLVideoElement, t: number): Promise<void> {
  const p = esperarOuFalhar(amb, v, "seeked", 20000);
  v.currentTime = t;
  await p;
}

/** `mkVideo` do bench.js: abre, espera metadados (falha em MediaError) e `canplay`; guarda os tempos de abertura. */
export async function abrirVideo(amb: Ambiente, nome: string): Promise<HTMLVideoElement & { __tMeta?: number; __tCanplay?: number }> {
  const v = amb.video() as HTMLVideoElement & { __tMeta?: number; __tCanplay?: number };
  v.muted = true;
  v.preload = "auto";
  v.playsInline = true;
  v.src = amb.url(nome);
  const t0 = amb.agora();
  if (v.readyState < 1) {
    const erro = new Promise<never>((_, rej) =>
      v.addEventListener(
        "error",
        () => rej(new Error("MediaError " + (v.error && v.error.code) + " " + (v.error && v.error.message) + " " + nome)),
        { once: true },
      ),
    );
    try {
      await Promise.race([esperarOuFalhar(amb, v, "loadedmetadata"), erro]);
    } catch (e) {
      v.remove();
      throw e;
    }
  }
  v.__tMeta = amb.agora() - t0;
  if (v.readyState < 3) await esperarOuFalhar(amb, v, "canplay").catch(() => {});
  v.__tCanplay = amb.agora() - t0;
  return v;
}

/** Leitor do código binário do quadro: 12 blocos no canto superior esquerdo, lado = largura/48 (`readFrameNo`). */
export function leitorDeCodigo(amb: Ambiente): (v: HTMLVideoElement) => number {
  const rd = amb.canvas(48, 4);
  const rdx = rd.getContext("2d", { willReadFrequently: true });
  return (v) => {
    try {
      if (!rdx) return -1;
      const w = v.videoWidth;
      rdx.imageSmoothingEnabled = false;
      rdx.drawImage(v, 0, 0, w / 4, w / 48, 0, 0, 48, 4);
      const d = rdx.getImageData(0, 0, 48, 4).data;
      let n = 0;
      for (let i = 0; i < 12; i++) {
        const p = (2 * 48 + i * 4 + 2) * 4;
        if ((d[p] ?? 0) + (d[p + 1] ?? 0) + (d[p + 2] ?? 0) > 384) n |= 1 << i;
      }
      return n;
    } catch {
      return -1;
    }
  };
}

export const r4 = (v: number) => Math.round(v * 1e4) / 1e4;
export const r1 = (v: number) => Math.round(v * 10) / 10;

export function textoDeErro(e: unknown): string {
  return String((e instanceof Error && e.stack) || e);
}
