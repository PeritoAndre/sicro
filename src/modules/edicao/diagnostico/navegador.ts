/** `Ambiente` de verdade: rAF, `performance.now`, `<video>` no palco e cancelamento por `AbortSignal`. */
import { mediaSrc } from "@core/mediaSrc";
import { type Ambiente, Cancelado } from "./base";
import type { MidiasDiagnostico } from "./comandos";
import type { Navegador } from "./executar";

export interface AmbienteDoNavegador extends Ambiente {
  /** Pausa, solta e remove todo `<video>` criado (libera os decodificadores). */
  limpar(): void;
}

export function ambienteDoNavegador(o: {
  midias: MidiasDiagnostico;
  palco: HTMLCanvasElement;
  /** Onde os `<video>` ficam (dentro do palco, visível). */
  caixa: HTMLElement;
  sinal: AbortSignal;
}): AmbienteDoNavegador {
  const criados = new Set<HTMLVideoElement>();
  const checa = () => {
    if (o.sinal.aborted) throw new Cancelado();
  };
  const cors = !mediaSrc(o.midias.pasta).startsWith("file:");
  const caminho = (nome: string) =>
    o.midias.midias.find((m) => m.nome === nome)?.caminho ?? `${o.midias.pasta}/${nome}`;
  const escondida = () => document.visibilityState !== "visible";
  let ocultou = escondida();
  const aoMudar = () => {
    if (escondida()) ocultou = true;
  };
  document.addEventListener("visibilitychange", aoMudar);
  return {
    palco: o.palco,
    agora: () => performance.now(),
    quadro: () =>
      new Promise<number>((res, rej) => {
        requestAnimationFrame((t) => (o.sinal.aborted ? rej(new Cancelado()) : res(t)));
      }),
    dormir: (ms) =>
      new Promise<void>((res, rej) => {
        if (o.sinal.aborted) return rej(new Cancelado());
        const fim = () => {
          clearTimeout(t);
          rej(new Cancelado());
        };
        const t = setTimeout(() => {
          o.sinal.removeEventListener("abort", fim);
          res();
        }, ms);
        o.sinal.addEventListener("abort", fim, { once: true });
      }),
    video: () => {
      checa();
      const v = document.createElement("video");
      Object.assign(v.style, { position: "absolute", left: "0", top: "0", width: "2px", height: "2px", opacity: "0.01", pointerEvents: "none" });
      const remover = v.remove.bind(v);
      v.remove = () => {
        criados.delete(v);
        soltar(v);
        remover();
      };
      // fora do Linux a mídia vem pelo asset protocol (outra origem): sem CORS o canvas fica manchado
      if (cors) v.crossOrigin = "anonymous";
      o.caixa.appendChild(v);
      criados.add(v);
      return v;
    },
    canvas: (w, h) => {
      const c = document.createElement("canvas");
      c.width = w;
      c.height = h;
      return c;
    },
    url: (nome) => mediaSrc(caminho(nome)),
    oculta: () => {
      const r = ocultou || escondida();
      ocultou = escondida();
      return r;
    },
    limpar: () => {
      document.removeEventListener("visibilitychange", aoMudar);
      for (const v of [...criados]) v.remove();
    },
  };
}

function soltar(v: HTMLVideoElement) {
  try {
    v.pause();
    v.removeAttribute("src");
    v.load();
  } catch {
    /* já solto */
  }
}

export function navegadorAtual(): Navegador {
  const n = navigator as Navigator & { deviceMemory?: number };
  return {
    ua: n.userAgent,
    plataforma: n.platform,
    nucleos: n.hardwareConcurrency ?? null,
    memoria_gb: n.deviceMemory ?? null,
    tela: typeof screen !== "undefined" ? [screen.width, screen.height] : null,
    dpr: typeof devicePixelRatio === "number" ? devicePixelRatio : null,
    idioma: n.language ?? null,
  };
}
