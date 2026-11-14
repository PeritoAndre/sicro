/**
 * Espelhos × fixtures do Rust (D14): mesmos bits. `tempo-*` (P0.3b) e `primitivas-*` (P0.5) existem desde a leva 3:
 * nenhum teste é pulado (o P0.8b fechou).
 */

import { describe, expect, it } from "vitest";
import type { AlvoQuadro, Animavel, Caixa, Chave, Efeito, EstiloTexto, PedidoSprite, Primitiva, ProjetoEdicao, QuadroPalco, Racional, Rastreio, TabelaQuadros, Velocidade } from "@domain/edicao";
import { avaliar as avaliarCurva, suave } from "./curva";
import { aDe, caixaNoCanvas, inversa, ponto, regiaoEm, type Afim, type Tamanho } from "./geometria";
import { avaliar as avaliarRastreio, emAusencia, type Instante } from "./rastreio";
import { amostraDoSlot, fatorRacional, instanteDoSlot, mapaTrecho, nQSemFonte, quadroEm, resolver, slots, type MapaQuadros } from "./tempo";
import { cosSenGraus, primitivas } from "../palco/desenho/primitivas";
import { casos, comPrefixo, conferirCasos, diferenca } from "./testeFixtures";

describe("curva × fixtures", () => {
  it("curva-avaliar.json", () => {
    const lista = casos<{ tipo: string; a: Animavel<number | Caixa>; t: number }, unknown>("curva-avaliar.json");
    expect(lista.length).toBeGreaterThan(100);
    expect(conferirCasos(lista, (e) => avaliarCurva(e.a, e.t))).toEqual([]);
  });

  it("curva-suave.json", () => {
    const lista = casos<{ f: number }, number>("curva-suave.json");
    expect(lista.length).toBeGreaterThan(20);
    expect(conferirCasos(lista, (e) => suave(e.f))).toEqual([]);
  });

  it("lista de chaves vazia é erro (o front nunca grava)", () => {
    expect(() => avaliarCurva([] as Chave<number>[], 0)).toThrow();
  });
});

describe("geometria × fixtures", () => {
  it("geometria-a.json: A(τ), inversa, região, pontos e caixas", () => {
    type E = { efeitos: Efeito[]; tau: number; origem: Tamanho; canvas: Tamanho; pontos: { x: number; y: number }[]; caixas: Caixa[] };
    const lista = casos<E, unknown>("geometria-a.json");
    expect(lista.length).toBeGreaterThan(50);
    const erros = conferirCasos(lista, (e) => {
      const a: Afim = aDe(e.efeitos, e.tau, e.origem, e.canvas);
      return {
        a,
        inversa: inversa(a),
        regiao: regiaoEm(e.efeitos, e.tau),
        pontos: e.pontos.map((p) => ponto(a, p)),
        caixas: e.caixas.map((c) => caixaNoCanvas(a, c)),
      };
    });
    expect(erros).toEqual([]);
  });

  it("a comparação separa −0 de 0", () => {
    expect(diferenca({ a: [0] }, { a: [-0] })).not.toBeNull();
    expect(diferenca({ a: 1 }, { a: 1, b: 2 })).not.toBeNull();
  });
});

describe("rastreio × fixtures", () => {
  it("rastreio-avaliar.json", () => {
    const lista = casos<{ rastreio: Rastreio; em: Instante }, unknown>("rastreio-avaliar.json");
    expect(lista.length).toBeGreaterThan(60);
    expect(conferirCasos(lista, (e) => avaliarRastreio(e.rastreio, e.em))).toEqual([]);
  });

  it("rastreio-ausencia.json", () => {
    const lista = casos<{ rastreio: Rastreio; t: number }, boolean>("rastreio-ausencia.json");
    expect(lista.length).toBeGreaterThan(5);
    expect(conferirCasos(lista, (e) => emAusencia(e.rastreio, e.t))).toEqual([]);
  });
});

// ---- leva 3 (P0.3b e P0.5): formato dos geradores do Rust ----

/** Arquivos que este teste sabe conferir. */
const TEMPO = ["tempo-mapa.json", "tempo-quadro.json", "tempo-contas.json", "tempo-resolver.json"];
const PRIMITIVAS = ["primitivas-efeitos.json", "primitivas-seno.json", "primitivas-titulo.json"];

/** Erro de validação do Rust vira `{ erro: "<código>[: detalhe]" }` na fixture. */
const resultadoOuErro = (f: () => unknown) => {
  try {
    return f();
  } catch (e) {
    return { erro: (e as Error).message };
  }
};
const mapaComoRust = (m: MapaQuadros) => ({ ...m, c: Number(m.c), r: Number(m.r), m: Number(m.m), kn: Number(m.kn), kd: Number(m.kd), e: Number(m.e), p0: Number(m.p0) });

