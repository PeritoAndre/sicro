/**
 * Sobrepostos no palco (texto livre, título, imagem): o que aparece no slot do programa e as alças do selecionado —
 * mover (centro), cantos (tamanho da letra, proporcional), lados (largura da caixa), giro (rotação); arrastar com
 * chaves grava a chave no cursor (D4, D7). O palco (P3.3) pinta e liga as alças por aqui. Dono: pacote do texto livre.
 *
 * Contas em px do canvas da saída: o título é um objeto do tamanho do canvas, posto pela `Transformacao` (centro do
 * canvas em (x, y), escala, giro horário); o bloco do texto livre fica no meio dele. A caixa das alças é a do texto
 * visível (o sprite sem a folga da sombra), com a largura da caixa do texto (`largura_max`) quando ela é maior.
 */

import type { Alca, Animavel, BlocoTexto, Id, LinhaResolvida, Mudanca, Ponto, Primitiva, ProjetoEdicao, Seg, SobImagem, SobTitulo } from "@domain/edicao";
import { folgaDaSombra, transformacaoEm, type TransformacaoAvaliada } from "../modelo/sobreposto";
import { primitivas as primitivasDoTitulo } from "../modelo/titulos";
import { avaliar } from "../modelo/curva";
import { sobrepostoConhecido } from "../modelo/projeto";
import { gravarEm as gravarNaChave, instanteNoQuadro } from "../linha/curvas/chaves";
import { instanteDoSlot } from "../modelo/tempo";
import { SOMBRA_MAX, ehTextoLivre } from "../inspetor/secoes/textoLivre";
import type { FontesCena } from "./cena";

export interface LivreNoQuadro {
  trilha: Id;
  clipe: SobTitulo | SobImagem;
  /** Tempo local no slot. */
  tau: Seg;
  transformacao: TransformacaoAvaliada;
  /** Primitivas do título (0..1 do canvas, antes da transformação); imagem: vazio (o pintor desenha a mídia). */
  primitivas: Primitiva[];
}

/** Sobrepostos visíveis no slot `q` do programa, na ordem de pintura (trilhas não ocultas, de baixo para cima). */
export function livresNoQuadro(doc: ProjetoEdicao, linha: LinhaResolvida, q: number, fontes: FontesCena): LivreNoQuadro[] {
  const canvas = { w: doc.saida.largura, h: doc.saida.altura };
  const saida: LivreNoQuadro[] = [];
  for (const t of doc.sobreposicoes) {
    if (t.oculta) continue;
    for (const c of t.clipes) {
      // vídeo sobreposto é da Fase 3 (o motor recusa); desconhecido não pinta
      if (!sobrepostoConhecido(c) || c.tipo === "video") continue;
      const l = linha.livres.find((x) => x.id === c.id && x.trilha === t.id);
      if (!l || q < l.inicio_q || q >= l.inicio_q + l.n_q) continue;
      const tau = instanteDoSlot(q - l.inicio_q, linha.fps);
      let prims: Primitiva[] = [];
      if (c.tipo === "titulo") {
        try {
          prims = primitivasDoTitulo(c.titulo, { canvas, sprite: (p) => fontes.sprite(p), midia: (id) => fontes.midia(id) });
        } catch {
          // dado inválido (a análise acusa): sem desenho, como o sprite que não chegou
          prims = [];
        }
      }
      saida.push({ trilha: t.id, clipe: c, tau, transformacao: transformacaoEm(c, tau, instanteDoSlot(l.n_q, linha.fps)), primitivas: prims });
    }
  }
  return saida;
}

// ---------------------------------------------------------------- geometria (px do canvas)

type V = { x: number; y: number };
const mais = (a: V, b: V): V => ({ x: a.x + b.x, y: a.y + b.y });
const menos = (a: V, b: V): V => ({ x: a.x - b.x, y: a.y - b.y });
const vezes = (a: V, k: number): V => ({ x: a.x * k, y: a.y * k });
const escalar = (a: V, b: V) => a.x * b.x + a.y * b.y;

