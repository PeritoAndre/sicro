/**
 * Faixas de efeito (contratos-fase4.md D2, D6): `ativo` ↔ slots de ida e volta num trecho a 0,5× (base fonte) e num
 * congelado (base local), borda do clipe = campo nulo (na ocultação da mídia, nunca), "Começar/Terminar no cursor".
 */

import { describe, expect, it } from "vitest";
import type { ClipeCongelar, Intervalo } from "@domain/edicao";
import { novoId } from "../modelo/projeto";
import { resolver, tempoDoQuadro } from "../modelo/tempo";
import { novoDestaque, novoEnquadramento, novoRastreio, novoTexto } from "../inspetor/novos";
import { cenaDoClipe, ativoEm } from "../palco/cena";
import { ativoDoArrasto, ativoNoCursor, chaveDaFaixa, chavesDoEfeito, slotsDoAtivo } from "./efeitos";
import { donoNaLinha, tempoDoSlot } from "./modelo";
import { projetoSintetico, trechoNosQuadros } from "./testeLinha";

function montar() {
  const { doc, tab, ctx } = projetoSintetico();
  const a = trechoNosQuadros("m1", tab, 0, 60);
  const b = trechoNosQuadros("m1", tab, 100, 160, 0.5);
  const cong: ClipeCongelar = { tipo: "congelar", id: novoId(), rotulo: "", desligado: false, transicao_entrada: null, efeitos: [], midia: "m1", instante_s: tempoDoQuadro(tab, 200), instante_pts: null, duracao_s: 3 };
  doc.principal.push(a, b, cong);
  return { doc, tab, ctx, a, b, cong, linha: resolver(doc, ctx) };
}

/** Slots do programa em que o efeito vale pela regra do palco (`ativoEm` no tempo do quadro exibido em cada slot). */
function slotsQueValem(m: ReturnType<typeof montar>, clipe: string, ativo: Intervalo | null): number[] {
  const d = donoNaLinha(m.doc, m.linha, clipe, m.ctx)!;
  const borda: [number, number] = d.base === "fonte" ? [(d.clipe as { inicio_s: number }).inicio_s, (d.clipe as { fim_s: number }).fim_s] : [0, (d.clipe as ClipeCongelar).duracao_s];
  const r: number[] = [];
  for (let s = 0; s < d.n_q; s++) if (ativoEm(ativo, tempoDoSlot(d, s), borda)) r.push(d.inicio_q + s);
  return r;
}

describe("ativo ↔ slots", () => {
  it("trecho a 0,5×: a barra arrastada vale exatamente nos slots dela (ida e volta), bordas nulas na ponta", () => {
    const m = montar();
    const ini = m.linha.clipes[1]!.inicio_q;
    for (const [de, ate] of [
      [ini + 10, ini + 30],
      [ini + 11, ini + 31],
      [ini, ini + 50],
      [ini + 60, ini + 120],
      [ini + 7, ini + 8],
    ] as const) {
      const at = ativoDoArrasto(m.doc, m.linha, m.b.id, de, ate, m.ctx);
      // o quadro se repete em dois slots: a barra fica no par inteiro (fronteira de quadro)
      const q0 = de - ((de - ini) % 2);
      const q1 = ate + ((ate - ini) % 2);
      expect(slotsDoAtivo(m.doc, m.linha, m.b.id, at, m.ctx)).toEqual([q0, q1]);
      expect(slotsQueValem(m, m.b.id, at)).toEqual(Array.from({ length: q1 - q0 }, (_, i) => q0 + i));
      if (de === ini) expect(at.de).toBeNull();
      if (ate === ini + 120) expect(at.ate).toBeNull();
    }
  });

  it("congelado (base local): ida e volta slot a slot", () => {
    const m = montar();
    const ini = m.linha.clipes[2]!.inicio_q;
    for (const [de, ate] of [
      [ini + 3, ini + 40],
      [ini + 1, ini + 2],
      [ini + 89, ini + 90],
    ] as const) {
      const at = ativoDoArrasto(m.doc, m.linha, m.cong.id, de, ate, m.ctx);
      expect(slotsDoAtivo(m.doc, m.linha, m.cong.id, at, m.ctx)).toEqual([de, ate]);
      expect(slotsQueValem(m, m.cong.id, at)).toEqual(Array.from({ length: ate - de }, (_, i) => de + i));
    }
    expect(ativoDoArrasto(m.doc, m.linha, m.cong.id, ini + 3, ini + 90, m.ctx)).toEqual({ de: 0.1, ate: null });
  });

  it("sempre dentro do clipe e com um slot; ocultação da mídia nunca nula", () => {
    const m = montar();
    const ini = m.linha.clipes[0]!.inicio_q;
    expect(ativoDoArrasto(m.doc, m.linha, m.a.id, ini - 50, ini + 999, m.ctx)).toEqual({ de: null, ate: null });
    expect(ativoDoArrasto(m.doc, m.linha, m.a.id, ini + 30, ini + 10, m.ctx)).toEqual({ de: tempoDoQuadro(m.tab, 30), ate: tempoDoQuadro(m.tab, 30) });
    expect(ativoDoArrasto(m.doc, m.linha, m.a.id, ini, ini + 60, m.ctx, { daMidia: true })).toEqual({ de: tempoDoQuadro(m.tab, 0), ate: tempoDoQuadro(m.tab, 59) });
  });

  it("o palco desenha a barra nos slots dela (cena do clipe com o efeito)", () => {
    const m = montar();
    const e = novoTexto();
    e.conteudo = "x";
    const ini = m.linha.clipes[1]!.inicio_q;
    e.ativo = ativoDoArrasto(m.doc, m.linha, m.b.id, ini + 20, ini + 40, m.ctx);
    m.b.efeitos.push(e);
    const d = donoNaLinha(m.doc, m.linha, m.b.id, m.ctx)!;
    const fontes = { sprite: () => ({ relativo: "s.png", w: 10, h: 10 }), atlas: () => null, midia: () => null };
    const desenha = (s: number) => cenaDoClipe(m.doc, m.b, { t: tempoDoSlot(d, s), pts: null, indice: null, q: ini + s, programa_s: 0 }, { w: 1280, h: 720 }, fontes).primitivas.length > 0;
    expect([19, 20, 39, 40].map(desenha)).toEqual([false, true, true, false]);
  });
});

