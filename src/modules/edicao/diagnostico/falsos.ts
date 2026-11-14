/**
 * Falsos para o vitest (sem DOM): relógio virtual com agenda, `<video>` que toca e busca pelo relógio e canvas 2D
 * que devolve no `getImageData` os pixels da mídia desenhada (código binário do quadro ou os dois níveis de cinza
 * das fontes r*). Só os testes importam este arquivo.
 */
import type { Ambiente } from "./base";
import { Cancelado } from "./base";

export class Agenda {
  t = 0;
  private fila: { t: number; seq: number; fn: () => void }[] = [];
  private seq = 0;
  private chamadas = 0;
  cancelado = false;

  /** `passo` = ms que o relógio anda a cada leitura: laço síncrono que mede o tempo termina. */
  constructor(private passo = 1e-3) {}

  agora(): number {
    this.chamadas++;
    return this.t + this.chamadas * this.passo;
  }

  dormir(ms: number): Promise<void> {
    return new Promise((res, rej) => {
      if (this.cancelado) return rej(new Cancelado());
      this.fila.push({ t: this.t + Math.max(0, ms), seq: this.seq++, fn: () => (this.cancelado ? rej(new Cancelado()) : res()) });
    });
  }

  /** Roda a promessa avançando o relógio de timer em timer. */
  async rodar<T>(p: Promise<T>, limite = 5_000_000): Promise<T> {
    let fim = false;
    let val: T | undefined;
    let erro: unknown = null;
    let falhou = false;
    p.then(
      (v) => {
        fim = true;
        val = v;
      },
      (e) => {
        fim = true;
        falhou = true;
        erro = e;
      },
    );
    for (let i = 0; ; i++) {
      await new Promise((r) => setImmediate(r));
      if (fim) break;
      if (i > limite) throw new Error("agenda: limite");
      this.fila.sort((a, b) => a.t - b.t || a.seq - b.seq);
      const prox = this.fila.shift();
      if (!prox) throw new Error("agenda: travou sem timer");
      this.t = Math.max(this.t, prox.t);
      prox.fn();
    }
    if (falhou) throw erro;
    return val as T;
  }
}

/** Mídia falsa. `inicio` = tempo do elemento do quadro 0 (o que o motor "faz" com o início do arquivo). */
export interface FonteFalsa {
  w: number;
  h: number;
  quadros: number;
  inicio: number;
  /** RGB do pixel (x, y) da imagem do quadro n. */
  pixel(n: number, x: number, y: number): [number, number, number];
  /** "play": o `play()` nunca resolve; "parado": toca sem o tempo andar; "carga": nunca chega metadado. */
  trava?: "play" | "parado" | "carga";
}

const NIVEL_RGB = (y: number) => Math.round(((y - 16) * 255) / 219);

/** Fonte r*: metade esquerda 20+13·⌊N/16⌋, direita 20+13·(N mod 16). */
export function fonteR(inicio: number): FonteFalsa {
  return {
    w: 640,
    h: 360,
    quadros: 180,
    inicio,
    pixel: (n, x) => {
      const y = x < 320 ? 20 + 13 * Math.floor(n / 16) : 20 + 13 * (n % 16);
      const c = NIVEL_RGB(y);
      return [c, c, c];
    },
  };
}

/** Fonte com o código binário do quadro (12 blocos de lado w/48) e cinza no resto. */
export function fonteCodigo(w: number, h: number, quadros = 900): FonteFalsa {
  const b = w / 48;
  return {
    w,
    h,
    quadros,
    inicio: 0,
    pixel: (n, x, y) => {
      if (y < b && x < 12 * b) return (n >> Math.floor(x / b)) & 1 ? [235, 235, 235] : [16, 16, 16];
      return [128, 128, 128];
    },
  };
}

type Ouvinte = () => void;

export class VideoFalso {
  muted = false;
  preload = "";
  playsInline = false;
  loop = false;
  crossOrigin: string | null = null;
  playbackRate = 1;
  readyState = 0;
  seeking = false;
  paused = true;
  error: { code: number; message: string } | null = null;
  style: Record<string, string> = {};
  fonte: FonteFalsa | null = null;
  private _src = "";
  private ct = 0;
  private t0 = 0;
  private ouvintes = new Map<string, Set<Ouvinte>>();
  totalVideoFrames = 0;

