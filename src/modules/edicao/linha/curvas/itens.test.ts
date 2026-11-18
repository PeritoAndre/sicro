/**
 * Onde as curvas moram (P4.2, D7): leitura e gravação no documento, e a base de tempo das chaves em slots (local no
 * sobreposto e no clipe de áudio; fonte no som do trecho, pelo mapa de quadros do motor, a 0,5× e sem tabela).
 */

import { describe, expect, it } from "vitest";
import { produce } from "immer";
import type { Caixa, Chave, ClipeTrecho, EfEnquadramento, LinhaResolvida, ProjetoEdicao, SobTitulo, TabelaQuadros } from "@domain/edicao";
import { lerFixture } from "../../inspetor/testeTela";
import { mapaTrecho, ptsDaTabela, resolver, tempoDoQuadro } from "../../modelo/tempo";
import { moverChave } from "./chaves";
import { baseDoItem, comPtsDoSlot, cursorNaChave, cursorNoSlot, gravarProp, instanteNoItem, lerProp, propsDe, ROTULO_DA_PROP } from "./itens";

const p0 = lerFixture<ProjetoEdicao>("projeto-do-store.json");
const ids = p0.principal.map((c) => (c as { id: string }).id);

function tabela(n: number, passo: number, tb: [number, number] = [1, 15360]): TabelaQuadros {
  return { tb: { num: tb[0], den: tb[1] }, pts0: 0, passos: Array(n - 1).fill(passo), passo_mediano: passo, chaves: [0], vfr: false, origem: "decodificador" };
}

/** O projeto com um texto livre preso ao segundo trecho (0,5 s) e um clipe de áudio no programa (2 s). */
function projeto(): ProjetoEdicao {
  return produce(p0, (d) => {
    d.sobreposicoes = [
      {
        id: "s1",
        nome: "",
        oculta: false,
        clipes: [
          {
            tipo: "titulo",
            id: "tl",
            rotulo: "",
            pos: { ref: "clipe", clipe: ids[2]!, desloc: 0.5 },
            duracao_s: 2,
            transformacao: { x: [{ t: 0, v: 0.2, interp: "suave" }, { t: 1, v: 0.6 }], y: 0.5, escala: 1, rotacao: [{ t: 0.5, v: 10 }], opacidade: 1 },
            entrada_s: 0,
            saida_s: 0,
            efeitos: [],
            titulo: { modelo: "livre", fundo: { tipo: "transparente" }, blocos: [] },
          } as SobTitulo,
        ],
      },
    ];
    d.audios[0]!.clipes = [{ id: "n1", nome: "", midia: "m3", pos: { ref: "programa", t: 2 }, de_s: 0, ate_s: 3, volume: 0.7, entrada_s: 0, saida_s: 0 }];
  });
}

describe("ler e gravar", () => {
  const p = projeto();
  it("propriedades de cada item e as curvas delas (posição = x e y)", () => {
    expect(propsDe(p, { k: "livre", id: "tl" })).toEqual(["posicao", "rotacao", "escala", "opacidade"]);
    expect(propsDe(p, { k: "audio", id: "n1" })).toEqual(["volume"]);
    expect(propsDe(p, { k: "som", id: ids[1]! })).toEqual(["volume"]);
    expect(propsDe(p, { k: "som", id: ids[0]! })).toEqual([]);
    expect(propsDe(p, { k: "livre", id: "nada" })).toEqual([]);
    expect(lerProp(p, { k: "livre", id: "tl" }, "posicao")).toEqual([[{ t: 0, v: 0.2, interp: "suave" }, { t: 1, v: 0.6 }], 0.5]);
    expect(lerProp(p, { k: "livre", id: "tl" }, "volume")).toBeNull();
    expect(lerProp(p, { k: "audio", id: "n1" }, "volume")).toEqual([0.7]);
    expect(lerProp(p, { k: "som", id: ids[1]! }, "volume")).toEqual([1]);
    expect(Object.values(ROTULO_DA_PROP)).toEqual(["Posição", "Giro", "Escala", "Opacidade", "Volume", "Enquadramento"]);
  });

  it("grava no lugar certo e só ali", () => {
    const q = produce(p, (d) => {
      gravarProp({ k: "livre", id: "tl" }, "posicao", [0.1, [{ t: 0, v: 0.9 }]])(d);
      gravarProp({ k: "livre", id: "tl" }, "opacidade", [[{ t: 1, v: 0.5 }]])(d);
      gravarProp({ k: "audio", id: "n1" }, "volume", [[{ t: 0.5, v: 0 }]])(d);
      gravarProp({ k: "som", id: ids[1]! }, "volume", [0.25])(d);
    });
    const t = q.sobreposicoes[0]!.clipes[0] as SobTitulo;
    expect(t.transformacao).toEqual({ x: 0.1, y: [{ t: 0, v: 0.9 }], escala: 1, rotacao: [{ t: 0.5, v: 10 }], opacidade: [{ t: 1, v: 0.5 }] });
    expect(q.audios[0]!.clipes[0]!.volume).toEqual([{ t: 0.5, v: 0 }]);
    expect((q.principal[1] as ClipeTrecho).audio.volume).toBe(0.25);
    expect((q.principal[2] as ClipeTrecho).audio.volume).toBe(1);
  });
});

