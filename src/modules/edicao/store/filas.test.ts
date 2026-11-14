import { beforeEach, describe, expect, it, vi } from "vitest";
import type { Progresso, Tarefa, TarefaFundo } from "@domain/edicao";
import { EVENTO_FILA, EVENTO_FUNDO, EVENTO_PROGRESSO } from "@domain/edicao";

const ev = vi.hoisted(() => ({ ouvintes: new Map<string, (e: { payload: unknown }) => void>() }));
vi.mock("@tauri-apps/api/event", () => ({
  listen: vi.fn(async (nome: string, f: (e: { payload: unknown }) => void) => {
    ev.ouvintes.set(nome, f);
    return () => ev.ouvintes.delete(nome);
  }),
}));
const cmd = vi.hoisted(() => ({ edicaoFila: vi.fn(), edicaoFundo: vi.fn(), edicaoCancelar: vi.fn(), edicaoFilaLimpar: vi.fn(), edicaoEnfileirar: vi.fn() }));
vi.mock("@core/commands", () => ({ commands: cmd }));

import { tarefasDoCaso, tarefasDoProjeto, useRenderStore } from "./renderStore";
import { chaveFundo, fundoDaMidia, fundoPorChave, useFundoStore } from "./fundoStore";

const tarefa = (id: string, estado: Tarefa["estado"], caso = "c1", projeto = "p1"): Tarefa => ({
  id,
  tipo: "exportar",
  caso,
  projeto_id: projeto,
  titulo: "t",
  estado,
  criado_em: "",
});
const progresso = (id: string, feito: number): Progresso => ({ tarefa_id: id, fase: { tipo: "blocos", feitos: 1, total: 3 }, feito_q: feito, total_q: 900, qps: 120, restante_s: 6 });

describe("fila de render", () => {
  beforeEach(() => {
    useRenderStore.setState({ tarefas: [], progresso: {}, erro: null });
  });

  it("assina os eventos e lê a fila; a lista do evento substitui a anterior", async () => {
    cmd.edicaoFila.mockResolvedValue([tarefa("a", { tipo: "na_fila" })]);
    await useRenderStore.getState().ouvir();
    expect(useRenderStore.getState().tarefas.map((t) => t.id)).toEqual(["a"]);
    ev.ouvintes.get(EVENTO_FILA)!({ payload: [tarefa("a", { tipo: "preparando" }), tarefa("b", { tipo: "na_fila" }, "c2", "p2")] });
    const s = useRenderStore.getState();
    expect(s.tarefas.map((t) => t.estado.tipo)).toEqual(["preparando", "na_fila"]);
    expect(tarefasDoCaso(s.tarefas, "c2").map((t) => t.id)).toEqual(["b"]);
    expect(tarefasDoProjeto(s.tarefas, "p1").map((t) => t.id)).toEqual(["a"]);
  });

  it("progresso vira estado rodando; progresso atrasado não desfaz o fim", async () => {
    await useRenderStore.getState().ouvir();
    ev.ouvintes.get(EVENTO_FILA)!({ payload: [tarefa("a", { tipo: "preparando" })] });
    ev.ouvintes.get(EVENTO_PROGRESSO)!({ payload: progresso("a", 300) });
    let t = useRenderStore.getState().tarefas[0]!;
    expect(t.estado).toEqual({ tipo: "rodando", fase: { tipo: "blocos", feitos: 1, total: 3 }, feito_q: 300, total_q: 900, qps: 120, restante_s: 6 });
    expect(useRenderStore.getState().progresso.a?.feito_q).toBe(300);
    ev.ouvintes.get(EVENTO_FILA)!({ payload: [tarefa("a", { tipo: "concluida", saida_id: "s1" })] });
    ev.ouvintes.get(EVENTO_PROGRESSO)!({ payload: progresso("a", 600) });
    t = useRenderStore.getState().tarefas[0]!;
    expect(t.estado).toEqual({ tipo: "concluida", saida_id: "s1" });
    expect(useRenderStore.getState().progresso.a).toBeUndefined();
    // tarefa que não está na lista: ignorado
    ev.ouvintes.get(EVENTO_PROGRESSO)!({ payload: progresso("zz", 1) });
    expect(useRenderStore.getState().tarefas).toHaveLength(1);
  });

  it("erro de comando guarda o código", async () => {
    cmd.edicaoCancelar.mockRejectedValueOnce({ kind: "validation", message: "validation error: nao_encontrada: Tarefa não existe" });
    await expect(useRenderStore.getState().cancelar("x")).rejects.toBeTruthy();
    expect(useRenderStore.getState().erro).toEqual({ codigo: "nao_encontrada", frase: "Tarefa não existe" });
  });
});

describe("fila de fundo", () => {
  const sha = "f".repeat(64);
  const t = (id: string, chave: string, estado: TarefaFundo["estado"], feito = 0): TarefaFundo => ({ id, tipo: "tabela", chave, feito, estado });

  it("assina o evento; o cartão acha a sua pela chave", async () => {
    cmd.edicaoFundo.mockResolvedValue([]);
    await useFundoStore.getState().ouvir();
    ev.ouvintes.get(EVENTO_FUNDO)!({
      payload: [t("1", chaveFundo("tabela", sha), { tipo: "falhou", mensagem: "x" }), t("2", chaveFundo("tabela", sha), { tipo: "rodando" }, 0.4), t("3", chaveFundo("copia_previa", "e".repeat(64)), { tipo: "na_fila" })],
    });
    const lista = useFundoStore.getState().tarefas;
    expect(fundoPorChave(lista, `tabela:${sha}`)?.feito).toBe(0.4);
    expect(fundoPorChave(lista, `proxy:${sha}`)).toBeNull();
    expect(fundoDaMidia(lista, sha).map((x) => x.id)).toEqual(["1", "2"]);
  });
});
