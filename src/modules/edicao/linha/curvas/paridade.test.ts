/**
 * Curvas: palco × render (P4.2, D7). O que o motor pintou, medido no render dos dois ffmpeg (`curvas-render.json`,
 * gravada por `edicao::testes::curvas::posicao_escala_rotacao_opacidade_no_render`), contra a conta do palco: os slots
 * pelo `resolver`, o valor pelas curvas (`livresNoQuadro` → `transformacaoEm` → `avaliar`), o centro e o giro da caixa
 * do texto por `cantosDoTexto` (as alças), os lados pelo sprite com a conta do pintor da prévia viva (P3.3
 * `palco/motor/pintor.ts::livre`: o objeto do tamanho do canvas, centro em x·W, y·H, escala, giro horário) e a imagem
 * pela mesma conta (largura escala·W). A até 1 px e 1°.
 */

import { describe, expect, it } from "vitest";
import type { Primitiva, ProjetoEdicao } from "@domain/edicao";
import { casos } from "../../modelo/testeFixtures";
import { resolver } from "../../modelo/tempo";
import { cantosDoTexto, livresNoQuadro } from "../../palco/livres";
import type { FontesCena } from "../../palco/cena";

type Medida = { q: number; cx: number; cy: number; maior: number; menor: number; angulo: number; opacidade: number };
/** `sprite`: tamanho e lados medidos (momentos do alfa, como no render) do sprite do texto que o Rust rasteriza. */
type Entrada = { projeto: ProjetoEdicao; sprite: { w: number; h: number; maior: number; menor: number }; imagem: { w: number; h: number } };

/** Diferença de ângulos de eixo (mod 180°). */
const difAngulo = (a: number, b: number) => {
  const d = (((a - b) % 180) + 180) % 180;
  return Math.min(d, 180 - d);
};

describe("curvas: palco × render (curvas-render.json, dois ffmpeg)", () => {
  it("texto livre e imagem com posição, escala, giro e opacidade animados: cada slot a até 1 px e 1°", () => {
    const [caso] = casos<Entrada, { texto: Medida[]; imagem: Medida[] }>("curvas-render.json");
    const { projeto: doc, sprite, imagem } = caso!.entrada;
    const W = doc.saida.largura;
    const H = doc.saida.altura;
    const linha = resolver(doc, { tabela: () => null });
    const fontes: FontesCena = { sprite: () => ({ relativo: "sprites/r.png", w: sprite.w, h: sprite.h }), atlas: () => null, midia: () => imagem };
    const no = (q: number, id: string) => livresNoQuadro(doc, linha, q, fontes).find((l) => l.clipe.id === id);
    const pior = { px: 0, grau: 0, lado: 0, op: 0 };
    const conferir = (nome: string, m: Medida, e: Omit<Medida, "q">) => {
      const px = Math.max(Math.abs(e.cx - m.cx), Math.abs(e.cy - m.cy));
      const grau = difAngulo(e.angulo, m.angulo);
      const lado = Math.max(Math.abs(e.maior - m.maior), Math.abs(e.menor - m.menor));
      const op = Math.abs(e.opacidade - m.opacidade);
      expect(px, `${nome} slot ${m.q}: centro`).toBeLessThanOrEqual(1);
      expect(grau, `${nome} slot ${m.q}: giro`).toBeLessThanOrEqual(1);
      expect(lado, `${nome} slot ${m.q}: lados`).toBeLessThanOrEqual(1);
      expect(op, `${nome} slot ${m.q}: opacidade`).toBeLessThanOrEqual(0.03);
      pior.px = Math.max(pior.px, px);
      pior.grau = Math.max(pior.grau, grau);
      pior.lado = Math.max(pior.lado, lado);
      pior.op = Math.max(pior.op, op);
    };

    // o palco mostra cada item nos mesmos slots que o render
    for (const [id, ms] of [["tl", caso!.saida.texto], ["im", caso!.saida.imagem]] as const) {
      const l = linha.livres.find((x) => x.id === id)!;
      expect([l.inicio_q, l.inicio_q + l.n_q], id).toEqual([ms[0]!.q, ms.at(-1)!.q + 1]);
      expect(no(l.inicio_q - 1, id), id).toBeUndefined();
    }
    for (const m of caso!.saida.texto) {
      const it = no(m.q, "tl")!;
      const k = cantosDoTexto(it, { w: W, h: H }).map((p) => ({ x: p.x * W, y: p.y * H }));
      const [a, b, c, d] = k as [{ x: number; y: number }, { x: number; y: number }, { x: number; y: number }, { x: number; y: number }];
      void d;
      // a caixa das alças vai até a largura da caixa do texto; o que se pinta é o sprite, na escala do objeto
      const spr = it.primitivas.find((p): p is Extract<Primitiva, { k: "sprite" }> => p.k === "sprite")!;
      expect([spr.caixa.w * W, spr.caixa.h * H]).toEqual([sprite.w, sprite.h]);
      conferir("texto", m, {
        cx: (a.x + c.x) / 2,
        cy: (a.y + c.y) / 2,
        maior: sprite.maior * it.transformacao.escala,
        menor: sprite.menor * it.transformacao.escala,
        angulo: (Math.atan2(b.y - a.y, b.x - a.x) * 180) / Math.PI,
        opacidade: it.transformacao.opacidade,
      });
    }
    for (const m of caso!.saida.imagem) {
      const t = no(m.q, "im")!.transformacao;
      const lado = t.escala * W;
      conferir("imagem", m, { cx: t.x * W, cy: t.y * H, maior: lado, menor: (lado * imagem.h) / imagem.w, angulo: t.rotacao, opacidade: t.opacidade });
    }
    // as curvas andaram (um palco parado no primeiro valor não passaria)
    const faixa = (ms: Medida[], k: keyof Medida) => Math.max(...ms.map((m) => m[k])) - Math.min(...ms.map((m) => m[k]));
    expect(faixa(caso!.saida.texto, "cx")).toBeGreaterThan(30);
    expect(faixa(caso!.saida.texto, "angulo")).toBeGreaterThan(40);
    expect(faixa(caso!.saida.texto, "maior")).toBeGreaterThan(15);
    expect(faixa(caso!.saida.imagem, "opacidade")).toBeGreaterThan(0.4);
    expect(pior.px).toBeLessThan(1);
  });
});
