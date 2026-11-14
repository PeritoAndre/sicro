/**
 * `tempo3` (critica-editor M1, M2; `ce/wk/page/tempo3.js`): base de tempo do `<video>` em cada contêiner e busca
 * exata. Fontes r*: metade esquerda = 20+13·⌊N/16⌋, direita = 20+13·(N mod 16); o quadro é identificado pelos
 * dois níveis, calibrados no `r0.mp4`. Com `p0s` (pts do 1º quadro, do ffprobe) o alvo é o meio do quadro n e
 * `erros_previsto` conta os quadros lidos ≠ previstos.
 */
import { type Ambiente, comPrazo, esperar, PRAZO_PLAY_MS, r1, r4, textoDeErro } from "./base";

/** [alvo, currentTime depois do seeked (ou "timeout@ct"), quadro lido, videoWidth]. */
export type Busca = [number, number | string, number | string | null, number];

export interface ArquivoTempo3 {
  arquivo: string;
  meta?: "ok" | "timeout";
  erro?: number | null;
  duration?: number;
  wh?: [number, number];
  readyState?: number;
  ct0?: number;
  seekable?: [number, number] | null;
  buffered?: [number, number] | null;
  q0?: number | string | null;
  previsto?: number[];
  buscas?: Busca[];
  erros_previsto?: number;
  fim?: [number, number | string, number | string | null, boolean][];
  tocar?: { de: number; q_de: number | string | null; ate: number; q_ate: number | string | null; parede_ms: number };
  play_erro?: string;
  /** Buscas não feitas: o elemento deu erro ou ficou sem metadados ao abrir. */
  pulado?: "erro" | "sem_dados";
  excecao?: string;
}

export interface ResTempo3 {
  calib_meta?: "ok" | "timeout";
  calib: { fino: number[]; grosso: number[] };
  arquivos: ArquivoTempo3[];
}

export const FONTES_TEMPO3 = [
  "r0.mp4", "r0.mkv", "r3.mp4", "r3.mov", "r3.mkv", "r3v.mkv", "r3d.mkv", "r007.mp4", "r007.mkv",
  "r12h.mkv", "r12h.mp4", "r0.ts", "rb.mp4", "rb.mkv",
];
/** Cópias de prévia (§7.1) e os MP4 originais, no modo "previsto". */
export const COPIAS_TEMPO3 = [
  "cp_r3_mkv.mp4", "cp_r3d_mkv.mp4", "cp_r007_mkv.mp4", "cp_r0_ts.mp4", "cp_r3_mp4.mp4", "cp_r12h_mkv.mp4",
  "r0.mp4", "r3.mp4", "r007.mp4",
];

const perto = (tab: number[], x: number): [number, number] => {
  let b = -1;
  let bd = 1e9;
  for (let i = 0; i < tab.length; i++) {
    const d = Math.abs((tab[i] ?? 0) - x);
    if (d < bd) {
      bd = d;
      b = i;
    }
  }
  return [b, bd];
};

