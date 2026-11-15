/**
 * Espelho da avaliação com denso × `rastreio-denso.json` (gravada por `rastreio::avaliar::testes::fixture_denso`).
 */

import { describe, expect, it } from "vitest";
import type { ArquivoDenso, Rastreio } from "@domain/edicao";
import { avaliar, type Instante } from "./rastreio";
import { casos, conferirCasos } from "./testeFixtures";

describe("rastreio com denso × fixtures", () => {
  it("rastreio-denso.json", () => {
    const lista = casos<{ rastreio: Rastreio; em: Instante; denso: ArquivoDenso }, unknown>("rastreio-denso.json");
    expect(lista.length).toBeGreaterThan(50);
    expect(conferirCasos(lista, (e) => avaliar(e.rastreio, e.em, e.denso))).toEqual([]);
  });

  it("sem o denso a mesma lista cai nas âncoras (a fixture distingue os dois)", () => {
    const lista = casos<{ rastreio: Rastreio; em: Instante; denso: ArquivoDenso }, unknown>("rastreio-denso.json");
    expect(conferirCasos(lista, (e) => avaliar(e.rastreio, e.em)).length).toBeGreaterThan(20);
  });
});
