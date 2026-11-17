/** Propriedades do §3.1 (as fixtures `tempo-*` do P0.3b conferem os bits em `espelhos.test.ts`). */

import { describe, expect, it } from "vitest";
import type { ClipeTitulo, ClipeTrecho, ProjetoEdicao, TabelaQuadros } from "@domain/edicao";
import {
  amostraDoSlot,
  doPar,
  exibido,
  faixaDoTrecho,
  fatorRacional,
  fimDoPar,
  indiceDoPts,
  instanteDoSlot,
  mapaTrecho,
  nQSemFonte,
  resolver,
  slotsDoQuadro,
  MAX_SLOTS,
  type MapaQuadros,
} from "./tempo";
import { novoProjeto, novoTrecho } from "./projeto";

const constante = (fator: number) => ({ tipo: "constante" as const, fator });
const fps = (num: number, den = 1) => ({ num, den });
/** Tabela de n quadros com passo fixo (ou a lista de passos). */
function tabela(tb: [number, number], pts0: number, passos: number[] | { n: number; passo: number }): TabelaQuadros {
  const p = Array.isArray(passos) ? passos : Array(passos.n - 1).fill(passos.passo);
  const mediano = [...p].sort((a, b) => a - b)[Math.floor(p.length / 2)] ?? 1;
  return { tb: { num: tb[0], den: tb[1] }, pts0, passos: p, passo_mediano: mediano, chaves: [0], vfr: false, origem: "decodificador" };
}
const contagem = (m: MapaQuadros) => {
  const c = new Map<number, number>();
  for (const j of m.quadro_do_slot) c.set(j, (c.get(j) ?? 0) + 1);
  return c;
};

