/**
 * `pool8` (critica-editor M3; `ce/wk/page/pool8.js`): tela dividida de N câmeras = N elementos tocando e N sendo
 * preparados para o clipe seguinte. Mede, no mesmo laço de rAF: cadência com os N tocando (grade no canvas
 * 1280×720), custo de preparar os outros N (busca pausada) e a troca na fronteira.
 */
import { type Ambiente, comPrazo, esperar, leitorDeCodigo, PRAZO_PLAY_MS, r1 } from "./base";

export interface OpcoesPool8 {
  /** "" = 1080p GOP 2 s; "_p720" = proxy 720p GOP 0,5 s. */
  suf: string;
  n?: number;
  ciclos?: number;
  /** ms entre preparar e trocar. */
  dur?: number;
  /** ms da linha de base antes do 1º ciclo. */
  base_ms?: number;
}

type Est = { n: number; p50: number; p95: number; max: number } | null;
export interface FasePool8 { intervalo_ms: Est; custo_js_ms: Est; fps: number | null; acima_50ms: number; acima_100ms: number }
export interface CicloPool8 {
  alvo: number;
  seeked: ("ok" | "timeout")[];
  preparo_ms: number;
  quadro_entrada_esperado: number;
  quadro_entrada_lido: number[];
  play_ms: number;
  todos_andando_ms: number | null;
  espalhamento_ms_1s_depois: number;
}
export interface ResPool8 {
  suf: string;
  n: number;
  abrir_8_ms?: number;
  ciclos: CicloPool8[];
  quadros?: ([number, number] | null)[];
  base?: FasePool8;
  preparo?: FasePool8;
  troca?: FasePool8;
}

function est(a: number[]): Est {
  if (!a.length) return null;
  const s = [...a].sort((x, y) => x - y);
  const q = (p: number) => s[Math.min(s.length - 1, Math.floor(p * s.length))] ?? 0;
  return { n: s.length, p50: r1(q(0.5)), p95: r1(q(0.95)), max: r1(s[s.length - 1] ?? 0) };
}

