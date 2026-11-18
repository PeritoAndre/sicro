/**
 * Gestos da linha (contratos-fase4.md D3, D5): mover na V1 por inserção, aparar pelo quadro exibido no slot (trecho a
 * 0,5×, para fora, sem fonte), lâmina sem fonte, livre → `pos` nos três casos (preso, preso com Alt, do programa),
 * entre trilhas e trilha nova, aparar livre e clipe de áudio, ímã.
 */

import { describe, expect, it } from "vitest";
import { produce } from "immer";
import type { ClipeAudio, ClipeTitulo, ClipeTrecho, ProjetoEdicao, SobTitulo, SobVideo } from "@domain/edicao";
import { resolver, tempoDoQuadro, ptsDaTabela } from "../modelo/tempo";
import { novaTrilhaNarracao, novoClipeAudio, novoTexto, novoTitulo } from "../inspetor/novos";
import { alvosDasTrilhas, alvosDoIma, aparar, apararLivre, destinoNaPrincipal, dividirSemFonte, imantar, moverLivre, moverNaPrincipal } from "./arrastar";
import { marcadoresNaLinha, trilhasDaLinha } from "./modelo";
import { midiaSintetica, projetoSintetico, textoLivre, trechoNosQuadros, trilhaS } from "./testeLinha";

function montar() {
  const { doc, tab, ctx } = projetoSintetico();
  const a = trechoNosQuadros("m1", tab, 0, 60);
  const b = trechoNosQuadros("m1", tab, 100, 160, 0.5);
  const t = novoTitulo(2);
  doc.principal.push(a, b, t);
  return { doc, tab, ctx, a, b, t };
}

const mudar = (doc: ProjetoEdicao, f: (d: ProjetoEdicao) => void) => produce(doc, (d) => void f(d as ProjetoEdicao));
const ids = (d: ProjetoEdicao) => d.principal.map((c) => (c as { id: string }).id);

describe("V1: mover por inserção", () => {
  it("o destino pelo meio dos outros clipes, como o arrasto da Sequência", () => {
    const { doc, ctx, a, b, t } = montar();
    const linha = resolver(doc, ctx);
    // a (0..60) solta depois do meio de b (60..180, meio 120): fica entre b e t
    expect(destinoNaPrincipal(doc, linha, a.id, 130)).toBe(1);
    expect(ids(mudar(doc, (d) => moverNaPrincipal(d, a.id, 1)))).toEqual([b.id, a.id, t.id]);
    // solto no fim
    expect(destinoNaPrincipal(doc, linha, a.id, 500)).toBe(2);
    expect(ids(mudar(doc, (d) => moverNaPrincipal(d, a.id, 2)))).toEqual([b.id, t.id, a.id]);
    // t para o começo
    expect(destinoNaPrincipal(doc, linha, t.id, 5)).toBe(0);
    // solto no próprio lugar: nada muda
    expect(destinoNaPrincipal(doc, linha, b.id, 100)).toBe(1);
  });

  it("item de versão futura no meio fica onde está entre os vizinhos", () => {
    const { doc, ctx, a, b, t } = montar();
    doc.principal.splice(1, 0, { tipo: "multicam", id: "x" } as never);
    const linha = resolver(doc, ctx);
    const para = destinoNaPrincipal(doc, linha, t.id, 70);
    expect(ids(mudar(doc, (d) => moverNaPrincipal(d, t.id, para)))).toEqual([a.id, "x", t.id, b.id]);
  });
});