  constructor(
    private ag: Agenda,
    private fontes: Record<string, FonteFalsa>,
    private aoRemover: (v: VideoFalso) => void,
  ) {}

  addEventListener(ev: string, fn: Ouvinte, _o?: unknown) {
    let s = this.ouvintes.get(ev);
    if (!s) this.ouvintes.set(ev, (s = new Set()));
    s.add(fn);
  }
  removeEventListener(ev: string, fn: Ouvinte) {
    this.ouvintes.get(ev)?.delete(fn);
  }
  private emite(ev: string) {
    for (const f of [...(this.ouvintes.get(ev) ?? [])]) f();
  }

  get src() {
    return this._src;
  }
  set src(u: string) {
    this._src = u;
    const f = this.fontes[u];
    void this.ag.dormir(20).then(() => {
      if (!f) {
        this.error = { code: 4, message: "sem fonte" };
        this.emite("error");
        return;
      }
      if (f.trava === "carga") return;
      this.fonte = f;
      this.ct = 0;
      this.readyState = 4;
      for (const e of ["loadedmetadata", "loadeddata", "canplay"]) this.emite(e);
    }, () => {});
  }
  get videoWidth() {
    return this.fonte?.w ?? 0;
  }
  get videoHeight() {
    return this.fonte?.h ?? 0;
  }
  get duration() {
    return this.fonte ? this.fonte.inicio + this.fonte.quadros / 30 : NaN;
  }
  get ended() {
    return this.currentTime >= this.duration;
  }
  get seekable() {
    const f = this.fonte;
    return { length: f ? 1 : 0, start: () => 0, end: () => this.duration };
  }
  get buffered() {
    return this.seekable;
  }
  get currentTime() {
    if (this.paused || this.fonte?.trava === "parado") return this.ct;
    const t = this.ct + ((this.ag.t - this.t0) / 1000) * this.playbackRate;
    return Math.min(t, this.duration);
  }
  set currentTime(t: number) {
    const tocando = !this.paused;
    this.ct = t;
    this.t0 = this.ag.t;
    this.seeking = true;
    void this.ag.dormir(10).then(() => {
      this.seeking = false;
      if (tocando) this.t0 = this.ag.t;
      this.emite("seeked");
    }, () => {});
  }
  play(): Promise<void> {
    if (this.fonte?.trava === "play") return new Promise(() => {});
    return this.ag.dormir(15).then(() => {
      if (this.paused) {
        this.t0 = this.ag.t;
        this.paused = false;
      }
    });
  }
  pause() {
    if (!this.paused) this.ct = this.currentTime;
    this.paused = true;
  }
  /** Quadro exibido no tempo atual do elemento. */
  quadroAtual(): number {
    const f = this.fonte;
    if (!f) return -1;
    const n = Math.floor((this.currentTime - f.inicio) * 30 + 1e-9);
    return Math.max(0, Math.min(f.quadros - 1, n));
  }
  getVideoPlaybackQuality() {
    return { totalVideoFrames: this.totalVideoFrames, droppedVideoFrames: 0 };
  }
  remove() {
    this.aoRemover(this);
  }
}

type Op =
  | { k: "cor"; rgb: [number, number, number]; x: number; y: number; w: number; h: number }
  | { k: "img"; amostra: (x: number, y: number) => [number, number, number]; sx: number; sy: number; sw: number; sh: number; x: number; y: number; w: number; h: number };

