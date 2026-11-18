/**
 * Tiras da V1 pelo comando do P4.3 com as regras do revisor dele: blocos visíveis (zoom profundo em `clipe#bloco`),
 * o `n` da resposta, `cancelada` só da última chave do canal, `tabela_pendente` até `tabela:<sha>` ficar pronta.
 */

import { describe, expect, it, vi } from "vitest";
import type { PedidoTira, RespostaTira, TarefaFundo, TiraMiniaturas } from "@domain/edicao";
import { alturaPedida, BLOCO_PX, blocosDoTrecho, chaveDoPedido, janelasDaTira, larguraPedida, Tiras } from "./tiras";

const SHA = "a".repeat(64);
const tira = (n: number, miniaturas: number[]): TiraMiniaturas => ({ relativo: `tiras/${SHA}/x.jpg`, w: 64, h: 36, n, de_s: 0, ate_s: 10, celulas: miniaturas.map((c) => ({ pts: c, t: c })), miniaturas });
const tarefa = (id: string, chave: string, estado: TarefaFundo["estado"]): TarefaFundo => ({ id, tipo: chave.startsWith("tabela") ? "tabela" : "tira", chave, feito: 1, estado });
const esperar = () => new Promise((r) => setTimeout(r, 0));

describe("blocos", () => {
  it("até BLOCO_PX: um bloco, canal = id do clipe; fora da tela: nenhum", () => {
    const b = blocosDoTrecho({ clipe: "c1", sha256: SHA, de_s: 10, ate_s: 20, largura: 800, altura: 40, vis0: -100, vis1: 900 });
    expect(b).toHaveLength(1);
    expect(b[0]!.canal).toBe("c1");
    expect(b[0]!.pedido).toMatchObject({ sha256: SHA, de_s: 10, ate_s: 20, n: null, largura: larguraPedida(800), altura: 40, canal: "c1" });
    expect(blocosDoTrecho({ clipe: "c1", sha256: SHA, de_s: 10, ate_s: 20, largura: 800, altura: 40, vis0: 900, vis1: 1500 })).toEqual([]);
    expect(blocosDoTrecho({ clipe: "c1", sha256: SHA, de_s: 10, ate_s: 20, largura: 20, altura: 40, vis0: 0, vis1: 20 })).toEqual([]);
  });

  it("zoom profundo: blocos de duração em potência de 2, só os visíveis, canal clipe#i, cobrindo a faixa sem buraco", () => {
    // 600 s em 120 000 px: 200 px/s → blocos de ~10 s → 8 s
    const todos = blocosDoTrecho({ clipe: "c1", sha256: SHA, de_s: 100, ate_s: 700, largura: 120_000, altura: 40, vis0: -1, vis1: 1e9 });
    expect(todos[0]!.pedido.ate_s - todos[0]!.pedido.de_s).toBe(8);
    expect(todos.every((b) => b.w <= 2 * BLOCO_PX)).toBe(true);
    expect(todos[0]!.pedido.de_s).toBe(100);
    expect(todos[todos.length - 1]!.pedido.ate_s).toBe(700);
    for (let i = 1; i < todos.length; i++) expect(todos[i]!.pedido.de_s).toBe(todos[i - 1]!.pedido.ate_s);
    const vis = blocosDoTrecho({ clipe: "c1", sha256: SHA, de_s: 100, ate_s: 700, largura: 120_000, altura: 40, vis0: 50_000, vis1: 51_400 });
    expect(vis.map((b) => b.canal)).toEqual(["c1#31", "c1#32"]);
    expect(vis[0]!.x).toBe(31 * 8 * 200);
  });

  it("largura e altura em degraus (o zoom contínuo não pede tira nova a cada passo)", () => {
    expect(larguraPedida(1000)).toBe(larguraPedida(1010));
    expect(larguraPedida(1000)).not.toBe(larguraPedida(1300));
    expect(alturaPedida(37, 1)).toBe(40);
    expect(alturaPedida(37, 2)).toBe(80);
    expect(alturaPedida(2, 1)).toBe(16);
    expect(alturaPedida(900, 1)).toBe(360);
  });

  it("janelas pelo n da resposta: célula na proporção, cortada na janela", () => {
    const t = tira(4, [0, 0, 1, 2]);
    const j = janelasDaTira(t, 400, 36);
    expect(j.map((x) => [x.celula, x.x, x.largura, x.sx])).toEqual([
      [0, 0, 64, 0],
      [0, 100, 64, 0],
      [1, 200, 64, 64],
      [2, 300, 64, 128],
    ]);
    const estreita = janelasDaTira(t, 100, 36);
    expect(estreita[0]!.largura).toBeCloseTo(25.5, 6);
    expect(estreita[0]!.sw).toBeCloseTo((25.5 / 64) * 64, 6);
    expect(janelasDaTira({ ...t, relativo: null }, 100, 36)).toEqual([]);
  });
});