describe("mapa de quadros (§3.1)", () => {
  const mp4 = tabela([1, 15360], 0, { n: 300, passo: 512 });
  // MKV de 1 ms a 30 qps: passos 33/34/33
  const mkv = tabela([1, 1000], 0, Array.from({ length: 299 }, (_, i) => Math.round(((i + 1) * 1000) / 30) - Math.round((i * 1000) / 30)));

  it("na própria taxa: 0 repetidos e 0 descartados, um quadro por slot", () => {
    for (const t of [mp4, mkv]) {
      const m = mapaTrecho(t, 10, 250, constante(1), fps(30));
      expect([m.n_q, m.repetidos, m.descartados]).toEqual([240, 0, 0]);
      expect(m.quadro_do_slot).toEqual(Array.from({ length: 240 }, (_, i) => 10 + i));
    }
  });

  it("a 0,5× e 0,25× todo quadro fica 2 e 4 slots", () => {
    for (const [f, k] of [
      [0.5, 2],
      [0.25, 4],
    ] as const) {
      for (const t of [mp4, mkv]) {
        const m = mapaTrecho(t, 0, 60, constante(f), fps(30));
        expect(m.n_q).toBe(60 * k);
        expect([...contagem(m).values()].every((v) => v === k)).toBe(true);
      }
    }
  });

  it("4×: continua o quadro exibido (0, 4, 8…); o slot 0 mostra sempre j_in", () => {
    const m = mapaTrecho(mp4, 7, 107, constante(4), fps(30));
    expect(m.quadro_do_slot.slice(0, 5)).toEqual([7, 11, 15, 19, 23]);
    expect(m.descartados).toBe(75);
    expect(mapaTrecho(mp4, 7, 8, constante(1), fps(30)).quadro_do_slot).toEqual([7]);
  });

  it("30 → 29,97 e 30 → 25: sem quadro fora de ordem; fim da mídia pelo passo mediano", () => {
    for (const f of [fps(30000, 1001), fps(25)]) {
      const m = mapaTrecho(mp4, 0, 300, constante(1), f);
      for (let q = 1; q < m.n_q; q++) expect(m.quadro_do_slot[q]! >= m.quadro_do_slot[q - 1]!).toBe(true);
      expect(m.quadro_do_slot.at(-1)).toBe(299);
    }
  });

  it("slotsDoQuadro", () => {
    const m = mapaTrecho(mp4, 0, 10, constante(0.5), fps(30));
    expect(slotsDoQuadro(m, 3)).toEqual([6, 2]);
    expect(slotsDoQuadro(m, 10)).toBeNull();
  });

  it("erros: velocidade fora, intervalo vazio, mapa grande", () => {
    expect(() => mapaTrecho(mp4, 0, 10, constante(0.01), fps(30))).toThrow("velocidade");
    expect(() => mapaTrecho(mp4, 10, 10, constante(1), fps(30))).toThrow("intervalo");
    // gravador em µs desde a época: P = pts·7 passa de 2^53
    const epoca = tabela([1, 1_000_000], 1_700_000_000_000_000, { n: 10, passo: 33_333 });
    expect(() => mapaTrecho(epoca, 0, 10, constante(1), fps(7))).toThrow("mapa_grande");
    expect(mapaTrecho(epoca, 0, 10, constante(1), fps(30000, 1001)).n_q).toBe(10);
    expect(() => mapaTrecho(mp4, 0, 10, { tipo: "rampa", pontos: [] }, fps(30))).toThrow("velocidade");
  });

  it("rampa 1× → 0,25× → 1× (P3.4): nada descartado, 4 slots no mais lento, a duração da integral", () => {
    const t = (j: number) => (j * 512) / 15360;
    const m = mapaTrecho(mp4, 30, 150, { tipo: "rampa", pontos: [ { t: t(60), v: 1 }, { t: t(90), v: 0.25 }, { t: t(120), v: 1 } ] }, fps(30));
    const tau = 2 + (2 * Math.log(4)) / 0.75;
    expect(m.n_q).toBe(Math.ceil(tau * 30 - 0.5));
    expect([m.quadro_do_slot[0], m.descartados, m.kd]).toEqual([30, 0, 0n]);
    expect(contagem(m).get(90)).toBe(4);
    expect(contagem(m).get(40)).toBe(1);
    // pontos suaves ou fora de ordem: erro de velocidade
    expect(() => mapaTrecho(mp4, 30, 150, { tipo: "rampa", pontos: [{ t: 1, v: 0.5, interp: "suave" }] }, fps(30))).toThrow("velocidade");
    expect(() => mapaTrecho(mp4, 30, 150, { tipo: "rampa", pontos: [ { t: 2, v: 1 }, { t: 1, v: 0.5 } ] }, fps(30))).toThrow("velocidade");
  });

  it("teto de slots (MAX_SLOTS do Rust): mapa_grande antes de alocar", () => {
    // 2 s a 10 milhões de qps = 2·10^7 slots, abaixo de 2^53 e acima de 2^24
    const dois = tabela([1, 1000], 0, [1000]);
    expect(MAX_SLOTS).toBe(2 ** 24);
    expect(() => mapaTrecho(dois, 0, 2, constante(1), fps(10_000_000))).toThrow("mapa_grande");
    expect(mapaTrecho(dois, 0, 2, constante(1), fps(8_000_000)).n_q).toBe(16_000_000);
  });

  it("fator por milésimos", () => {
    expect(fatorRacional(0.25)).toEqual({ num: 1, den: 4 });
    expect(fatorRacional(1.5)).toEqual({ num: 3, den: 2 });
    expect(fatorRacional(16)).toEqual({ num: 16, den: 1 });
    expect(() => fatorRacional(0.0505)).toThrow("velocidade");
  });
});

