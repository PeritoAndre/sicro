/**
 * Lâmina, Ctrl+B e "Congelar aqui" não mudam o programa (revisão P4.1): o que está preso ao clipe cortado fica no
 * mesmo slot; o que começa no pedaço da direita passa a ele (apagar o primeiro pedaço não leva o texto que está sobre o
 * segundo); "até o fim do clipe" não encolhe até o corte.
 */

import { describe, expect, it } from "vitest";
import type { ClipeAudio, ClipeTrecho, LinhaResolvida, ProjetoEdicao } from "@domain/edicao";
import { novoTitulo, novoClipeAudio, novaTrilhaNarracao } from "../inspetor/novos";
import { resolver } from "../modelo/tempo";
import { congelarAqui, dividirTrecho } from "../sequencia/contas";
import { removerComPresos } from "../sequencia/acoes";
import { dividirSemFonte } from "./arrastar";
import { marcadoresNaLinha } from "./modelo";
import { projetoSintetico, textoLivre, trechoNosQuadros, trilhaS } from "./testeLinha";

function montar(fator = 1) {
  const { doc, tab, ctx } = projetoSintetico();
  const c1 = { ...trechoNosQuadros("m1", tab, 0, 90, fator), id: "c1" } as ClipeTrecho;
  const c2 = { ...trechoNosQuadros("m1", tab, 100, 190), id: "c2" } as ClipeTrecho;
  doc.principal = [c1, c2];
  // depois do corte (slot 60 a 1×), "até o fim do clipe" desde antes do corte, e um que fica na esquerda
  doc.sobreposicoes = [trilhaS([{ ...textoLivre({ ref: "clipe", clipe: "c1", desloc: 2 }, 0.5), id: "depois" }, { ...textoLivre({ ref: "clipe", clipe: "c1", desloc: 0.5 }, null), id: "ateFim" }, { ...textoLivre({ ref: "clipe", clipe: "c1", desloc: 0.2 }, 0.3), id: "antes" }])];
  const a = novaTrilhaNarracao();
  a.clipes.push({ ...novoClipeAudio("m1", "n", { ref: "clipe", clipe: "c1", desloc: 2.5 }), id: "narr", ate_s: 1 } as ClipeAudio);
  doc.audios = [a];
  doc.marcadores = [{ id: "mk", rotulo: "", cor: "#5aa9e6", pos: { ref: "clipe", clipe: "c1", desloc: 2.2 } }];
  return { doc, tab, ctx };
}

const lugares = (l: LinhaResolvida) => Object.fromEntries(l.livres.map((x) => [x.id, [x.inicio_q, x.n_q]]));
const presos = (d: ProjetoEdicao) => Object.fromEntries([...d.sobreposicoes.flatMap((t) => t.clipes), ...d.audios.flatMap((t) => t.clipes)].map((c) => [(c as { id: string }).id, (c as { pos: { ref: string; clipe?: string } }).pos.clipe ?? null]));

describe("corte não muda o programa", () => {
  it("trecho (Ctrl+B e lâmina): presos no mesmo slot; os que começam na direita vão para o pedaço dela", () => {
    const { doc, ctx } = montar();
    const l0 = resolver(doc, ctx);
    const m0 = marcadoresNaLinha(doc, l0, ctx).map((m) => m.q);
    const dir = dividirTrecho(doc, "c1", 45, ctx.tabela("m1")!)!;
    const l1 = resolver(doc, ctx);
    expect(lugares(l1)).toEqual(lugares(l0));
    expect(marcadoresNaLinha(doc, l1, ctx).map((m) => m.q)).toEqual(m0);
    expect(presos(doc)).toEqual({ depois: dir, ateFim: "c1", antes: "c1", narr: dir });
    expect(doc.marcadores[0]!.pos).toMatchObject({ clipe: dir });
    // apagar o primeiro pedaço leva só o que começa nele
    removerComPresos(doc, "c1");
    expect(Object.keys(presos(doc)).sort()).toEqual(["depois", "narr"]);
    expect(doc.marcadores).toHaveLength(1);
  });

  it("trecho a 0,5×: os mesmos slots (o corte cai na fronteira do quadro)", () => {
    const { doc, ctx } = montar(0.5);
    const l0 = resolver(doc, ctx);
    dividirTrecho(doc, "c1", 37, ctx.tabela("m1")!);
    expect(lugares(resolver(doc, ctx))).toEqual(lugares(l0));
  });

  it("título (lâmina pela duração): os mesmos slots", () => {
    const { doc, ctx } = montar();
    const t = { ...novoTitulo(4), id: "c1" };
    doc.principal[0] = t;
    const l0 = resolver(doc, ctx);
    const dir = dividirSemFonte(doc, l0, "c1", 45)!;
    expect(lugares(resolver(doc, ctx))).toEqual(lugares(l0));
    expect(presos(doc)).toMatchObject({ depois: dir, ateFim: "c1", narr: dir });
  });

  it("congelar aqui: o que começa depois do corte anda com o pedaço da direita (fica sobre o mesmo quadro)", () => {
    const { doc, ctx } = montar();
    const l0 = resolver(doc, ctx);
    const ini0 = l0.livres.find((x) => x.id === "depois")!.inicio_q;
    const cong = congelarAqui(doc, "c1", 45, ctx.tabela("m1")!, 1)!;
    const l1 = resolver(doc, ctx);
    const n = l1.clipes.find((c) => c.id === cong)!.n_q;
    expect(l1.livres.find((x) => x.id === "depois")!.inicio_q).toBe(ini0 + n);
    expect(l1.livres.find((x) => x.id === "antes")!.inicio_q).toBe(l0.livres.find((x) => x.id === "antes")!.inicio_q);
  });
});