export async function pool8(amb: Ambiente, o: OpcoesPool8): Promise<ResPool8> {
  const SUF = o.suf;
  const N = o.n ?? 4;
  const CICLOS = o.ciclos ?? 4;
  const DUR = o.dur ?? 2500;
  const ler = leitorDeCodigo(amb);
  const mk = async (f: string) => {
    const v = amb.video();
    v.muted = true;
    v.preload = "auto";
    v.playsInline = true;
    v.src = amb.url(f);
    if (v.readyState < 3) await esperar(amb, v, "canplay", 15000);
    return v;
  };
  const res: ResPool8 = { suf: SUF, n: N, ciclos: [] };
  const nomes = [1, 2, 3, 4].slice(0, N).map((i) => "cam" + i + SUF + ".mp4");
  const A: HTMLVideoElement[] = [];
  const B: HTMLVideoElement[] = [];
  let parar = false;
  try {
    const t0 = amb.agora();
    for (const f of nomes) A.push(await mk(f));
    for (const f of nomes) B.push(await mk(f));
    res.abrir_8_ms = Math.round(amb.agora() - t0);
    const W = 1280;
    const H = 720;
    const cv = amb.palco;
    cv.width = W;
    cv.height = H;
    const ctx = cv.getContext("2d", { alpha: false });
    if (!ctx) throw new Error("sem canvas 2d");
    const cel: [number, number, number, number][] =
      N <= 2
        ? [[0, 0, W / 2, H], [W / 2, 0, W / 2, H]]
        : [[0, 0, W / 2, H / 2], [W / 2, 0, W / 2, H / 2], [0, H / 2, W / 2, H / 2], [W / 2, H / 2, W / 2, H / 2]];
    let ativo = A;
    let reserva = B;
    const pinta = () => {
      ctx.fillStyle = "#000";
      ctx.fillRect(0, 0, W, H);
      ativo.forEach((v, i) => {
        const c = cel[i];
        if (c) ctx.drawImage(v, 0, 0, v.videoWidth, v.videoHeight, c[0], c[1], c[2], c[3]);
      });
    };
    const seek = (v: HTMLVideoElement, t: number) => {
      const p = esperar(amb, v, "seeked", 20000);
      v.currentTime = t;
      return p;
    };
    await Promise.all(ativo.map((v) => seek(v, 1.0 + 1 / 60)));
    await comPrazo(amb, Promise.all(ativo.map((v) => v.play())), PRAZO_PLAY_MS, "play");
    // laço único
    type Fase = "base" | "preparo" | "troca";
    const iv: Record<Fase, number[]> = { base: [], preparo: [], troca: [] };
    const custo: Record<Fase, number[]> = { base: [], preparo: [], troca: [] };
    let fase: Fase = "base";
    let ult = amb.agora();
    let erroLaco: unknown = null;
    const laco = (async () => {
      try {
        while (!parar) {
          await amb.quadro();
          const a = amb.agora();
          pinta();
          const b = amb.agora();
          iv[fase].push(a - ult);
          custo[fase].push(b - a);
          ult = a;
        }
      } catch (e) {
        erroLaco = e;
      }
    })();
    await amb.dormir(o.base_ms ?? 4000); // linha de base: N tocando, N parados
    let alvo = 6;
    for (let c = 0; c < CICLOS; c++) {
      if (erroLaco) throw erroLaco;
      alvo += 3;
      const tAlvo = alvo + 1 / 60;
      const qAlvo = Math.floor(tAlvo * 30);
      fase = "preparo";
      const p0 = amb.agora();
      const st = await Promise.all(reserva.map((v) => seek(v, tAlvo))); // prepara os outros N, pausados, com os atuais tocando
      const preparo_ms = Math.round(amb.agora() - p0);
      const qEntrada = reserva.map(ler);
      fase = "base";
      await amb.dormir(Math.max(0, DUR - preparo_ms)); // espera a "fronteira"
      fase = "troca";
      const s0 = amb.agora();
      const ct0 = reserva.map((v) => v.currentTime);
      await comPrazo(amb, Promise.all(reserva.map((v) => v.play())), PRAZO_PLAY_MS, "play");
      const play_ms = Math.round(amb.agora() - s0);
      ativo.forEach((v) => v.pause());
      const velho = ativo;
      ativo = reserva;
      reserva = velho;
      let andou_ms: number | null = null;
      for (let k = 0; k < 120; k++) {
        await amb.quadro();
        if (ativo.every((v, i) => v.currentTime > (ct0[i] ?? 0) + 0.001)) {
          andou_ms = Math.round(amb.agora() - s0);
          break;
        }
      }
      await amb.dormir(1000);
      const ct = ativo.map((v) => v.currentTime);
      res.ciclos.push({
        alvo: tAlvo,
        seeked: st,
        preparo_ms,
        quadro_entrada_esperado: qAlvo,
        quadro_entrada_lido: qEntrada,
        play_ms,
        todos_andando_ms: andou_ms,
        espalhamento_ms_1s_depois: Math.round((Math.max(...ct) - Math.min(...ct)) * 1000),
      });
      fase = "base";
    }
    await amb.dormir(500);
    parar = true;
    await laco;
    if (erroLaco) throw erroLaco;
    res.quadros = [...A, ...B].map((v) => {
      const q = v.getVideoPlaybackQuality ? v.getVideoPlaybackQuality() : null;
      return q ? [q.totalVideoFrames, q.droppedVideoFrames] : null;
    });
    for (const k of ["base", "preparo", "troca"] as const) {
      const ivk = iv[k].slice(1);
      res[k] = {
        intervalo_ms: est(ivk),
        custo_js_ms: est(custo[k]),
        fps: iv[k].length > 1 ? r1(1000 / (ivk.reduce((x, y) => x + y, 0) / (iv[k].length - 1))) : null,
        acima_50ms: iv[k].filter((x) => x > 50).length,
        acima_100ms: iv[k].filter((x) => x > 100).length,
      };
    }
    return res;
  } finally {
    parar = true;
    for (const v of [...A, ...B]) {
      v.pause();
      v.remove();
    }
  }
}
