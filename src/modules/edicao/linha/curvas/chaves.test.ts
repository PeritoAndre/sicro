/**
 * Quadros-chave (P4.2, contratos-fase4.md D7): chave no cursor, tirar, mover sem cruzar, interpolação, e a posição
 * (x e y) andando junto. A avaliação é o espelho de `curva.rs` (fixtures `curva-*.json`, `modelo/espelhos.test.ts`).
 */

import { describe, expect, it } from "vitest";
import type { Animavel, Chave } from "@domain/edicao";
import { avaliar } from "../../modelo/curva";
import { acompanhar, chaveEm, chaveNoCursor, chavesDe, gravarEm, INTERP_NOVA, interpDe, moverChave, tirarChaveEm, trocarInterp } from "./chaves";

const ks = (...xs: [number, number, Chave<number>["interp"]?][]): Chave<number>[] => xs.map(([t, v, interp]) => (interp ? { t, v, interp } : { t, v }));

describe("chave no cursor (D7)", () => {
  it("valor fixo vira uma chave no instante, suave (a linguagem do zoom); null = linear sem o campo", () => {
    expect(INTERP_NOVA).toBe("suave");
    expect(chaveNoCursor(0.4, 1.5, 0.4)).toEqual([{ t: 1.5, v: 0.4, interp: "suave" }]);
    expect(chaveNoCursor(0.4, 1.5, 0.4, null)).toEqual([{ t: 1.5, v: 0.4 }]);
    expect(chaveNoCursor(0.4, 1.5, 0.4, "degrau")).toEqual([{ t: 1.5, v: 0.4, interp: "degrau" }]);
  });

  it("com chaves: entra em ordem; a do mesmo instante troca o valor e fica com a curva e o pts dela", () => {
    const a: Chave<number>[] = [{ t: 0, v: 1, interp: "degrau" }, { t: 2, pts: 60, v: 3, interp: "linear" }];
    expect(chaveNoCursor(a, 1, 9)).toEqual([a[0], { t: 1, v: 9, interp: "suave" }, a[1]]);
    expect(chaveNoCursor(a, -1, 9)).toEqual([{ t: -1, v: 9, interp: "suave" }, ...a]);
    expect(chaveNoCursor(a, 5, 9)).toEqual([...a, { t: 5, v: 9, interp: "suave" }]);
    expect(chaveNoCursor(a, 2 + 1e-9, 7)).toEqual([a[0], { t: 2, pts: 60, v: 7, interp: "linear" }]);
    expect(chaveNoCursor(a, 0, 5)).toEqual([{ t: 0, v: 5, interp: "degrau" }, a[1]]);
    // a lista de entrada não muda
    expect(a).toEqual([{ t: 0, v: 1, interp: "degrau" }, { t: 2, pts: 60, v: 3, interp: "linear" }]);
  });

  it("gravarEm: fixo troca o valor; com chaves, a chave do instante", () => {
    expect(gravarEm(0.5, 3, 0.7)).toBe(0.7);
    expect(gravarEm(ks([0, 1]), 3, 0.7)).toEqual([{ t: 0, v: 1 }, { t: 3, v: 0.7, interp: "suave" }]);
    expect(gravarEm(ks([0, 1]), 3, 0.7, null)).toEqual([{ t: 0, v: 1 }, { t: 3, v: 0.7 }]);
    expect(gravarEm(ks([0, 1, "degrau"]), 0, 0.2)).toEqual([{ t: 0, v: 0.2, interp: "degrau" }]);
  });

  it("tirar: a do instante sai; a última volta a valor fixo (nunca lista vazia); sem chave ali, nada", () => {
    const a = ks([0, 1], [1, 2], [2, 3]);
    expect(tirarChaveEm(a, 1)).toEqual(ks([0, 1], [2, 3]));
    expect(tirarChaveEm(a, 0.5)).toBe(a);
    expect(tirarChaveEm(ks([4, 0.3, "suave"]), 4)).toBe(0.3);
    expect(tirarChaveEm(0.3, 4)).toBe(0.3);
    expect(chavesDe(a)).toEqual([0, 1, 2]);
    expect(chavesDe(0.5)).toEqual([]);
    expect([chaveEm(a, 2), chaveEm(a, 2.5), chaveEm(0.5, 0)]).toEqual([2, -1, -1]);
  });
});

