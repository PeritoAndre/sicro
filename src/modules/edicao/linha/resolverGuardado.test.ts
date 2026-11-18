/**
 * A medida guardada por clipe em `resolver` (P4.1, rodada 2) dá o mesmo resultado que medir tudo de novo: depois de
 * edições pelo immer, de mudanças no lugar (contas de corte) e de tabela nova da mídia.
 */

import { describe, expect, it } from "vitest";
import { produce, setAutoFreeze } from "immer";
import type { ClipeTrecho, ProjetoEdicao } from "@domain/edicao";
import { novoTitulo } from "../inspetor/novos";
import { resolver, type ContextoTempo } from "../modelo/tempo";
import { projetoSintetico, tabelaSintetica, trechoNosQuadros } from "./testeLinha";

/** Gerador determinístico. */
function sorteio(semente: number) {
  let x = semente >>> 0;
  return () => ((x = (x * 1664525 + 1013904223) >>> 0) / 2 ** 32);
}

// as mudanças no lugar pedem objetos não congelados (como os dos testes e das contas de corte)
setAutoFreeze(false);

const doZero = (doc: ProjetoEdicao, ctx: ContextoTempo) => resolver(structuredClone(doc), ctx);

describe("resolver com a medida guardada", () => {
  it("200 edições sorteadas (immer e no lugar): igual a resolver uma cópia nova", () => {
    const { doc: d0, tab, ctx: ctx0 } = projetoSintetico({ num: 30, den: 1 }, 600);
    let ctx = ctx0;
    let doc = d0;
    doc.principal = Array.from({ length: 12 }, (_, i) => ({ ...trechoNosQuadros("m1", tab, i * 40, i * 40 + 30 + (i % 3) * 5), id: `c${i}` }) as ClipeTrecho);
    doc.principal.splice(4, 0, { ...novoTitulo(1.5), id: "t" });
    const r = sorteio(7);
    for (let passo = 0; passo < 200; passo++) {
      const i = Math.floor(r() * doc.principal.length);
      const op = Math.floor(r() * 7);
      const mudar = (f: (c: ClipeTrecho) => void) => {
        const c = doc.principal[i] as ClipeTrecho;
        if (c.tipo === "trecho") f(c);
      };
      if (op === 0)
        doc = produce(doc, (d) => {
          const c = d.principal[i] as ClipeTrecho;
          if (c.tipo === "trecho") {
            c.fim_s = Math.max(c.inicio_s + 0.1, c.fim_s - 0.05);
            c.fim_pts = null;
          } else (c as unknown as { duracao_s: number }).duracao_s += 0.1;
        });
      else if (op === 1)
        doc = produce(doc, (d) => {
          const c = d.principal[i] as ClipeTrecho;
          if (c.tipo === "trecho") c.velocidade = { tipo: "constante", fator: [0.5, 1, 2][Math.floor(r() * 3)]! };
        });
      else if (op === 2)
        doc = produce(doc, (d) => {
          const [c] = d.principal.splice(i, 1);
          d.principal.splice(Math.floor(r() * d.principal.length), 0, c!);
        });
      // no lugar, fora do immer (como as contas de corte nos testes): o objeto é o mesmo, os campos mudam
      else if (op === 3)
        mudar((c) => {
          c.inicio_s += 0.02;
          c.inicio_pts = null;
        });
      else if (op === 4) mudar((c) => (c.velocidade = { tipo: "rampa", pontos: [{ t: c.inicio_s, v: 0.5 + r() }] }));
      else if (op === 5)
        mudar((c) => {
          if (c.velocidade.tipo === "rampa") c.velocidade.pontos[0]!.v = 0.25 + r();
        });
      // tabela nova da mídia (chegou a do decodificador)
      else {
        const t2 = tabelaSintetica(600, 30, Math.floor(r() * 3) * 512);
        ctx = { tabela: (id) => (id === "m1" ? t2 : null) };
      }
      expect(resolver(doc, ctx), `passo ${passo}, op ${op}`).toEqual(doZero(doc, ctx));
    }
  });

  // verificação 6.4: o sorteio quase nunca cai numa rampa já criada; aqui a chave da rampa é conferida sozinha
  it("ponto da rampa mudado no lugar (mesmo número de pontos) mede de novo", () => {
    const { doc, tab, ctx } = projetoSintetico();
    const c = { ...trechoNosQuadros("m1", tab, 0, 90), id: "a" } as ClipeTrecho;
    c.velocidade = { tipo: "rampa", pontos: [{ t: c.inicio_s, v: 1 }] };
    doc.principal = [c];
    const antes = resolver(doc, ctx).clipes[0]!.n_q;
    c.velocidade.pontos[0]!.v = 0.5;
    const depois = resolver(doc, ctx);
    expect(depois).toEqual(doZero(doc, ctx));
    expect(depois.clipes[0]!.n_q).toBe(2 * antes);
  });

  it("o clipe que não mudou devolve a mesma medida; a taxa de saída nova mede de novo", () => {
    const { doc, tab, ctx } = projetoSintetico();
    doc.principal = [{ ...trechoNosQuadros("m1", tab, 0, 90), id: "a" } as ClipeTrecho, { ...trechoNosQuadros("m1", tab, 100, 190), id: "b" } as ClipeTrecho];
    const l1 = resolver(doc, ctx);
    const doc2 = produce(doc, (d) => {
      const b = d.principal[1] as ClipeTrecho;
      b.fim_s -= 0.5;
      b.fim_pts = null;
    });
    const l2 = resolver(doc2, ctx);
    expect(l2.clipes[0]!.fonte).toBe(l1.clipes[0]!.fonte);
    expect(l2.clipes[1]!.n_q).toBe(l1.clipes[1]!.n_q - 15);
    const doc3 = produce(doc2, (d) => void (d.saida.fps = { num: 25, den: 1 }));
    expect(resolver(doc3, ctx)).toEqual(doZero(doc3, ctx));
  });
});
