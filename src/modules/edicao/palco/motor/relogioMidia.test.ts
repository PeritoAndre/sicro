import { describe, expect, it } from "vitest";
import type { PreviaMidia, TabelaQuadros } from "@domain/edicao";
import { relogioMidia } from "./relogioMidia";

/** MKV de base 1/1000 que começa em 1,4 s, passos irregulares (33/34/40/27 ms); cópia de prévia em zero. */
const tabela: TabelaQuadros = { tb: { num: 1, den: 1000 }, pts0: 1400, passos: [33, 34, 33, 40, 27, 33], passo_mediano: 33, chaves: [0], vfr: true, origem: "decodificador" };
const previa: PreviaMidia = { relativo: "previa/x.mp4", tipo: "copia", desloc_s: 1.4, de_s: 1.4, ate_s: 1.633, largura: 1280, altura: 720, pronta: true };

describe("relogioMidia", () => {
  const r = relogioMidia(previa, tabela);

  it("alvo(n) = meio do quadro n pela tabela, em tempo do elemento", () => {
    const pts = [1400, 1433, 1467, 1500, 1540, 1567, 1600];
    const dur = [33, 34, 33, 40, 27, 33, 33];
    expect(r.n).toBe(7);
    for (let n = 0; n < 7; n++) {
      const meio = (pts[n]! + dur[n]! / 2) / 1000;
      expect(r.meio(n)).toBe(meio);
      expect(r.alvo(n)).toBeCloseTo(meio - 1.4, 12);
      // o alvo cai dentro do quadro n, longe das duas bordas
      const tEl = r.alvo(n);
      expect(tEl).toBeGreaterThan((pts[n]! - 1400) / 1000 + 0.01);
      expect(tEl).toBeLessThan((pts[n]! + dur[n]! - 1400) / 1000 - 0.01);
    }
  });

  it("ida e volta: o quadro do alvo é o próprio quadro", () => {
    for (let n = 0; n < r.n; n++) expect(r.quadro(r.alvo(n))).toBe(n);
  });

  it("paraElemento e paraFonte pelo deslocamento; fora da tabela fica na ponta", () => {
    expect(r.paraElemento(1.5)).toBeCloseTo(0.1, 12);
    expect(r.paraFonte(0.1)).toBeCloseTo(1.5, 12);
    expect(r.alvo(99)).toBe(r.alvo(6));
    expect(r.alvo(-3)).toBe(r.alvo(0));
    expect(r.quadro(-5)).toBe(0);
    expect(r.quadro(99)).toBe(6);
    expect([r.de_s, r.ate_s]).toEqual([1.4, 1.633]);
  });

  it("original que começa em zero: deslocamento nulo", () => {
    const o = relogioMidia({ ...previa, tipo: "original", desloc_s: 0 }, { ...tabela, pts0: 0 });
    expect(o.alvo(0)).toBe(0.0165);
    expect(o.quadro(0.0165)).toBe(0);
  });
});
