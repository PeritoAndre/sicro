/** Sobrepostos no palco (pacote do texto livre): quais aparecem no slot e as alças do texto livre → `Mudanca`. */

import { describe, expect, it } from "vitest";
import type { Animavel, BlocoTexto, LinhaResolvida, Mudanca, PedidoSprite, ProjetoEdicao, SobImagem, SobTitulo } from "@domain/edicao";
import { aplicarMudanca, novoProjeto } from "../modelo/projeto";
import { folgaDaSombra, transformacaoEm } from "../modelo/sobreposto";
import { alcasDoLivre, cantosDoTexto, dentroDoTexto, livresNoQuadro, type LivreNoQuadro } from "./livres";
import type { FontesCena } from "./cena";
import { estiloNovo, SOMBRA } from "../inspetor/secoes/textoLivre";
import { casos } from "../modelo/testeFixtures";

const W = 1920;
const H = 1080;
const canvas = { w: W, h: H };

/** Sprite falso com as contas do Rust no que importa às alças: faixa = largura inteira; folga da sombra dos dois lados. */
const fontes: FontesCena = {
  sprite(p: PedidoSprite) {
    const b = p.blocos[0] as BlocoTexto;
    const e = b.estilo;
    const m = e.caixa ? e.caixa.margem * p.altura_canvas : 0;
    const natural = Math.min(b.conteudo.length * 0.5 * e.tamanho * p.altura_canvas, p.largura_max * p.altura_canvas);
    const largura = e.caixa?.faixa ? p.largura_max * p.altura_canvas : natural;
    const f = e.sombra ? folgaDaSombra(e.sombra, p.altura_canvas) : { x: 0, y: 0 };
    return { relativo: `sprites/${b.conteudo}.png`, w: Math.ceil(largura + 2 * m) + 2 * f.x, h: Math.ceil(e.tamanho * p.altura_canvas * 1.2 + 2 * m) + 2 * f.y };
  },
  atlas: () => null,
  midia: () => ({ w: 400, h: 300 }),
};

function texto(id: string, extra: Partial<SobTitulo> = {}, conteudo = "Policiais chegam"): SobTitulo {
  return {
    tipo: "titulo",
    id,
    rotulo: "",
    pos: { ref: "programa", t: 0 },
    duracao_s: 2,
    transformacao: { x: 0.25, y: 0.75, escala: 1, rotacao: 0, opacidade: 1 },
    entrada_s: 0.3,
    saida_s: 0.3,
    efeitos: [],
    titulo: { modelo: "livre", fundo: { tipo: "transparente" }, blocos: [{ id: `${id}b`, papel: "corpo", conteudo, estilo: estiloNovo(), lugar: { x: 0.5, y: 0.5, h: "centro", v: "meio" }, largura_max: 0.4 }] },
    ...extra,
  };
}

function cena(clipes: (SobTitulo | SobImagem)[], oculta = false): { doc: ProjetoEdicao; linha: LinhaResolvida } {
  const doc = novoProjeto();
  doc.saida.largura = W;
  doc.saida.altura = H;
  doc.saida.fps = { num: 30, den: 1 };
  doc.sobreposicoes = [
    { id: "s1", nome: "", oculta, clipes: clipes.slice(0, 1) },
    { id: "s2", nome: "", oculta: false, clipes: clipes.slice(1) },
  ];
  const linha: LinhaResolvida = {
    fps: { num: 30, den: 1 },
    total_q: 120,
    clipes: [],
    livres: clipes.map((c, i) => ({ id: c.id, trilha: i === 0 ? "s1" : "s2", inicio_q: 10 * i, n_q: 60 })),
  };
  return { doc, linha };
}

const item = (doc: ProjetoEdicao, linha: LinhaResolvida, q: number, id = "t1"): LivreNoQuadro => {
  const x = livresNoQuadro(doc, linha, q, fontes).find((l) => l.clipe.id === id);
  if (!x) throw new Error(`sem ${id} em ${q}`);
  return x;
};
const alca = (it: LivreNoQuadro, nome: string) => {
  const a = alcasDoLivre({} as ProjetoEdicao, it, canvas).find((x) => x.id.endsWith(`:${nome}`));
  if (!a) throw new Error(nome);
  return a;
};
const novo = (m: Mudanca) => (m as { sobreposto: SobTitulo }).sobreposto;
const px = (p: { x: number; y: number }) => ({ x: p.x * W, y: p.y * H });
const mais = (p: { x: number; y: number }, dx: number, dy: number) => ({ x: p.x + dx / W, y: p.y + dy / H });
const fixo = (a: Animavel<number>) => a as number;
/** O item com o clipe trocado (como o palco depois de aplicar a mudança). */
function depois(doc: ProjetoEdicao, linha: LinhaResolvida, m: Mudanca, q: number): LivreNoQuadro {
  const d = structuredClone(doc);
  expect(aplicarMudanca(d, m)).toBe(true);
  return item(d, linha, q);
}

