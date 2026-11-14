/** Regras do §5.5 e §8.3 que valem qualquer que seja a geometria fina (essa é conferida pelas fixtures `primitivas-*`). */

import { describe, expect, it } from "vitest";
import type { AlvoRegiao, Caixa, EfCarimbo, EfDestaque, EfHolofote, EfImagem, EfLupa, EfOcultar, EfTexto, Efeito, EstiloTexto, QuadroPalco } from "@domain/edicao";
import { cosSenGraus, primitivas } from "./primitivas";
import type { Afim } from "../../modelo/geometria";

const base = { id: "e1", ligado: true, nome: "", ativo: null };
const estilo: EstiloTexto = { fonte: "sans", negrito: false, italico: false, tamanho: 0.05, cor: "#ffffff", alinhamento: "centro", caixa: null, contorno: null };
const alvo: AlvoRegiao = { rastreio: "r1", forma: "retangulo", vertices: null, margem: 0, borda_suave: 0.1, inverter: false };

function quadro(A: Afim, caixa: Caixa | null, extra: Partial<QuadroPalco> = {}): QuadroPalco {
  return {
    q: 12,
    t: 3725.5,
    pts: 100,
    indice: null,
    programa_s: 0,
    A,
    canvas: { w: 1280, h: 720 },
    rastreio: (id) => (id === "r1" ? caixa : null),
    sprite: () => ({ relativo: "sprites/a.png", w: 128, h: 36 }),
    atlas: () => ({ relativo: "atlas/b.png", celula: { w: 20, h: 36 }, glifos: "0123456789:,/ " }),
    midia: () => ({ w: 400, h: 200 }),
    ...extra,
  };
}
// metade do tamanho, deslocada: a origem ocupa [0,25; 0,75] do canvas
const MEIO: Afim = [0.5, 0, 0, 0.5, 0.25, 0.25];
const r: Caixa = { x: 0.2, y: 0.4, w: 0.2, h: 0.2 };

