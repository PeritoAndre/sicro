import { createElement } from "react";
import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it } from "vitest";
import type { TabelaQuadros } from "@domain/edicao";
import { CampoTempo } from "./CampoTempo";
import { formatarTempo, lerTempo, passoQuadro, quadroDe, type RefTempo } from "./tempoTexto";

/** 1/15360, 30 qps, começa em 2 s. */
const tabela: TabelaQuadros = { tb: { num: 1, den: 15360 }, pts0: 30720, passos: Array(399).fill(512), passo_mediano: 512, chaves: [0], vfr: false, origem: "pacotes" };
const fonte: RefTempo = { base: "fonte", tabela };
const programa: RefTempo = { base: "programa", fps: { num: 30000, den: 1001 } };

describe("CampoTempo: texto", () => {
  it("mostra HH:MM:SS,mmm", () => {
    expect(formatarTempo(12.345)).toBe("00:00:12,345");
    expect(formatarTempo(3723.25)).toBe("01:02:03,250");
    expect(formatarTempo(-0.5)).toBe("-00:00:00,500");
    expect(formatarTempo(0.0004)).toBe("00:00:00,000");
    expect(formatarTempo(1505762700.04)).toBe("418267:25:00,040");
  });

  it("aceita 12,5 · 1:02,5 · 1:02:03,250 · ponto decimal · sinal", () => {
    expect(lerTempo("12,5", fonte)).toEqual({ t: 12.5, pts: null });
    expect(lerTempo("12.5", fonte)).toEqual({ t: 12.5, pts: null });
    expect(lerTempo("1:02,5", fonte)).toEqual({ t: 62.5, pts: null });
    expect(lerTempo("01:02:03,250", fonte)).toEqual({ t: 3723.25, pts: null });
    expect(lerTempo(" 7 ", fonte)).toEqual({ t: 7, pts: null });
    expect(lerTempo("-0,5", fonte)).toEqual({ t: -0.5, pts: null });
    expect(lerTempo(formatarTempo(3723.25), fonte)).toEqual({ t: 3723.25, pts: null });
    for (const ruim of ["", "abc", "1:2:3:4", "12,5s", "#", "#-1", "1,2,3"]) expect(lerTempo(ruim, fonte)).toBeNull();
  });

  it("#312 pela tabela (fonte, com o pts) e pela taxa da saída (programa)", () => {
    expect(lerTempo("#0", fonte)).toEqual({ t: 2, pts: 30720 });
    expect(lerTempo("#312", fonte)).toEqual({ t: (30720 + 312 * 512) / 15360, pts: 30720 + 312 * 512 });
    expect(lerTempo("#400", fonte)).toBeNull();
    expect(lerTempo("#30", programa)).toEqual({ t: (30 * 1001) / 30000, pts: null });
    expect(lerTempo("#3", { base: "fonte" })).toBeNull();
  });

  it("quadro mostrado e passo de um quadro", () => {
    expect(quadroDe(2, fonte)).toBe(0);
    expect(quadroDe(2 + 312 / 30 + 0.01, fonte)).toBe(312);
    expect(quadroDe(1.001, programa)).toBe(30);
    const p = passoQuadro(2 + 10 / 30, 1, fonte);
    expect(p).toEqual({ t: (30720 + 11 * 512) / 15360, pts: 30720 + 11 * 512 });
    expect(passoQuadro(2, -1, fonte)).toEqual({ t: 2, pts: 30720 });
    // além do fim da mídia é o fim (↑ fica, sem pts)
    expect(passoQuadro(99, 1, fonte)).toEqual({ t: 99, pts: null });
    expect(passoQuadro(1, 1, { base: "fonte" })).toEqual({ t: 1.001, pts: null });
  });
});

describe("CampoTempo: fim da mídia", () => {
  it("no fim (o Fim de um trecho até o fim), ↓ vai ao último quadro e ↑ fica; nunca anda para o lado errado", () => {
    // 400 quadros a partir de 2 s: o fim é a fronteira depois do quadro 399
    const fim = (30720 + 400 * 512) / 15360;
    expect(passoQuadro(fim, -1, fonte)).toEqual({ t: (30720 + 399 * 512) / 15360, pts: 30720 + 399 * 512 });
    expect(passoQuadro(fim, 1, fonte)).toEqual({ t: fim, pts: null });
    // do último quadro, ↓ vai ao penúltimo
    expect(passoQuadro((30720 + 399 * 512) / 15360, -1, fonte).pts).toBe(30720 + 398 * 512);
  });
});

describe("CampoTempo: tela", () => {
  it("mostra o valor formatado, o quadro e o ícone da base; nenhum texto de exemplo", () => {
    const html = renderToStaticMarkup(createElement(CampoTempo, { label: "Início", value: 2 + 312 / 30, onChange: () => {}, base: "fonte", tabela }));
    expect(html).toContain('value="00:00:12,400"');
    expect(html).toContain("#312");
    expect(html).toContain("Início");
    expect(html).not.toMatch(/placeholder/i);
    expect(html).toContain("<svg");
    const vazio = renderToStaticMarkup(createElement(CampoTempo, { value: null, onChange: () => {}, base: "programa" }));
    expect(vazio).toContain('value=""');
    expect(vazio).not.toMatch(/placeholder|#\d/);
  });
});
