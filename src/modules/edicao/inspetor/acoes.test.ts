/** Ações do Inspetor sem tela: região marcada no início e no fim, giro, marcação genérica do palco, efeito removido
 *  leva junto o rastreio que só ele usava. */

import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { EfEnquadramento, EfGirar, EfTexto, ProjetoAberto, ProjetoEdicao, ClipeTrecho } from "@domain/edicao";

const cmd = vi.hoisted(() => ({ edicaoAbrir: vi.fn(), edicaoSalvar: vi.fn(), edicaoAnalisar: vi.fn(), edicaoExcluir: vi.fn() }));
vi.mock("@core/commands", () => ({ commands: cmd }));
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn(async () => () => {}) }));

import { useEdicaoStore } from "../store/edicaoStore";
import { ler } from "../modelo/testeFixtures";
import { adicionarEfeito, aplicarMarcado, marcarRegiao, mudarGiro, removerEfeito, tirarFimRegiao } from "./acoes";

const st = () => useEdicaoStore.getState();
const trecho = (i: number) => st().doc!.principal[i] as ClipeTrecho;
const efeito = <E>(i: number, tipo: string) => trecho(i).efeitos.find((e) => (e as { tipo: string }).tipo === tipo) as E | undefined;

beforeEach(async () => {
  vi.useFakeTimers({ now: new Date("2026-11-14T10:00:00Z") });
  for (const f of Object.values(cmd)) f.mockReset();
  cmd.edicaoAnalisar.mockImplementation(() => new Promise(() => {}));
  const projeto = ler<ProjetoEdicao>("projeto-do-store.json");
  const r: ProjetoAberto = { projeto, importado_de: null, so_leitura: false, avisos: [], raiz_cache: "" };
  cmd.edicaoAbrir.mockResolvedValueOnce(r);
  await st().abrir("/c", projeto.id);
});
afterEach(async () => {
  await st().fechar({ descartar: true });
  vi.useRealTimers();
});