/** Linha de texto (em fração do tamanho da letra) antes de o sprite chegar: ascendente + descendente da Inter. */
const LINHA = 1.2109375;
/** Distância da alça de giro acima da caixa, em fração da altura do canvas. */
export const GIRO_ACIMA = 0.045;
/** Tamanho da letra (fração da altura) e largura da caixa (fração da largura) que as alças aceitam. */
export const TAMANHO_MIN = 0.008;
export const TAMANHO_MAX = 0.4;
export const LARGURA_MIN = 0.02;
export const LARGURA_MAX = 2;

/** Caixa do texto antes da transformação (px do canvas): centro e tamanho. */
interface CaixaPx { c: V; w: number; h: number }

const fh = (h: BlocoTexto["lugar"]["h"]) => (h === "esq" ? 0 : h === "centro" ? 0.5 : 1);

/** Margem e contorno (px) somados de cada lado do texto. */
function bordas(b: BlocoTexto, H: number): number {
  const e = b.estilo;
  return (e.caixa ? e.caixa.margem * H : 0) + (e.contorno ? Math.max(e.contorno.largura, 0) * H : 0);
}

/** Caixa visível do bloco do texto livre (px, antes da transformação) para estilo e largura dados, com o sprite
 *  quando chegou (`sprite` = caixa 0..1 do sprite no canvas). */
function caixaDoBloco(b: BlocoTexto, sprite: { x: number; y: number; w: number; h: number } | null, W: number, H: number): CaixaPx {
  const caixaTexto = Math.max(b.largura_max, 0) * W + 2 * bordas(b, H);
  if (sprite) {
    const f = b.estilo.sombra ? folgaDaSombra(b.estilo.sombra, H) : { x: 0, y: 0 };
    const sx = sprite.x * W + f.x;
    const sw = sprite.w * W - 2 * f.x;
    const w = Math.max(sw, caixaTexto);
    // a caixa larga cresce a partir do ponto de apoio do bloco (o sprite fica nele)
    const x0 = sx - (w - sw) * fh(b.lugar.h);
    return { c: { x: x0 + w / 2, y: sprite.y * H + (sprite.h * H) / 2 }, w, h: sprite.h * H - 2 * f.y };
  }
  const h = Math.max(b.estilo.tamanho, 0) * H * LINHA + 2 * bordas(b, H);
  const fv = b.lugar.v === "topo" ? 0 : b.lugar.v === "meio" ? 0.5 : 1;
  return { c: { x: b.lugar.x * W + caixaTexto * (0.5 - fh(b.lugar.h)), y: b.lugar.y * H + h * (0.5 - fv) }, w: caixaTexto, h };
}

/** Transformação do objeto (o canvas inteiro) no canvas: p ↦ M·(p − centro do canvas) + (x·W, y·H). */
interface Transf { s: number; cos: number; sen: number; X: V; C: V }
function transf(t: { x: number; y: number; escala: number; rotacao: number }, W: number, H: number): Transf {
  const r = (t.rotacao * Math.PI) / 180;
  return { s: t.escala, cos: Math.cos(r), sen: Math.sin(r), X: { x: t.x * W, y: t.y * H }, C: { x: W / 2, y: H / 2 } };
}
/** M·v (giro horário na tela, y para baixo). */
const girar = (m: Transf, v: V): V => ({ x: m.s * (m.cos * v.x - m.sen * v.y), y: m.s * (m.sen * v.x + m.cos * v.y) });
const aplicarT = (m: Transf, p: V): V => mais(girar(m, menos(p, m.C)), m.X);

/** Valor animável no instante, e o mesmo campo com `v` gravado no instante (chave no cursor, se já tem chaves). */
const lerEm = (a: Animavel<number>, tau: Seg) => avaliar(a, tau);
// P4.2 (D7): a mesma regra do Inspetor e da linha (`linha/curvas/chaves.ts`): chave nova "suave", a que já existe no
// quadro do cursor fica com a curva dela
function gravarEm(a: Animavel<number>, v: number, tau: Seg, fps: { num: number; den: number } | undefined): Animavel<number> {
  return gravarNaChave(a, instanteNoQuadro(a, tau, fps && ((t) => Math.round((t * fps.num) / fps.den))), v);
}

