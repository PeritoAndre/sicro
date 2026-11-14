/**
 * `compose` (previa-motor §3.2; `p5/page/bench.js`, `t=compose`): custo de compor no canvas 2D com o vídeo tocando
 * (zoom animado, pixelização, desfoque em elipse por reduzir e ampliar, holofote, formas, lupa 3×, textos), sem
 * `ctx.filter`. Variantes "um desenho": pixelização e desfoque tirados do próprio canvas de saída.
 * Os textos desenhados têm o comprimento dos da bancada, sem frase de exemplo.
 */
import { type Ambiente, abrirVideo, buscar, comPrazo, leitorDeCodigo, PRAZO_PLAY_MS, stats } from "./base";

export interface OpcoesCompose {
  f: string;
  cw?: number;
  ch?: number;
  /** ms do laço "completo"; os outros usam a metade. */
  dur?: number;
  /** ms da rajada sem rAF. */
  rajada_ms?: number;
}

interface OpcoesDesenho {
  soDraw?: boolean;
  daSaida?: boolean;
  semLupa?: boolean;
}

export interface Laco {
  dur_ms: number;
  rafs: number;
  fps_raf: number;
  intervalo_ms: ReturnType<typeof stats>;
  custo_js_ms: ReturnType<typeof stats>;
  sync: boolean;
  quadros_distintos_ct: number;
  intervalos_acima_25ms: number;
  intervalos_acima_50ms: number;
  video: { rvfc: number; fps_rvfc: number; avanco_s: number; decodificados: number | null; descartados: number | null }[];
}

export interface ResCompose {
  arquivo: string;
  video: [number, number];
  canvas: [number, number];
  so_drawimage: Laco;
  completo: Laco;
  completo_sync: Laco;
  so_drawimage_sync: Laco;
  um_desenho_com_lupa: Laco;
  um_desenho_sem_lupa: Laco;
  um_desenho_sem_lupa_sync: Laco;
  rajada_por_s: number;
  quadro_lido_fim: number;
}

type Rvfc = HTMLVideoElement & { requestVideoFrameCallback?: (cb: () => void) => number };

export function criarScratch(amb: Ambiente) {
  const scratch: Record<string, HTMLCanvasElement> = {};
  return (name: string, w: number, h: number): HTMLCanvasElement => {
    let c = scratch[name];
    if (!c) c = scratch[name] = amb.canvas(w, h);
    if (c.width !== w || c.height !== h) {
      c.width = w;
      c.height = h;
    }
    return c;
  };
}

const ctx2d = (c: HTMLCanvasElement, opts?: CanvasRenderingContext2DSettings): CanvasRenderingContext2D => {
  const x = c.getContext("2d", opts);
  if (!x) throw new Error("sem canvas 2d");
  return x;
};

