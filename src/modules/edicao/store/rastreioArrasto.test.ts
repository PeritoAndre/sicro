// @vitest-environment happy-dom
/**
 * Marcas por quadro num efeito longo (1 h a 30 qps, denso inteiro): o arrasto de uma alça de âncora muda o rastreio
 * a cada passo, e as marcas (painel e faixa) não podem ser refeitas quadro a quadro em cada passo.
 */

import { beforeEach, describe, expect, it } from "vitest";
import { produce, type Draft } from "immer";
import type { ArquivoDenso, Chave, Caixa, ClipeTrecho, Efeito, ProjetoEdicao, Rastreio, TabelaQuadros } from "@domain/edicao";
import { marcasDoAlvo, useRastreioStore, type AlvoRastreio } from "./rastreioStore";
import { pedacos } from "../palco/FaixaRastreio";
import { tempoDoQuadro } from "../modelo/tempo";
import { podarAncorasDoPrograma } from "../modelo/rastreio";
import { projeto } from "../rastreio/cenario.teste";

/** Limite real de um passo do arrasto (marcas do painel + pedaços da faixa); medido com folga de 3× (O1). */
const LIMITE_PASSO_MS = 4;
const FOLGA = 3;

const N = 30 * 3600;
const PASSO = 512;
const tab: TabelaQuadros = { tb: { num: 1, den: 15360 }, pts0: 0, passos: Array(N - 1).fill(PASSO), passo_mediano: PASSO, chaves: [0], vfr: false, origem: "decodificador" };
const SHA = "f".repeat(64);
const CAIXA: Caixa = { x: 0.4, y: 0.4, w: 0.1, h: 0.2 };
const q = (j: number) => ({ t: tempoDoQuadro(tab, j), pts: j * PASSO });

function denso(): ArquivoDenso {
  const pts = Array.from({ length: N }, (_, j) => j * PASSO);
  const v = (x: number) => Array(N).fill(x);
  return {
    esquema: 2, rastreio: "r1", midia_sha256: "d".repeat(64), espaco: "original", pixel: { w: 640, h: 360 },
    pts, x: v(0.4), y: v(0.4), w: v(0.1), h: v(0.2), conf: v(0.9), estado: pts.map((_, j) => (j % 997 === 0 ? 1 : 0)), tipo: v(0), corrida: v(0), corridas: [],
  };
}

function alvo(ancoras: Chave<Caixa>[]): AlvoRastreio {
  const rastreio: Rastreio = {
    id: "r1", nome: "", cor: "#ffcc00", midia: "m", base: "fonte", tipo: "caixa", espaco: "original", ancoras,
    denso: { sha256: SHA, n: N, de_s: 0, ate_s: q(N - 1).t, ok: N, incertos: 0, ausentes: 0 }, silhuetas: null, ausencias: [], origem: { tipo: "assistido" }, confirmacao: null,
  };
  return { efeito: { id: "e1" } as Efeito, rastreio, clipe: { id: "c" } as ClipeTrecho, de: 0, ate: q(N - 1).t, ateExclusivo: false, tabela: tab };
}

beforeEach(() => {
  useRastreioStore.setState({ densos: { [SHA]: denso() } });
});

describe("arrasto em efeito de 1 h", () => {
  it("cada passo do arrasto de uma alça fica abaixo do limite (marcas e faixa)", () => {
    const ancoras = [{ ...q(30), v: CAIXA }, { ...q(54000), v: CAIXA }, { ...q(N - 10), v: CAIXA }];
    let t0 = performance.now();
    const mm0 = marcasDoAlvo(alvo(ancoras))!;
    const frio = performance.now() - t0;
    expect(mm0.m).toHaveLength(N);
    expect(mm0.m[30]).toBe("ancora");
    expect(mm0.m[997]).toBe("incerto");
    const tempos = mm0.m.map((_, i) => tempoDoQuadro(tab, mm0.j0 + i));
    const passos = 20;
    t0 = performance.now();
    for (let k = 1; k <= passos; k++) {
      // a alça move a caixa da âncora do meio (o mesmo quadro)
      const a = alvo(ancoras.map((x, i) => (i === 1 ? { ...x, v: { ...CAIXA, x: 0.4 + k / 1000 } } : x)));
      const mm = marcasDoAlvo(a)!;
      pedacos(mm.m, tempos, 0, a.ate, 1200);
    }
    const passo = (performance.now() - t0) / passos;
    console.log(`marcas de ${N} quadros: ${frio.toFixed(1)} ms a frio; passo do arrasto ${passo.toFixed(2)} ms`);
    expect(passo).toBeLessThan(LIMITE_PASSO_MS * FOLGA);
  });

  it("com milhares de âncoras do programa (Simplificar de 1 h), cada passo do arrasto poda a lista abaixo do limite", () => {
    const M = 8000;
    const doc = projeto();
    const r = doc.rastreios[0] as Rastreio;
    r.ancoras = Array.from({ length: M }, (_, j) => ({ ...q(j * 13), v: CAIXA }));
    r.ancoras_do_programa = r.ancoras.slice(1).map((k) => k.t);
    let antes: ProjetoEdicao = produce(doc, () => {});
    const mudar = (b: ProjetoEdicao, g: (d: Draft<ProjetoEdicao>) => void) => produce(b, g);
    const passos = 20;
    let gasto = 0;
    for (let k = 1; k <= passos; k++) {
      // a alça move a caixa da âncora do perito (a 0): a lista fica
      const depois = produce(antes, (d) => void (d.rastreios[0]!.ancoras[0]!.v = { ...CAIXA, x: 0.4 + k / 1000 }));
      const t0 = performance.now();
      antes = podarAncorasDoPrograma(antes, depois, mudar);
      gasto += performance.now() - t0;
    }
    const passo = gasto / passos;
    console.log(`podar ${M} âncoras do programa: ${passo.toFixed(2)} ms por passo`);
    expect(antes.rastreios[0]!.ancoras_do_programa).toHaveLength(M - 1);
    expect(passo).toBeLessThan(LIMITE_PASSO_MS * FOLGA);
    // a do programa arrastada sai (e só ela)
    const solta = produce(antes, (d) => void (d.rastreios[0]!.ancoras[7]!.v = { ...CAIXA, y: 0.3 }));
    expect(podarAncorasDoPrograma(antes, solta, mudar).rastreios[0]!.ancoras_do_programa).toEqual(r.ancoras.slice(1).map((k) => k.t).filter((_, i) => i !== 6));
  });
});