describe("mover e interpolação", () => {
  const a: Chave<number>[] = [{ t: 0, v: 0 }, { t: 1, pts: 30, v: 1, interp: "suave" }, { t: 2, v: 0 }];

  it("move dentro do vão, nunca cruza nem encosta na vizinha; o pts sai", () => {
    expect(moverChave(a, 1, 1.4)).toEqual([a[0], { t: 1.4, v: 1, interp: "suave" }, a[2]]);
    const alem = moverChave(a, 1, 7);
    expect(alem[1]!.t).toBeLessThan(2);
    expect(alem[1]!.t).toBeGreaterThan(1.99);
    expect(moverChave(a, 1, -7)[1]!.t).toBeGreaterThan(0);
    // folga de um quadro
    expect(moverChave(a, 1, 1.99, 1 / 30)[1]!.t).toBeCloseTo(2 - 1 / 30, 12);
    expect(moverChave(ks([0, 0], [0.02, 1], [0.04, 0]), 1, 0.03, 1 / 30)).toEqual(ks([0, 0], [0.02, 1], [0.04, 0]));
    // pontas sem vizinha andam livres; a ordem nunca muda
    expect(moverChave(a, 0, -3)[0]!.t).toBe(-3);
    expect(moverChave(a, 2, 9)[2]!.t).toBe(9);
    for (const t of [-5, -0.5, 0.5, 1.5, 2.5, 5]) {
      const m = moverChave(a, 1, t);
      expect(m.map((k) => k.t)).toEqual([...m.map((k) => k.t)].sort((x, y) => x - y));
    }
    expect(moverChave(a, 5, 1)).toEqual(a);
    expect(moverChave(a, 1, Number.NaN)).toEqual(a);
  });

  it("interpolação da chave até a seguinte; ausente = linear", () => {
    expect(trocarInterp(a, 0, "degrau")[0]).toEqual({ t: 0, v: 0, interp: "degrau" });
    expect(interpDe(a[0]!)).toBe("linear");
    expect(interpDe(a[1]!)).toBe("suave");
    // a curva muda com ela: degrau segura o valor até a chave seguinte
    expect(avaliar(trocarInterp(a, 0, "degrau"), 0.9)).toBe(0);
    expect(avaliar(a, 0.25)).toBe(0.25);
    expect(avaliar(trocarInterp(a, 0, "suave"), 0.25)).toBe(0.15625);
  });
});

describe("posição: x e y juntos", () => {
  const x: Animavel<number> = ks([0, 0.2, "suave"], [1, 0.5, "suave"], [2, 0.8, "suave"]);
  const y: Animavel<number> = ks([0, 0.6, "suave"], [1, 0.4, "suave"], [2, 0.3, "suave"]);

  it("o que mudou em x vai para y: mover, interpolação, tirar", () => {
    const xs = x as Chave<number>[];
    expect(acompanhar(x, moverChave(xs, 1, 1.25), y)).toEqual(ks([0, 0.6, "suave"], [1.25, 0.4, "suave"], [2, 0.3, "suave"]));
    expect(acompanhar(x, trocarInterp(xs, 0, "degrau"), y)).toEqual(ks([0, 0.6, "degrau"], [1, 0.4, "suave"], [2, 0.3, "suave"]));
    expect(acompanhar(x, tirarChaveEm(x, 1), y)).toEqual(ks([0, 0.6, "suave"], [2, 0.3, "suave"]));
    // tirar a última de x leva a de y junto (y volta a valor fixo)
    expect(acompanhar(ks([1, 0.5]), 0.5, ks([1, 0.4]))).toBe(0.4);
    // y fixo ou sem chave no instante: fica
    expect(acompanhar(x, moverChave(xs, 1, 1.25), 0.4)).toBe(0.4);
    expect(acompanhar(x, moverChave(xs, 1, 1.25), ks([0.5, 0.1]))).toEqual(ks([0.5, 0.1]));
  });
});