describe("V1: aparar", () => {
  it("trecho a 0,5×: início e fim pelo quadro exibido no slot, com os pts da tabela", () => {
    const { doc, ctx, tab, b } = montar();
    const linha = resolver(doc, ctx);
    const pts = ptsDaTabela(tab);
    // b começa no slot 60: slot 70 mostra o quadro 105
    const d1 = mudar(doc, (d) => aparar(d, linha, b.id, "inicio", 70, ctx));
    const b1 = d1.principal[1] as ClipeTrecho;
    expect([b1.inicio_s, b1.inicio_pts]).toEqual([tempoDoQuadro(tab, 105), pts[105]]);
    expect([b1.fim_s, b1.fim_pts]).toEqual([b.fim_s, b.fim_pts]);
    // fim no slot 100 (exclusivo): o último slot 99 mostra o quadro 119 → fim no 120
    const d2 = mudar(doc, (d) => aparar(d, linha, b.id, "fim", 100, ctx));
    expect([(d2.principal[1] as ClipeTrecho).fim_s, (d2.principal[1] as ClipeTrecho).fim_pts]).toEqual([tempoDoQuadro(tab, 120), pts[120]]);
    expect(resolver(d2, ctx).clipes[1]!.n_q).toBe(40);
  });

  it("para fora do clipe segue pela velocidade (alargar); nunca vazio", () => {
    const { doc, ctx, tab, b } = montar();
    const linha = resolver(doc, ctx);
    // 20 slots antes a 0,5× = 10 quadros antes
    const d1 = mudar(doc, (d) => aparar(d, linha, b.id, "inicio", 40, ctx));
    expect((d1.principal[1] as ClipeTrecho).inicio_s).toBe(tempoDoQuadro(tab, 90));
    // além do fim da mídia: fica no fim (pts nulo)
    const d2 = mudar(doc, (d) => aparar(d, linha, b.id, "fim", 5000, ctx));
    expect((d2.principal[1] as ClipeTrecho).fim_pts).toBeNull();
    // início depois do fim: um quadro
    const d3 = mudar(doc, (d) => aparar(d, linha, b.id, "inicio", 900, ctx));
    expect(resolver(d3, ctx).clipes[1]!.n_q).toBe(2);
  });

  it("sem tabela: segundos pela velocidade, pts nulo (o Rust prende)", () => {
    const { doc, a } = montar();
    const sem = { tabela: () => null };
    const linha = resolver(doc, sem);
    const d1 = mudar(doc, (d) => aparar(d, linha, a.id, "fim", 45, sem));
    expect((d1.principal[0] as ClipeTrecho).fim_s).toBeCloseTo(1.5, 9);
    expect((d1.principal[0] as ClipeTrecho).fim_pts).toBeNull();
  });

  it("título: a duração (mín. 1 slot); o começo fica no lugar", () => {
    const { doc, ctx, t } = montar();
    const linha = resolver(doc, ctx);
    const ini = linha.clipes[2]!.inicio_q;
    const d1 = mudar(doc, (d) => aparar(d, linha, t.id, "fim", ini + 45, ctx));
    expect((d1.principal[2] as ClipeTitulo).duracao_s).toBe(1.5);
    const d2 = mudar(doc, (d) => aparar(d, linha, t.id, "inicio", ini + 15, ctx));
    expect((d2.principal[2] as ClipeTitulo).duracao_s).toBe(1.5);
    const d3 = mudar(doc, (d) => aparar(d, linha, t.id, "fim", ini - 30, ctx));
    expect(resolver(d3, ctx).clipes[2]!.n_q).toBe(1);
  });

  it("lâmina no título: duas cópias que somam a duração; os efeitos da segunda no tempo dela", () => {
    const { doc, ctx, t } = montar();
    const e = novoTexto();
    e.ativo = { de: 1.2, ate: null };
    t.efeitos.push(e);
    const linha = resolver(doc, ctx);
    let novo: string | null = null;
    const d1 = mudar(doc, (d) => void (novo = dividirSemFonte(d, linha, t.id, linha.clipes[2]!.inicio_q + 30)));
    expect(novo).not.toBeNull();
    const [x, y] = [d1.principal[2] as ClipeTitulo, d1.principal[3] as ClipeTitulo];
    expect([x.duracao_s, y.duracao_s, y.id === novo]).toEqual([1, 1, true]);
    expect((y.efeitos[0] as { ativo: unknown }).ativo).toEqual({ de: expect.closeTo(0.2, 9), ate: null });
    expect(resolver(d1, ctx).total_q).toBe(linha.total_q);
    // na ponta não divide
    expect(mudar(doc, (d) => void dividirSemFonte(d, linha, t.id, linha.clipes[2]!.inicio_q)).principal.length).toBe(3);
  });
});

