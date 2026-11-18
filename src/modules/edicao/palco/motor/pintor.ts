/**
 * Pintor da prévia viva (§7.2): fundo, UM `drawImage(video)` por camada com `A(τ)` (zoom que segue incluído:
 * `modelo/segue.ts`), as primitivas dos efeitos (`palco/cena.ts`), os sobrepostos com a transformação
 * (`modelo/sobreposto.ts`, primitivas de `palco/livres.ts`), a camada global (legendas queimadas e selos) e a
 * transição (5 grupos, como o `xfade` do motor; o resto como fundido). Canvas `alpha: false`, até 1280×720 tocando.
 * Nenhuma leitura de DOM: os canvas de apoio vêm de `fontes.apoio`.
 */

import type { ClipePrincipal, Id, Primitiva, ProjetoEdicao, Pts } from "@domain/edicao";
import { acharClipe } from "../../modelo/projeto";
import { aDe, IDENTIDADE } from "../../modelo/geometria";
import { ptsDaTabela } from "../../modelo/tempo";
import { efeitosNoInstante } from "../../modelo/segue";
import { pintar } from "../desenho/pintar";
import { cenaDoClipe, midiaDoClipe, type FontesCena, type Instante } from "../cena";
import type { LivreNoQuadro } from "../livres";
import type { CamadaPrevia, EstadoPrevia, TransicaoPrevia } from "./plano";

export interface FontesPintor {
  /** Elemento pronto do segmento (null = ainda não: pinta o quadro anterior). */
  video(clipe: Id): HTMLVideoElement | null;
  /** Sprite de texto ou imagem do projeto (`src` das primitivas); null = ainda não chegou. */
  imagem(src: string): CanvasImageSource | null;
  // CONTRATO: acréscimos (P3.3)
  /** Sprites, atlas e tamanhos de imagem (os mesmos do modo clipe). */
  cena: FontesCena;
  /** Tamanho exibido da mídia (px); null = sem info (vale o canvas da saída). */
  origem(midia: Id): { w: number; h: number } | null;
  /** Sobrepostos no slot (`palco/livres.ts`). */
  livres(q: number): LivreNoQuadro[];
  /** Legendas queimadas e selos no slot (camada global), já em primitivas. */
  globais(q: number): Primitiva[];
  /** Canvas de apoio por nome ("rascunho", "entra", "livre:<id>"); null = sem apoio (a parte fica de fora). */
  apoio(nome: string): HTMLCanvasElement | null;
}

/** Primitivas que o clipe sem fonte desenha por si (título e imagem), no canvas da saída `W`×`H`. */
export function primitivasProprias(c: ClipePrincipal, W: number, H: number, fontes: FontesCena): Primitiva[] {
  if (c.tipo === "imagem") {
    const t = fontes.midia(c.midia);
    if (!t || !(t.w > 0 && t.h > 0)) return [];
    // imagem inteira contida no canvas
    const r = Math.min(W / t.w, H / t.h);
    const w = (t.w * r) / W;
    const h = (t.h * r) / H;
    return [{ k: "sprite", src: `midia:${c.midia}`, caixa: { x: (1 - w) / 2, y: (1 - h) / 2, w, h }, opacidade: 1 }];
  }
  if (c.tipo !== "titulo") return [];
  const saida: Primitiva[] = [];
  const f = c.titulo.fundo;
  if (f.tipo === "cor") saida.push({ k: "caminho", d: [["M", 0, 0], ["L", 1, 0], ["L", 1, 1], ["L", 0, 1], ["Z"]], preenche: f.cor });
  else if (f.tipo === "imagem") saida.push(...primitivasProprias({ ...c, tipo: "imagem", midia: f.midia } as unknown as ClipePrincipal, W, H, fontes));
  for (const b of c.titulo.blocos) {
    if (b.conteudo === "") continue;
    const s = fontes.sprite({ blocos: [b], largura_max: (b.largura_max * W) / H, altura_canvas: H, fundo: true });
    if (!s) continue;
    const fh = b.lugar.h === "esq" ? 0 : b.lugar.h === "centro" ? 0.5 : 1;
    const fv = b.lugar.v === "topo" ? 0 : b.lugar.v === "meio" ? 0.5 : 1;
    const x = Math.floor(b.lugar.x * W - s.w * fh + 0.5);
    const y = Math.floor(b.lugar.y * H - s.h * fv + 0.5);
    saida.push({ k: "sprite", src: s.relativo, caixa: { x: x / W, y: y / H, w: s.w / W, h: s.h / H }, opacidade: 1 });
  }
  return saida;
}