const limitar = (v: number, a: number, b: number) => Math.min(Math.max(v, a), b);
/** Centro posto no pixel inteiro da saída: sem giro nem escala o texto sai do sprite sem reamostrar (nítido). */
const noPixel = (v: number, lado: number) => Math.round(v * lado) / lado;

function cursorDe(tipo: Alca["tipo"], d: V): string {
  if (tipo === "mover") return "move";
  if (tipo === "giro") return "grab";
  if (tipo === "borda") return Math.abs(d.x) > Math.abs(d.y) ? "ew-resize" : "ns-resize";
  return d.x * d.y > 0 ? "nwse-resize" : "nesw-resize";
}

/** Alças do sobreposto selecionado (0..1 do canvas); cada uma devolve `{ livre, trilha, sobreposto }`. Texto livre:
 *  mover, quatro cantos, dois lados e giro; outro título ou imagem: só mover. Shift: mover num eixo, giro de 15°
 *  em 15°. Alt: canto e lado a partir do centro. */
export function alcasDoLivre(doc: ProjetoEdicao, item: LivreNoQuadro, canvas: { w: number; h: number }): Alca[] {
  const fps = doc.saida?.fps;
  const { w: W, h: H } = canvas;
  if (!(W > 0 && H > 0)) return [];
  const c0 = item.clipe;
  const tau = item.tau;
  const tr = c0.transformacao;
  // o que se vê agora (com o deslize da entrada/saída) e o que está gravado no instante
  const visto = item.transformacao;
  const desliza = { x: visto.x - lerEm(tr.x, tau), y: visto.y - lerEm(tr.y, tau) };
  const m = transf(visto, W, H);
  const id = c0.id;
  const mudanca = (novo: SobTitulo | SobImagem): Mudanca => ({ livre: id, trilha: item.trilha, sobreposto: novo });
  const copia = () => structuredClone(c0) as SobTitulo | SobImagem;
  /** Grava o centro do objeto que se vê em `X` (px), tirando o deslize. */
  const comCentro = (novo: SobTitulo | SobImagem, X: V) => {
    novo.transformacao.x = gravarEm(tr.x, noPixel(X.x / W - desliza.x, W), tau, fps);
    novo.transformacao.y = gravarEm(tr.y, noPixel(X.y / H - desliza.y, H), tau, fps);
  };
  const em = (p: V): Ponto => ({ x: p.x / W, y: p.y / H });
  const px = (p: Ponto): V => ({ x: p.x * W, y: p.y * H });

  const mover: Alca = {
    id: `livre:${id}:mover`,
    tipo: "mover",
    em: em(m.X),
    cursor: "move",
    arrastar(de, para, mod) {
      let d = menos(px(para), px(de));
      if (mod.shift) d = Math.abs(d.x) >= Math.abs(d.y) ? { x: d.x, y: 0 } : { x: 0, y: d.y };
      const novo = copia();
      const X = mais(m.X, d);
      comCentro(novo, { x: limitar(X.x, 0, W), y: limitar(X.y, 0, H) });
      return mudanca(novo);
    },
  };
  if (c0.tipo !== "titulo" || !ehTextoLivre(c0)) return [mover];

  const b = c0.titulo.blocos[0] as BlocoTexto;
  const spr = item.primitivas.find((p): p is Extract<Primitiva, { k: "sprite" }> => p.k === "sprite" && !p.src.startsWith("midia:"));
  const cx = caixaDoBloco(b, spr ? spr.caixa : null, W, H);
  // ponto da caixa (antes da transformação) em (sx, sy) ∈ {−1, 0, 1}²
  const naCaixa = (k: CaixaPx, sx: number, sy: number): V => ({ x: k.c.x + (sx * k.w) / 2, y: k.c.y + (sy * k.h) / 2 });
  const centro = aplicarT(m, cx.c);
  /** Centro do objeto que põe o centro da caixa (antes) `cPre` em `C` (na tela), com o giro e a escala de `mm`. */
  const objetoPara = (mm: Transf, cPre: V, C: V): V => menos(C, girar(mm, menos(cPre, mm.C)));

  const alcas: Alca[] = [{ ...mover, em: em(centro) }];

  // cantos: tamanho da letra (e tudo do estilo, na mesma proporção), âncora no canto oposto (Alt: no centro)
  for (const [nome, sx, sy] of [["no", -1, -1], ["ne", 1, -1], ["se", 1, 1], ["so", -1, 1]] as const) {
    const P = aplicarT(m, naCaixa(cx, sx, sy));
    alcas.push({
      id: `livre:${id}:${nome}`,
      tipo: "canto",
      em: em(P),
      cursor: cursorDe("canto", girar({ ...m, s: 1 }, { x: sx, y: sy })),
      arrastar(de, para, mod) {
        const A = mod.alt ? centro : aplicarT(m, naCaixa(cx, -sx, -sy));
        const P2 = mais(P, menos(px(para), px(de)));
        const base = menos(P, A);
        const n2 = escalar(base, base);
        let k = n2 > 0 ? escalar(menos(P2, A), base) / n2 : 1;
        const e = b.estilo;
        k = limitar(k, TAMANHO_MIN / Math.max(e.tamanho, 1e-9), TAMANHO_MAX / Math.max(e.tamanho, 1e-9));
        const novo = copia() as SobTitulo;
        const nb = novo.titulo.blocos[0] as BlocoTexto;
        escalarEstilo(nb, k);
        // a caixa cresce a partir do ponto de apoio do bloco; o centro dela vai para A + (centro − A)·k
        const ancora: V = { x: b.lugar.x * W, y: b.lugar.y * H };
        const cPre = mais(ancora, vezes(menos(cx.c, ancora), k));
        comCentro(novo, objetoPara(m, cPre, mais(A, vezes(menos(centro, A), k))));
        return mudanca(novo);
      },
    });
  }

  // lados: largura da caixa; o lado oposto fica (Alt: os dois lados andam)
  for (const [nome, sx] of [["o", -1], ["l", 1]] as const) {
    const P = aplicarT(m, naCaixa(cx, sx, 0));
    const u = girar({ ...m, s: 1 }, { x: 1, y: 0 });
    alcas.push({
      id: `livre:${id}:${nome}`,
      tipo: "borda",
      em: em(P),
      cursor: cursorDe("borda", u),
      arrastar(de, para, mod) {
        const s = Math.max(m.s, 1e-9);
        const fora = (escalar(menos(px(para), px(de)), u) * sx) / s;
        const extras = 2 * bordas(b, H);
        const w1 = limitar(cx.w + (mod.alt ? 2 * fora : fora), LARGURA_MIN * W + extras, LARGURA_MAX * W + extras);
        const novo = copia() as SobTitulo;
        const nb = novo.titulo.blocos[0] as BlocoTexto;
        nb.largura_max = (w1 - extras) / W;
        const dw = w1 - cx.w;
        // centro da caixa (antes) com a largura nova, pelo ponto de apoio do bloco
        const cPre = { x: cx.c.x + dw * (0.5 - fh(b.lugar.h)), y: cx.c.y };
        const C = mod.alt ? centro : mais(centro, vezes(u, (sx * dw * m.s) / 2));
        comCentro(novo, objetoPara(m, cPre, C));
        return mudanca(novo);
      },
    });
  }

  // giro: em torno do centro da caixa, acima da borda de cima; Shift = de 15° em 15°
  const topo = naCaixa(cx, 0, -1);
  const G = aplicarT(m, { x: topo.x, y: topo.y - (GIRO_ACIMA * H) / Math.max(m.s, 1e-9) });
  alcas.push({
    id: `livre:${id}:giro`,
    tipo: "giro",
    em: em(G),
    cursor: "grab",
    arrastar(de, para, mod) {
      const a0 = Math.atan2(px(de).y - centro.y, px(de).x - centro.x);
      const a1 = Math.atan2(px(para).y - centro.y, px(para).x - centro.x);
      let r = lerEm(tr.rotacao, tau) + ((a1 - a0) * 180) / Math.PI;
      if (mod.shift) r = Math.round(r / 15) * 15;
      r = ((((r + 180) % 360) + 360) % 360) - 180;
      const novo = copia();
      novo.transformacao.rotacao = gravarEm(tr.rotacao, Math.round(r * 100) / 100 + 0, tau, fps);
      const m2 = transf({ ...visto, rotacao: visto.rotacao + (r - lerEm(tr.rotacao, tau)) }, W, H);
      comCentro(novo, objetoPara(m2, cx.c, centro));
      return mudanca(novo);
    },
  });
  return alcas;
}