describe("pedidos", () => {
  const pedido = (canal: string, largura = 512): PedidoTira => ({ sha256: SHA, de_s: 0, ate_s: 10, n: null, largura, altura: 40, canal });

  it("pronta no cache: uma chamada; a tela é avisada; mesmo pedido de novo não chama", async () => {
    const pedir = vi.fn(async (): Promise<RespostaTira> => ({ chave: "tira:k1", tira: tira(8, [0, 1, 2, 3, 4, 5, 6, 7]) }));
    const avisa = vi.fn();
    const t = new Tiras(pedir, avisa);
    t.pedir("/ws", pedido("c1"));
    await esperar();
    expect(t.tira(pedido("c1"))?.n).toBe(8);
    expect(avisa).toHaveBeenCalledTimes(1);
    t.pedir("/ws", pedido("c1"));
    // a tira não depende do canal
    expect(t.tira(pedido("outro"))?.n).toBe(8);
    expect(pedir).toHaveBeenCalledTimes(1);
  });

  it("na fila de fundo: pede de novo quando a chave do canal fica pronta", async () => {
    let n = 0;
    const pedir = vi.fn(async (): Promise<RespostaTira> => (n++ === 0 ? { chave: "tira:k1", tira: null } : { chave: "tira:k1", tira: tira(3, [0, 1, 2]) }));
    const t = new Tiras(pedir, () => {});
    t.pedir("/ws", pedido("c1"));
    await esperar();
    expect(t.situacao("c1")).toBe("fundo");
    t.receberFundo([tarefa("f1", "tira:k1", { tipo: "rodando" })]);
    expect(pedir).toHaveBeenCalledTimes(1);
    t.receberFundo([tarefa("f1", "tira:k1", { tipo: "pronta" })]);
    await esperar();
    expect(pedir).toHaveBeenCalledTimes(2);
    expect(t.tira(pedido("c1"))?.n).toBe(3);
    // a mesma lista de novo não pede outra vez
    t.receberFundo([tarefa("f1", "tira:k1", { tipo: "pronta" })]);
    expect(pedir).toHaveBeenCalledTimes(2);
  });

  it("cancelada: pede de novo só se a chave ainda é a última do canal", async () => {
    const respostas: Record<number, string> = { 512: "tira:velha", 1024: "tira:nova" };
    const pedir = vi.fn(async (_ws: string, p: PedidoTira): Promise<RespostaTira> => ({ chave: respostas[p.largura as number] as string, tira: null }));
    const t = new Tiras(pedir, () => {});
    t.pedir("/ws", pedido("c1", 512));
    await esperar();
    t.pedir("/ws", pedido("c1", 1024));
    await esperar();
    expect(pedir).toHaveBeenCalledTimes(2);
    // a velha foi cancelada pelo pedido novo do canal: nada
    t.receberFundo([tarefa("f1", "tira:velha", { tipo: "cancelada" })]);
    await esperar();
    expect(pedir).toHaveBeenCalledTimes(2);
    // a última do canal cancelada (fila limpa): pede de novo
    t.receberFundo([tarefa("f2", "tira:nova", { tipo: "cancelada" })]);
    await esperar();
    expect(pedir).toHaveBeenCalledTimes(3);
    expect(pedir.mock.calls[2]![1].largura).toBe(1024);
  });

  it("tabela_pendente: pede de novo quando tabela:<sha> fica pronta", async () => {
    let n = 0;
    const pedir = vi.fn(async (): Promise<RespostaTira> => {
      if (n++ === 0) throw { kind: "validation", message: "validation error: tabela_pendente: tira" };
      return { chave: "tira:k", tira: tira(2, [0, 1]) };
    });
    const t = new Tiras(pedir, () => {});
    t.pedir("/ws", pedido("c1"));
    await esperar();
    expect(t.situacao("c1")).toBe("tabela");
    t.receberFundo([tarefa("x", `tabela:${"b".repeat(64)}`, { tipo: "pronta" })]);
    await esperar();
    expect(pedir).toHaveBeenCalledTimes(1);
    t.receberFundo([tarefa("t1", `tabela:${SHA}`, { tipo: "pronta" })]);
    await esperar();
    expect(pedir).toHaveBeenCalledTimes(2);
    expect(t.tira(pedido("c1"))?.n).toBe(2);
  });

  it("outra falha: faixa lisa, sem repetir", async () => {
    const pedir = vi.fn(async (): Promise<RespostaTira> => {
      throw { kind: "validation", message: "validation error: nao_suportado: tira sem vídeo" };
    });
    const t = new Tiras(pedir, () => {});
    t.pedir("/ws", pedido("c1"));
    await esperar();
    expect(t.situacao("c1")).toBe("falhou");
    t.receberFundo([tarefa("t1", `tabela:${SHA}`, { tipo: "pronta" })]);
    await esperar();
    expect(pedir).toHaveBeenCalledTimes(1);
  });

  it("tarefa pronta antes de a resposta chegar: pede já, com teto (sem laço)", async () => {
    const pedir = vi.fn(async (): Promise<RespostaTira> => ({ chave: "tira:k", tira: null }));
    const t = new Tiras(pedir, () => {}, () => [tarefa("f", "tira:k", { tipo: "pronta" })]);
    t.pedir("/ws", pedido("c1"));
    for (let i = 0; i < 5; i++) await esperar();
    expect(pedir).toHaveBeenCalledTimes(3);
    expect(t.situacao("c1")).toBe("falhou");
  });

  it("chave do pedido sem o canal", () => {
    expect(chaveDoPedido(pedido("a"))).toBe(chaveDoPedido(pedido("b")));
    expect(chaveDoPedido(pedido("a", 512))).not.toBe(chaveDoPedido(pedido("a", 1024)));
  });
});