/** O instante da camada como o `cenaDoClipe` pede (pts do quadro mostrado, slot do programa). */
export function instanteDaCamada(c: CamadaPrevia, estado: Pick<EstadoPrevia, "q" | "programa_s">): Instante {
  const tab = c.segmento.tabela;
  const pts: Pts | null = c.indice !== null && tab ? ((ptsDaTabela(tab)[c.indice] as number) ?? null) : null;
  const congelado = c.segmento.fonte?.congelado ?? false;
  return { t: c.t, pts: congelado ? null : pts, indice: c.indice, q: estado.q, programa_s: estado.programa_s };
}

/** Uma camada (vídeo + efeitos do clipe) no canvas inteiro; null = o fundo. */
function camada(ctx: CanvasRenderingContext2D, c: CamadaPrevia | null, estado: EstadoPrevia, doc: ProjetoEdicao, f: FontesPintor, W: number, H: number) {
  const fundo = doc.saida.cor_fundo;
  const saida = { w: doc.saida.largura, h: doc.saida.altura };
  const achado = c ? acharClipe(doc, c.segmento.clipe) : null;
  const clipe = achado?.onde === "principal" ? achado.clipe : null;
  if (!c || !clipe) {
    pintar({ ctx, w: W, h: H, fundo, video: null, A: IDENTIDADE, primitivas: [], imagem: f.imagem });
    return;
  }
  const midia = midiaDoClipe(clipe);
  const origem = (midia ? f.origem(midia) : null) ?? saida;
  const inst = instanteDaCamada(c, estado);
  const A = aDe(efeitosNoInstante(doc, clipe.efeitos, inst.t), inst.t, origem, saida);
  let prims: Primitiva[] = primitivasProprias(clipe, saida.w, saida.h, f.cena);
  try {
    prims = [...prims, ...cenaDoClipe(doc, clipe, inst, origem, f.cena, A).primitivas];
  } catch {
    // dado inválido num efeito: o vídeo segue sem as anotações (o Inspetor acusa)
  }
  const video = c.segmento.fonte ? f.video(clipe.id) : null;
  pintar({ ctx, w: W, h: H, fundo, video, A, primitivas: prims, imagem: f.imagem, rascunho: () => f.apoio("rascunho") });
  if (c.luz < 1) {
    ctx.setTransform(1, 0, 0, 1, 0, 0);
    ctx.globalAlpha = 1;
    ctx.fillStyle = `rgba(0,0,0,${1 - c.luz})`;
    ctx.fillRect(0, 0, W, H);
  }
}

/** Põe a camada que entra (no canvas `b`) sobre a que sai, pelo tipo da transição (o `xfade` do motor). */
export function compor(ctx: CanvasRenderingContext2D, b: HTMLCanvasElement, tr: TransicaoPrevia, W: number, H: number): void {
  const p = Math.min(Math.max(tr.progresso, 0), 1);
  ctx.setTransform(1, 0, 0, 1, 0, 0);
  ctx.globalAlpha = 1;
  const faixa = (sx: number, dx: number, w: number) => {
    const ww = Math.round(w);
    if (ww >= 1) ctx.drawImage(b, Math.round(sx), 0, ww, H, Math.round(dx), 0, ww, H);
  };
  switch (tr.tipo) {
    case "cortina_esq":
      // wipeleft: o que entra ocupa a direita
      faixa(W * (1 - p), W * (1 - p), W * p);
      return;
    case "cortina_dir":
      faixa(0, 0, W * p);
      return;
    case "deslizar_esq":
      // coverleft: o que entra vem da direita, empurrado
      faixa(0, W * (1 - p), W * p);
      return;
    case "deslizar_dir":
      faixa(W * (1 - p), 0, W * p);
      return;
    case "circulo": {
      // circleopen: peso 1 − clamp(r/z − 3(p − ½)) do centro para fora
      const bc = b.getContext("2d");
      const z = Math.hypot(W / 2, H / 2);
      const c = 3 * (p - 0.5);
      if (!bc || c + 1 <= 0) return;
      const r0 = Math.max(0, c * z);
      const r1 = (c + 1) * z;
      const g = bc.createRadialGradient(W / 2, H / 2, r0, W / 2, H / 2, r1);
      g.addColorStop(0, `rgba(0,0,0,${c < 0 ? 1 + c : 1})`);
      g.addColorStop(1, "rgba(0,0,0,0)");
      bc.save();
      bc.setTransform(1, 0, 0, 1, 0, 0);
      bc.globalCompositeOperation = "destination-in";
      bc.fillStyle = g;
      bc.fillRect(0, 0, W, H);
      bc.restore();
      ctx.drawImage(b, 0, 0);
      return;
    }
    default:
      // fundir (e o que não tem pintor): mistura linear
      ctx.globalAlpha = p;
      ctx.drawImage(b, 0, 0);
      ctx.globalAlpha = 1;
  }
}

