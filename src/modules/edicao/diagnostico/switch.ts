/**
 * `switch` (previa-motor §3.6; `p5/page/bench.js`, `t=switch`): troca de clipe na fronteira. A toca até o ponto de
 * saída; B, já posicionado e pausado no ponto de entrada, assume. Com `velb` o clipe seguinte entra nessa
 * velocidade, definida com ele ainda pausado.
 */
import { type Ambiente, abrirVideo, buscar, comPrazo, leitorDeCodigo, PRAZO_PLAY_MS, stats } from "./base";

/** Teto do laço até a fronteira (2 s de clipe; 4 s a 0,5×): elemento parado vira falha da etapa. */
export const PRAZO_FRONTEIRA_MS = 15000;

export interface OpcoesSwitch {
  fa?: string;
  fb?: string;
  /** ms de antecedência do `play()` de B (0 = B pausado no ponto exato). */
  pre?: number;
  reps?: number;
  velb?: number;
}

export interface TrocaSwitch {
  i: number;
  pre_disparo_ct?: number;
  passou_do_ponto_ms?: number;
  nxt_ct_na_troca_menos_entrada_ms?: number;
  quadro_entrada_lido?: number;
  quadro_entrada_esperado?: number;
  play_ms?: number;
  tempo_andou_ms?: number;
  primeiro_quadro_novo_ms?: number | null;
  tempo_ja_andava_ms?: number;
  preparo_do_proximo_ms?: number;
  vel_do_proximo?: number;
  quadro_apos_definir_vel?: number;
  quadro_esperado?: number;
  vel_efetiva_do_atual?: number;
  vel_pedida_do_atual?: number;
}

export interface ResSwitch {
  fa: string;
  fb: string;
  pre_ms: number;
  velb?: number;
  trocas: TrocaSwitch[];
  resumo?: Record<string, ReturnType<typeof stats>>;
}

type Rvfc = HTMLVideoElement & { requestVideoFrameCallback?: (cb: (n: number, meta: unknown) => void) => number };

/** Quadro apresentado (rVFC) com teto de espera; sem rVFC, dois rAF e null (`nextFrame`). */
function proximoQuadro(amb: Ambiente, v: HTMLVideoElement, ms = 3000): Promise<unknown | null> {
  const r = v as Rvfc;
  if (!r.requestVideoFrameCallback) return amb.quadro().then(() => amb.quadro()).then(() => null);
  return Promise.race([
    new Promise<unknown>((res) => r.requestVideoFrameCallback?.((_n, meta) => res(meta))),
    amb.dormir(ms).then(() => null),
  ]);
}

