import { describe, expect, it } from "vitest";
import type { PessoaView } from "@domain/video_pessoas";
import { caixaEm, quadroMaisProximo, segmentos, ultimoAte, visiveisEm } from "./pessoasMath";

const pessoa = (ts: number[], extra: Partial<PessoaView> = {}): PessoaView => ({
  id: "a",
  numero: 1,
  nome: "",
  cor: "#e6194b",
  estado: "auto",
  classe: 0,
  classe_nome: "pessoa",
  origem: "auto",
  primeiro_s: ts[0] ?? 0,
  ultimo_s: ts[ts.length - 1] ?? 0,
  quadros: ts.length,
  caixas: ts.map((t) => [t, t * 10, 5, 40, 100]),
  ajustes_t: [],
  ...extra,
});
const C1 = { frameDur: 0.1, passo: 1 };

describe("caixaEm", () => {
  const p = pessoa([1.0, 1.1, 1.2]);
  it("segura o quadro exibido e some depois da folga", () => {
    expect(caixaEm(p, 0.95, C1)).toBeNull();
    expect(caixaEm(p, 1.05, C1)?.x).toBe(10);
    expect(caixaEm(p, 1.2, C1)?.x).toBe(12);
    expect(caixaEm(p, 1.5, C1)).toBeNull();
  });
  it("com passo maior interpola entre analisados", () => {
    const q = pessoa([1.0, 1.4]);
    expect(caixaEm(q, 1.2, { frameDur: 0.1, passo: 4 })?.x).toBeCloseTo(12, 6);
  });
});

describe("busca e trechos", () => {
  it("ultimoAte", () => {
    const p = pessoa([1, 2, 3]);
    expect(ultimoAte(p.caixas, 0.5)).toBe(-1);
    expect(ultimoAte(p.caixas, 2)).toBe(1);
    expect(ultimoAte(p.caixas, 9)).toBe(2);
  });
  it("quadroMaisProximo escolhe o mais perto", () => {
    const p = pessoa([1, 2, 3]);
    expect(quadroMaisProximo(p, 2.4)).toBe(2);
    expect(quadroMaisProximo(p, 2.6)).toBe(3);
  });
  it("segmentos separa as lacunas (oclusão)", () => {
    const p = pessoa([1.0, 1.1, 1.2, 2.0, 2.1]);
    const s = segmentos(p, C1);
    expect(s).toHaveLength(2);
    expect(s[0]![0]).toBe(1.0);
    expect(s[1]![0]).toBe(2.0);
  });
  it("visiveisEm", () => {
    const a = pessoa([1.0, 1.1]);
    const b = pessoa([5.0, 5.1], { id: "b", numero: 2 });
    expect(visiveisEm([a, b], 1.05, C1).map((x) => x.numero)).toEqual([1]);
  });
});