describe("ações", () => {
  it("região: fixa → marcar fim anima (2 chaves, sem vizinho) → marcar início troca a 1ª → tirar o fim volta a fixa", () => {
    const id = trecho(1).id;
    const e = () => efeito<EfEnquadramento>(1, "enquadramento")!;
    st().alterar("Interpolação", (d) => {
      const q = (d.principal[1] as ClipeTrecho).efeitos.find((x) => (x as { tipo: string }).tipo === "enquadramento") as EfEnquadramento;
      q.interpolacao = "vizinho";
    });
    marcarRegiao(id, "fim", 14, null, { x: 0.5, y: 0.5, w: 0.2, h: 0.2 }, "Marcar fim");
    expect(e().regiao).toEqual([
      { t: 12, v: { x: 0.25, y: 0.25, w: 0.5, h: 0.5 } },
      { t: 14, v: { x: 0.5, y: 0.5, w: 0.2, h: 0.2 } },
    ]);
    expect(e().interpolacao).toBe("bilinear");
    marcarRegiao(id, "inicio", 12.5, null, { x: 0, y: 0, w: 1, h: 1 }, "Marcar início");
    expect((e().regiao as { t: number }[]).map((k) => k.t)).toEqual([12.5, 14]);
    marcarRegiao(id, "fim", 15, null, { x: 0.1, y: 0.1, w: 0.5, h: 0.5 }, "Marcar fim");
    expect((e().regiao as { t: number }[]).map((k) => k.t)).toEqual([12.5, 15]);
    tirarFimRegiao(id, "Marcar fim");
    expect(e().regiao).toEqual({ x: 0, y: 0, w: 1, h: 1 });
    expect(st().passado.slice(-5).map((p) => p.rotulo)).toEqual(["Interpolação", "Marcar fim", "Marcar início", "Marcar fim", "Marcar fim"]);
  });

  it("região: a chave grava o pts que o palco entrega (contratos (h) 22)", () => {
    const id = trecho(1).id;
    const e = () => efeito<EfEnquadramento>(1, "enquadramento")!;
    marcarRegiao(id, "fim", 14, 215040, { x: 0.5, y: 0.5, w: 0.2, h: 0.2 }, "Marcar fim");
    const ks = e().regiao as { t: number; pts?: number | null }[];
    expect(ks.at(-1)).toMatchObject({ t: 14, pts: 215040 });
    marcarRegiao(id, "inicio", 12.5, 192000, { x: 0, y: 0, w: 1, h: 1 }, "Marcar início");
    expect((e().regiao as { pts?: number | null }[]).map((k) => k.pts)).toEqual([192000, 215040]);
  });

  it("sem enquadramento, marcar o fim cria um zoom a partir da imagem inteira", () => {
    st().alterar("Remover", (d) => void ((d.principal[2] as ClipeTrecho).efeitos = (d.principal[2] as ClipeTrecho).efeitos.filter((x) => (x as { tipo: string }).tipo !== "enquadramento")));
    marcarRegiao(trecho(2).id, "fim", 6, null, { x: 0.3, y: 0.3, w: 0.3, h: 0.3 }, "Marcar fim");
    expect(efeito<EfEnquadramento>(2, "enquadramento")!.regiao).toEqual([
      { t: 3, v: { x: 0, y: 0, w: 1, h: 1 } },
      { t: 6, v: { x: 0.3, y: 0.3, w: 0.3, h: 0.3 } },
    ]);
  });

  it("sem enquadramento, marcar o fim antes do começo do clipe (cursor fora da faixa) deixa as chaves em ordem", () => {
    st().alterar("Remover", (d) => void ((d.principal[2] as ClipeTrecho).efeitos = (d.principal[2] as ClipeTrecho).efeitos.filter((x) => (x as { tipo: string }).tipo !== "enquadramento")));
    marcarRegiao(trecho(2).id, "fim", 2, null, { x: 0.3, y: 0.3, w: 0.3, h: 0.3 }, "Marcar fim");
    expect((efeito<EfEnquadramento>(2, "enquadramento")!.regiao as { t: number }[]).map((k) => k.t)).toEqual([2, 3]);
  });

  it("girar: muda, espelha e some em 0° sem espelho", () => {
    const id = trecho(1).id;
    mudarGiro(id, { espelhar_v: true }, "Espelhar");
    expect(efeito<EfGirar>(1, "girar")).toMatchObject({ graus: 90, espelhar_v: true });
    mudarGiro(id, { graus: 0 }, "Girar");
    expect(efeito<EfGirar>(1, "girar")).toMatchObject({ graus: 0, espelhar_v: true });
    mudarGiro(id, { espelhar_v: false }, "Espelhar");
    expect(efeito<EfGirar>(1, "girar")).toBeUndefined();
  });

  it("marcação genérica do palco: âncora, lugar e região", () => {
    const d = efeito<{ rastreio: string; id: string }>(1, "destaque")!;
    aplicarMarcado({ tipo: "ponto", rastreio: d.rastreio }, { x: 0.4, y: 0.6 }, 13, 199680);
    expect(st().doc!.rastreios.find((r) => r.id === d.rastreio)!.ancoras.at(-1)).toEqual({ t: 13, pts: 199680, v: { x: 0.4, y: 0.6, w: 0, h: 0 } });
    const t = efeito<EfTexto>(2, "texto")!;
    aplicarMarcado({ tipo: "lugar", efeito: t.id }, { x: 0.2, y: 0.3, h: "esq", v: "topo" }, 4, null);
    expect(efeito<EfTexto>(2, "texto")!.lugar).toEqual({ x: 0.2, y: 0.3, h: "esq", v: "topo" });
    const q = efeito<EfEnquadramento>(1, "enquadramento")!;
    aplicarMarcado({ tipo: "regiao", efeito: q.id }, { x: 0.1, y: 0.1, w: 0.8, h: 0.45 }, 12, null);
    expect(efeito<EfEnquadramento>(1, "enquadramento")!.regiao).toEqual({ x: 0.1, y: 0.1, w: 0.8, h: 0.45 });
  });

  it("remover o efeito leva o rastreio que só ele usava; ocultar de congelar vai para a mídia, no instante do quadro (h 23)", () => {
    const d = efeito<{ rastreio: string; id: string }>(1, "destaque")!;
    removerEfeito(d.id, "Remover");
    expect(st().doc!.rastreios.some((r) => r.id === d.rastreio)).toBe(false);
    st().alterar("Congelar aqui", (x) =>
      void x.principal.push({ tipo: "congelar", id: "cg", rotulo: "", desligado: false, transicao_entrada: null, efeitos: [], midia: "m1", instante_s: 13, instante_pts: 199680, duracao_s: 2 }),
    );
    // cursor no congelado em tempo LOCAL: não pode virar o tempo da âncora de base fonte
    st().posicionar({ modo: "clipe", clipe: "cg", t: 0.5 });
    const n0 = st().doc!.midias.find((m) => m.id === "m1")!.ocultacoes.length;
    const id = adicionarEfeito("cg", "ocultar", "Ocultar")!;
    expect((st().doc!.principal.at(-1) as { efeitos: { id: string }[] }).efeitos).toEqual([]);
    const ocs = st().doc!.midias.find((m) => m.id === "m1")!.ocultacoes;
    expect(ocs.length).toBe(n0 + 1);
    const o = ocs.at(-1)!;
    expect(o.id).toBe(id);
    expect(o.ativo).toEqual({ de: 13, ate: 13 });
    const r = st().doc!.rastreios.find((x) => x.id === o.alvo.rastreio)!;
    expect([r.base, r.midia, r.ancoras]).toEqual(["fonte", "m1", [{ t: 13, pts: 199680, v: expect.any(Object) }]]);
    expect(st().selecao.efeito).toBe(id);
    // destaque no congelado: âncora também no instante dele
    const dz = adicionarEfeito("cg", "destaque", "Destaque")!;
    const rd = st().doc!.rastreios.find((x) => x.id === (st().doc!.principal.at(-1) as { efeitos: { id: string; rastreio: string }[] }).efeitos.find((e) => e.id === dz)!.rastreio)!;
    expect(rd.ancoras[0]).toMatchObject({ t: 13, pts: 199680 });
  });
});

describe("importar", () => {
  it("o diálogo oferece só as extensões que o armazém aceita (EXT_IMAGEM, EXT_AUDIO)", async () => {
    const { readFileSync } = await import("node:fs");
    const { EXTENSOES } = await import("./secoes/importar");
    const rs = readFileSync(new URL("../../../../src-tauri/src/edicao/armazem.rs", import.meta.url), "utf8");
    const lista = (nome: string) => [...(new RegExp(`const ${nome}: &\\[&str\\] = &\\[([^\\]]*)\\]`).exec(rs)?.[1] ?? "").matchAll(/"(\w+)"/g)].map((m) => m[1]).sort();
    expect([...EXTENSOES.imagem].sort()).toEqual(lista("EXT_IMAGEM"));
    expect([...EXTENSOES.audio].sort()).toEqual(lista("EXT_AUDIO"));
  });
});