describe("livresNoQuadro", () => {
  it("só no intervalo do item, trilhas não ocultas de baixo para cima, sem vídeo nem desconhecido", () => {
    const img: SobImagem = { ...texto("i1"), tipo: "imagem", midia: "m1" } as unknown as SobImagem;
    const { doc, linha } = cena([texto("t1"), img]);
    doc.sobreposicoes[1]!.clipes.push({ tipo: "video", id: "v1" } as never, { tipo: "do_futuro", id: "x" } as never);
    expect(livresNoQuadro(doc, linha, 9, fontes).map((l) => l.clipe.id)).toEqual(["t1"]);
    expect(livresNoQuadro(doc, linha, 10, fontes).map((l) => l.clipe.id)).toEqual(["t1", "i1"]);
    expect(livresNoQuadro(doc, linha, 60, fontes).map((l) => l.clipe.id)).toEqual(["i1"]);
    expect(livresNoQuadro(doc, linha, 70, fontes)).toEqual([]);
    const oculta = cena([texto("t1"), img], true);
    expect(livresNoQuadro(oculta.doc, oculta.linha, 20, fontes).map((l) => l.clipe.id)).toEqual(["i1"]);
  });

  it("tempo local, transformação com a entrada e o sprite do bloco no meio do canvas", () => {
    const { doc, linha } = cena([texto("t1", { animacao: { entrada: "deslizar", saida: "aparecer", lado: "baixo" } })]);
    const l = item(doc, linha, 3);
    expect(l.tau).toBe(0.1);
    expect(l.transformacao).toEqual(transformacaoEm(l.clipe, 0.1, 2));
    expect(l.transformacao.y).toBeGreaterThan(0.75);
    expect(l.primitivas).toHaveLength(1);
    const s = l.primitivas[0] as Extract<LivreNoQuadro["primitivas"][number], { k: "sprite" }>;
    expect(s.k).toBe("sprite");
    expect(s.caixa.x * W + (s.caixa.w * W) / 2).toBeCloseTo(W / 2, 0);
    // vazio: sem desenho (o palco mostra só a caixa com as alças)
    const v = cena([texto("t1", {}, "")]);
    expect(item(v.doc, v.linha, 3).primitivas).toEqual([]);
  });
});

