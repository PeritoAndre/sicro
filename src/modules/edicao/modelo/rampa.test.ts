/** Rampa (P3.4) × fixture do Rust `rampa-taus.json`: o τ de cada instante e o tempo de programa nos mesmos bits. */

import { describe, expect, it } from "vitest";
import type { Chave } from "@domain/edicao";
import { Rampa, programaDaFonte } from "./tempo";
import { casos, conferirCasos } from "./testeFixtures";

type E = { pontos: Chave<number>[]; t0: number; us: number[]; fora: number[] };

describe("rampa × fixture", () => {
  it("rampa-taus.json: τ, duração e programa da fonte", () => {
    const lista = casos<E, unknown>("rampa-taus.json");
    expect(lista.length).toBeGreaterThanOrEqual(7);
    const erros = conferirCasos(lista, (e) => {
      const r = new Rampa(e.pontos, e.t0);
      return { taus: r.taus(e.us), duracao: r.duracao(4.2), programa: e.fora.map((t) => programaDaFonte({ tipo: "rampa", pontos: e.pontos }, e.t0, t)) };
    });
    expect(erros).toEqual([]);
  });
});
