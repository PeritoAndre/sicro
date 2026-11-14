/** Espelho de `camada::titulo` × fixture `primitivas-titulo.json` (mesmos bits) e o que só o front faz. */

import { describe, expect, it } from "vitest";
import type { BlocoTexto, PedidoSprite, Primitiva, SpriteTexto, Titulo } from "@domain/edicao";
import { casos, conferirCasos, diferenca } from "./testeFixtures";
import { pedido, primitivas } from "./titulos";

type Tamanho = { w: number; h: number };
type E = {
  titulo: Titulo;
  quadro: { canvas: Tamanho };
  imagens: Record<string, Tamanho>;
  sprites: { pedido: PedidoSprite; sprite: SpriteTexto }[];
};

describe("títulos × fixture", () => {
  it("primitivas-titulo.json: primitivas e pedidos iguais aos do Rust", () => {
    const lista = casos<E, unknown>("primitivas-titulo.json");
    expect(lista.length).toBeGreaterThan(20);
    const erros = conferirCasos(lista, (e) => {
      const canvas = e.quadro.canvas;
      const pedidos: PedidoSprite[] = [];
      const q = {
        canvas,
        // o falso do Rust registrou pedido e resposta: o pedido do espelho tem de ser o mesmo
        sprite: (p: PedidoSprite) => {
          pedidos.push(p);
          return e.sprites.find((s) => diferenca(p, s.pedido) === null)?.sprite ?? null;
        },
        // imagem que o falso não conhece = ausente (tamanho 0)
        midia: (id: string) => e.imagens[id] ?? { w: 0, h: 0 },
        atlas: () => null,
      };
      let r: { primitivas: Primitiva[] } | { erro: string };
      try {
        r = { primitivas: primitivas(e.titulo, q) };
      } catch (x) {
        r = { erro: (x as Error).message.split(":")[0]!.trim() };
      }
      // os mesmos pedidos, na mesma ordem
      const d = diferenca(pedidos, e.sprites.map((s) => s.pedido));
      if (d) throw new Error(`pedidos de sprite: ${d}`);
      return { ...r, pedidos: Array.from({ length: e.titulo.blocos.length + 1 }, (_, i) => pedido(e.titulo, i, canvas)) };
    });
    expect(erros).toEqual([]);
  });
});

describe("títulos no front", () => {
  const estilo = { fonte: "sans", negrito: false, italico: false, tamanho: 0.05, cor: "#ffffff", alinhamento: "centro", caixa: null, contorno: null } as const;
  const bloco = (id: string, conteudo: string): BlocoTexto => ({ id, papel: "corpo", conteudo, estilo, lugar: { x: 0.5, y: 0.5, h: "centro", v: "meio" }, largura_max: 0.9 });
  const canvas = { w: 1280, h: 720 };
  const sprite = () => ({ relativo: "sprites/a.png", w: 101, h: 36 });

  it("sprite ou imagem que ainda não chegou: sem desenho, o resto aparece", () => {
    const t: Titulo = { modelo: "abertura", fundo: { tipo: "imagem", midia: "m1" }, blocos: [bloco("a", "Um"), bloco("b", "Dois")] };
    let n = 0;
    const p = primitivas(t, { canvas, sprite: () => (n++ === 0 ? null : sprite()), midia: () => null });
    expect(p).toEqual([{ k: "sprite", src: "sprites/a.png", caixa: { x: 590 / 1280, y: 342 / 720, w: 101 / 1280, h: 36 / 720 }, opacidade: 1 }]);
    expect(() => primitivas(t, { canvas, sprite, midia: () => ({ w: 0, h: 10 }) })).toThrow(/^midia_ausente: m1/);
  });

  it("os seis modelos dão as mesmas primitivas; vazios não desenham", () => {
    const blocos = [bloco("a", "Título"), bloco("v", ""), bloco("c", "Rodapé")];
    const de = (modelo: Titulo["modelo"]) => primitivas({ modelo, fundo: { tipo: "cor", cor: "#000000" }, blocos }, { canvas, sprite, midia: () => null });
    const base = de("cartela");
    expect(base.length).toBe(3);
    for (const m of ["abertura", "evento", "terco_inferior", "encerramento", "livre"] as const) expect(de(m)).toEqual(base);
    expect(primitivas({ modelo: "livre", fundo: { tipo: "transparente" }, blocos: [] }, { canvas, sprite, midia: () => null })).toEqual([]);
    expect(pedido({ modelo: "livre", fundo: { tipo: "transparente" }, blocos: [] }, 0, canvas)).toEqual({ blocos: [], largura_max: 0, altura_canvas: 720, fundo: true });
  });
});