/** Sobreposto com a `Transformacao` avaliada: o título (canvas inteiro, já pintado em `apoio`) ou a imagem (tamanho
 *  natural), centro em (x, y), largura `escala` × canvas, giro horário (como `formas::transformacao`). */
function livre(ctx: CanvasRenderingContext2D, l: LivreNoQuadro, f: FontesLivres, W: number, H: number) {
  const t = l.transformacao;
  if (!(t.opacidade > 0) || !(t.escala > 0)) return;
  let fonte: CanvasImageSource | null = null;
  let ow = W;
  let oh = H;
  if (l.clipe.tipo === "imagem") {
    fonte = f.imagem(`midia:${l.clipe.midia}`);
    const tam = f.cena.midia(l.clipe.midia);
    if (!fonte || !tam || !(tam.w > 0 && tam.h > 0)) return;
    [ow, oh] = [tam.w, tam.h];
  } else {
    const cv = f.apoio(`livre:${l.clipe.id}`);
    if (!cv) return;
    // as primitivas só mudam com o documento ou um sprite que chega: o canvas guarda a última pintura
    const chave = `${W}x${H}|${JSON.stringify(l.primitivas)}|${l.primitivas.map((p) => (p.k === "sprite" || p.k === "atlas" ? (f.imagem(p.src) ? 1 : 0) : 2)).join("")}`;
    if (cv.dataset.chave !== chave) {
      if (cv.width !== W || cv.height !== H) [cv.width, cv.height] = [W, H];
      const c2 = cv.getContext("2d");
      if (!c2) return;
      c2.setTransform(1, 0, 0, 1, 0, 0);
      c2.clearRect(0, 0, W, H);
      pintar({ ctx: c2, w: W, h: H, fundo: "rgba(0,0,0,0)", video: null, A: IDENTIDADE, primitivas: l.primitivas, imagem: f.imagem });
      cv.dataset.chave = chave;
    }
    fonte = cv;
  }
  const s = (t.escala * W) / ow;
  ctx.save();
  ctx.setTransform(1, 0, 0, 1, 0, 0);
  ctx.globalAlpha = Math.min(Math.max(t.opacidade, 0), 1);
  ctx.translate(t.x * W, t.y * H);
  ctx.rotate((t.rotacao * Math.PI) / 180);
  ctx.scale(s, s);
  ctx.drawImage(fonte, -ow / 2, -oh / 2, ow, oh);
  ctx.restore();
}

type FontesLivres = Pick<FontesPintor, "imagem" | "cena" | "apoio">;

/** Os sobrepostos por cima do que já está no canvas (o modo clipe também os pinta, no slot do quadro mostrado). */
export function pintarLivres(ctx: CanvasRenderingContext2D, livres: readonly LivreNoQuadro[], f: FontesLivres, W: number, H: number): void {
  for (const l of livres) livre(ctx, l, f, W, H);
  ctx.setTransform(1, 0, 0, 1, 0, 0);
  ctx.globalAlpha = 1;
}

export function pinta(ctx: CanvasRenderingContext2D, estado: EstadoPrevia, doc: ProjetoEdicao, fontes: FontesPintor, tela: { w: number; h: number }): void {
  const { w: W, h: H } = tela;
  const tr = estado.transicao;
  const [a, b] = estado.camadas;
  // quem sai vai direto no canvas; quem entra, num canvas de apoio, composto por cima
  const sai = !tr ? (a ?? null) : tr.fundo === "sai" ? null : (a ?? null);
  const entra = !tr ? undefined : tr.fundo === "sai" ? (a ?? null) : tr.fundo === "entra" ? null : (b ?? null);
  camada(ctx, sai, estado, doc, fontes, W, H);
  if (tr && entra !== undefined) {
    const cv = fontes.apoio("entra");
    const c2 = cv?.getContext("2d", { alpha: true }) as CanvasRenderingContext2D | null | undefined;
    if (cv && c2) {
      if (cv.width !== W || cv.height !== H) [cv.width, cv.height] = [W, H];
      camada(c2, entra, estado, doc, fontes, W, H);
      compor(ctx, cv, tr, W, H);
    }
  }
  pintarLivres(ctx, fontes.livres(estado.q), fontes, W, H);
  const g = fontes.globais(estado.q);
  if (g.length > 0) pintar({ ctx, w: W, h: H, fundo: "rgba(0,0,0,0)", video: null, A: IDENTIDADE, primitivas: g, imagem: fontes.imagem });
  ctx.setTransform(1, 0, 0, 1, 0, 0);
  ctx.globalAlpha = 1;
}
