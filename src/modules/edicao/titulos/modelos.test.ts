/** Modelos de título (P1.7): o espelho de `camada::titulo` × fixture `titulos-modelos.json`, bit a bit. */

import { describe, expect, it } from "vitest";
import type { Caixa, Titulo } from "@domain/edicao";
import { casos, conferirCasos } from "../modelo/testeFixtures";
import { aplicarModelo, arranjo, blocoNoModelo, escalaDoCanvas, naAreaSegura, tituloNoModelo, type ModeloTitulo, type Papel } from "../modelo/titulos";

type Tamanho = { w: number; h: number };
type E =
  | { conta: "arranjo"; modelo: ModeloTitulo; papel: Papel }
  | { conta: "escala"; canvas: Tamanho }
  | { conta: "aplicar"; titulo: Titulo; modelo: ModeloTitulo; sobreposto: boolean; canvas: Tamanho }
  | { conta: "novo"; modelo: ModeloTitulo; sobreposto: boolean; id: string; canvas: Tamanho }
  | { conta: "bloco"; titulo: Titulo; papel: Papel; id: string; conteudo: string; canvas: Tamanho }
  | { conta: "area"; caixa: Omit<Caixa, "x"> & { x: number | "nan" } };

describe("modelos de título × fixture", () => {
  it("titulos-modelos.json: arranjos, escala, aplicar, título novo, bloco novo e área segura iguais aos do Rust", () => {
    const lista = casos<E, unknown>("titulos-modelos.json");
    const contas = new Set(lista.map((c) => c.entrada.conta));
    expect([...contas].sort()).toEqual(["aplicar", "area", "arranjo", "bloco", "escala", "novo"]);
    expect(lista.length).toBeGreaterThan(150);
    const erros = conferirCasos(lista, (e) => {
      switch (e.conta) {
        case "arranjo":
          return arranjo(e.modelo, e.papel);
        case "escala":
          return escalaDoCanvas(e.canvas);
        case "aplicar": {
          const antes = JSON.stringify(e.titulo);
          const r = aplicarModelo(e.titulo, e.modelo, e.sobreposto, e.canvas);
          // puro: a entrada não muda
          if (JSON.stringify(e.titulo) !== antes) throw new Error("aplicarModelo mudou a entrada");
          return r;
        }
        case "novo":
          return tituloNoModelo(e.modelo, e.sobreposto, e.id, e.canvas);
        case "bloco":
          return blocoNoModelo(e.titulo, e.papel, e.id, e.conteudo, e.canvas);
        case "area":
          return naAreaSegura({ ...e.caixa, x: e.caixa.x === "nan" ? Number.NaN : e.caixa.x });
      }
    });
    expect(erros).toEqual([]);
  });
});
