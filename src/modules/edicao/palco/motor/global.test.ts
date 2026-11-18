/**
 * Camada global da prévia viva (P3.3): selos com os textos e as emendas de `camada/selos.rs` (velocidade, imagem
 * parada, ampliação, realce, média; na sobreposição de uma transição, os dos dois clipes) e a legenda queimada só
 * quando a última exportação queima.
 */

import { describe, expect, it } from "vitest";
import type { ClipeCongelar, ClipeTrecho, Legenda, ProjetoEdicao, TabelaQuadros } from "@domain/edicao";
import { novoProjeto, novoTrecho } from "../../modelo/projeto";
import { resolver, tempoDoQuadro } from "../../modelo/tempo";
import { faixasDosSelos, globalDe, textoDaVelocidade, textosNoSlot } from "./global";

const TAB: TabelaQuadros = { tb: { num: 1, den: 30 }, pts0: 0, passos: Array(299).fill(1), passo_mediano: 1, chaves: [0], vfr: false, origem: "decodificador" };
const CTX = { tabela: () => TAB };
const trecho = (id: string, j0: number, j1: number, fator = 1): ClipeTrecho => ({ ...novoTrecho("m", tempoDoQuadro(TAB, j0), tempoDoQuadro(TAB, j1)), id, velocidade: { tipo: "constante", fator } });

function projeto(principal: ProjetoEdicao["principal"]): ProjetoEdicao {
  const p = novoProjeto();
  p.saida.fps = { num: 30, den: 1 };
  p.midias = [{ id: "m", sha256: "a".repeat(64), tipo: "video", ocultacoes: [] } as unknown as ProjetoEdicao["midias"][0]];
  p.principal = principal;
  return p;
}

describe("selos", () => {
  it("textos do Rust: velocidade em três casas sem zeros, rampa, imagem parada, ampliada, tratada, média", () => {
    expect([0.5, 2, 0.333, 1.25].map(textoDaVelocidade)).toEqual(["Velocidade 0,5×", "Velocidade 2×", "Velocidade 0,333×", "Velocidade 1,25×"]);
    const a = trecho("a", 0, 30, 0.5);
    a.efeitos = [
      { id: "z", tipo: "enquadramento", ligado: true, nome: "", ativo: null, regiao: { x: 0.2, y: 0.2, w: 0.5, h: 0.5 }, ajuste: "cobrir", interpolacao: "bilinear" },
      { id: "n", tipo: "nitidez", ligado: true, nome: "", ativo: null, quantidade: 0.5 },
      { id: "mt", tipo: "media_temporal", ligado: true, nome: "", ativo: null, quadros: 8 },
    ];
    const g: ClipeCongelar = { tipo: "congelar", id: "g", rotulo: "", desligado: false, transicao_entrada: null, efeitos: [], midia: "m", instante_s: 0.5, instante_pts: 15, duracao_s: 1 };
    const b = trecho("b", 40, 70);
    b.velocidade = { tipo: "rampa", pontos: [{ t: tempoDoQuadro(TAB, 40), v: 1 }, { t: tempoDoQuadro(TAB, 60), v: 0.5 }] };
    const p = projeto([a, g, b, trecho("c", 100, 130)]);
    const f = faixasDosSelos(p, resolver(p, CTX));
    expect(f.map((x) => x.texto)).toEqual(["Velocidade 0,5× · Imagem ampliada · Imagem tratada · Média de 8 imagens", "Imagem parada", "Velocidade variável"]);
    // desligados na saída: só o fixo (média)
    p.saida.selos = { velocidade: false, ampliacao: false, realce: false };
    expect(faixasDosSelos(p, resolver(p, CTX)).map((x) => x.texto)).toEqual(["Média de 8 imagens"]);
  });

  it("na sobreposição de uma transição valem os selos dos dois clipes; depois, só os de quem entrou", () => {
    const a = trecho("a", 0, 60, 0.5);
    const b = trecho("b", 100, 160);
    b.transicao_entrada = { tipo: "fundir", duracao_s: 0.5, audio: "acompanhar" };
    b.efeitos = [{ id: "l", tipo: "lupa", ligado: true, nome: "", ativo: null, rastreio: "r", janela: 0.2, fator: 2, forma: "circulo", interpolacao: "bilinear", onde: { modo: "fixa", x: 0.8, y: 0.2 }, moldura: { cor: "#ffffff", espessura: 0.004, contorno: false }, ligacao: false }];
    const p = projeto([a, b]);
    const linha = resolver(p, CTX);
    const ini = linha.clipes[1]!.inicio_q;
    expect(faixasDosSelos(p, linha)).toEqual([
      { de: 0, ate: ini, texto: "Velocidade 0,5×" },
      { de: ini, ate: ini + 15, texto: "Velocidade 0,5× · Imagem ampliada" },
      { de: ini + 15, ate: linha.total_q, texto: "Imagem ampliada" },
    ]);
    const g = globalDe(p, linha);
    expect(textosNoSlot(p, g, ini + 3).map((e) => e.conteudo)).toEqual(["Velocidade 0,5× · Imagem ampliada"]);
    expect(textosNoSlot(p, g, ini + 3)[0]).toMatchObject({ lugar: { x: 0.98, y: 0.03, h: "dir", v: "topo" }, largura_max: 0.5, estilo: { tamanho: 0.03, caixa: { cor: "#000000b3" } } });
  });

  it("legenda queimada só quando a última exportação queima; por baixo do selo", () => {
    const p = projeto([trecho("a", 0, 90, 2)]);
    const l: Legenda = { id: "l1", texto: "Bom dia", locutor: null, origem: "perito", conferida: true, locutor_auto: false, ref: { tipo: "programa", de: 0.2, ate: 0.9 } };
    p.legendas.itens = [l];
    const linha = resolver(p, CTX);
    p.saida.exportacao.legendas = "arquivo";
    expect(textosNoSlot(p, globalDe(p, linha), 10).map((e) => e.id)).toEqual(["selo"]);
    p.saida.exportacao.legendas = "queimar";
    const v = textosNoSlot(p, globalDe(p, linha), 10);
    expect(v.map((e) => [e.id, e.conteudo])).toEqual([
      ["legenda", "Bom dia"],
      ["selo", "Velocidade 2×"],
    ]);
    expect(textosNoSlot(p, globalDe(p, linha), 40).map((e) => e.id)).toEqual(["selo"]);
  });
});