describe("livres → pos (D5)", () => {
  function comLivre(pos: SobTitulo["pos"]) {
    const m = montar();
    const l = textoLivre(pos, 1);
    const s1 = trilhaS([l]);
    const s2 = trilhaS([]);
    m.doc.sobreposicoes.push(s1, s2);
    return { ...m, l, s1, s2, linha: resolver(m.doc, m.ctx) };
  }
  const livre = (d: ProjetoEdicao, id: string) => d.sobreposicoes.flatMap((t) => t.clipes).find((c) => (c as { id: string }).id === id) as SobTitulo;

  it("preso a clipe: passa ao clipe sob o novo começo; antes do começo, o primeiro; depois do fim, o último", () => {
    const { doc, a, b, t, l, linha, ctx } = comLivre({ ref: "clipe", clipe: "?", desloc: 0 });
    l.pos = { ref: "clipe", clipe: a.id, desloc: 0.5 };
    const d1 = mudar(doc, (d) => moverLivre(d, linha, l.id, 90, {}));
    expect(livre(d1, l.id).pos).toEqual({ ref: "clipe", clipe: b.id, desloc: 1 });
    expect(resolver(d1, ctx).livres[0]).toMatchObject({ inicio_q: 90 });
    const d2 = mudar(doc, (d) => moverLivre(d, linha, l.id, 400, {}));
    expect(livre(d2, l.id).pos).toEqual({ ref: "clipe", clipe: t.id, desloc: (400 - 180) / 30 });
    // duração não muda
    expect(livre(d2, l.id).duracao_s).toBe(1);
  });

  it("preso com Alt: fica no mesmo clipe, só o deslocamento (pode ser negativo ou passar do fim)", () => {
    const { doc, a, l, ctx } = comLivre({ ref: "clipe", clipe: "?", desloc: 0 });
    l.pos = { ref: "clipe", clipe: a.id, desloc: 0 };
    const linha = resolver(doc, ctx);
    const d1 = mudar(doc, (d) => moverLivre(d, linha, l.id, 150, { manterAncora: true }));
    expect(livre(d1, l.id).pos).toEqual({ ref: "clipe", clipe: a.id, desloc: 5 });
    const d2 = mudar(doc, (d) => moverLivre(d, linha, l.id, 0, { manterAncora: true }));
    expect(livre(d2, l.id).pos).toEqual({ ref: "clipe", clipe: a.id, desloc: 0 });
  });

  it("do programa: t = q·den/num (29,97)", () => {
    const { doc, ctx, l } = comLivre({ ref: "programa", t: 0 });
    doc.saida.fps = { num: 30000, den: 1001 };
    const linha = resolver(doc, ctx);
    const d1 = mudar(doc, (d) => moverLivre(d, linha, l.id, 77, {}));
    expect(livre(d1, l.id).pos).toEqual({ ref: "programa", t: (77 * 1001) / 30000 });
    expect(resolver(d1, ctx).livres[0]!.inicio_q).toBe(77);
  });

  it("para outra trilha (mesmo id) e para uma trilha nova acima da de cima", () => {
    const { doc, l, s1, s2, linha } = comLivre({ ref: "programa", t: 0 });
    const d1 = mudar(doc, (d) => moverLivre(d, linha, l.id, 10, { trilha: s2.id }));
    expect(d1.sobreposicoes.find((x) => x.id === s1.id)!.clipes).toHaveLength(0);
    expect((d1.sobreposicoes.find((x) => x.id === s2.id)!.clipes[0] as SobTitulo).id).toBe(l.id);
    const d2 = mudar(doc, (d) => moverLivre(d, linha, l.id, 10, { trilha: "nova" }));
    expect(d2.sobreposicoes).toHaveLength(3);
    expect(d2.sobreposicoes[2]).toMatchObject({ nome: "", oculta: false });
    expect((d2.sobreposicoes[2]!.clipes[0] as SobTitulo).id).toBe(l.id);
  });

  it("aparar: fim grava a duração ('até o fim do clipe' vira explícita); início anda e o fim fica", () => {
    const { doc, a, l, ctx } = comLivre({ ref: "programa", t: 0 });
    l.pos = { ref: "clipe", clipe: a.id, desloc: 0.5 };
    l.duracao_s = null;
    const linha = resolver(doc, ctx);
    expect(linha.livres[0]).toMatchObject({ inicio_q: 15, n_q: 45 });
    const d1 = mudar(doc, (d) => apararLivre(d, linha, l.id, "fim", 30));
    expect(livre(d1, l.id).duracao_s).toBe(0.5);
    const d2 = mudar(doc, (d) => apararLivre(d, linha, l.id, "inicio", 21));
    expect(livre(d2, l.id).pos).toEqual({ ref: "clipe", clipe: a.id, desloc: expect.closeTo(0.7, 9) });
    expect(livre(d2, l.id).duracao_s).toBeNull();
    expect(resolver(d2, ctx).livres[0]).toMatchObject({ inicio_q: 21, n_q: 39 });
  });

  it("vídeo sobreposto: aparar o início anda o começo da mídia junto (não antes do começo dela)", () => {
    const { doc, ctx, s1 } = comLivre({ ref: "programa", t: 0 });
    const v: SobVideo = { ...textoLivre({ ref: "programa", t: 1 }, 2), tipo: "video", midia: "m1", inicio_s: 0.5, inicio_pts: 7680, velocidade: { tipo: "constante", fator: 2 }, audio: null, moldura: null, sincronia: null } as unknown as SobVideo;
    s1.clipes.push(v);
    const linha = resolver(doc, ctx);
    const d1 = mudar(doc, (d) => apararLivre(d, linha, v.id, "inicio", 36));
    const v1 = livre(d1, v.id) as unknown as SobVideo;
    expect([v1.inicio_s, v1.inicio_pts, v1.duracao_s]).toEqual([0.9, null, expect.closeTo(1.8, 9)]);
    // não começa antes do começo da mídia (0): o mais perto em passos inteiros de slot
    const d2 = mudar(doc, (d) => apararLivre(d, linha, v.id, "inicio", 0));
    const ini = (livre(d2, v.id) as unknown as SobVideo).inicio_s;
    expect(ini).toBeGreaterThanOrEqual(0);
    expect(ini).toBeLessThan(2 / 30);
  });

  it("clipe de áudio: mesma regra do pos, entre trilhas de áudio; aparar corta o som", () => {
    const { doc, ctx, a } = montar();
    doc.midias.push({ ...midiaSintetica("n1", null), tipo: "audio", duracao_s: 4 });
    const t1 = novaTrilhaNarracao();
    const t2 = novaTrilhaNarracao();
    const c = novoClipeAudio("n1", "fala", { ref: "clipe", clipe: a.id, desloc: 0 });
    t1.clipes.push(c);
    doc.audios.push(t1, t2);
    const linha = resolver(doc, ctx);
    const aud = (d: ProjetoEdicao) => d.audios.flatMap((x) => x.clipes).find((x) => x.id === c.id) as ClipeAudio;
    const d1 = mudar(doc, (d) => moverLivre(d, linha, c.id, 75, { trilha: t2.id }));
    expect(aud(d1).pos).toMatchObject({ ref: "clipe", desloc: 0.5 });
    expect(d1.audios[1]!.clipes[0]!.id).toBe(c.id);
    const d2 = mudar(doc, (d) => apararLivre(d, linha, c.id, "inicio", 30));
    expect([aud(d2).de_s, aud(d2).pos]).toEqual([1, { ref: "clipe", clipe: a.id, desloc: 1 }]);
    const d3 = mudar(doc, (d) => apararLivre(d, linha, c.id, "fim", 500));
    expect(aud(d3).ate_s).toBe(4);
    const d4 = mudar(doc, (d) => apararLivre(d, linha, c.id, "inicio", -40));
    expect(aud(d4).de_s).toBe(0);
  });
});