describe("alcasDoLivre", () => {
  const base = () => cena([texto("t1")]);

  it("texto livre: mover, quatro cantos, dois lados e giro; título de outro modelo e imagem: só mover", () => {
    const { doc, linha } = base();
    const it = item(doc, linha, 20);
    const as = alcasDoLivre(doc, it, canvas);
    expect(as.map((a) => [a.id.split(":").pop(), a.tipo])).toEqual([
      ["mover", "mover"], ["no", "canto"], ["ne", "canto"], ["se", "canto"], ["so", "canto"], ["o", "borda"], ["l", "borda"], ["giro", "giro"],
    ]);
    // a caixa da faixa: largura da caixa do texto (0,4 da largura + margens), centrada no texto
    const [no, , se] = cantosDoTexto(it, canvas).map(px) as [{ x: number; y: number }, unknown, { x: number; y: number }];
    expect(Math.abs(se.x - no.x - (0.4 * W + 2 * 0.012 * H))).toBeLessThanOrEqual(1);
    expect((no.x + se.x) / 2).toBeCloseTo(0.25 * W, 0);
    expect((no.y + se.y) / 2).toBeCloseTo(0.75 * H, 0);
    const outro = cena([{ ...texto("t1"), titulo: { ...texto("t1").titulo, modelo: "evento" } }]);
    expect(alcasDoLivre(outro.doc, item(outro.doc, outro.linha, 20), canvas).map((a) => a.tipo)).toEqual(["mover"]);
  });

  it("mover: o centro anda o arrasto, no pixel inteiro; Shift num eixo; fica no canvas", () => {
    const { doc, linha } = base();
    const it = item(doc, linha, 20);
    const a = alca(it, "mover");
    const m = a.arrastar(a.em, mais(a.em, 100.4, -50.2), { shift: false, alt: false });
    expect(m).toMatchObject({ livre: "t1", trilha: "s1" });
    const t = novo(m).transformacao;
    expect([fixo(t.x) * W, fixo(t.y) * H]).toEqual([0.25 * W + 100, 0.75 * H - 50]);
    const so = novo(a.arrastar(a.em, mais(a.em, 30, 200), { shift: true, alt: false })).transformacao;
    expect([fixo(so.x), fixo(so.y) * H]).toEqual([0.25, 0.75 * H + 200]);
    const fora = novo(a.arrastar(a.em, mais(a.em, -5000, 5000), { shift: false, alt: false })).transformacao;
    expect([fixo(fora.x), fixo(fora.y)]).toEqual([0, 1]);
    // o resto do clipe fica (mesmo id, mesmo estilo)
    expect(novo(m).titulo).toEqual((it.clipe as SobTitulo).titulo);
    expect(novo(m).id).toBe("t1");
    // o item do palco não muda (o arrasto parte sempre do começo do gesto)
    expect(it.clipe.transformacao.x).toBe(0.25);
  });

  it("mover durante o deslize grava o centro sem o deslocamento da entrada", () => {
    const { doc, linha } = cena([texto("t1", { animacao: { entrada: "deslizar", saida: "aparecer", lado: "dir" } })]);
    const it = item(doc, linha, 2);
    expect(it.transformacao.x).toBeGreaterThan(0.25);
    const a = alca(it, "mover");
    const t = novo(a.arrastar(a.em, mais(a.em, 10, 0), { shift: false, alt: false })).transformacao;
    expect(fixo(t.x) * W).toBeCloseTo(0.25 * W + 10, 6);
  });

  it("canto: tamanho da letra e tudo do estilo na mesma proporção, o canto oposto parado; Alt: o centro parado", () => {
    const { doc, linha } = cena([texto("t1", {}, "Policiais chegam")]);
    (doc.sobreposicoes[0]!.clipes[0] as SobTitulo).titulo.blocos[0]!.estilo.sombra = { ...SOMBRA };
    (doc.sobreposicoes[0]!.clipes[0] as SobTitulo).titulo.blocos[0]!.estilo.contorno = { cor: "#000000", largura: 0.003 };
    const it = item(doc, linha, 20);
    const se = alca(it, "se");
    const [no0, , se0] = cantosDoTexto(it, canvas).map(px);
    // arrasta o canto na diagonal até 1,5× da distância ao canto oposto
    const alvo = { x: (no0!.x + 1.5 * (se0!.x - no0!.x)) / W, y: (no0!.y + 1.5 * (se0!.y - no0!.y)) / H };
    const m = se.arrastar(se.em, alvo, { shift: false, alt: false });
    const b0 = (it.clipe as SobTitulo).titulo.blocos[0]!;
    const b1 = novo(m).titulo.blocos[0]!;
    expect(b1.estilo.tamanho / b0.estilo.tamanho).toBeCloseTo(1.5, 6);
    expect(b1.largura_max / b0.largura_max).toBeCloseTo(1.5, 6);
    expect(b1.estilo.caixa!.margem / b0.estilo.caixa!.margem).toBeCloseTo(1.5, 6);
    expect(b1.estilo.contorno!.largura).toBeCloseTo(0.0045, 9);
    expect(b1.estilo.sombra!.dy).toBeCloseTo(SOMBRA.dy * 1.5, 9);
    const [no1] = cantosDoTexto(depois(doc, linha, m, 20), canvas).map(px);
    // o canto oposto fica a até 1 px (o sprite arredonda ao pixel)
    expect(Math.abs(no1!.x - no0!.x)).toBeLessThanOrEqual(1.5);
    expect(Math.abs(no1!.y - no0!.y)).toBeLessThanOrEqual(1.5);
    const ma = se.arrastar(se.em, alvo, { shift: false, alt: true });
    const c1 = cantosDoTexto(depois(doc, linha, ma, 20), canvas).map(px);
    const c0 = cantosDoTexto(it, canvas).map(px);
    const meio = (c: { x: number; y: number }[]) => ({ x: (c[0]!.x + c[2]!.x) / 2, y: (c[0]!.y + c[2]!.y) / 2 });
    expect(Math.abs(meio(c1).x - meio(c0).x)).toBeLessThanOrEqual(1);
    expect(Math.abs(meio(c1).y - meio(c0).y)).toBeLessThanOrEqual(1);
    // nunca abaixo do mínimo
    const minimo = novo(se.arrastar(se.em, alca(it, "no").em, { shift: false, alt: false })).titulo.blocos[0]!;
    expect(minimo.estilo.tamanho).toBeGreaterThanOrEqual(0.008 - 1e-12);
  });

  it("lado: a largura da caixa; o lado oposto parado; Alt: os dois lados andam", () => {
    const { doc, linha } = base();
    const it = item(doc, linha, 20);
    const l = alca(it, "l");
    const [no0, ne0] = cantosDoTexto(it, canvas).map(px);
    const m = l.arrastar(l.em, mais(l.em, 192, 40), { shift: false, alt: false });
    expect(novo(m).titulo.blocos[0]!.largura_max).toBeCloseTo(0.4 + 0.1, 3);
    const [no1, ne1] = cantosDoTexto(depois(doc, linha, m, 20), canvas).map(px);
    expect(Math.abs(no1!.x - no0!.x)).toBeLessThanOrEqual(1);
    expect(Math.abs(ne1!.x - ne0!.x - 192)).toBeLessThanOrEqual(1);
    const ma = l.arrastar(l.em, mais(l.em, 96, 0), { shift: false, alt: true });
    expect(novo(ma).titulo.blocos[0]!.largura_max).toBeCloseTo(0.5, 3);
    expect(fixo(novo(ma).transformacao.x)).toBe(0.25);
    // girado 90°: o lado "direito" anda na vertical da tela
    const g = cena([texto("t1", { transformacao: { x: 0.5, y: 0.5, escala: 1, rotacao: 90, opacidade: 1 } })]);
    const ig = item(g.doc, g.linha, 20);
    const lg = alca(ig, "l");
    expect(px(lg.em).y).toBeGreaterThan(H / 2 + 100);
    expect(novo(lg.arrastar(lg.em, mais(lg.em, 0, 192), { shift: false, alt: false })).titulo.blocos[0]!.largura_max).toBeCloseTo(0.5, 3);
  });

  it("giro: em torno do centro da caixa; Shift de 15° em 15°", () => {
    const { doc, linha } = base();
    const it = item(doc, linha, 20);
    const g = alca(it, "giro");
    const c = { x: 0.25 * W, y: 0.75 * H };
    expect(px(g.em).y).toBeLessThan(c.y);
    // de cima do centro para a direita dele: 90° no sentido horário
    const m = g.arrastar(g.em, { x: (c.x + 300) / W, y: c.y / H }, { shift: false, alt: false });
    expect(fixo(novo(m).transformacao.rotacao)).toBeCloseTo(90, 1);
    const [a, , b] = cantosDoTexto(depois(doc, linha, m, 20), canvas).map(px);
    expect(Math.abs((a!.x + b!.x) / 2 - c.x)).toBeLessThanOrEqual(1);
    expect(Math.abs((a!.y + b!.y) / 2 - c.y)).toBeLessThanOrEqual(1);
    const s = g.arrastar(g.em, { x: (c.x + 300) / W, y: (c.y - 70) / H }, { shift: true, alt: false });
    expect(fixo(novo(s).transformacao.rotacao) % 15).toBe(0);
  });

  it("com chaves: grava a chave no instante do cursor e deixa as outras", () => {
    const x: Animavel<number> = [{ t: 0, v: 0.2 }, { t: 1.5, v: 0.6 }];
    const { doc, linha } = cena([texto("t1", { transformacao: { x, y: 0.5, escala: 1, rotacao: 0, opacidade: 1 } })]);
    const it = item(doc, linha, 15);
    const a = alca(it, "mover");
    const t = novo(a.arrastar(a.em, mais(a.em, 96, 0), { shift: false, alt: false })).transformacao;
    expect(Array.isArray(t.x)).toBe(true);
    const ks = t.x as { t: number; v: number }[];
    expect(ks.map((k) => k.t)).toEqual([0, 0.5, 1.5]);
    expect(ks[1]!.v * W).toBeCloseTo((0.2 + (0.4 * 0.5) / 1.5) * W + 96, 0);
    expect([ks[0]!.v, ks[2]!.v]).toEqual([0.2, 0.6]);
    expect(t.y).toBe(0.5);
  });

  it("clique dentro da caixa (girada) acha o texto; fora não", () => {
    const { doc, linha } = cena([texto("t1", { transformacao: { x: 0.5, y: 0.5, escala: 1, rotacao: 30, opacidade: 1 } })]);
    const it = item(doc, linha, 20);
    expect(dentroDoTexto(it, canvas, { x: 0.5, y: 0.5 })).toBe(true);
    const [no, ne] = cantosDoTexto(it, canvas);
    // perto do canto de cima à direita, por dentro e por fora (no eixo girado)
    const perto = (k: number) => ({ x: 0.5 + (ne!.x - 0.5) * k, y: 0.5 + (ne!.y - 0.5) * k });
    expect(dentroDoTexto(it, canvas, perto(0.95))).toBe(true);
    expect(dentroDoTexto(it, canvas, perto(1.05))).toBe(false);
    // o canto da caixa sem giro (ponto que o giro tirou da caixa) fica fora
    expect(dentroDoTexto(it, canvas, { x: no!.x, y: ne!.y })).toBe(false);
  });

  it("imagem e título de outro modelo: mover leva o centro", () => {
    const img = { ...texto("i1"), tipo: "imagem", midia: "m1" } as unknown as SobImagem;
    const { doc, linha } = cena([img]);
    const it = livresNoQuadro(doc, linha, 20, fontes)[0]!;
    const [a] = alcasDoLivre(doc, it, canvas);
    const m = a!.arrastar(a!.em, mais(a!.em, -20, 10), { shift: false, alt: false });
    expect(m).toMatchObject({ livre: "i1", trilha: "s1" });
    expect([fixo(novo(m).transformacao.x) * W, fixo(novo(m).transformacao.y) * H]).toEqual([0.25 * W - 20, 0.75 * H + 10]);
  });
});

