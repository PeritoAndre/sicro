/**
 * Contador de marcações à mão (contratos-fase4.md D12): âncoras do perito, fora as do "Simplificar" que ele não mexeu.
 * A conta é a do `ProvRastreio.ancoras` do motor (`motor/mod.rs::do_programa`: `(x − t).abs() <= 1e-6`), que o PDF
 * escreve como "marcações do perito: n".
 */

import { describe, expect, it } from "vitest";
import type { Caixa, Chave } from "@domain/edicao";
import { marcadasAMao } from "./contagem";

const V: Caixa = { x: 0.4, y: 0.4, w: 0.1, h: 0.2 };
const ks = (...ts: number[]): Chave<Caixa>[] => ts.map((t) => ({ t, pts: null, v: V }));

describe("marcadasAMao", () => {
  it("sem âncoras do programa: todas são do perito; sem âncoras, zero", () => {
    expect(marcadasAMao({ ancoras: [] })).toBe(0);
    expect(marcadasAMao({ ancoras: ks(1, 2, 3) })).toBe(3);
    expect(marcadasAMao({ ancoras: ks(1, 2, 3), ancoras_do_programa: [] })).toBe(3);
  });

  it("as do Simplificar não contam; o perito por cima de uma delas (outro t) conta", () => {
    expect(marcadasAMao({ ancoras: ks(1, 1.5, 2, 2.5, 3), ancoras_do_programa: [1.5, 2.5] })).toBe(3);
    // t do programa que já não está nas âncoras (o perito apagou): não desconta nada
    expect(marcadasAMao({ ancoras: ks(1, 3), ancoras_do_programa: [2] })).toBe(2);
    // todas do programa
    expect(marcadasAMao({ ancoras: ks(0.5, 1), ancoras_do_programa: [1, 0.5] })).toBe(0);
  });

  it("folga de 1e-6 s no t, a do Rust (dentro desconta; fora conta)", () => {
    expect(marcadasAMao({ ancoras: ks(10 + 9e-7), ancoras_do_programa: [10] })).toBe(0);
    expect(marcadasAMao({ ancoras: ks(10 - 9e-7), ancoras_do_programa: [10] })).toBe(0);
    expect(marcadasAMao({ ancoras: ks(10 + 2e-6), ancoras_do_programa: [10] })).toBe(1);
    expect(marcadasAMao({ ancoras: ks(10 - 2e-6), ancoras_do_programa: [10] })).toBe(1);
  });

  it("lista do programa fora de ordem e longa (efeito de minutos): a mesma conta que a busca direta", () => {
    const passo = 1 / 30;
    const ancoras = ks(...Array.from({ length: 5000 }, (_, i) => i * passo));
    // o programa ficou com 1 de cada 3, embaralhada; o perito marcou as outras
    const programa = ancoras.filter((_, i) => i % 3 === 0).map((k) => k.t);
    for (let i = programa.length - 1; i > 0; i--) {
      const j = (i * 7919) % (i + 1);
      [programa[i], programa[j]] = [programa[j] as number, programa[i] as number];
    }
    const direta = ancoras.filter((k) => !programa.some((x) => Math.abs(x - k.t) <= 1e-6)).length;
    expect(marcadasAMao({ ancoras, ancoras_do_programa: programa })).toBe(direta);
    expect(direta).toBe(5000 - Math.ceil(5000 / 3));
  });

  it("não mexe no rastreio", () => {
    const programa = [3, 1, 2];
    const r = { ancoras: ks(1, 2, 3, 4), ancoras_do_programa: programa };
    expect(marcadasAMao(r)).toBe(1);
    expect(programa).toEqual([3, 1, 2]);
  });
});