describe("primitivas", () => {
  it("ocultar: região no espaço da FONTE, sem A(τ); sólido leva a cor", () => {
    const e: EfOcultar = { ...base, tipo: "ocultar", modo: "solido", alvo, intensidade: 1, cor: "#000000ff", conteudo: "placa", motivo: "" };
    const [p] = primitivas(e, quadro(MEIO, r));
    expect(p).toMatchObject({ k: "regiao", modo: "solido", espaco: "fonte", cor: "#000000ff", borda_suave: 0.1, inverter: false });
    expect(p && p.k === "regiao" && p.forma).toEqual({ k: "caminho", d: [["M", 0.2, 0.4], ["L", 0.4, 0.4], ["L", 0.4, 0.6000000000000001], ["L", 0.2, 0.6000000000000001], ["Z"]] });
    const desfoque = primitivas({ ...e, modo: "desfoque" }, quadro(MEIO, r))[0];
    expect(desfoque && "cor" in desfoque).toBe(false);
  });

  it("holofote: região no canvas (passa por A), escurece fora", () => {
    const e: EfHolofote = { ...base, tipo: "holofote", alvo: { ...alvo, forma: "elipse" }, escurecer: 0.6, dessaturar: 0.5 };
    const [p] = primitivas(e, quadro(MEIO, r));
    expect(p).toMatchObject({ k: "regiao", modo: "escurecer", espaco: "canvas", inverter: true, intensidade: 0.6 });
    const f = p && p.k === "regiao" ? p.forma : null;
    expect(f?.k).toBe("elipse");
    if (f?.k === "elipse") [f.cx, f.cy, f.rx, f.ry].forEach((v, i) => expect(v).toBeCloseTo([0.4, 0.5, 0.05, 0.05][i]!, 12));
  });

  it("destaque retângulo sobre caixa: a caixa levada ao canvas; sem rastreio no quadro, nada", () => {
    const e: EfDestaque = { ...base, tipo: "destaque", forma: "retangulo", rastreio: "r1", traco: { cor: "#ffcc00", espessura: 0.006, contorno: true }, tamanho: 0.12, angulo_graus: 45, margem: 0, rotulo: null };
    const [p] = primitivas(e, quadro(MEIO, r));
    expect(p).toMatchObject({ k: "caminho", traco: { cor: "#ffcc00", largura: 0.006, contorno: "#000000bf" } });
    expect(p && p.k === "caminho" && p.d[0]).toEqual(["M", 0.35, 0.45]);
    expect(primitivas(e, quadro(MEIO, null))).toEqual([]);
    for (const forma of ["circulo", "elipse", "seta", "colchetes"] as const) {
      expect(primitivas({ ...e, forma }, quadro(MEIO, r)).length).toBe(1);
      expect(primitivas({ ...e, forma }, quadro(MEIO, { x: 0.5, y: 0.5, w: 0, h: 0 })).length).toBe(1);
    }
  });

  it("texto e imagem: sprite posto pelo lugar; sem sprite ainda, nada", () => {
    const e: EfTexto = { ...base, tipo: "texto", conteudo: "Rua A", estilo, lugar: { x: 0.5, y: 0.9, h: "centro", v: "base" }, largura_max: 0.8 };
    expect(primitivas(e, quadro(MEIO, r))).toEqual([{ k: "sprite", src: "sprites/a.png", caixa: { x: 0.45, y: 0.85, w: 0.1, h: 0.05 }, opacidade: 1 }]);
    expect(primitivas(e, quadro(MEIO, r, { sprite: () => null }))).toEqual([]);
    const im: EfImagem = { ...base, tipo: "imagem", midia: "m9", lugar: { x: 0, y: 0, h: "esq", v: "topo" }, largura: 0.25, opacidade: 0.5 };
    expect(primitivas(im, quadro(MEIO, r))).toEqual([{ k: "sprite", src: "midia:m9", caixa: { x: 0, y: 0, w: 0.25, h: (0.25 * 1280 * 200) / 400 / 720 }, opacidade: 0.5 }]);
  });

  it("carimbo: atlas com o prefixo fixo e o tempo", () => {
    const e: EfCarimbo = { ...base, tipo: "carimbo", mostra: "tempo_fonte", formato: "hms_ms", relogio: null, estilo, lugar: { x: 0.98, y: 0.02, h: "dir", v: "topo" } };
    const [p] = primitivas(e, quadro(MEIO, r));
    expect(p).toMatchObject({ k: "atlas", texto: "Original 01:02:05,500", altura: 0.05 });
  });

  it("carimbo de quadro e de programa pelo que o quadro traz; sem índice, nada", () => {
    const e: EfCarimbo = { ...base, tipo: "carimbo", mostra: "quadro", formato: "hms_ms", relogio: null, estilo, lugar: { x: 0.98, y: 0.02, h: "dir", v: "topo" } };
    expect(primitivas(e, { ...quadro(MEIO, r), indice: 312 })[0]).toMatchObject({ texto: "Quadro 312" });
    expect(primitivas(e, quadro(MEIO, r))).toEqual([]);
    expect(primitivas({ ...e, mostra: "tempo_programa", formato: "hms" }, { ...quadro(MEIO, r), programa_s: 4.004 })[0]).toMatchObject({ texto: "Vídeo 00:00:04" });
    // 86 300 s + (3 725,5 − 3 600) = 86 425,5 s: vira o dia (2028 é bissexto)
    const rel = { media_time_s: 3600, clock_seconds: 86300, clock_date: "2028-02-28" };
    expect(primitivas({ ...e, mostra: "relogio", formato: "data_hora", relogio: rel }, quadro(MEIO, r))[0]).toMatchObject({ texto: "Câmera 29/02/2028 00:00:25" });
    expect(primitivas({ ...e, mostra: "relogio", formato: "hms_ms", relogio: rel }, quadro(MEIO, r))[0]).toMatchObject({ texto: "Câmera 00:00:25,500" });
    expect(() => primitivas({ ...e, mostra: "relogio" }, quadro(MEIO, r))).toThrow(/^item_invalido/);
  });

  it("Fase 1 e silhueta: nao_suportado; polígono sem vértices: item_invalido", () => {
    const lupa: EfLupa = { ...base, tipo: "lupa", rastreio: "r1", janela: 0.2, fator: 2, forma: "circulo", interpolacao: "bilinear", onde: { modo: "fixa", x: 0.8, y: 0.2 }, moldura: { cor: "#fff", espessura: 0.004, contorno: true }, ligacao: true };
    expect(() => primitivas(lupa, quadro(MEIO, r))).toThrow(/^nao_suportado: lupa/);
    // desligada não é erro (igual ao Rust: `ligado` vem antes)
    expect(primitivas({ ...lupa, ligado: false }, quadro(MEIO, r))).toEqual([]);
    const oc: EfOcultar = { ...base, tipo: "ocultar", modo: "solido", alvo: { ...alvo, forma: "silhueta" }, intensidade: 1, cor: "#000000ff", conteudo: "rosto", motivo: "" };
    expect(() => primitivas(oc, quadro(MEIO, r))).toThrow(/^nao_suportado: silhueta/);
    expect(() => primitivas({ ...oc, alvo: { ...alvo, forma: "poligono", vertices: [{ x: 0, y: 0 }] } }, quadro(MEIO, r))).toThrow(/^item_invalido/);
    expect(primitivas({ ...oc, ligado: false }, quadro(MEIO, r))).toEqual([]);
  });

  it("cosSenGraus: exato nos ângulos retos, perto da biblioteca no resto", () => {
    expect(cosSenGraus(90)).toEqual([0, 1]);
    expect(cosSenGraus(180)).toEqual([-1, 0]);
    expect(cosSenGraus(-90)).toEqual([0, -1]);
    expect(cosSenGraus(Number.NaN)).toEqual([1, 0]);
    let pior = 0;
    for (let i = -3600; i <= 3600; i++) {
      const g = i * 0.137;
      const [c, s] = cosSenGraus(g);
      const rad = g * (Math.PI / 180);
      pior = Math.max(pior, Math.abs(c - Math.cos(rad)), Math.abs(s - Math.sin(rad)));
    }
    // a referência (Math.sin do motor JS) também erra; os bits são conferidos pela fixture do Rust
    expect(pior).toBeLessThan(4e-15);
  });

  it("efeitos de pixel não desenham", () => {
    const ajustes: Efeito = { ...base, tipo: "ajustes", brilho: 0, contraste: 1, gama: 1, saturacao: 1 };
    expect(primitivas(ajustes, quadro(MEIO, r))).toEqual([]);
  });
});
