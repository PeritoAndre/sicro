import { describe, expect, it } from "vitest";
import type { MontagemTrecho } from "@domain/montagem";
import {
  duracaoItem,
  duracaoTotal,
  inserirQuadro,
  mover,
  novaCartela,
  novaMontagem,
  novaNarracao,
  novoCongelar,
  novoTrecho,
  posicaoEm,
  validarItem,
  validarMontagem,
} from "./montagemModel";

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

  it("valida zoom, destaques e desfoques", () => {
    const t = novoTrecho("m", 0, 4) as MontagemTrecho;
    t.zoom = { inicio: { x: 0, y: 0, w: 1, h: 1 }, fim: { x: 0.25, y: 0.25, w: 0.5, h: 0.5 } };
    expect(validarItem(t)).toBeNull();
    t.zoom.fim = { x: 0.8, y: 0, w: 0.5, h: 0.5 };
    expect(validarItem(t)).toMatch(/zoom/);
    t.zoom = null;
    t.destaques = [{ id: "d", forma: "circulo", cor: "#ff0000", quadros: [] }];
    expect(validarItem(t)).toMatch(/sem quadro/);
    t.destaques[0]!.quadros = [{ t: 1, x: 0.5, y: 0.5 }];
    expect(validarItem(t)).toBeNull();
    t.destaques[0]!.forma = "retangulo";
    expect(validarItem(t)).toMatch(/largura/);
    t.destaques[0]!.quadros = [{ t: 9, x: 0.5, y: 0.5, w: 0.1, h: 0.1 }];
    expect(validarItem(t)).toMatch(/fora do trecho/);
    t.destaques = [];
    t.desfoques = [{ id: "f", quadros: [{ t: 0, x: 0.1, y: 0.1, w: 0.2 }] }];
    expect(validarItem(t)).toMatch(/largura/);
    t.desfoques[0]!.quadros = [{ t: 0, x: 0.1, y: 0.1, w: 0.2, h: 0.2 }];
    expect(validarItem(t)).toBeNull();
  });
  it("JSON antigo sem os campos novos é válido", () => {
    const m = novaMontagem();
    delete m.narracoes;
    delete m.volume_original;
    m.itens = [novoTrecho("m", 0, 2)];
    expect(validarMontagem(m)).toBeNull();
  });
  it("valida narrações e volume", () => {
    const m = novaMontagem();
    m.narracoes = [novaNarracao("a/b.wav", "n", 1)];
    expect(validarMontagem(m)).toBeNull();
    m.narracoes[0]!.volume = 3;
    expect(validarMontagem(m)).toMatch(/narração/);
    m.narracoes[0]!.volume = 1;
    m.volume_original = -1;
    expect(validarMontagem(m)).toMatch(/volume/);
  });
  it("quadros: insere em ordem, interpola e fica parado no fim", () => {
    let q = inserirQuadro([], { t: 2, x: 0.8, y: 0.2 });
    q = inserirQuadro(q, { t: 0, x: 0.2, y: 0.2 });
    q = inserirQuadro(q, { t: 2, x: 0.6, y: 0.2 });
    expect(q.map((k) => k.t)).toEqual([0, 2]);
    expect(posicaoEm(q, -0.1)).toBeNull();
    expect(posicaoEm(q, 1)?.x).toBeCloseTo(0.4);
    expect(posicaoEm(q, 5)?.x).toBeCloseTo(0.6);
  });
});
