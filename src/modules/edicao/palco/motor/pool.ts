/**
 * Elementos `<video>` da prévia viva (§7.2): `tamanhoDoPool` elementos criados uma vez, `preload="auto"`, dentro do
 * palco com 2×2 px e opacidade ≈ 0; o som sai por eles mesmos (volume posto pelo motor, `audio.ts`); chave = arquivo
 * (trocar o `src` custa 210 a 410 ms). O seguinte é preparado quando o segmento atual COMEÇA (§7.3). O som de trilha de áudio usa o mesmo
 * elemento (um `<video>` toca arquivo só de som).
 */

import type { Seg } from "@domain/edicao";
import { porNoTempo, tempoDoElemento } from "./elemento";

export const POOL_MIN = 4;
export const POOL_MAX = 8;

/** `max(4, 2 × maior número de células)`, até 8. */
export function tamanhoDoPool(celulasMax: number): number {
  const n = Number.isFinite(celulasMax) ? Math.floor(celulasMax) : 1;
  return Math.min(POOL_MAX, Math.max(POOL_MIN, 2 * n));
}

export interface ElementoPool {
  el: HTMLVideoElement;
  /** Arquivo que ele tem (null = livre). */
  chave: string | null;
  // CONTRATO: acréscimos (P3.3; `ganho` saiu na rodada 2: o volume é do elemento)
  /** Endereço que ele toca. */
  src: string | null;
  /** Em uso pelo plano (não é trocado). */
  preso: boolean;
  /** Último uso (para o menos usado ceder). */
  usado: number;
}

export interface Pool {
  /** O elemento do arquivo (reaproveita se já tem; senão o menos usado). */
  pedir(chave: string, src: string): ElementoPool;
  /** Pausado no tempo do elemento `t_el` (meio do quadro de entrada). */
  preparar(chave: string, src: string, t_el: Seg): Promise<void>;
  soltar(chave: string): void;
  destruir(): void;
  // CONTRATO: acréscimos (P3.3)
  /** O elemento dessa chave, se há (sem pedir). */
  achar(chave: string): ElementoPool | null;
  readonly elementos: readonly ElementoPool[];
}

/** Espera a busca terminar (`seeked`), com teto: o que não chega em `ms` dá por chegado. */
function aposBusca(el: HTMLVideoElement, ms = 3000): Promise<void> {
  return new Promise((ok) => {
    if (!el.seeking) return ok();
    const fim = () => {
      clearTimeout(h);
      el.removeEventListener("seeked", fim);
      ok();
    };
    const h = setTimeout(fim, ms);
    el.addEventListener("seeked", fim);
  });
}

function aposDados(el: HTMLVideoElement, ms = 5000): Promise<void> {
  return new Promise((ok) => {
    if (el.readyState >= 1) return ok();
    const fim = () => {
      clearTimeout(h);
      el.removeEventListener("loadedmetadata", fim);
      el.removeEventListener("error", fim);
      ok();
    };
    const h = setTimeout(fim, ms);
    el.addEventListener("loadedmetadata", fim);
    el.addEventListener("error", fim);
  });
}

// CONTRATO: `mistura` e `barra` saíram (rodada 2): o som não passa pelo Web Audio.
export function criarPool(op: { tamanho: number; host: HTMLElement; criar?: () => HTMLVideoElement }): Pool {
  let relogio = 0;
  const lista: ElementoPool[] = [];
  for (let i = 0; i < Math.min(Math.max(op.tamanho, 1), POOL_MAX); i++) {
    const el = op.criar ? op.criar() : document.createElement("video");
    el.preload = "auto";
    el.playsInline = true;
    el.muted = false;
    el.setAttribute("data-pool", String(i));
    // dentro do palco, quase invisível: o WebKit pode pausar vídeo mudo fora da área visível
    Object.assign(el.style, { position: "absolute", left: "0", top: "0", width: "2px", height: "2px", opacity: "0.01", pointerEvents: "none" });
    op.host.appendChild(el);
    lista.push({ el, chave: null, src: null, preso: false, usado: 0 });
  }
  const achar = (chave: string) => lista.find((e) => e.chave === chave) ?? null;
  const pedir = (chave: string, src: string): ElementoPool => {
    relogio++;
    let e = achar(chave);
    if (!e) {
      const livres = lista.filter((x) => !x.preso);
      // o mesmo arquivo livre num elemento: sem trocar o src
      e = livres.find((x) => x.src === src) ?? livres.sort((a, b) => a.usado - b.usado)[0] ?? lista.slice().sort((a, b) => a.usado - b.usado)[0]!;
      e.chave = chave;
    }
    if (e.src !== src) {
      e.el.pause();
      e.src = src;
      e.el.src = src;
    }
    e.preso = true;
    e.usado = relogio;
    return e;
  };
  return {
    elementos: lista,
    achar,
    pedir,
    preparar: async (chave, src, t_el) => {
      const e = pedir(chave, src);
      const el = e.el;
      if (!el.paused) el.pause();
      await aposDados(el);
      if (e.chave !== chave) return;
      if (Math.abs(tempoDoElemento(el) - t_el) > 1e-4 || el.readyState < 2) {
        porNoTempo(el, Math.max(0, t_el));
        await aposBusca(el);
      }
    },
    soltar: (chave) => {
      const e = achar(chave);
      if (!e) return;
      e.preso = false;
      if (!e.el.paused) e.el.pause();
      // parado, volume 1: o servidor de som guarda o último valor do aplicativo (`audio.ts`)
      e.el.volume = 1;
    },
    destruir: () => {
      for (const e of lista) {
        e.el.pause();
        e.el.removeAttribute("src");
        try {
          e.el.load();
        } catch {
          // sem mídia: nada a descarregar
        }
        e.el.remove();
      }
      lista.length = 0;
    },
  };
}