/** Tamanho da letra, largura da caixa, margem, raio, contorno e sombra multiplicados por `k` (canto arrastado). */
export function escalarEstilo(b: BlocoTexto, k: number): void {
  const e = b.estilo;
  e.tamanho *= k;
  b.largura_max *= k;
  if (e.caixa) {
    e.caixa.margem *= k;
    e.caixa.raio *= k;
  }
  if (e.contorno) e.contorno.largura *= k;
  if (e.sombra) {
    const s = e.sombra;
    s.dx = limitar(s.dx * k, -SOMBRA_MAX, SOMBRA_MAX);
    s.dy = limitar(s.dy * k, -SOMBRA_MAX, SOMBRA_MAX);
    s.desfoque = limitar(s.desfoque * k, 0, SOMBRA_MAX);
  }
}

/** O ponto `p` (0..1 do canvas) cai na caixa do texto livre (girada)? Clique para selecionar e arrastar pela caixa. */
export function dentroDoTexto(item: LivreNoQuadro, canvas: { w: number; h: number }, p: Ponto): boolean {
  const c = cantosDoTexto(item, canvas).map((x) => ({ x: x.x * canvas.w, y: x.y * canvas.h }));
  if (c.length !== 4) return false;
  const q = { x: p.x * canvas.w, y: p.y * canvas.h };
  // do mesmo lado das quatro arestas (caixa convexa, em qualquer sentido)
  let sinal = 0;
  for (let i = 0; i < 4; i++) {
    const a = c[i] as V;
    const b = c[(i + 1) % 4] as V;
    const z = (b.x - a.x) * (q.y - a.y) - (b.y - a.y) * (q.x - a.x);
    if (z === 0) continue;
    if (sinal === 0) sinal = Math.sign(z);
    else if (Math.sign(z) !== sinal) return false;
  }
  return true;
}

/** Caixa do texto livre na tela (0..1 do canvas): os quatro cantos, na ordem no, ne, se, so (contorno das alças). */
export function cantosDoTexto(item: LivreNoQuadro, canvas: { w: number; h: number }): Ponto[] {
  const { w: W, h: H } = canvas;
  if (item.clipe.tipo !== "titulo" || !ehTextoLivre(item.clipe) || !(W > 0 && H > 0)) return [];
  const b = item.clipe.titulo.blocos[0] as BlocoTexto;
  const spr = item.primitivas.find((p): p is Extract<Primitiva, { k: "sprite" }> => p.k === "sprite" && !p.src.startsWith("midia:"));
  const cx = caixaDoBloco(b, spr ? spr.caixa : null, W, H);
  const m = transf(item.transformacao, W, H);
  return [[-1, -1], [1, -1], [1, 1], [-1, 1]].map(([sx, sy]) => {
    const p = aplicarT(m, { x: cx.c.x + ((sx as number) * cx.w) / 2, y: cx.c.y + ((sy as number) * cx.h) / 2 });
    return { x: p.x / W, y: p.y / H };
  });
}
