/** Espelho da tabela do clipe (`realce::tabela`, P1.4) × fixture do Rust, e o resolver usando a tabela de campos. */

import { describe, expect, it } from "vitest";
import type { Efeito, TabelaQuadros } from "@domain/edicao";
import { novoProjeto, novoTrecho } from "./projeto";
import { campoCabe, porCampo, tabelaDoClipe } from "./realce";
import { indiceDoPts, nQuadros, ptsDaTabela, resolver } from "./tempo";
import { casos, conferirCasos } from "./testeFixtures";

const campo = (ligado = true): Efeito => ({ tipo: "desentrelacar", id: "d1", ligado, nome: "", ativo: null, modo: "campo", ordem: "auto" });
const ntsc = (n: number): TabelaQuadros => ({ tb: { num: 1, den: 30000 }, pts0: 0, passos: Array<number>(n - 1).fill(1001), passo_mediano: 1001, chaves: [0], vfr: false, origem: "decodificador" });

describe("realce × fixtures", () => {
  it("realce-tabela.json: a tabela do clipe do Rust", () => {
    const lista = casos<{ tab: TabelaQuadros; efeitos: Efeito[] }, TabelaQuadros>("realce-tabela.json");
    expect(lista.length).toBe(40);
    expect(conferirCasos(lista, (e) => tabelaDoClipe(e.tab, e.efeitos))).toEqual([]);
    // a fixture tem dos dois: tabela dobrada e tabela da fonte
    const dobra = lista.filter((c) => c.saida.passos.length !== c.entrada.tab.passos.length);
    expect([dobra.length > 3, dobra.length < lista.length]).toEqual([true, true]);
  });

  it("sem desentrelace por campo, e em base de um tique por quadro, devolve a própria tabela", () => {
    const t = ntsc(5);
    expect(tabelaDoClipe(t, [])).toBe(t);
    expect(tabelaDoClipe(t, [campo(false)])).toBe(t);
    expect(tabelaDoClipe(t, [{ ...campo(), modo: "quadro" }])).toBe(t);
    const avi: TabelaQuadros = { ...t, tb: { num: 1, den: 25 }, passos: [1, 1, 1, 1], passo_mediano: 1 };
    expect([porCampo([campo()]), campoCabe(avi), campoCabe(t)]).toEqual([true, false, true]);
    expect(tabelaDoClipe(avi, [campo()])).toBe(avi);
  });

  it("campo: o pts do quadro acha o primeiro campo dele; o fim da mídia não muda", () => {
    const t = ntsc(5);
    const c = tabelaDoClipe(t, [campo()]);
    expect(nQuadros(c)).toBe(10);
    expect(ptsDaTabela(c)).toEqual([0, 501, 1001, 1502, 2002, 2503, 3003, 3504, 4004, 4505]);
    expect(ptsDaTabela(t).map((p) => indiceDoPts(c, p))).toEqual([0, 2, 4, 6, 8]);
    expect(ptsDaTabela(c).at(-1)! + c.passo_mediano).toBe(ptsDaTabela(t).at(-1)! + t.passo_mediano);
  });

  it("resolver: o trecho por campo dobra de quadros e, na taxa dos campos, de slots", () => {
    const t = ntsc(30);
    const p = novoProjeto();
    const s = (j: number) => (j * 1001) / 30000;
    const trecho = { ...novoTrecho("m1", s(4), s(12)), inicio_pts: 4 * 1001, fim_pts: 12 * 1001 };
    p.principal = [trecho];
    const ctx = { tabela: () => t };
    p.saida.fps = { num: 30000, den: 1001 };
    const quadro = resolver(p, ctx).clipes[0]!;
    expect([quadro.n_q, quadro.fonte?.j_in, quadro.fonte?.j_out, quadro.fonte?.lidos]).toEqual([8, 4, 12, 8]);
    trecho.efeitos = [campo()];
    p.saida.fps = { num: 60000, den: 1001 };
    const campos = resolver(p, ctx).clipes[0]!;
    expect([campos.n_q, campos.fonte?.j_in, campos.fonte?.j_out, campos.fonte?.lidos, campos.fonte?.repetidos, campos.fonte?.descartados]).toEqual([16, 8, 24, 16, 0, 0]);
    expect(campos.fonte?.pts_in).toBe(4 * 1001);
  });
});