describe("base das chaves em slots", () => {
  const p = projeto();
  const sem = () => null;
  const linha = resolver(p, { tabela: sem });

  it("sobreposto e clipe de áudio: tempo local (τ = (q − início)/fps), ida e volta", () => {
    // título 90 slots + trecho 105 + 15 de deslocamento
    const b = baseDoItem(p, linha, { k: "livre", id: "tl" }, sem)!;
    expect([b.de_q, b.ate_q]).toEqual([210, 270]);
    expect(b.deSlot(210)).toBe(0);
    expect(b.deSlot(240)).toBe(1);
    for (const q of [210, 211, 239, 269]) expect(b.paraSlot(b.deSlot(q))).toBe(q);
    const a = baseDoItem(p, linha, { k: "audio", id: "n1" }, sem)!;
    expect([a.de_q, a.ate_q, a.deSlot(75)]).toEqual([60, 150, 0.5]);
  });

  it("som do trecho: tempo de FONTE do quadro mostrado no slot, também a 0,5× (dois slots por quadro)", () => {
    const tab = tabela(600, 512);
    const lento = produce(p, (d) => void ((d.principal[1] as ClipeTrecho).velocidade = { tipo: "constante", fator: 0.5 }));
    const l = resolver(lento, { tabela: () => tab });
    const r = l.clipes[1]!;
    expect(r.n_q).toBe(210);
    const b = baseDoItem(lento, l, { k: "som", id: ids[1]! }, () => tab)!;
    const mapa = mapaTrecho(tab, r.fonte!.j_in, r.fonte!.j_out, { tipo: "constante", fator: 0.5 }, l.fps);
    for (const k of [0, 1, 2, 3, 101, 209]) expect(b.deSlot(r.inicio_q + k)).toBe(tempoDoQuadro(tab, mapa.quadro_do_slot[k]!));
    expect(b.deSlot(r.inicio_q)).toBe(12);
    expect(b.deSlot(r.inicio_q + 1)).toBe(12);
    expect(b.deSlot(r.inicio_q + 2)).toBeCloseTo(12 + 1 / 30, 12);
    // a volta dá o primeiro slot do quadro
    expect(b.paraSlot(12 + 1 / 30)).toBe(r.inicio_q + 2);
    expect(b.paraSlot(5)).toBe(r.inicio_q);
    expect(b.paraSlot(99)).toBe(r.inicio_q + r.n_q - 1);
    // sem tabela: pela velocidade
    const est = baseDoItem(lento, resolver(lento, { tabela: sem }), { k: "som", id: ids[1]! }, sem)!;
    expect(est.deSlot(est.de_q + 30)).toBeCloseTo(12.5, 12);
    expect(est.paraSlot(13)).toBe(est.de_q + 60);
  });

  it("cursor no item: dentro, fora, no programa e no clipe; ir para a chave volta ao modo do cursor", () => {
    const b = baseDoItem(p, linha, { k: "livre", id: "tl" }, sem)!;
    const tl = { k: "livre" as const, id: "tl" };
    expect(instanteNoItem(p, linha, tl, b, { modo: "programa", q: 240 })).toEqual({ t: 1, dentro: true });
    expect(instanteNoItem(p, linha, tl, b, { modo: "programa", q: 100 })).toEqual({ t: 0, dentro: false });
    expect(instanteNoItem(p, linha, tl, b, { modo: "programa", q: 400 })).toEqual({ t: 59 / 30, dentro: false });
    // no clipe: o trecho 2 começa no slot 195 (fonte 3 s); 3,5 s = slot 210 = τ 0
    expect(instanteNoItem(p, linha, tl, b, { modo: "clipe", clipe: ids[2]!, t: 3.5 })).toEqual({ t: 0, dentro: true });
    const som = { k: "som" as const, id: ids[1]! };
    const bs = baseDoItem(p, linha, som, sem)!;
    expect(instanteNoItem(p, linha, som, bs, { modo: "clipe", clipe: ids[1]!, t: 13 })).toEqual({ t: 13, dentro: true });
    expect(instanteNoItem(p, linha, som, bs, { modo: "clipe", clipe: ids[1]!, t: 20 })).toEqual({ t: 15.5, dentro: false });
    expect(instanteNoItem(p, linha, som, bs, { modo: "clipe", clipe: ids[2]!, t: 4 })).toEqual({ t: 12, dentro: false });
    expect(instanteNoItem(p, linha, som, bs, { modo: "programa", q: 120 }).dentro).toBe(true);
    // ir para: programa → slot; clipe → o clipe sob o slot no tempo dele
    expect(cursorNoSlot(p, linha, 240, { modo: "programa", q: 0 }, sem)).toEqual({ modo: "programa", q: 240 });
    expect(cursorNoSlot(p, linha, 240, { modo: "clipe", clipe: ids[0]!, t: 0 }, sem)).toEqual({ modo: "clipe", clipe: ids[2], t: 3 + 45 / 30 });
    expect(cursorNoSlot(p, linha, 30, { modo: "clipe", clipe: ids[1]!, t: 13 }, sem)).toEqual({ modo: "clipe", clipe: ids[0], t: 1 });
    expect(cursorNaChave(p, linha, som, bs, 13.25, { modo: "clipe", clipe: ids[1]!, t: 12 }, sem)).toEqual({ modo: "clipe", clipe: ids[1], t: 13.25 });
    expect(cursorNaChave(p, linha, tl, b, 1, { modo: "programa", q: 0 }, sem)).toEqual({ modo: "programa", q: 240 });
  });

  it("item fora da linha: sem base", () => {
    const vazia: LinhaResolvida = { fps: { num: 30, den: 1 }, total_q: 0, clipes: [], livres: [] };
    expect(baseDoItem(p, vazia, { k: "livre", id: "tl" }, sem)).toBeNull();
    expect(baseDoItem(p, vazia, { k: "som", id: ids[1]! }, sem)).toBeNull();
  });
});

