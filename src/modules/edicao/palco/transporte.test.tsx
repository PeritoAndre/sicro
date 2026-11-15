// @vitest-environment happy-dom
/**
 * Forma de onda no transporte (P1.9): colunas de px pelos baldes da onda (`edicao_onda`), pedida de novo quando a
 * tarefa `onda:<sha256>` fica pronta na fila de fundo; só na base de fonte; nenhum texto na tela.
 */

import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { createElement } from "react";
import type { ClipeTrecho, MidiaRef, Onda, TarefaFundo } from "@domain/edicao";

const cmd = vi.hoisted(() => ({ edicaoOnda: vi.fn(), edicaoFundo: vi.fn() }));
vi.mock("@core/commands", () => ({ commands: cmd }));
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn(async () => () => {}) }));

import { Transporte, caminhoDaOnda, colunasDaOnda, type TransporteProps } from "./Transporte";
import { useEdicaoStore } from "../store/edicaoStore";
import { useFundoStore } from "../store/fundoStore";
import { novoProjeto, novoTrecho } from "../modelo/projeto";
import { desmontarTudo, esperar, lerFixture, montar } from "../inspetor/testeTela";

const SHA = "a".repeat(64);

/** 10 s de som a partir de 1 s: silêncio, com ±20 000 de 3 a 4 s (baldes 200..299) e −32 768 no balde 950. */
function onda(): Onda {
  const min = Array<number>(1000).fill(0);
  const max = Array<number>(1000).fill(0);
  for (let i = 200; i < 300; i++) [min[i], max[i]] = [-20000, 20000];
  min[950] = -32768;
  return { inicio_s: 1, por_s: 100, min, max };
}

describe("colunasDaOnda", () => {
  it("cada px leva o mínimo e o máximo dos baldes que cobre; fora do som não há coluna", () => {
    // 0..20 s em 200 px: 0,1 s (10 baldes) por px; o som vai de 1 a 11 s
    const c = colunasDaOnda(onda(), 0, 20, 200);
    expect(c.map((x) => x.x)).toEqual(Array.from({ length: 100 }, (_, i) => 10 + i));
    const forte = c.filter((x) => x.max === 20000 && x.min === -20000).map((x) => x.x);
    expect(forte).toEqual(Array.from({ length: 10 }, (_, i) => 30 + i));
    expect(c.find((x) => x.x === 105)).toEqual({ x: 105, min: -32768, max: 0 });
    expect(c.filter((x) => x.max === 0 && x.min === 0)).toHaveLength(89);
  });

  it("com zoom (px menor que o balde) cada coluna leva o balde em que cai", () => {
    // 2,995..3,005 s em 40 px: os 20 primeiros no balde 199 (silêncio), os 20 últimos no 200 (forte)
    const c = colunasDaOnda(onda(), 2.995, 3.005, 40);
    expect(c).toHaveLength(40);
    expect(c.slice(0, 20).every((x) => x.max === 0)).toBe(true);
    expect(c.slice(20).every((x) => x.max === 20000 && x.min === -20000)).toBe(true);
  });

  it("balde cortado pela fronteira de dois px entra nos dois (o pico não some com a largura)", () => {
    // 0..20 s em 1999 px: a fronteira do px 1050 (10,5053 s) cai dentro do balde 950 (10,50 a 10,51 s)
    const c = colunasDaOnda(onda(), 0, 20, 1999);
    const em = (x: number) => c.find((k) => k.x === x)!.min;
    expect([em(1048), em(1049), em(1050), em(1051)]).toEqual([0, -32768, -32768, 0]);
  });

  it("onda vazia, largura zero ou faixa invertida: nada", () => {
    expect(colunasDaOnda({ inicio_s: 0, por_s: 100, min: [], max: [] }, 0, 10, 100)).toEqual([]);
    expect(colunasDaOnda(onda(), 0, 10, 0)).toEqual([]);
    expect(colunasDaOnda(onda(), 5, 5, 100)).toEqual([]);
  });

  it("fixture do Rust (`ondas-picos.json`): um balde por 80 amostras a 8 kHz, o último parcial; a pista inteira num px leva os extremos", () => {
    const casos = lerFixture<{ entrada: { taxa: number; amostras: number[] }; saida: Onda }[]>("ondas-picos.json");
    expect(casos).toHaveLength(1);
    const { entrada, saida } = casos[0]!;
    expect(Object.keys(saida).sort()).toEqual(["inicio_s", "max", "min", "por_s"]);
    const porBalde = entrada.taxa / saida.por_s;
    expect([saida.por_s, porBalde]).toEqual([100, 80]);
    const n = Math.ceil(entrada.amostras.length / porBalde);
    expect([saida.min.length, saida.max.length]).toEqual([n, n]);
    for (let i = 0; i < n; i++) {
      const balde = entrada.amostras.slice(i * porBalde, (i + 1) * porBalde);
      expect([saida.min[i], saida.max[i]]).toEqual([Math.min(...balde), Math.max(...balde)]);
    }
    const tudo = colunasDaOnda(saida, saida.inicio_s - 1, saida.inicio_s + 1, 1);
    expect(tudo).toEqual([{ x: 0, min: Math.min(...entrada.amostras), max: Math.max(...entrada.amostras) }]);
    // antes do som não há coluna
    expect(colunasDaOnda(saida, 0, saida.inicio_s, 50)).toEqual([]);
  });

  it("caminho: um traço por coluna na escala do maior pico, nunca mais fino que 1", () => {
    const d = caminhoDaOnda(
      [
        { x: 0, min: -20000, max: 20000 },
        { x: 1, min: 0, max: 0 },
      ],
      20000,
    );
    expect(d).toBe("M0.5 0.0V100.0M1.5 49.5V50.5");
    // som baixo não some: a escala é a do pico (mínimo 512)
    expect(caminhoDaOnda([{ x: 3, min: -256, max: 256 }], 256)).toBe("M3.5 25.0V75.0");
  });
});

