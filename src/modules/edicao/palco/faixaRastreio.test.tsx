// @vitest-environment happy-dom
/**
 * Faixa do rastreio (P2.4): pedaços em px com o pior estado de cada coluna, medidos coluna a coluna; montada sobre o
 * efeito aberto, com o denso em cache, a ausência aceita, as âncoras e onde a corrida está. Sem texto.
 */

import { act } from "react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { ProjetoAberto } from "@domain/edicao";

const cmd = vi.hoisted(() => ({ edicaoAbrir: vi.fn(), edicaoSalvar: vi.fn(), edicaoAnalisar: vi.fn(), edicaoRastreioLer: vi.fn() }));
vi.mock("@core/commands", () => ({ commands: cmd }));
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn(async () => () => {}) }));

import { useEdicaoStore } from "../store/edicaoStore";
import { useRecursos } from "./recursos";
import { useInspetorUi } from "../inspetor/ui";
import { desmontarTudo, esperar, montar, textosVisiveis } from "../inspetor/testeTela";
import { esquecerRastreios, useRastreioStore } from "../store/rastreioStore";
import { FaixaRastreio, pedacos } from "./FaixaRastreio";
import { CAIXA, densoDe, faixa, info, PASSO, ponteiro, projeto, q, SHA, tab, WS } from "../rastreio/cenario.teste";

const st = () => useEdicaoStore.getState();
const A = densoDe("r1", [...faixa(30, 59), ...faixa(60, 64, 1), ...faixa(65, 79)]);
const SHA_A = "a".repeat(64);

describe("pedaços da faixa", () => {
  it("cada coluna fica com o pior estado; colunas iguais juntas; fora da escala não entra", () => {
    // 6 quadros em 3 colunas (2 por coluna)
    const t = [0, 1, 2, 3, 4, 5];
    expect(pedacos(["ok", "incerto", "ok", "ok", "aceita", "nada"], t, 0, 6, 3)).toEqual([
      { x0: 0, x1: 1, marca: "incerto" },
      { x0: 1, x1: 2, marca: "ok" },
      { x0: 2, x1: 3, marca: "ausente" },
    ]);
    expect(pedacos(["ancora", "ok", "ok", "ok"], [0, 1, 2, 3], 0, 4, 4)).toEqual([{ x0: 0, x1: 4, marca: "ok" }]);
    // quadro mais largo que a coluna: as colunas do meio ficam com ele
    expect(pedacos(["ok", "interpolado"], [0, 1], 0, 2, 8)).toEqual([
      { x0: 0, x1: 4, marca: "ok" },
      { x0: 4, x1: 5, marca: "interpolado" },
    ]);
    expect(pedacos(["ok"], [-1], 0, 2, 8)).toEqual([]);
    expect(pedacos([], [], 0, 2, 8)).toEqual([]);
  });
});

describe("FaixaRastreio montada", () => {
  beforeEach(async () => {
    for (const f of Object.values(cmd)) f.mockReset();
    cmd.edicaoSalvar.mockImplementation(async () => ({ updated_at: new Date().toISOString() }));
    cmd.edicaoAnalisar.mockImplementation(() => new Promise(() => {}));
    cmd.edicaoRastreioLer.mockResolvedValue(A);
    esquecerRastreios();
    useInspetorUi.setState({ ferramenta: null, aoMarcar: null, chave: null });
    const p = projeto();
    p.rastreios[0]!.denso = ponteiro(SHA_A, A);
    p.rastreios[0]!.origem = { tipo: "assistido" };
    cmd.edicaoAbrir.mockResolvedValue({ projeto: p, importado_de: null, so_leitura: false, avisos: [], raiz_cache: "/cache/r" } satisfies ProjetoAberto);
    await st().abrir(WS, p.id);
    useRecursos.setState((s) => ({ midias: { [SHA]: { info, tabela: tab, erro: null, pedindo: false } }, versao: s.versao + 1 }));
    st().posicionar({ modo: "clipe", clipe: "c", t: q(30).t });
  });
  afterEach(async () => {
    desmontarTudo();
    await st()
      .fechar({ descartar: true })
      .catch(() => {});
  });

  const rects = (raiz: HTMLElement, sel: string) =>
    [...raiz.querySelectorAll<SVGRectElement>(sel)].map((r) => [Number(r.getAttribute("x")), Number(r.getAttribute("width")), r.getAttribute("data-marca")]);

  it("sem efeito aberto, nada; com ele, os estados do denso na faixa do efeito, coluna a coluna", async () => {
    const t = await montar(<FaixaRastreio de={0} ate={10} largura={200} />);
    expect(t.raiz.querySelector("svg")).toBeNull();
    await act(async () => st().selecionar({ ...st().selecao, efeito: "e1" }));
    // o denso é lido na primeira pintura e a faixa refaz quando ele chega
    await esperar(() => new Promise((r) => setTimeout(r, 0)));
    expect(cmd.edicaoRastreioLer).toHaveBeenCalledTimes(1);
    const svg = t.raiz.querySelector<SVGElement>("[data-faixa-rastreio]")!;
    expect(svg.getAttribute("viewBox")).toBe("0 0 200 22");
    // 30 qps em 200 px por 10 s: x = chão(j·2/3); o efeito vale nos quadros 0..199
    expect(rects(t.raiz, "rect[data-marca]")).toEqual([
      [0, 20, "ausente"], // 0..29: antes da âncora, sem caixa
      [20, 20, "ok"], // âncora 30 e medidos 31..59
      [40, 3, "incerto"], // 60..64
      [43, 10, "ok"], // 65..79
      [53, 80, "ausente"], // 80..199: perdeu (rastreio assistido não segura a última)
    ]);
    expect(rects(t.raiz, "rect[data-ancora]").map(([x]) => x)).toEqual([19.5]);
    expect(textosVisiveis(t.raiz)).toEqual([]);
  });

  it("ausência aceita e corrida em andamento", async () => {
    await act(async () => st().selecionar({ ...st().selecao, efeito: "e1" }));
    const t = await montar(<FaixaRastreio de={0} ate={10} largura={200} />);
    await esperar(() => new Promise((r) => setTimeout(r, 0)));
    await act(async () =>
      st().alterar("Aceitar ausência", (d) => void d.rastreios[0]!.ausencias.push({ de_s: q(80).t, ate_s: q(109).t, motivo: "oculto", aceita_em: "", por: "" })),
    );
    expect(rects(t.raiz, "rect[data-marca]").slice(-2)).toEqual([
      [53, 20, "aceita"], // 80..109 (x 53..72)
      [73, 60, "ausente"], // 110..199
    ]);
    await act(async () =>
      useRastreioStore.setState({
        corrida: {
          tarefa: "t1", rastreio: "r1", pedido: { rastreio_id: "r1", clipe_id: "c", ancora: { ...q(30), v: CAIXA }, sentido: "frente", ate_s: q(199).t, motor: "auto" },
          ultimo: { tipo: "avanco", tarefa_id: "t1", rastreio_id: "r1", sentido: "frente", pts: 150 * PASSO, caixa: CAIXA, conf: 0.9, estado: 0, feitos: 120, miniatura: null },
        },
      }),
    );
    expect(Number(t.raiz.querySelector("rect[data-cabeca]")!.getAttribute("x"))).toBeCloseTo(100 - 0.5, 9);
  });
});