describe("região do zoom na barra do enquadramento (chaves do ZS)", () => {
  const tab = tabela(600, 512);
  const pts = ptsDaTabela(tab);
  const ch = (j: number, v: Caixa, interp?: Chave<Caixa>["interp"]): Chave<Caixa> => ({ t: tempoDoQuadro(tab, j), pts: pts[j]!, v, ...(interp ? { interp } : {}) });
  const inteira = { x: 0, y: 0, w: 1, h: 1 };
  const perto = { x: 0.3, y: 0.3, w: 0.4, h: 0.4 };
  const p = produce(p0, (d) => {
    const t = d.principal[1] as ClipeTrecho;
    t.efeitos = [{ tipo: "enquadramento", id: "z1", ligado: true, nome: "", ativo: null, regiao: [ch(372, inteira, "suave"), ch(390, perto), ch(420, inteira)], ajuste: "cobrir", interpolacao: "bilinear" } as EfEnquadramento];
  });
  const linha = resolver(p, { tabela: () => tab });
  const z = { k: "efeito" as const, id: "z1" };

  it("a base é a do clipe dono (fonte, pelo mapa); mover a chave leva o pts do quadro novo; a caixa não muda", () => {
    expect(propsDe(p, z)).toEqual(["regiao"]);
    expect(ROTULO_DA_PROP.regiao).toBe("Enquadramento");
    const b = baseDoItem(p, linha, z, () => tab)!;
    expect([b.de_q, b.ate_q]).toEqual([90, 195]);
    // trecho de 12 s (quadro 360) a 1×: o quadro 390 fica no slot 90 + 30
    const ks = lerProp(p, z, "regiao")![0] as unknown as Chave<Caixa>[];
    expect(ks.map((k) => b.paraSlot(k.t))).toEqual([102, 120, 150]);
    const movida = moverChave(ks, 1, b.deSlot(130));
    expect(movida[1]!.pts).toBeUndefined();
    const comPts = comPtsDoSlot(ks, movida, b) as Chave<Caixa>[];
    expect(comPts[1]).toEqual({ t: tempoDoQuadro(tab, 400), pts: pts[400], v: perto });
    expect([comPts[0], comPts[2]]).toEqual([ks[0], ks[2]]);
    const q = produce(p, (d) => gravarProp(z, "regiao", [comPts as never])(d));
    expect(((q.principal[1] as ClipeTrecho).efeitos[0] as EfEnquadramento).regiao).toEqual(comPts);
    // chave sem pts (volume) segue sem
    expect(comPtsDoSlot([{ t: 13, v: 1 }], [{ t: 13.5, v: 1 }], b)).toEqual([{ t: 13.5, v: 1 }]);
    // cursor no clipe dono: instante de fonte; ir para a chave fica no clipe
    expect(instanteNoItem(p, linha, z, b, { modo: "clipe", clipe: ids[1]!, t: 13 })).toEqual({ t: 13, dentro: true });
    expect(cursorNaChave(p, linha, z, b, 13, { modo: "clipe", clipe: ids[0]!, t: 0 }, () => tab)).toEqual({ modo: "clipe", clipe: ids[1], t: 13 });
  });
});