describe("faixa e losangos", () => {
  it("chave da faixa: tipo + rastreio seguido, ou só o tipo", () => {
    const r = novoRastreio("m1", 0);
    expect(chaveDaFaixa(novoDestaque(r.id))).toBe(`destaque:${r.id}`);
    expect(chaveDaFaixa(novoTexto())).toBe("texto");
    const z = novoEnquadramento({ x: 0, y: 0, w: 1, h: 1 });
    expect(chaveDaFaixa(z)).toBe("enquadramento");
    expect(chaveDaFaixa({ ...z, segue: { rastreio: r.id, suavizar_s: 1 } })).toBe(`enquadramento:${r.id}`);
    expect(chaveDaFaixa({ tipo: "neblina" })).toBe("?neblina");
  });

  it("instantes das chaves: região do enquadramento e âncoras do rastreio seguido (em ordem, sem repetir)", () => {
    const m = montar();
    const r = novoRastreio("m1", 2);
    r.ancoras.push({ t: 1, pts: null, v: { x: 0, y: 0, w: 0, h: 0 } });
    m.doc.rastreios.push(r);
    expect(chavesDoEfeito(m.doc, novoDestaque(r.id))).toEqual([1, 2]);
    const z = { ...novoEnquadramento({ x: 0, y: 0, w: 1, h: 1 }), segue: { rastreio: r.id, suavizar_s: 1 } };
    z.regiao = [
      { t: 2, v: { x: 0, y: 0, w: 1, h: 1 } },
      { t: 3, v: { x: 0, y: 0, w: 1, h: 1 } },
    ];
    expect(chavesDoEfeito(m.doc, z)).toEqual([1, 2, 3]);
    expect(chavesDoEfeito(m.doc, { tipo: "neblina" })).toEqual([]);
  });
});

describe("Começar e Terminar no cursor", () => {
  const passo = 1 / 30;
  it("a borda vai ao cursor e a outra fica", () => {
    expect(ativoNoCursor(null, "de", 5, passo)).toEqual({ de: 5, ate: null });
    expect(ativoNoCursor(null, "ate", 5, passo)).toEqual({ de: null, ate: 5 });
    expect(ativoNoCursor({ de: 1, ate: 9 }, "de", 4, passo)).toEqual({ de: 4, ate: 9 });
    expect(ativoNoCursor({ de: 1, ate: 9 }, "ate", 4, passo)).toEqual({ de: 1, ate: 4 });
  });
  it("nunca cruza a outra: fica um quadro antes ou depois", () => {
    expect(ativoNoCursor({ de: 1, ate: 3 }, "de", 7, passo)).toEqual({ de: 3 - passo, ate: 3 });
    expect(ativoNoCursor({ de: 6, ate: 9 }, "ate", 2, passo)).toEqual({ de: 6, ate: 6 + passo });
    expect(ativoNoCursor({ de: 6, ate: null }, "de", 20, passo)).toEqual({ de: 20, ate: null });
  });
});
