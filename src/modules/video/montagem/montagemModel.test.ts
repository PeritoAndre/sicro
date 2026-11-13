import { describe, expect, it } from "vitest";
import { duracaoItem, duracaoTotal, mover, novaCartela, novoCongelar, novoTrecho, validarItem } from "./montagemModel";

describe("montagemModel", () => {
  it("duração considera a velocidade", () => {
    const t = novoTrecho("m", 2, 4);
    if (t.tipo === "trecho") t.velocidade = 0.5;
    expect(duracaoItem(t)).toBe(4);
    expect(duracaoTotal([t, novaCartela("x", 2), novoCongelar("m", 1, 1.5)])).toBe(7.5);
  });
  it("move itens", () => {
    expect(mover([1, 2, 3], 0, 2)).toEqual([2, 3, 1]);
    expect(mover([1, 2, 3], 1, 5)).toEqual([1, 2, 3]);
  });
  it("valida", () => {
    expect(validarItem(novoTrecho("m", 5, 5))).not.toBeNull();
    expect(validarItem(novoTrecho("m", 5, 6))).toBeNull();
    expect(validarItem(novaCartela("x", 0))).not.toBeNull();
  });
});
