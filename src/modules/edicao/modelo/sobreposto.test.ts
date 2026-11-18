import { describe, expect, it } from "vitest";
import type { ClipeSobreposto } from "@domain/edicao";
import { recusa, transformacaoEm } from "./sobreposto";
import { casos, conferirCasos } from "./testeFixtures";

type Base = Omit<ClipeSobreposto, "tipo">;

describe("sobreposto × fixtures (espelho de edicao/sobreposto.rs)", () => {
  it("sobreposto-transformacao.json: opacidade da entrada e da saída, curvas da transformação", () => {
    const lista = casos<{ base: Base; tau: number; dur: number }, unknown>("sobreposto-transformacao.json");
    expect(lista.length).toBeGreaterThan(20);
    expect(conferirCasos(lista, (e) => transformacaoEm(e.base, e.tau, e.dur))).toEqual([]);
  });

  it("recusa o que não é aparecer, como o Rust", () => {
    expect(recusa({})).toBeNull();
    expect(recusa({ animacao: { entrada: "aparecer", saida: "aparecer", lado: "esq" } })).toBeNull();
    expect(recusa({ animacao: { entrada: "corte", saida: "aparecer", lado: "dir" } })).toBe("animacao_sobreposto");
    expect(recusa({ animacao: { entrada: "aparecer", saida: "deslizar", lado: "dir" } })).toBe("animacao_sobreposto");
  });
});
