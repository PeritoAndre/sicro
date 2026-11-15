/**
 * Espelho das contas da régua e dos textos (P1.3) × fixtures do Rust (`medidas-pontas.json`, `medidas-texto.json`):
 * mesmos bits. As primitivas (`primitivas-medidas.json`) são conferidas em `modelo/espelhos.test.ts`.
 */

import { describe, expect, it } from "vitest";
import type { EfRegua, RetratoCalculo } from "@domain/edicao";
import { casos, conferirCasos } from "../../modelo/testeFixtures";
import { pontasDaRegua, textoDaVelocidade, textoDoComprimento } from "./medidas";

const ouErro = <T,>(f: () => T) => {
  try {
    return f();
  } catch (e) {
    return { erro: (e as Error).message };
  }
};

describe("medidas × fixtures (P1.3)", () => {
  it("medidas-pontas.json: pontas e divisões da régua pela homografia", () => {
    const lista = casos<{ efeito: EfRegua }, unknown>("medidas-pontas.json");
    expect(lista.length).toBeGreaterThan(10);
    expect(conferirCasos(lista, (e) => ouErro(() => ({ pontos: pontasDaRegua(e.efeito) })))).toEqual([]);
  });

  it("medidas-texto.json: velocidade com faixa ou ≈, e o rótulo da régua", () => {
    const lista = casos<{ calculo?: RetratoCalculo; comprimento_m?: number }, unknown>("medidas-texto.json");
    expect(lista.length).toBeGreaterThan(15);
    expect(conferirCasos(lista, (e) => ({ texto: e.calculo ? textoDaVelocidade(e.calculo) : textoDoComprimento(e.comprimento_m!) }))).toEqual([]);
    for (const c of lista) if ("calculo" in c.entrada) expect((c.saida as { texto: string }).texto).toMatch(/^(≈ [\d,]+ km\/h|[\d,]+ km\/h \([\d,]+ a [\d,]+\))$/);
  });
});