/** Desenha o quadro composto. `k` = fase 0..1 para animar zoom e regiões. Sem ctx.filter. */
export function compor(
  sc: ReturnType<typeof criarScratch>,
  ctx: CanvasRenderingContext2D,
  v: HTMLVideoElement,
  W: number,
  H: number,
  k: number,
  opt: OpcoesDesenho = {},
): void {
  const vw = v.videoWidth;
  const vh = v.videoHeight;
  // 1. zoom e pan: recorte que anda de 100% a 45% da imagem
  const z = 1 - 0.55 * (0.5 - 0.5 * Math.cos(k * Math.PI * 2));
  const sw = vw * z;
  const sh = vh * z;
  const sx = (vw - sw) * (0.3 + 0.4 * k);
  const sy = (vh - sh) * 0.5;
  ctx.imageSmoothingEnabled = true;
  ctx.imageSmoothingQuality = "low";
  ctx.drawImage(v, sx, sy, sw, sh, 0, 0, W, H);
  if (opt.soDraw) return;
  const fx = W / sw;
  const fy = H / sh; // fonte -> saída
  // 2. pixelização: reduz a região a blocos e amplia sem suavizar
  {
    const rx = vw * (0.15 + 0.2 * k);
    const ry = vh * 0.25;
    const rw = vw * 0.12;
    const rh = vh * 0.2;
    const bl = 12; // blocos no lado maior
    const pc = sc("pix", bl, Math.max(1, Math.round((bl * rh) / rw)));
    const px = ctx2d(pc);
    px.imageSmoothingEnabled = true;
    if (opt.daSaida) px.drawImage(ctx.canvas, (rx - sx) * fx, (ry - sy) * fy, rw * fx, rh * fy, 0, 0, pc.width, pc.height);
    else px.drawImage(v, rx, ry, rw, rh, 0, 0, pc.width, pc.height);
    ctx.imageSmoothingEnabled = false;
    ctx.drawImage(pc, (rx - sx) * fx, (ry - sy) * fy, rw * fx, rh * fy);
    ctx.imageSmoothingEnabled = true;
  }
  // 3. desfoque sem ctx.filter: reduz 1/4, 1/8 e 1/16 e amplia com suavização, recortado em elipse
  {
    const rx = vw * (0.6 - 0.2 * k);
    const ry = vh * 0.45;
    const rw = vw * 0.16;
    const rh = vh * 0.26;
    const dx = (rx - sx) * fx;
    const dy = (ry - sy) * fy;
    const dw = rw * fx;
    const dh = rh * fy;
    const a = sc("b1", Math.max(2, Math.round(dw / 4)), Math.max(2, Math.round(dh / 4)));
    const b = sc("b2", Math.max(2, Math.round(dw / 8)), Math.max(2, Math.round(dh / 8)));
    const c = sc("b3", Math.max(2, Math.round(dw / 16)), Math.max(2, Math.round(dh / 16)));
    const ax = ctx2d(a);
    const bx = ctx2d(b);
    const cx = ctx2d(c);
    if (opt.daSaida) ax.drawImage(ctx.canvas, dx, dy, dw, dh, 0, 0, a.width, a.height);
    else ax.drawImage(v, rx, ry, rw, rh, 0, 0, a.width, a.height);
    bx.drawImage(a, 0, 0, b.width, b.height);
    cx.drawImage(b, 0, 0, c.width, c.height);
    bx.drawImage(c, 0, 0, b.width, b.height);
    ax.drawImage(b, 0, 0, a.width, a.height);
    ctx.save();
    ctx.beginPath();
    ctx.ellipse(dx + dw / 2, dy + dh / 2, dw / 2, dh / 2, 0, 0, Math.PI * 2);
    ctx.clip();
    ctx.drawImage(a, dx, dy, dw, dh);
    ctx.restore();
  }
  // 4. holofote: escurece tudo menos um círculo
  {
    const cx0 = W * (0.3 + 0.4 * k);
    const cy0 = H * 0.3;
    const r = H * 0.12;
    ctx.save();
    ctx.beginPath();
    ctx.rect(0, 0, W, H);
    ctx.arc(cx0, cy0, r, 0, Math.PI * 2, true);
    ctx.fillStyle = "rgba(0,0,0,0.35)";
    ctx.fill("evenodd");
    ctx.restore();
    ctx.lineWidth = 4;
    ctx.strokeStyle = "#ffe119";
    ctx.beginPath();
    ctx.arc(cx0, cy0, r, 0, Math.PI * 2);
    ctx.stroke();
  }
  // 5. formas: retângulo, seta
  ctx.lineWidth = 3;
  ctx.strokeStyle = "#ff3b30";
  ctx.strokeRect(W * 0.62, H * 0.55 + 20 * Math.sin(k * 6.28), W * 0.14, H * 0.2);
  ctx.beginPath();
  const ax0 = W * 0.2;
  const ay0 = H * 0.8;
  const ax1 = W * (0.3 + 0.1 * k);
  const ay1 = H * 0.62;
  ctx.moveTo(ax0, ay0);
  ctx.lineTo(ax1, ay1);
  const an = Math.atan2(ay1 - ay0, ax1 - ax0);
  ctx.moveTo(ax1, ay1);
  ctx.lineTo(ax1 - 18 * Math.cos(an - 0.4), ay1 - 18 * Math.sin(an - 0.4));
  ctx.moveTo(ax1, ay1);
  ctx.lineTo(ax1 - 18 * Math.cos(an + 0.4), ay1 - 18 * Math.sin(an + 0.4));
  ctx.stroke();
  // 6. lupa fixa: recorte ampliado 3x num círculo com borda
  if (!opt.semLupa) {
    const lx = W - 170;
    const ly = 150;
    const lr = 110;
    const src = lr / 3;
    const ux = vw * 0.5;
    const uy = vh * 0.5;
    ctx.save();
    ctx.beginPath();
    ctx.arc(lx, ly, lr, 0, Math.PI * 2);
    ctx.clip();
    ctx.drawImage(v, ux - src / fx, uy - src / fy, (2 * src) / fx, (2 * src) / fy, lx - lr, ly - lr, 2 * lr, 2 * lr);
    ctx.restore();
    ctx.lineWidth = 3;
    ctx.strokeStyle = "#fff";
    ctx.beginPath();
    ctx.arc(lx, ly, lr, 0, Math.PI * 2);
    ctx.stroke();
  }
  // 7. texto: faixa inferior, título com contorno, carimbo de tempo
  ctx.fillStyle = "rgba(0,0,0,0.6)";
  ctx.fillRect(0, H - 86, W, 60);
  ctx.font = "600 30px sans-serif";
  ctx.textBaseline = "middle";
  ctx.fillStyle = "#fff";
  ctx.fillText("00000000 · 0000-00-00 00:00:00 · 000000 000000 · 0000×0000", 30, H - 56);
  ctx.font = "700 44px sans-serif";
  ctx.lineWidth = 6;
  ctx.strokeStyle = "#000";
  ctx.lineJoin = "round";
  ctx.strokeText("ÁÇÉÍÕÚ 0123456789", 30, 50);
  ctx.fillText("ÁÇÉÍÕÚ 0123456789", 30, 50);
  ctx.font = "500 24px monospace";
  const ts = v.currentTime.toFixed(3);
  ctx.strokeText(ts, W - 150, H - 20);
  ctx.fillText(ts, W - 150, H - 20);
}

