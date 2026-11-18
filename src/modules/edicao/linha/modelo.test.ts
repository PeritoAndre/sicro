/**
 * Linha do tempo × documento × `resolver` (contratos-fase4.md D1, D2): a ordem das trilhas, os itens nos slots da
 * linha resolvida, o desconhecido apagado, as faixas de efeito pela chave e na ordem da primeira aparição.
 */

import { describe, expect, it } from "vitest";
import type { Desconhecido, EfOcultar, ProjetoEdicao } from "@domain/edicao";
import { novoId, novoTrecho } from "../modelo/projeto";
import { resolver, tempoDoQuadro } from "../modelo/tempo";
import { novoDestaque, novoEnquadramento, novoOcultar, novoRastreio, novoTexto, novoTitulo, novaTrilhaNarracao, novoClipeAudio } from "../inspetor/novos";
import { REGISTRO } from "../inspetor/registro";
import { bordaNoX, donoNaLinha, marcadoresNaLinha, ROTULO_DO_EFEITO, slotDoTempo, slotNoX, slotsNaBase, trilhasDaLinha, TRILHA_LEGENDAS, TRILHA_PRINCIPAL, TRILHA_SOM, xDoSlot } from "./modelo";
import { midiaSintetica, projetoSintetico, textoLivre, trechoNosQuadros, trilhaS } from "./testeLinha";

function montar() {
  const { doc, tab, ctx } = projetoSintetico();
  const a = trechoNosQuadros("m1", tab, 0, 60);
  const b = trechoNosQuadros("m1", tab, 100, 160, 0.5); // 60 quadros a 0,5× = 120 slots
  const t = novoTitulo(2); // 60 slots
  doc.principal.push(a, b, t);
  return { doc, tab, ctx, a, b, t };
}