const props = (p: Partial<TransporteProps> = {}): TransporteProps => ({
  de: 0,
  ate: 20,
  faixa: [2, 10],
  cursor: 3,
  quadro: 90,
  base: "fonte",
  tocando: false,
  bordas: true,
  fiel: "viva",
  render: "nada",
  expandido: false,
  onBuscar() {},
  onBorda() {},
  onTocar() {},
  onPasso() {},
  onInicio() {},
  onFim() {},
  onQuadroFinal() {},
  onPreviaRender() {},
  onAjustar() {},
  onExpandir() {},
  sha256: SHA,
  ...p,
});

describe("Transporte com a onda", () => {
  let largura: PropertyDescriptor | undefined;
  beforeEach(() => {
    cmd.edicaoOnda.mockReset();
    useEdicaoStore.setState({ ws: "/casos/onda" });
    useFundoStore.setState({ tarefas: [] });
    // a pista tem 200 px (o happy-dom não mede)
    largura = Object.getOwnPropertyDescriptor(HTMLElement.prototype, "clientWidth");
    Object.defineProperty(HTMLElement.prototype, "clientWidth", { configurable: true, get: () => 200 });
  });
  afterEach(() => {
    desmontarTudo();
    if (largura) Object.defineProperty(HTMLElement.prototype, "clientWidth", largura);
    useEdicaoStore.setState({ ws: null });
  });

  const tracos = (raiz: HTMLElement) => (raiz.querySelector("[data-onda] path")?.getAttribute("d") ?? "").split("M").length - 1;

  it("pedida à fila: aparece quando a tarefa `onda:<sha256>` fica pronta; sem texto nenhum", async () => {
    cmd.edicaoOnda.mockResolvedValueOnce(null).mockResolvedValueOnce(onda());
    const t = await montar(createElement(Transporte, props()));
    await esperar();
    expect(cmd.edicaoOnda).toHaveBeenCalledWith("/casos/onda", SHA);
    expect(t.raiz.querySelector("[data-onda]")).toBeNull();
    const pronta: TarefaFundo = { id: "t1", tipo: "onda", chave: `onda:${SHA}`, feito: 1, estado: { tipo: "pronta" } };
    await esperar(() => void useFundoStore.setState({ tarefas: [pronta] }));
    await esperar();
    expect(cmd.edicaoOnda).toHaveBeenCalledTimes(2);
    const svg = t.raiz.querySelector("[data-onda]")!;
    expect(svg).not.toBeNull();
    expect(svg.getAttribute("viewBox")).toBe("0 0 200 100");
    expect(tracos(t.raiz)).toBe(100);
    // dentro da pista, antes da faixa do clipe (fica por baixo)
    expect(svg.parentElement?.hasAttribute("data-pista")).toBe(true);
    expect(svg.nextElementSibling?.hasAttribute("data-faixa")).toBe(true);
    expect(svg.textContent).toBe("");
    expect(svg.querySelector("title, text")).toBeNull();
  });

  it("na base de programa (congelado, título) não pede nem desenha", async () => {
    cmd.edicaoOnda.mockResolvedValue(onda());
    // SHA que nenhum teste anterior guardou (as ondas recebidas ficam no módulo)
    const t = await montar(createElement(Transporte, props({ base: "programa", sha256: "d".repeat(64) })));
    await esperar();
    expect(cmd.edicaoOnda).not.toHaveBeenCalled();
    expect(t.raiz.querySelector("[data-onda]")).toBeNull();
  });

  it("erro do comando (ou comando ausente) não quebra o transporte", async () => {
    cmd.edicaoOnda.mockRejectedValue(new Error("midia_ausente: x"));
    const t = await montar(createElement(Transporte, props({ sha256: "b".repeat(64) })));
    await esperar();
    expect(t.raiz.querySelector("[data-pista]")).not.toBeNull();
    expect(t.raiz.querySelector("[data-onda]")).toBeNull();
  });

  it("sem a prop (como o Palco monta hoje): pede a onda da mídia do clipe do cursor, ou do clipe em foco", async () => {
    const antes = useEdicaoStore.getState();
    const [sha1, sha2] = ["e".repeat(64), "f".repeat(64)];
    const p = novoProjeto();
    p.midias = [
      { id: "m1", sha256: sha1, tipo: "video", nome: "a.mp4", relativo: "a.mp4", vfr: false, ocultacoes: [] } as unknown as MidiaRef,
      { id: "m2", sha256: sha2, tipo: "video", nome: "b.mp4", relativo: "b.mp4", vfr: false, ocultacoes: [] } as unknown as MidiaRef,
    ];
    p.principal = [{ ...novoTrecho("m1", 0, 5), id: "c1" } as ClipeTrecho, { ...novoTrecho("m2", 0, 5), id: "c2" } as ClipeTrecho];
    cmd.edicaoOnda.mockResolvedValue(onda());
    try {
      useEdicaoStore.setState({ doc: p, cursor: { modo: "clipe", clipe: "c2", t: 1 } });
      const semProp = props();
      delete semProp.sha256;
      const t = await montar(createElement(Transporte, semProp));
      await esperar();
      expect(cmd.edicaoOnda.mock.calls).toEqual([["/casos/onda", sha2]]);
      expect(tracos(t.raiz)).toBe(100);
      // cursor no programa: vale o clipe em foco
      await esperar(() => void useEdicaoStore.setState({ cursor: { modo: "programa", q: 0 }, selecao: { ...antes.selecao, foco: { k: "clipe", id: "c1" } } }));
      await esperar();
      expect(cmd.edicaoOnda.mock.calls.at(-1)).toEqual(["/casos/onda", sha1]);
      // sem clipe: nada a pedir, nada desenhado
      const pedidos = cmd.edicaoOnda.mock.calls.length;
      await esperar(() => void useEdicaoStore.setState({ selecao: antes.selecao }));
      await esperar();
      expect(cmd.edicaoOnda.mock.calls.length).toBe(pedidos);
      expect(t.raiz.querySelector("[data-onda]")).toBeNull();
    } finally {
      useEdicaoStore.setState({ doc: antes.doc, cursor: antes.cursor, selecao: antes.selecao });
    }
  });

  it("guardada por caso e mídia: montar de novo não pede outra vez", async () => {
    const sha = "c".repeat(64);
    cmd.edicaoOnda.mockResolvedValue(onda());
    const a = await montar(createElement(Transporte, props({ sha256: sha })));
    await esperar();
    expect(tracos(a.raiz)).toBe(100);
    a.desmontar();
    const b = await montar(createElement(Transporte, props({ sha256: sha })));
    await esperar();
    expect(cmd.edicaoOnda).toHaveBeenCalledTimes(1);
    expect(tracos(b.raiz)).toBe(100);
  });
});