/** Roda um laço de composição por `durMs` e mede cadência e custo (`measureLoop`). */
export async function medirLaco(
  amb: Ambiente,
  sc: ReturnType<typeof criarScratch>,
  canvas: HTMLCanvasElement,
  durMs: number,
  draw: (ctx: CanvasRenderingContext2D, k: number) => void,
  { sync = false, videos = [] as HTMLVideoElement[] } = {},
): Promise<Laco> {
  const ctx = ctx2d(canvas, { alpha: false });
  const one = ctx2d(sc("one", 1, 1), { willReadFrequently: true });
  const iv: number[] = [];
  const cost: number[] = [];
  const vf = videos.map(() => 0);
  const stopFlags = { stop: false };
  videos.forEach((v, i) => {
    const r = v as Rvfc;
    if (!r.requestVideoFrameCallback) return;
    const cb = () => {
      vf[i] = (vf[i] ?? 0) + 1;
      if (!stopFlags.stop) r.requestVideoFrameCallback?.(cb);
    };
    r.requestVideoFrameCallback(cb);
  });
  const qual = (v: HTMLVideoElement) => (v.getVideoPlaybackQuality ? v.getVideoPlaybackQuality() : null);
  const q0 = videos.map(qual);
  const t0 = amb.agora();
  let last = t0;
  let frames = 0;
  const ct0 = videos.map((v) => v.currentTime);
  // quadros distintos de vídeo efetivamente desenhados (pelo tempo do elemento)
  let distinct = 0;
  let lastCt = -1;
  try {
    for (;;) {
      await amb.quadro();
      const a = amb.agora();
      draw(ctx, ((a - t0) / durMs) % 1);
      if (sync) {
        one.drawImage(canvas, 0, 0, 1, 1, 0, 0, 1, 1);
        one.getImageData(0, 0, 1, 1);
      }
      const b = amb.agora();
      cost.push(b - a);
      iv.push(a - last);
      last = a;
      frames++;
      const ct = videos[0] ? videos[0].currentTime : frames;
      if (ct !== lastCt) {
        distinct++;
        lastCt = ct;
      }
      if (a - t0 >= durMs) break;
    }
  } finally {
    stopFlags.stop = true;
  }
  const el = amb.agora() - t0;
  iv.shift();
  const q1 = videos.map(qual);
  return {
    dur_ms: Math.round(el),
    rafs: frames,
    fps_raf: Math.round((frames / el) * 1000 * 100) / 100,
    intervalo_ms: stats(iv),
    custo_js_ms: stats(cost),
    sync,
    quadros_distintos_ct: distinct,
    intervalos_acima_25ms: iv.filter((x) => x > 25).length,
    intervalos_acima_50ms: iv.filter((x) => x > 50).length,
    video: videos.map((v, i) => {
      const a = q0[i];
      const b = q1[i];
      return {
        rvfc: vf[i] ?? 0,
        fps_rvfc: Math.round(((vf[i] ?? 0) / el) * 1000 * 100) / 100,
        avanco_s: Math.round((v.currentTime - (ct0[i] ?? 0)) * 1000) / 1000,
        decodificados: a && b ? b.totalVideoFrames - a.totalVideoFrames : null,
        descartados: a && b ? b.droppedVideoFrames - a.droppedVideoFrames : null,
      };
    }),
  };
}