export async function trocaDeClipe(amb: Ambiente, o: OpcoesSwitch = {}): Promise<ResSwitch> {
  const fa = o.fa ?? "cam1.mp4";
  const fb = o.fb ?? "cam2.mp4";
  const pre = o.pre ?? 0;
  const reps = o.reps ?? 6;
  const W = 1280;
  const H = 720;
  const cv = amb.palco;
  cv.width = W;
  cv.height = H;
  const ctx = cv.getContext("2d", { alpha: false });
  if (!ctx) throw new Error("sem canvas 2d");
  const ler = leitorDeCodigo(amb);
  const criados: HTMLVideoElement[] = [];
  try {
    const A = await abrirVideo(amb, fa);
    criados.push(A);
    const B = await abrirVideo(amb, fb);
    criados.push(B);
    const r: ResSwitch = { fa, fb, pre_ms: pre, trocas: [] };
    let cur: HTMLVideoElement = A;
    let nxt: HTMLVideoElement = B;
    let outT = 4.0;
    let inT = 12.0;
    const velB = o.velb ?? 0; // velocidade do clipe seguinte, aplicada com ele ainda pausado
    await buscar(amb, cur, 2.0);
    await buscar(amb, nxt, inT - pre / 1000);
    if (velB) {
      nxt.playbackRate = velB;
      await amb.dormir(300);
      r.velb = velB;
    }
    await comPrazo(amb, cur.play(), PRAZO_PLAY_MS, "play");
    for (let i = 0; i < reps; i++) {
      const rec: TrocaSwitch = { i };
      let started = false;
      let tDecisao = 0;
      // toca até a fronteira
      const inicioLaco = amb.agora();
      for (;;) {
        await amb.quadro();
        if (amb.agora() - inicioLaco > PRAZO_FRONTEIRA_MS) throw new Error("timeout fronteira");
        ctx.drawImage(cur, 0, 0, W, H);
        if (pre > 0 && !started && cur.currentTime >= outT - pre / 1000) {
          started = true;
          nxt.muted = true;
          void nxt.play();
          rec.pre_disparo_ct = cur.currentTime;
        }
        if (cur.currentTime >= outT) {
          tDecisao = amb.agora();
          rec.passou_do_ponto_ms = Math.round((cur.currentTime - outT) * 1000 * 10) / 10;
          break;
        }
      }
      // troca
      const ctN0 = nxt.currentTime;
      rec.nxt_ct_na_troca_menos_entrada_ms = Math.round((ctN0 - inT) * 1000 * 10) / 10;
      const fp = proximoQuadro(amb, nxt, 3000);
      const pp = nxt.paused ? nxt.play() : Promise.resolve();
      cur.pause();
      ctx.drawImage(nxt, 0, 0, W, H); // quadro de entrada já decodificado (estava posicionado)
      rec.quadro_entrada_lido = ler(nxt);
      rec.quadro_entrada_esperado = Math.floor(inT * 30 + 1e-6);
      await comPrazo(amb, pp, PRAZO_PLAY_MS, "play");
      rec.play_ms = Math.round((amb.agora() - tDecisao) * 10) / 10;
      {
        const w = amb.agora();
        while (nxt.currentTime <= ctN0 + 1e-4 && amb.agora() - w < 2000) await amb.quadro();
        rec.tempo_andou_ms = Math.round((amb.agora() - tDecisao) * 10) / 10;
      }
      const m = await Promise.race([fp, amb.dormir(400).then(() => null)]);
      rec.primeiro_quadro_novo_ms = m ? Math.round((amb.agora() - tDecisao) * 10) / 10 : null;
      rec.tempo_ja_andava_ms = Math.round((nxt.currentTime - ctN0) * 1000);
      // prepara o próximo: o que saiu vai para o ponto de entrada seguinte, enquanto o atual toca
      const old = cur;
      cur = nxt;
      nxt = old;
      outT = cur.currentTime + 2.0;
      inT = 3.0 + ((i * 3.7) % 20);
      const ts = amb.agora();
      await buscar(amb, nxt, Math.max(0, inT - pre / 1000));
      rec.preparo_do_proximo_ms = Math.round((amb.agora() - ts) * 10) / 10;
      if (velB) {
        // alterna: o próximo entra em velB, o seguinte em 1x
        const alvo = i % 2 === 0 ? 1 : velB;
        nxt.playbackRate = alvo;
        await amb.dormir(250);
        rec.vel_do_proximo = alvo;
        rec.quadro_apos_definir_vel = ler(nxt);
        rec.quadro_esperado = Math.floor(inT * 30 + 1e-6);
        // velocidade efetiva do clipe que está tocando agora
        const c0 = cur.currentTime;
        const w0 = amb.agora();
        await amb.dormir(1000);
        rec.vel_efetiva_do_atual = Math.round(((cur.currentTime - c0) / ((amb.agora() - w0) / 1000)) * 1000) / 1000;
        rec.vel_pedida_do_atual = cur.playbackRate;
      }
      r.trocas.push(rec);
    }
    cur.pause();
    const col = (f: (x: TrocaSwitch) => number | null | undefined) =>
      stats(r.trocas.map(f).filter((x): x is number => typeof x === "number"));
    r.resumo = {
      passou_do_ponto_ms: col((x) => x.passou_do_ponto_ms),
      play_ms: col((x) => x.play_ms),
      primeiro_quadro_novo_ms: col((x) => x.primeiro_quadro_novo_ms),
      tempo_andou_ms: col((x) => x.tempo_andou_ms),
      preparo_do_proximo_ms: col((x) => x.preparo_do_proximo_ms),
      erro_entrada_ms: col((x) => x.nxt_ct_na_troca_menos_entrada_ms),
    };
    return r;
  } finally {
    for (const v of criados) {
      v.pause();
      v.remove();
    }
  }
}