describe("trilhas da linha", () => {
  it("ordem: sobreposições (a de cima primeiro), V1, faixas, Som, áudios na ordem, Legendas", () => {
    const { doc, ctx, a } = montar();
    const s1 = trilhaS([], "baixo");
    const s2 = trilhaS([], "cima");
    doc.sobreposicoes.push(s1, s2);
    const n1 = novaTrilhaNarracao();
    const n2 = { ...novaTrilhaNarracao(), papel: "musica" as const };
    doc.audios.push(n1, n2);
    const r = novoRastreio("m1", 1);
    r.nome = "PP1";
    r.cor = "#ef6b6b";
    doc.rastreios.push(r);
    a.efeitos.push(novoDestaque(r.id));
    const ts = trilhasDaLinha(doc, resolver(doc, ctx), ctx);
    expect(ts.map((t) => t.sigla ?? t.tipo)).toEqual(["S2", "S1", "V1", "efeito", "som", "A1", "A2", "legendas"]);
    expect(ts[0]!.id).toBe(s2.id);
    expect(ts.find((t) => t.tipo === "efeito")!.nome).toBe("PP1 · Destaque");
    expect(ts.find((t) => t.tipo === "efeito")!.cor).toBe("#ef6b6b");
    expect(ts.find((t) => t.id === n2.id)!.nome).toBe("Música");
    expect(ts.find((t) => t.id === TRILHA_SOM)!.nome).toBe("Som");
    expect(ts.find((t) => t.id === TRILHA_LEGENDAS)!.nome).toBe("Legendas");
  });

  it("V1 = clipes da linha resolvida, na ordem e nos slots dela", () => {
    const { doc, ctx } = montar();
    const linha = resolver(doc, ctx);
    const v1 = trilhasDaLinha(doc, linha, ctx).find((t) => t.id === TRILHA_PRINCIPAL)!;
    expect(v1.itens.map((i) => [i.id, i.de_q, i.ate_q])).toEqual(linha.clipes.map((c) => [c.id, c.inicio_q, c.inicio_q + c.n_q]));
    expect(v1.itens.map((i) => i.ate_q - i.de_q)).toEqual([60, 120, 60]);
  });

  it("desconhecido e desligado: marca apagada no lugar, sem tempo; efeito desconhecido apagado na faixa do tipo cru", () => {
    const { doc, ctx, a, b } = montar();
    const futuro: Desconhecido = { tipo: "multicam", id: "x1" };
    doc.principal.splice(1, 0, futuro as never);
    b.desligado = true;
    a.efeitos.push({ tipo: "neblina", id: "e9", ativo: { de: tempoDoQuadro(montar().tab, 10), ate: null } } as never);
    const linha = resolver(doc, ctx);
    const ts = trilhasDaLinha(doc, linha, ctx);
    const v1 = ts.find((t) => t.id === TRILHA_PRINCIPAL)!;
    const x = v1.itens.find((i) => i.id === "x1")!;
    expect(x).toMatchObject({ apagado: true, de_q: 60, ate_q: 60 });
    expect(v1.itens.find((i) => i.id === b.id)).toMatchObject({ desligado: true, de_q: 60, ate_q: 60 });
    const f = ts.find((t) => t.id === "faixa:?neblina")!;
    expect(f.itens[0]).toMatchObject({ id: "e9", apagado: true, de_q: 10, ate_q: 60 });
    // fora da linha: o resolver também não conta
    expect(linha.clipes.map((c) => c.id)).not.toContain(b.id);
  });

  it("barra do efeito: trecho a 0,5× (fonte) e título (local), ativo nas bordas = clipe inteiro", () => {
    const { doc, ctx, tab, b, t } = montar();
    const e1 = novoTexto();
    e1.ativo = { de: tempoDoQuadro(tab, 110), ate: tempoDoQuadro(tab, 119) };
    b.efeitos.push(e1);
    const e2 = novoTexto();
    e2.ativo = { de: 0.5, ate: 1 };
    t.efeitos.push(e2);
    const e3 = novoTexto();
    t.efeitos.push(e3);
    const ts = trilhasDaLinha(doc, resolver(doc, ctx), ctx);
    const f = ts.find((x) => x.id === "faixa:texto")!;
    // b começa no slot 60; o quadro 110 aparece nos slots 20 e 21 dele, o 119 nos 38 e 39
    expect(f.itens.find((i) => i.id === e1.id)).toMatchObject({ de_q: 80, ate_q: 100, clipe: b.id });
    // título a partir do slot 180: 0,5 s = slot 15, 1 s = slot 30 (inclusivo)
    expect(f.itens.find((i) => i.id === e2.id)).toMatchObject({ de_q: 195, ate_q: 211 });
    expect(f.itens.find((i) => i.id === e3.id)).toMatchObject({ de_q: 180, ate_q: 240 });
    // e2 e e3 se cruzam: duas raias
    expect(f.raias).toBe(2);
  });

  it("faixa = tipo + rastreio seguido, na ordem da primeira aparição; ocultação da mídia em todo clipe da mídia", () => {
    const { doc, ctx, tab, a, b } = montar();
    const r1 = novoRastreio("m1", 1);
    r1.nome = "PP1";
    const r2 = novoRastreio("m1", 1);
    r2.nome = "PP2";
    doc.rastreios.push(r1, r2);
    a.efeitos.push(novoDestaque(r2.id), novoDestaque(r1.id));
    b.efeitos.push(novoDestaque(r1.id), novoEnquadramento({ x: 0, y: 0, w: 0.5, h: 0.5 }));
    const o = novoOcultar(r1.id, { de: tempoDoQuadro(tab, 50), ate: tempoDoQuadro(tab, 120) }) as EfOcultar;
    doc.midias[0]!.ocultacoes.push(o);
    const ts = trilhasDaLinha(doc, resolver(doc, ctx), ctx);
    const faixas = ts.filter((t) => t.tipo === "efeito");
    expect(faixas.map((f) => f.nome)).toEqual(["PP2 · Destaque", "PP1 · Destaque", "PP1 · Ocultar", "Enquadramento"]);
    expect(faixas[1]!.itens.map((i) => i.clipe)).toEqual([a.id, b.id]);
    const oc = faixas[2]!.itens;
    // em a: quadros 50..59 (o resto passa da borda: cortada à direita); em b: quadros 100..120 a 0,5×
    expect(oc.map((i) => [i.clipe, i.de_q, i.ate_q, i.daMidia, i.cortado])).toEqual([
      [a.id, 50, 60, true, { de: false, ate: true }],
      [b.id, 60, 102, true, { de: true, ate: false }],
    ]);
  });

  it("losangos: chaves da região do enquadramento e âncoras do rastreio seguido, dentro do clipe", () => {
    const { doc, ctx, tab, a } = montar();
    const r = novoRastreio("m1", tempoDoQuadro(tab, 10));
    r.ancoras.push({ t: tempoDoQuadro(tab, 40), pts: null, v: { x: 0, y: 0, w: 0.1, h: 0.1 } }, { t: tempoDoQuadro(tab, 200), pts: null, v: { x: 0, y: 0, w: 0.1, h: 0.1 } });
    doc.rastreios.push(r);
    a.efeitos.push(novoDestaque(r.id));
    const z = novoEnquadramento({ x: 0, y: 0, w: 1, h: 1 });
    z.regiao = [
      { t: tempoDoQuadro(tab, 5), v: { x: 0, y: 0, w: 1, h: 1 } },
      { t: tempoDoQuadro(tab, 30), v: { x: 0.2, y: 0.2, w: 0.5, h: 0.5 } },
    ];
    a.efeitos.push(z);
    const ts = trilhasDaLinha(doc, resolver(doc, ctx), ctx);
    expect(ts.find((t) => t.nome.endsWith("Destaque"))!.itens[0]!.chaves).toEqual([10, 40]);
    expect(ts.find((t) => t.nome === "Enquadramento")!.itens[0]!.chaves).toEqual([5, 30]);
  });

  it("sobreposições e áudio nos slots dos livres resolvidos; Som só de trecho com mídia com som", () => {
    const { doc, ctx, a } = montar();
    const l = textoLivre({ ref: "clipe", clipe: a.id, desloc: 0.5 }, 1);
    doc.sobreposicoes.push(trilhaS([l]));
    const tn = novaTrilhaNarracao();
    doc.midias.push({ ...midiaSintetica("n1", null), tipo: "audio", duracao_s: 3 });
    tn.clipes.push(novoClipeAudio("n1", "fala", { ref: "programa", t: 2 }));
    doc.audios.push(tn);
    const linha = resolver(doc, ctx);
    const ts = trilhasDaLinha(doc, linha, ctx);
    expect(ts[0]!.itens[0]).toMatchObject({ id: l.id, k: "livre", de_q: 15, ate_q: 45 });
    expect(ts.find((t) => t.id === tn.id)!.itens[0]).toMatchObject({ k: "audio", de_q: 60, ate_q: 150 });
    expect(ts.find((t) => t.id === TRILHA_SOM)!.itens.map((i) => i.id)).toEqual(linha.clipes.filter((c) => c.fonte).map((c) => c.id));
    doc.midias[0]!.tem_audio = false;
    expect(trilhasDaLinha(doc, resolver(doc, ctx), ctx).find((t) => t.id === TRILHA_SOM)!.itens).toEqual([]);
  });

  it("sem tabela: slots pelos segundos (o mesmo resolver da Sequência)", () => {
    const { doc } = montar();
    const sem = { tabela: () => null };
    const linha = resolver(doc, sem);
    const v1 = trilhasDaLinha(doc, linha, sem).find((t) => t.id === TRILHA_PRINCIPAL)!;
    expect(v1.itens.map((i) => i.ate_q - i.de_q)).toEqual(linha.clipes.map((c) => c.n_q));
  });

  it("rótulos dos tipos = os do registro do Inspetor", () => {
    for (const [tipo, rot] of Object.entries(ROTULO_DO_EFEITO)) expect(REGISTRO[tipo as keyof typeof REGISTRO].rotulo === rot || tipo === "enquadramento").toBe(true);
  });
});