export async function compose(amb: Ambiente, o: OpcoesCompose): Promise<ResCompose> {
  const f = o.f;
  const W = o.cw ?? 1280;
  const H = o.ch ?? 720;
  const dur = o.dur ?? 8000;
  const sc = criarScratch(amb);
  const cv = amb.palco;
  cv.width = W;
  cv.height = H;
  const ler = leitorDeCodigo(amb);
  const v = await abrirVideo(amb, f);
  try {
    v.loop = true;
    await buscar(amb, v, 0.5);
    await comPrazo(amb, v.play(), PRAZO_PLAY_MS, "play");
    await amb.dormir(800);
    const laco = (ms: number, op: OpcoesDesenho, sync = false) =>
      medirLaco(amb, sc, cv, ms, (ctx, k) => compor(sc, ctx, v, W, H, k, op), { videos: [v], sync });
    const so_drawimage = await laco(dur / 2, { soDraw: true });
    const completo = await laco(dur, {});
    const completo_sync = await laco(dur / 2, {}, true);
    const so_drawimage_sync = await laco(dur / 2, { soDraw: true }, true);
    const um_desenho_com_lupa = await laco(dur / 2, { daSaida: true });
    const um_desenho_sem_lupa = await laco(dur / 2, { daSaida: true, semLupa: true });
    const um_desenho_sem_lupa_sync = await laco(dur / 2, { daSaida: true, semLupa: true }, true);
    // rajada: quantas composições completas cabem em 1 s sem esperar o rAF (teto do desenho)
    let rajada = 0;
    {
      const ctx = ctx2d(cv, { alpha: false });
      const one = ctx2d(sc("one", 1, 1), { willReadFrequently: true });
      const t0 = amb.agora();
      const lim = o.rajada_ms ?? 1000;
      while (amb.agora() - t0 < lim) {
        compor(sc, ctx, v, W, H, (rajada % 100) / 100);
        one.drawImage(cv, 0, 0, 1, 1, 0, 0, 1, 1);
        one.getImageData(0, 0, 1, 1);
        rajada++;
      }
      rajada = Math.round((rajada * 1000) / lim);
    }
    return {
      arquivo: f,
      video: [v.videoWidth, v.videoHeight],
      canvas: [W, H],
      so_drawimage,
      completo,
      completo_sync,
      so_drawimage_sync,
      um_desenho_com_lupa,
      um_desenho_sem_lupa,
      um_desenho_sem_lupa_sync,
      rajada_por_s: rajada,
      quadro_lido_fim: ler(v),
    };
  } finally {
    v.pause();
    v.remove();
  }
}