function cor(s: unknown): [number, number, number] {
  const t = String(s);
  if (/^#[0-9a-f]{3}$/i.test(t)) return [1, 2, 3].map((i) => parseInt(t[i]! + t[i]!, 16)) as [number, number, number];
  if (/^#[0-9a-f]{6}$/i.test(t)) return [1, 3, 5].map((i) => parseInt(t.slice(i, i + 2), 16)) as [number, number, number];
  return [0, 0, 0];
}

export class CanvasFalso {
  ops: Op[] = [];
  private ctx: Record<string, unknown> | null = null;
  constructor(
    public width: number,
    public height: number,
  ) {}

  /** Pixel do canvas: a última operação que cobre o ponto. */
  pixel(x: number, y: number): [number, number, number] {
    for (let i = this.ops.length - 1; i >= 0; i--) {
      const o = this.ops[i]!;
      if (x < o.x || y < o.y || x >= o.x + o.w || y >= o.y + o.h) continue;
      if (o.k === "cor") return o.rgb;
      return o.amostra(o.sx + ((x + 0.5 - o.x) * o.sw) / o.w, o.sy + ((y + 0.5 - o.y) * o.sh) / o.h);
    }
    return [0, 0, 0];
  }

  getContext(_t: string, _o?: unknown): CanvasRenderingContext2D {
    if (!this.ctx) {
      const eu = this;
      const base: Record<string, unknown> = {
        canvas: this,
        fillStyle: "#000",
        fillRect(x: number, y: number, w: number, h: number) {
          eu.ops.push({ k: "cor", rgb: cor(base.fillStyle), x, y, w, h });
          if (eu.ops.length > 64) eu.ops.splice(0, eu.ops.length - 64);
        },
        drawImage(src: VideoFalso | CanvasFalso, ...a: number[]) {
          const sw0 = src instanceof VideoFalso ? src.videoWidth : src.width;
          const sh0 = src instanceof VideoFalso ? src.videoHeight : src.height;
          let [sx, sy, sw, sh, x, y, w, h] = [0, 0, sw0, sh0, 0, 0, sw0, sh0];
          if (a.length === 2) [x, y] = a as [number, number];
          else if (a.length === 4) [x, y, w, h] = a as [number, number, number, number];
          else [sx, sy, sw, sh, x, y, w, h] = a as [number, number, number, number, number, number, number, number];
          let amostra: (x: number, y: number) => [number, number, number];
          if (src instanceof VideoFalso) {
            const f = src.fonte;
            if (!f || src.readyState < 2) return;
            const n = src.quadroAtual();
            amostra = (px, py) => f.pixel(n, px, py);
          } else {
            const ops = [...src.ops];
            const c = new CanvasFalso(src.width, src.height);
            c.ops = ops;
            amostra = (px, py) => c.pixel(px, py);
          }
          eu.ops.push({ k: "img", amostra, sx, sy, sw, sh, x, y, w, h });
          if (eu.ops.length > 64) eu.ops.splice(0, eu.ops.length - 64);
        },
        getImageData(x: number, y: number, w: number, h: number) {
          const data = new Uint8ClampedArray(w * h * 4);
          for (let j = 0; j < h; j++)
            for (let i = 0; i < w; i++) {
              const [r, g, b] = eu.pixel(x + i, y + j);
              data.set([r, g, b, 255], (j * w + i) * 4);
            }
          return { data, width: w, height: h };
        },
      };
      this.ctx = new Proxy(base, {
        get: (t, k) => (k in t ? t[k as string] : () => {}),
        set: (t, k, v) => {
          t[k as string] = v;
          return true;
        },
      });
    }
    return this.ctx as unknown as CanvasRenderingContext2D;
  }
}

/** Ambiente falso: `url(nome)` = nome; `fontes` por nome. */
export function ambienteFalso(fontes: Record<string, FonteFalsa>, ag = new Agenda()) {
  const vivos = new Set<VideoFalso>();
  const amb: Ambiente & { vivos: Set<VideoFalso>; ag: Agenda } = {
    ag,
    vivos,
    agora: () => ag.agora(),
    quadro: () => ag.dormir(1000 / 60).then(() => ag.t),
    dormir: (ms) => ag.dormir(ms),
    video: () => {
      const v = new VideoFalso(ag, fontes, (x) => vivos.delete(x));
      vivos.add(v);
      return v as unknown as HTMLVideoElement;
    },
    canvas: (w, h) => new CanvasFalso(w, h) as unknown as HTMLCanvasElement,
    palco: new CanvasFalso(1280, 720) as unknown as HTMLCanvasElement,
    url: (nome) => nome,
  };
  return amb;
}