describe("conversões", () => {
  it("slot sob o x e x do slot (ida e volta)", () => {
    const fps = { num: 30000, den: 1001 };
    for (const q of [0, 1, 29, 30, 1799, 54000]) {
      const x = xDoSlot(q, 14, fps);
      expect(slotNoX(x + 0.01, 14, 0, fps)).toBe(q);
      expect(bordaNoX(x, 14, 0, fps)).toBe(q);
    }
    expect(slotNoX(140, 14, 2, { num: 30, den: 1 })).toBe(360);
    expect(slotNoX(-50, 14, 0, { num: 30, den: 1 })).toBe(0);
  });

  it("slot do tempo e slots da base (trecho a 0,5×)", () => {
    const { doc, ctx, tab, b } = montar();
    const linha = resolver(doc, ctx);
    const d = donoNaLinha(doc, linha, b.id, ctx)!;
    expect(slotDoTempo(d, tempoDoQuadro(tab, 100))).toBe(0);
    expect(slotDoTempo(d, tempoDoQuadro(tab, 101))).toBe(2);
    expect(slotDoTempo(d, tempoDoQuadro(tab, 99))).toBeNull();
    expect(slotDoTempo(d, tempoDoQuadro(tab, 160))).toBeNull();
    expect(slotsNaBase(d, null)).toEqual([60, 180]);
    expect(slotsNaBase(d, { de: tempoDoQuadro(tab, 200), ate: null })).toBeNull();
  });

  it("marcadores do programa, preso a clipe e de fonte (um por trecho que mostra o instante)", () => {
    const { doc, ctx, tab, a } = montar();
    const c2 = trechoNosQuadros("m1", tab, 40, 80);
    doc.principal.push(c2);
    doc.marcadores.push(
      { id: "m-p", rotulo: "", cor: "#fff", pos: { ref: "programa", t: 1 } },
      { id: "m-c", rotulo: "", cor: "#fff", pos: { ref: "clipe", clipe: a.id, desloc: 0.5 } },
      { id: "m-f", rotulo: "", cor: "#fff", pos: { ref: "fonte", midia: "m1", t: tempoDoQuadro(tab, 50) } },
    );
    const linha = resolver(doc, ctx);
    const ms = marcadoresNaLinha(doc, linha, ctx);
    const c2q = linha.clipes.find((c) => c.id === c2.id)!.inicio_q;
    expect(ms.map((m) => [m.id, m.q])).toEqual([
      ["m-p", 30],
      ["m-c", 15],
      ["m-f", 50],
      ["m-f", c2q + 10],
    ]);
  });
});

describe("o documento não muda", () => {
  it("trilhasDaLinha sobre documento congelado", () => {
    const { doc, ctx } = montar();
    const congelar = (o: unknown): void => {
      if (o && typeof o === "object" && !Object.isFrozen(o)) {
        Object.freeze(o);
        for (const v of Object.values(o)) congelar(v);
      }
    };
    doc.principal.push({ ...novoTrecho("m1", 0, 1), id: novoId() });
    congelar(doc);
    expect(() => trilhasDaLinha(doc as ProjetoEdicao, resolver(doc, ctx), ctx)).not.toThrow();
  });
});