describe("ímã", () => {
  it("o alvo mais perto a até o raio; fora dele, o próprio slot", () => {
    expect(imantar(103, [0, 100, 110], 4)).toBe(100);
    expect(imantar(106, [0, 100, 110], 4)).toBe(110);
    expect(imantar(105, [0, 100, 110], 4)).toBe(105);
    expect(imantar(50, [0, 100], 4)).toBe(50);
  });

  it("alvos: bordas dos itens (menos as do arrastado), losangos, marcadores e a cabeça", () => {
    const { doc, ctx, a, tab } = montar();
    doc.marcadores.push({ id: "mk", rotulo: "", cor: "#fff", pos: { ref: "programa", t: 7 } });
    const e = novoTexto();
    e.ativo = { de: tempoDoQuadro(tab, 20), ate: tempoDoQuadro(tab, 29) };
    a.efeitos.push(e);
    const linha = resolver(doc, ctx);
    const alvos = alvosDoIma(doc, linha, 333, a.id, ctx);
    expect(alvos).toEqual(alvosDasTrilhas(trilhasDaLinha(doc, linha, ctx), marcadoresNaLinha(doc, linha, ctx), 333, a.id));
    for (const q of [20, 30, 60, 180, 240, 210, 333]) expect(alvos).toContain(q);
    // as bordas do próprio a (0 e 60) só entram pelas dos vizinhos: o 0 sai
    expect(alvos).not.toContain(0);
  });
});
