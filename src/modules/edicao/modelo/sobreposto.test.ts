import { describe, expect, it } from "vitest";
import type { ClipeSobreposto, SombraTexto } from "@domain/edicao";
import { DESLIZE, folgaDaSombra, raiosDoDesfoque, recusa, transformacaoEm } from "./sobreposto";
import { casos, conferirCasos } from "./testeFixtures";

type Base = Omit<ClipeSobreposto, "tipo">;

describe("sobreposto × fixtures (espelho de edicao/sobreposto.rs)", () => {
  it("sobreposto-transformacao.json: aparecer, corte e deslizar em cada lado, cruzados, −0", () => {
    const lista = casos<{ base: Base; tau: number; dur: number }, unknown>("sobreposto-transformacao.json");
    expect(lista.length).toBeGreaterThan(250);
    // cada Surgir e cada lado aparecem na fixture
    const vistos = new Set(lista.flatMap((c) => (c.entrada.base.animacao ? [c.entrada.base.animacao.entrada, c.entrada.base.animacao.saida, c.entrada.base.animacao.lado] : [])));
    for (const v of ["corte", "aparecer", "deslizar", "cima", "baixo", "esq", "dir"]) expect(vistos).toContain(v);
    expect(conferirCasos(lista, (e) => transformacaoEm(e.base, e.tau, e.dur))).toEqual([]);
  });

  it("texto-folga-sombra.json: raios do desfoque e folga do sprite", () => {
    const lista = casos<{ sombra: SombraTexto; altura_canvas: number }, { x: number; y: number; raios: number[] }>("texto-folga-sombra.json");
    expect(lista.length).toBeGreaterThan(30);
    expect(
      conferirCasos(lista, (e) => ({ ...folgaDaSombra(e.sombra, e.altura_canvas), raios: raiosDoDesfoque(e.sombra.desfoque * e.altura_canvas) })),
    ).toEqual([]);
  });

  it("nada é recusado: corte, aparecer e deslizar renderizam", () => {
    expect(recusa({})).toBeNull();
    for (const [entrada, saida] of [["corte", "aparecer"], ["aparecer", "deslizar"], ["deslizar", "corte"]] as const)
      expect(recusa({ animacao: { entrada, saida, lado: "dir" } })).toBeNull();
  });

  it("deslizar: metade do caminho no meio da entrada; corte cheio nas pontas", () => {
    const base = (animacao?: Base["animacao"]): Pick<Base, "transformacao" | "entrada_s" | "saida_s" | "animacao"> => ({
      transformacao: { x: 0.5, y: 0.5, escala: 1, rotacao: 0, opacidade: 1 },
      entrada_s: 0.4,
      saida_s: 0.4,
      animacao,
    });
    const m = transformacaoEm(base({ entrada: "deslizar", saida: "aparecer", lado: "esq" }), 0.2, 2);
    expect(m.x).toBeCloseTo(0.5 - DESLIZE / 2, 12);
    expect(m.opacidade).toBe(0.5);
    const c = base({ entrada: "corte", saida: "corte", lado: "baixo" });
    expect(transformacaoEm(c, 0, 2).opacidade).toBe(1);
    expect(transformacaoEm(c, 2, 2).opacidade).toBe(1);
    expect(transformacaoEm(base(), 0, 2).opacidade).toBe(0);
  });
});