describe("tabela, slots e amostras", () => {
  const t = tabela([1, 90000], 126000, { n: 100, passo: 3003 });
  it("quadro exibido com folga de meio tick; pts e fim", () => {
    expect(exibido(t, 1.4)).toBe(0);
    expect(exibido(t, 0)).toBe(0);
    expect(exibido(t, (126000 + 3003 * 5) / 90000)).toBe(5);
    expect(exibido(t, (126000 + 3003 * 5) / 90000 - 0.4 / 90000)).toBe(5);
    expect(indiceDoPts(t, 126000 + 3003 * 7)).toBe(7);
    expect(indiceDoPts(t, 126001)).toBeNull();
    expect(fimDoPar(t, 999, null)).toBe(100);
    expect(faixaDoTrecho(t, 1.4, null, 999, null)).toEqual([0, 100]);
    expect(() => faixaDoTrecho(t, 2, null, 1, null)).toThrow("intervalo");
  });

  it("pts repetido (MPEG-PS): a ponta do trecho é o primeiro quadro do pts", () => {
    // quadros 0..5 com pts 0, 40, 40, 80, 120, 120
    const ps = tabela([1, 1000], 0, [40, 0, 40, 40, 0]);
    expect(exibido(ps, 0.05)).toBe(2);
    expect(doPar(ps, 0.05, null)).toBe(1);
    expect(doPar(ps, 0.05, 999)).toBe(1);
    expect(fimDoPar(ps, 0.13, null)).toBe(4);
    expect(faixaDoTrecho(ps, 0.05, null, 0.13, null)).toEqual([1, 4]);
    expect(() => mapaTrecho(ps, 2, 4, constante(1), fps(25))).toThrow("intervalo");
    expect(() => mapaTrecho(ps, 1, 5, constante(1), fps(25))).toThrow("intervalo");
    expect(mapaTrecho(ps, 1, 4, constante(1), fps(25)).n_q).toBe(2);
  });

  it("29,97: as fronteiras de amostra fecham exatas", () => {
    const f = fps(30000, 1001);
    expect(amostraDoSlot(30000, f)).toBe(48000 * 1001);
    expect(amostraDoSlot(1, f)).toBe(1602);
    expect(instanteDoSlot(30, fps(30))).toBe(1);
    expect(nQSemFonte(0, fps(30))).toBe(1);
    expect(nQSemFonte(2.5, fps(30))).toBe(75);
  });
});

describe("linha do tempo", () => {
  it("transição sobrepõe (menos escurecer e no primeiro clipe); livre preso a clipe", () => {
    const p: ProjetoEdicao = novoProjeto();
    p.saida.fps = fps(30);
    const tab = tabela([1, 15360], 0, { n: 900, passo: 512 });
    const t1 = novoTrecho("m1", 0, 10);
    t1.transicao_entrada = { tipo: "fundir", duracao_s: 1, audio: "acompanhar" };
    const titulo: ClipeTitulo = { tipo: "titulo", id: "tt", rotulo: "", desligado: false, transicao_entrada: { tipo: "fundir", duracao_s: 0.5, audio: "acompanhar" }, efeitos: [], duracao_s: 3, titulo: { modelo: "cartela", fundo: { tipo: "transparente" }, blocos: [] } };
    const t2: ClipeTrecho = { ...novoTrecho("m1", 20, 25), id: "t2", transicao_entrada: { tipo: "escurecer", duracao_s: 1, audio: "acompanhar" } };
    const desligado: ClipeTrecho = { ...novoTrecho("m1", 0, 1), desligado: true };
    p.principal = [t1, titulo, desligado, t2, { tipo: "do_futuro", id: "f" }];
    p.sobreposicoes = [{ id: "s", nome: "", oculta: false, clipes: [{ tipo: "imagem", id: "im", rotulo: "", midia: "x", pos: { ref: "clipe", clipe: "t2", desloc: 1 }, duracao_s: null, transformacao: { x: 0.5, y: 0.5, escala: 1, rotacao: 0, opacidade: 1 }, entrada_s: 0, saida_s: 0, efeitos: [] }] }];
    const l = resolver(p, { tabela: () => tab });
    expect(l.clipes.map((c) => [c.id, c.inicio_q, c.n_q, c.sobrepoe_q])).toEqual([
      [t1.id, 0, 300, 0],
      ["tt", 285, 90, 15],
      ["t2", 375, 150, 0],
    ]);
    expect(l.total_q).toBe(525);
    expect(l.clipes[0]!.fonte).toMatchObject({ j_in: 0, j_out: 300, lidos: 300, pts_in: 0, pts_out: 300 * 512 });
    expect(l.livres).toEqual([{ id: "im", trilha: "s", inicio_q: 405, n_q: 120 }]);
  });
});