export async function tempo3(amb: Ambiente, arquivos: string[], p0s?: (number | undefined)[]): Promise<ResTempo3> {
  const cv = amb.canvas(64, 36);
  const cx = cv.getContext("2d", { willReadFrequently: true });
  if (!cx) throw new Error("sem canvas 2d");
  const media = (x: number): number | null => {
    const d = cx.getImageData(x, 14, 8, 8).data;
    let s = 0;
    let g = 0;
    for (let i = 0; i < d.length; i += 4) {
      s += d[i] ?? 0;
      g += d[i + 1] ?? 0;
    }
    const n = d.length / 4;
    return Math.abs(s - g) / n > 40 ? null : s / n;
  };
  // [cinza esquerdo, cinza direito] ou null se o vídeo não desenhou (fundo magenta)
  const patches = (v: HTMLVideoElement): [number, number] | null => {
    cx.fillStyle = "#f0f";
    cx.fillRect(0, 0, 64, 36);
    try {
      cx.drawImage(v, 0, 0, 64, 36);
    } catch {
      return null;
    }
    const a = media(12);
    const b = media(44);
    return a == null || b == null ? null : [a, b];
  };
  const abrir = async (f: string) => {
    const v = amb.video();
    v.muted = true;
    v.preload = "auto";
    v.playsInline = true;
    v.src = amb.url(f);
    const st = await esperar(amb, v, "loadeddata", 8000);
    await amb.quadro();
    await amb.quadro();
    return { v, st };
  };
  const ir = async (v: HTMLVideoElement, t: number, espera = 40) => {
    const p = esperar(amb, v, "seeked", 4000);
    v.currentTime = t;
    const st = await p;
    await amb.quadro();
    await amb.quadro();
    await amb.dormir(espera);
    return st;
  };
  const fechar = (v: HTMLVideoElement) => {
    v.pause();
    v.remove();
  };

  const res: ResTempo3 = { calib: { fino: [], grosso: [] }, arquivos: [] };
  // 1. calibração em r0.mp4: 16 níveis finos (quadros 0..15) e 12 grossos (quadros 0,16,...,176)
  const fino: number[] = [];
  const grosso: number[] = [];
  {
    const { v, st } = await abrir("r0.mp4");
    res.calib_meta = st;
    try {
      for (let n = 0; n < 16; n++) {
        await ir(v, (n + 0.5) / 30, 30);
        const p = patches(v);
        fino.push(p ? p[1] : -999);
      }
      for (let k = 0; k < 12; k++) {
        await ir(v, (16 * k + 0.5) / 30, 30);
        const p = patches(v);
        grosso.push(p ? p[0] : -999);
      }
    } finally {
      fechar(v);
    }
  }
  res.calib = { fino: fino.map(r1), grosso: grosso.map(r1) };
  const quadro = (p: [number, number] | null): number | string | null => {
    if (!p) return null;
    const [g, dg] = perto(grosso, p[0]);
    const [f, df] = perto(fino, p[1]);
    return dg > 5 || df > 5 ? "?" + Math.round(p[0]) + "/" + Math.round(p[1]) : g * 16 + f;
  };

  // 2. arquivos
  for (const [i, f] of arquivos.entries()) {
    const o: ArquivoTempo3 = { arquivo: f };
    res.arquivos.push(o);
    let v: HTMLVideoElement | null = null;
    try {
      const ab = await abrir(f);
      v = ab.v;
      o.meta = ab.st;
      o.erro = v.error ? v.error.code : null;
      o.duration = v.duration;
      o.wh = [v.videoWidth, v.videoHeight];
      o.readyState = v.readyState;
      o.ct0 = v.currentTime;
      o.seekable = v.seekable.length ? [v.seekable.start(0), v.seekable.end(v.seekable.length - 1)] : null;
      o.buffered = v.buffered.length ? [v.buffered.start(0), v.buffered.end(v.buffered.length - 1)] : null;
      o.q0 = quadro(patches(v));
      // mídia recusada (TS no Chromium) ou sem metadados (MP4 em 12 h no WebKitGTK): sem `seeked`, cada busca
      // só esperaria os 4 s
      if (v.error || (ab.st === "timeout" && v.readyState === 0)) {
        o.pulado = v.error ? "erro" : "sem_dados";
        continue;
      }
      // modo "previsto": alvo = meio do quadro n; confere n lido = n previsto
      const p0 = p0s?.[i];
      const ns = [0, 1, 15, 30, 31, 60, 90, 120, 150, 178, 179];
      const alvos =
        p0 != null
          ? ns.map((n) => p0 + (n + 0.5) / 30)
          : f.startsWith("r12h")
            ? [0.0167, 1.0167, 4.0167, 43199, 43200.0167, 43201.0167, 43204.0167, 43205.9833]
            : [0.0167, 0.5167, 1.0167, 2.0167, 2.9, 3.0167, 3.5167, 4.0167, 5.0167, 5.9833, 7.0167, 8.5167, 8.9833];
      if (p0 != null) o.previsto = ns;
      o.buscas = [];
      for (const t of alvos) {
        const s = await ir(v, t, 40);
        o.buscas.push([t, s === "ok" ? r4(v.currentTime) : s + "@" + r4(v.currentTime), quadro(patches(v)), v.videoWidth]);
      }
      if (o.previsto) {
        const prev = o.previsto;
        o.erros_previsto = o.buscas.filter((b, k) => b[2] !== prev[k]).length;
      }
      // fim do arquivo: meio quadro antes da duração, a duração e depois dela
      o.fim = [];
      for (const t of [v.duration - 1 / 60, v.duration, v.duration + 1]) {
        if (!Number.isFinite(t)) continue;
        const s = await ir(v, t, 60);
        o.fim.push([r4(t), s === "ok" ? r4(v.currentTime) : s, quadro(patches(v)), v.ended]);
      }
      // tocar ~1 s a partir de um ponto de conteúdo conhecido
      const meio = f.startsWith("r12h")
        ? null
        : (o.buscas.find((b) => typeof b[2] === "number" && b[2] >= 20 && b[2] <= 100) ?? [null])[0];
      if (meio != null) {
        await ir(v, meio, 60);
        const c0 = v.currentTime;
        const q0 = quadro(patches(v));
        const w0 = amb.agora();
        try {
          await comPrazo(amb, v.play(), PRAZO_PLAY_MS, "play");
        } catch (e) {
          if ((e as Error)?.name === "Cancelado") throw e;
          o.play_erro = String(e);
        }
        await amb.dormir(1000);
        v.pause();
        await amb.quadro();
        await amb.quadro();
        o.tocar = { de: r4(c0), q_de: q0, ate: r4(v.currentTime), q_ate: quadro(patches(v)), parede_ms: Math.round(amb.agora() - w0) };
      }
    } catch (e) {
      if ((e as Error)?.name === "Cancelado") throw e;
      o.excecao = textoDeErro(e);
    } finally {
      if (v) fechar(v);
    }
  }
  return res;
}

/** Quadro previsto em cada busca pelo tempo absoluto da fonte (alvo − p0); null = fora do arquivo. */
export function previstoAbsoluto(t: number, p0: number, quadros = 180): number | null {
  const n = Math.floor((t - p0) * 30 + 1e-6);
  return n >= 0 && n < quadros ? n : null;
}

/**
 * Tempo do elemento = tempo de fonte? "absoluto" (alvo − p0), "rebaseado" (conteúdo começando em 0), "outro"
 * ou null (sem leitura). Cada hipótese só olha as buscas que caem dentro do arquivo por ela.
 */
export function baseDeTempo(o: ArquivoTempo3, p0: number): "absoluto" | "rebaseado" | "outro" | null {
  const lidas = (o.buscas ?? []).filter((b) => typeof b[2] === "number");
  if (!lidas.length) return null;
  // alvo no limite exato entre dois quadros (2,9 s) não decide nada
  const meioDoQuadro = (t: number, base: number) => {
    const fr = (t - base) * 30 - Math.floor((t - base) * 30);
    return fr > 0.2 && fr < 0.8;
  };
  const confere = (base: number) => {
    const dentro = lidas.filter((b) => previstoAbsoluto(b[0], base) != null && meioDoQuadro(b[0], base));
    return dentro.length >= 3 && dentro.every((b) => b[2] === previstoAbsoluto(b[0], base));
  };
  if (confere(p0)) return "absoluto";
  if (p0 !== 0 && confere(0)) return "rebaseado";
  return "outro";
}