describe("alças × render do motor (texto-caixa-render.json, dois ffmpeg)", () => {
  it("a caixa das alças é a que o motor pinta: centro, lados e ângulo a até 1 px, com o sprite de verdade", () => {
    type E = { clipe: SobTitulo; canvas: { w: number; h: number }; q: number; sprite: { w: number; h: number } };
    type S = { cx: number; cy: number; maior: number; menor: number; angulo: number };
    const lista = casos<E, S>("texto-caixa-render.json");
    expect(lista.length).toBeGreaterThanOrEqual(7);
    for (const { nome, entrada: e, saida: m } of lista) {
      const doc = novoProjeto();
      doc.saida.largura = e.canvas.w;
      doc.saida.altura = e.canvas.h;
      doc.sobreposicoes = [{ id: "s1", nome: "", oculta: false, clipes: [e.clipe] }];
      const linha: LinhaResolvida = { fps: { num: 30, den: 1 }, total_q: 30, clipes: [], livres: [{ id: e.clipe.id, trilha: "s1", inicio_q: 0, n_q: 30 }] };
      const f: FontesCena = { sprite: () => ({ relativo: "sprites/r.png", w: e.sprite.w, h: e.sprite.h }), atlas: () => null, midia: () => null };
      const it = livresNoQuadro(doc, linha, e.q, f)[0]!;
      const k = cantosDoTexto(it, e.canvas).map((p) => ({ x: p.x * e.canvas.w, y: p.y * e.canvas.h }));
      const [no, ne, , so] = k as [{ x: number; y: number }, { x: number; y: number }, unknown, { x: number; y: number }];
      const c = { x: (k[0]!.x + k[2]!.x) / 2, y: (k[0]!.y + k[2]!.y) / 2 };
      const lw = Math.hypot(ne.x - no.x, ne.y - no.y);
      const lh = Math.hypot(so.x - no.x, so.y - no.y);
      expect(Math.abs(c.x - m.cx), nome).toBeLessThanOrEqual(1);
      expect(Math.abs(c.y - m.cy), nome).toBeLessThanOrEqual(1);
      expect(Math.abs(Math.max(lw, lh) - m.maior), nome).toBeLessThanOrEqual(1);
      expect(Math.abs(Math.min(lw, lh) - m.menor), nome).toBeLessThanOrEqual(1);
      // o lado de cima da caixa no ângulo medido (momentos: −90..90; o giro de 90° dá ±90)
      const ang = (Math.atan2(ne.y - no.y, ne.x - no.x) * 180) / Math.PI;
      const dif = (((ang - m.angulo) % 180) + 180) % 180;
      expect(Math.min(dif, 180 - dif), nome).toBeLessThanOrEqual(0.5);
      // a alça "mover" fica no centro da caixa
      const mover = alcasDoLivre(doc, it, e.canvas).find((a) => a.tipo === "mover")!;
      expect(Math.hypot(mover.em.x * e.canvas.w - m.cx, mover.em.y * e.canvas.h - m.cy), nome).toBeLessThanOrEqual(1);
    }
  });
});