describe("tempo × fixtures (P0.3b)", () => {
  it("tempo-mapa.json: mapa de quadros em inteiros", () => {
    type E = { tabela: TabelaQuadros; j_in: number; j_out: number; velocidade: Velocidade; fps: Racional };
    const lista = casos<E, unknown>("tempo-mapa.json");
    expect(conferirCasos(lista, (e) => resultadoOuErro(() => mapaComoRust(mapaTrecho(e.tabela, e.j_in, e.j_out, e.velocidade, e.fps))))).toEqual([]);
  });

  it("tempo-quadro.json: quadro de um tempo, índice ou pts", () => {
    const lista = casos<{ tabela: TabelaQuadros; alvo: AlvoQuadro }, unknown>("tempo-quadro.json");
    expect(conferirCasos(lista, (e) => resultadoOuErro(() => quadroEm(e.tabela, e.alvo)))).toEqual([]);
  });

  it("tempo-contas.json: slots, amostras e fator", () => {
    type E = { conta: string; duracao_s?: number; q?: number; fps?: Racional; fator?: number };
    const lista = casos<E, unknown>("tempo-contas.json");
    const contas: Record<string, (e: E) => unknown> = {
      slots: (e) => slots(e.duracao_s!, e.fps!),
      n_q_sem_fonte: (e) => nQSemFonte(e.duracao_s!, e.fps!),
      instante_do_slot: (e) => instanteDoSlot(e.q!, e.fps!),
      amostra_do_slot: (e) => amostraDoSlot(e.q!, e.fps!),
      fator_racional: (e) => fatorRacional(e.fator!),
    };
    expect(conferirCasos(lista, (e) => resultadoOuErro(() => contas[e.conta]!(e)))).toEqual([]);
  });

  it("tempo-resolver.json: linha resolvida", () => {
    type E = { projeto: ProjetoEdicao; tabelas: Record<string, TabelaQuadros> };
    const lista = casos<E, unknown>("tempo-resolver.json");
    expect(conferirCasos(lista, (e) => resolver(e.projeto, { tabela: (id) => e.tabelas[id] ?? null }))).toEqual([]);
  });
});

describe("primitivas × fixtures (P0.5)", () => {
  /** Erro de validação do Rust vira `{ erro: "<código>" }` (o texto antes do ":"). */
  const primitivasOuErro = (f: () => Primitiva[]) => {
    try {
      return { primitivas: f() };
    } catch (e) {
      return { erro: (e as Error).message.split(":")[0]!.trim() };
    }
  };

  it("primitivas-efeitos.json: uma função por efeito", () => {
    type Sprite = { relativo: string; w: number; h: number };
    type Atlas = { relativo: string; celula: { w: number; h: number }; glifos: string };
    type E = {
      efeito: Efeito;
      quadro: { q: number; t: number; pts: number | null; a: QuadroPalco["A"]; canvas: { w: number; h: number }; rastreios: Record<string, Caixa>; indice?: number | null; programa_s?: number };
      imagens: Record<string, { w: number; h: number }>;
      sprites: { pedido: PedidoSprite; sprite: Sprite }[];
      atlas: { estilo: EstiloTexto; altura_canvas: number; atlas: Atlas }[];
    };
    const lista = casos<E, unknown>("primitivas-efeitos.json");
    const erros = conferirCasos(lista, (e) => {
      const q: QuadroPalco = {
        q: e.quadro.q,
        t: e.quadro.t,
        pts: e.quadro.pts,
        A: e.quadro.a,
        canvas: e.quadro.canvas,
        rastreio: (id) => e.quadro.rastreios[id] ?? null,
        // o falso do Rust registra pedido e resposta; o pedido do espelho tem de ser o mesmo
        sprite: (p) => e.sprites.find((s) => diferenca(p, s.pedido) === null)?.sprite ?? null,
        atlas: (estilo, h) => e.atlas.find((x) => x.altura_canvas === h && diferenca(estilo, x.estilo) === null)?.atlas ?? null,
        // imagem que o falso não conhece = ausente (tamanho 0)
        midia: (id) => e.imagens[id] ?? { w: 0, h: 0 },
        indice: e.quadro.indice ?? null,
        programa_s: e.quadro.programa_s ?? 0,
      };
      return primitivasOuErro(() => primitivas(e.efeito, q));
    });
    expect(erros).toEqual([]);
  });

  it("primitivas-seno.json: cos e sen sem a biblioteca", () => {
    const lista = casos<{ graus: number }, unknown>("primitivas-seno.json");
    expect(conferirCasos(lista, (e) => {
      const [cos, sen] = cosSenGraus(e.graus);
      return { cos, sen };
    })).toEqual([]);
  });

  it("toda fixture tempo-* e primitivas-* tem conferência aqui", () => {
    const sem = [...comPrefixo("tempo-").filter((n) => !TEMPO.includes(n)), ...comPrefixo("primitivas-").filter((n) => !PRIMITIVAS.includes(n))];
    expect(sem, "fixtures novas sem conferência no front (P0.8b)").toEqual([]);
  });
});
