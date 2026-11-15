// @vitest-environment happy-dom
/**
 * Editor inteiro, fecho da Fase 1 (§7.5, §7.6), com os comandos falsos:
 * - clipe com Lente: enquanto o intermediário de origem é feito, o palco toca o original, "Marcar" das anotações fica
 *   desligado (a ocultação não), há ícone no palco e progresso no cartão; pronto, o palco toca o intermediário e a
 *   anotação é marcada sobre ele; marcando `ocultar` ou fora da faixa do clipe, volta o original;
 * - a busca pausada é medida com a mídia parada (3 buscas); p50 acima de 120 ms pede o proxy e, pronto, o palco passa
 *   a tocá-lo; busca rápida não pede; clipe abaixo de 0,25× pede sem medir.
 */

import { act } from "react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { ClipeTrecho, EfLente, EstadoOrigem, MidiaInfo, MidiaRef, PreviaMidia, ProjetoAberto, ProjetoEdicao, TabelaQuadros } from "@domain/edicao";

const cmd = vi.hoisted(() => ({
  edicaoAbrir: vi.fn(),
  edicaoSalvar: vi.fn(),
  edicaoAnalisar: vi.fn(),
  edicaoTabelaQuadros: vi.fn(),
  edicaoMidiaInfo: vi.fn(),
  edicaoFila: vi.fn(),
  edicaoFundo: vi.fn(),
  edicaoSaidas: vi.fn(),
  edicaoSpriteTexto: vi.fn(),
  edicaoAtlasCarimbo: vi.fn(),
  edicaoQuadroFinal: vi.fn(),
  edicaoOrigemPedir: vi.fn(),
  edicaoOrigemEstado: vi.fn(),
  edicaoProxyPedir: vi.fn(),
  realceLenteObter: vi.fn(),
}));
vi.mock("@core/commands", () => ({ commands: cmd }));
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn(async () => () => {}) }));
vi.mock("@tauri-apps/api/core", () => ({ convertFileSrc: (p: string) => `asset://${p}`, invoke: vi.fn() }));

import { useEdicaoStore } from "../store/edicaoStore";
import { useDialogosAbertos } from "@components/Dialog/Dialog";
import { EditorEdicao } from "../EditorEdicao";
import { novoProjeto, novoTrecho } from "../modelo/projeto";
import { tempoDoQuadro } from "../modelo/tempo";
import { BUSCA_P50_MAX_MS, alvosDaMedida, garantirMidia, recursoDe, useRecursos } from "./recursos";
import { MEDIR_APOS_MS, esquecerMedidas } from "./Palco";
import { useFundoStore } from "../store/fundoStore";
import { useOrigemStore } from "../store/origemStore";
import { ESPERA, esquecerOrigens } from "../inspetor/origemDoClipe";
import { selecionarClipe } from "../sequencia/acoes";
import { ESPERA_DA_CONSULTA } from "../sequencia/origens";
import { meioDoQuadro } from "./faixa";
import { desmontarTudo, esperar, montar } from "../inspetor/testeTela";
import { useInspetorUi } from "../inspetor/ui";
import { pedirMarcacao } from "../inspetor/contexto";
import { novoDestaque, novoOcultar, novoRastreio } from "../inspetor/novos";

const SHA = "d".repeat(64);
const st = () => useEdicaoStore.getState();
/** 300 quadros a 30 qps em 1/15360. */
const tab: TabelaQuadros = { tb: { num: 1, den: 15360 }, pts0: 0, passos: Array(299).fill(512), passo_mediano: 512, chaves: [0], vfr: false, origem: "decodificador" };
const info: MidiaInfo = {
  id: "v1", sha256: SHA, nome: "cam.mp4", exibido: { w: 640, h: 360 }, taxa: { num: 30, den: 1 }, vfr: false, passo: { min: 1 / 30, max: 1 / 30 },
  de_s: 0, ate_s: 10, n: 300, tem_audio: false, previa: { relativo: "cam.mp4", tipo: "original", desloc_s: 0, de_s: 0, ate_s: 10, largura: 640, altura: 360, pronta: true },
  tabela: "decodificador", erros: 0, marcavel: true, cor: { matriz: "bt709", faixa: "tv" },
};
const tecla = (key: string, code: string, x: Partial<KeyboardEventInit> = {}) =>
  act(async () => void window.dispatchEvent(new KeyboardEvent("keydown", { key, code, bubbles: true, ...x })));
const espera = (ms: number) => esperar(() => new Promise((r) => setTimeout(r, ms)));

let WS = "";
let caso = 0;

/** Abre um projeto de um clipe (quadros 0 a 200), num caso novo (os pedidos de proxy do palco são por caso). */
async function abrir(mudar?: (p: ProjetoEdicao, c: ClipeTrecho) => void) {
  WS = `/casos/origem${++caso}`;
  const p = novoProjeto();
  p.midias = [{ id: "m", sha256: SHA, tipo: "video", nome: "cam.mp4", relativo: "cam.mp4", vfr: false, ocultacoes: [] } as unknown as MidiaRef];
  const c = { ...novoTrecho("m", tempoDoQuadro(tab, 0), tempoDoQuadro(tab, 200)), id: "c", inicio_pts: 0, fim_pts: 200 * 512 } as ClipeTrecho;
  p.principal = [c];
  mudar?.(p, c);
  cmd.edicaoAbrir.mockResolvedValue({ projeto: p, importado_de: null, so_leitura: false, avisos: [], raiz_cache: "/cache/x" } satisfies ProjetoAberto);
  await st().abrir(WS, p.id);
  useDialogosAbertos.setState({ pilha: [] });
  garantirMidia(WS, SHA);
  useRecursos.setState((s) => ({ midias: { [SHA]: { info, tabela: tab, erro: null, pedindo: false } }, versao: s.versao + 1 }));
}

beforeEach(() => {
  for (const f of Object.values(cmd)) f.mockReset();
  cmd.edicaoAnalisar.mockImplementation(() => new Promise(() => {}));
  cmd.edicaoSalvar.mockImplementation(async () => ({ updated_at: new Date().toISOString() }));
  for (const f of [cmd.edicaoTabelaQuadros, cmd.edicaoMidiaInfo, cmd.edicaoSpriteTexto, cmd.edicaoAtlasCarimbo, cmd.edicaoQuadroFinal]) f.mockImplementation(() => new Promise(() => {}));
  cmd.edicaoFila.mockResolvedValue([]);
  cmd.edicaoFundo.mockResolvedValue([]);
  cmd.edicaoSaidas.mockResolvedValue([]);
  cmd.edicaoOrigemEstado.mockResolvedValue(null);
  cmd.realceLenteObter.mockResolvedValue(null);
  esquecerMedidas();
  esquecerOrigens();
  useFundoStore.setState({ tarefas: [] });
  useInspetorUi.setState({ aba: "clipe", ferramenta: null, aoMarcar: null, chave: null });
});

afterEach(async () => {
  desmontarTudo();
  useFundoStore.setState({ tarefas: [] });
  await st()
    .fechar({ descartar: true })
    .catch(() => {});
});

const video = (raiz: HTMLElement) => raiz.querySelector<HTMLVideoElement>("video[data-elemento]")!;
const chegar = async (v: HTMLVideoElement) => {
  await act(async () => void v.dispatchEvent(new Event("seeked")));
  await espera(50);
};
/** Elemento carregado e o quadro pedido na tela. */
const carregar = async (v: HTMLVideoElement) => {
  await act(async () => void v.dispatchEvent(new Event("loadeddata")));
  await chegar(v);
};

async function noPalco() {
  const t = await montar(<EditorEdicao ativo />);
  await act(async () => selecionarClipe("c"));
  await espera(0);
  const v = video(t.raiz);
  expect(v).toBeTruthy();
  await carregar(v);
  return { t, v };
}

describe("intermediário de origem no palco (§7.5)", () => {
  const lente: EfLente = { id: "lente", tipo: "lente", ligado: true, nome: "", ativo: null, k1: -0.2, k2: 0, cx: 0.5, cy: 0.5, perfil: "cam" };
  const FAZENDO: EstadoOrigem = { tipo: "fazendo", tarefa: "t1", feito: 0.3 };
  const daOrigem: PreviaMidia = { relativo: "origem/abc.mp4", tipo: "origem", desloc_s: 0, de_s: 0, ate_s: tempoDoQuadro(tab, 200), largura: 640, altura: 360, pronta: true };
  const PRONTO: EstadoOrigem = { tipo: "pronto", previa: daOrigem, trf: null };

  async function comLente() {
    const r1 = { ...novoRastreio("m", 0), id: "r1", ancoras: [{ t: 0, pts: 0, v: { x: 0.1, y: 0.1, w: 0.2, h: 0.2 } }] };
    const r2 = { ...novoRastreio("m", 0), id: "r2", ancoras: [{ t: 0, pts: 0, v: { x: 0.5, y: 0.5, w: 0.2, h: 0.2 } }] };
    await abrir((p, c) => {
      p.rastreios = [r1, r2];
      c.efeitos = [lente, { ...novoDestaque("r1"), id: "dest" }, { ...novoOcultar("r2"), id: "ocul" }];
    });
    cmd.edicaoOrigemPedir.mockResolvedValue(FAZENDO);
    const { t, v } = await noPalco();
    // o palco pede o intermediário (a aba Clipe também está montada: um pedido só)
    await espera(ESPERA.mudanca + 80);
    return { t, v };
  }
  const abrirEfeito = (id: string) => act(async () => st().selecionar({ ...st().selecao, efeito: id }));
  const marcar = (t: { raiz: HTMLElement }) => t.raiz.querySelector<HTMLButtonElement>('[data-acao="marcar"]')!;
  const ancoras = (r: string) => st().doc!.rastreios.find((x) => x.id === r)!.ancoras.map((a) => a.pts);
  /** A fila de fundo termina a tarefa e o pedido seguinte responde pronto. */
  const terminar = async () => {
    cmd.edicaoOrigemPedir.mockResolvedValue(PRONTO);
    await act(async () => useFundoStore.setState({ tarefas: [{ id: "t1", tipo: "origem", chave: "origem:k", feito: 1, estado: { tipo: "pronta" } }] }));
    await espera(30);
  };

  it("em preparo: original no palco, 'Marcar' da anotação desligado (o da ocultação não), ícone no palco e progresso no cartão", async () => {
    const { t, v } = await comLente();
    // a seção Realce do P1.4 está na tela (linha "Estabilizar" oferecida) e também monta o pedido: sai UM só
    expect(t.raiz.querySelector('[data-inspetor] [data-realce="estabilizar"]')).not.toBeNull();
    expect(cmd.edicaoOrigemPedir).toHaveBeenCalledTimes(1);
    expect(cmd.edicaoOrigemPedir).toHaveBeenCalledWith(WS, st().doc, "c");
    expect(useOrigemStore.getState().porClipe.c).toEqual(FAZENDO);
    expect(video(t.raiz).getAttribute("src")).toContain("cam.mp4");
    expect(t.raiz.querySelector('[data-palco] [data-estado="origem-fazendo"]')).not.toBeNull();
    const barra = t.raiz.querySelector<HTMLElement>('[data-cartao="0"] [data-origem="fazendo"]')!;
    expect(barra.getAttribute("aria-valuenow")).toBe("30");
    expect((barra.firstElementChild as HTMLElement).style.width).toBe("30%");
    // progresso da fila de fundo: no cartão
    await act(async () => useFundoStore.setState({ tarefas: [{ id: "t1", tipo: "origem", chave: "origem:k", feito: 0.6, estado: { tipo: "rodando" } }] }));
    expect(t.raiz.querySelector('[data-cartao="0"] [data-origem="fazendo"]')!.getAttribute("aria-valuenow")).toBe("60");
    // anotação: botão desligado, o pedido não liga a ferramenta e o Enter não grava âncora
    await abrirEfeito("dest");
    expect(marcar(t).disabled).toBe(true);
    pedirMarcacao({ tipo: "caixa", rastreio: "r1", efeito: "dest" }, () => {}, "x");
    expect(useInspetorUi.getState().ferramenta).toBeNull();
    await act(async () => useInspetorUi.getState().pedir({ tipo: "caixa", rastreio: "r1", efeito: "dest" }, () => {}, "forcado"));
    await tecla("ArrowRight", "ArrowRight", { shiftKey: true });
    await chegar(v);
    await tecla("Enter", "Enter");
    expect(ancoras("r1")).toEqual([0]);
    await act(async () => useInspetorUi.getState().cancelarMarcacao());
    // ocultação: mora no original, marca já
    await abrirEfeito("ocul");
    expect(marcar(t).disabled).toBe(false);
    await act(async () => pedirMarcacao({ tipo: "caixa", rastreio: "r2", efeito: "ocul" }, () => {}, "o"));
    expect(useInspetorUi.getState().ferramenta).not.toBeNull();
    await tecla("Enter", "Enter");
    expect(ancoras("r2")).toEqual([0, 30 * 512]);
  });

  it("pronto: o palco toca o intermediário e a anotação é marcada sobre ele; marcando ocultar ou fora do clipe, o original", async () => {
    const { t } = await comLente();
    await abrirEfeito("dest");
    await terminar();
    expect(useOrigemStore.getState().porClipe.c).toEqual(PRONTO);
    // o elemento passa a tocar o arquivo do cache da Edição
    let v = video(t.raiz);
    expect(v.getAttribute("src")).toContain("/cache/x/origem/abc.mp4");
    expect(t.raiz.querySelector('[data-estado="origem-fazendo"]')).toBeNull();
    expect(t.raiz.querySelector('[data-cartao="0"] [data-origem]')).toBeNull();
    expect(marcar(t).disabled).toBe(false);
    // arquivo novo ainda sem o quadro: não marca; com ele na tela, marca
    await act(async () => pedirMarcacao({ tipo: "caixa", rastreio: "r1", efeito: "dest" }, () => {}, "d"));
    expect(useInspetorUi.getState().ferramenta).not.toBeNull();
    await tecla("Enter", "Enter");
    expect(ancoras("r1")).toEqual([0]);
    await carregar(v);
    await tecla("ArrowRight", "ArrowRight", { shiftKey: true });
    await chegar(v);
    await tecla("Enter", "Enter");
    expect(ancoras("r1")).toEqual([0, 30 * 512]);
    await act(async () => useInspetorUi.getState().cancelarMarcacao());
    // ocultar selecionado: a imagem do espaço ORIGINAL
    await abrirEfeito("ocul");
    v = video(t.raiz);
    expect(v.getAttribute("src")).toContain("cam.mp4");
    expect(v.getAttribute("src")).not.toContain("origem/");
    // de volta ao destaque, com o cursor fora da faixa do clipe: original, e a anotação não é marcada ali
    await abrirEfeito("dest");
    expect(video(t.raiz).getAttribute("src")).toContain("origem/abc.mp4");
    await act(async () => st().posicionar({ modo: "clipe", clipe: "c", t: meioDoQuadro(tab, 250) }));
    v = video(t.raiz);
    expect(v.getAttribute("src")).toContain("cam.mp4");
    await carregar(v);
    await act(async () => useInspetorUi.getState().pedir({ tipo: "caixa", rastreio: "r1", efeito: "dest" }, () => {}, "fora"));
    await tecla("Enter", "Enter");
    expect(ancoras("r1")).toEqual([0, 30 * 512]);
  });

  it("o que muda a imagem mudou (parâmetro da Lente): o intermediário pronto deixa de valer na hora; o palco volta ao original e a anotação para até o novo", async () => {
    const { t } = await comLente();
    await abrirEfeito("dest");
    await terminar();
    expect(video(t.raiz).getAttribute("src")).toContain("origem/abc.mp4");
    expect(marcar(t).disabled).toBe(false);
    // o pedido novo responde "em preparo" (só depois da espera)
    cmd.edicaoOrigemPedir.mockClear();
    cmd.edicaoOrigemPedir.mockResolvedValue({ tipo: "fazendo", tarefa: "t9", feito: 0 });
    await act(async () => st().alterar("Lente", (d) => void (((d.principal[0] as ClipeTrecho).efeitos[0] as EfLente).k1 = -0.3)));
    // já, antes de qualquer pedido
    expect(cmd.edicaoOrigemPedir).not.toHaveBeenCalled();
    expect(useOrigemStore.getState().porClipe.c).toBeUndefined();
    expect(video(t.raiz).getAttribute("src")).toContain("cam.mp4");
    expect(marcar(t).disabled).toBe(true);
    await espera(ESPERA.mudanca + 80);
    expect(cmd.edicaoOrigemPedir).toHaveBeenCalledTimes(1);
    expect(useOrigemStore.getState().porClipe.c).toMatchObject({ tipo: "fazendo", tarefa: "t9" });
    // voltar ao clipe sem mudar nada não derruba o intermediário pronto
    cmd.edicaoOrigemPedir.mockResolvedValue(PRONTO);
    await act(async () => useFundoStore.setState({ tarefas: [{ id: "t9", tipo: "origem", chave: "origem:k9", feito: 1, estado: { tipo: "pronta" } }] }));
    await espera(30);
    expect(useOrigemStore.getState().porClipe.c).toEqual(PRONTO);
    await act(async () => st().selecionar({ itens: [], foco: null, efeito: null }));
    await act(async () => st().posicionar({ modo: "programa", q: 0 }));
    expect(t.raiz.querySelector("video[data-elemento]")).toBeNull();
    await act(async () => selecionarClipe("c"));
    expect(useOrigemStore.getState().porClipe.c).toEqual(PRONTO);
    expect(video(t.raiz).getAttribute("src")).toContain("origem/abc.mp4");
  });

  it("o palco pede o intermediário mesmo com a seção Realce fora da tela (aba Saída aberta)", async () => {
    await abrir((_p, c) => void (c.efeitos = [lente]));
    cmd.edicaoOrigemPedir.mockResolvedValue(FAZENDO);
    useInspetorUi.setState({ aba: "saida" });
    const t = await montar(<EditorEdicao ativo />);
    // o inspetor troca para a aba Clipe ao focar o clipe: volta para Saída antes do pedido sair
    await act(async () => selecionarClipe("c"));
    await act(async () => useInspetorUi.getState().abrirAba("saida"));
    expect(t.raiz.querySelector('[data-painel="saida"]')).not.toBeNull();
    await espera(ESPERA.mudanca + 80);
    expect(cmd.edicaoOrigemPedir).toHaveBeenCalledTimes(1);
    expect(cmd.edicaoOrigemPedir).toHaveBeenCalledWith(WS, st().doc, "c");
    expect(useOrigemStore.getState().porClipe.c).toEqual(FAZENDO);
  });

  it("cartão de clipe fora do palco: a situação vem da consulta que não pede (edicao_origem_estado), com o progresso da fila", async () => {
    await abrir((p, c) => {
      c.efeitos = [lente];
      p.principal.push({ ...novoTrecho("m", tempoDoQuadro(tab, 200), tempoDoQuadro(tab, 260)), id: "d", inicio_pts: 200 * 512, fim_pts: 260 * 512, efeitos: [{ ...lente, id: "lente2" }] } as ClipeTrecho);
      p.principal.push({ ...novoTrecho("m", tempoDoQuadro(tab, 260), tempoDoQuadro(tab, 290)), id: "e", inicio_pts: 260 * 512, fim_pts: 290 * 512 } as ClipeTrecho);
    });
    cmd.edicaoOrigemPedir.mockResolvedValue(FAZENDO);
    cmd.edicaoOrigemEstado.mockResolvedValue({ tipo: "fazendo", tarefa: "t2", feito: 0.5 });
    const { t } = await noPalco();
    await espera(Math.max(ESPERA.mudanca, ESPERA_DA_CONSULTA) + 100);
    // só o clipe "d": o do palco tem quem peça; o "e" não tem efeito de origem
    expect(cmd.edicaoOrigemEstado.mock.calls.map((c) => c[2])).toEqual(["d"]);
    expect(cmd.edicaoOrigemEstado).toHaveBeenCalledWith(WS, st().doc, "d");
    expect(cmd.edicaoOrigemPedir.mock.calls.map((c) => c[2])).toEqual(["c"]);
    const barra = (i: number) => t.raiz.querySelector(`[data-cartao="${i}"] [data-origem="fazendo"]`);
    expect(barra(1)?.getAttribute("aria-valuenow")).toBe("50");
    expect(barra(2)).toBeNull();
    await act(async () => useFundoStore.setState({ tarefas: [{ id: "t2", tipo: "origem", chave: "origem:k2", feito: 0.8, estado: { tipo: "rodando" } }] }));
    expect(barra(1)?.getAttribute("aria-valuenow")).toBe("80");
    // a tarefa termina: nova consulta; falhou → ícone com a frase do catálogo
    cmd.edicaoOrigemEstado.mockRejectedValue({ kind: "validation", message: "validation error: fonte_danificada: Vídeo danificado" });
    await act(async () => useFundoStore.setState({ tarefas: [{ id: "t2", tipo: "origem", chave: "origem:k2", feito: 0.8, estado: { tipo: "falhou", mensagem: "fonte_danificada" } }] }));
    await espera(ESPERA_DA_CONSULTA + 100);
    expect(barra(1)).toBeNull();
    expect(t.raiz.querySelector('[data-cartao="1"] [data-origem="falhou"]')?.getAttribute("title")).toBe("Vídeo danificado");
  });

  it("clipe sem efeito que muda a imagem (só Brilho): nada espera e o palco não pede marcação parada", async () => {
    const r1 = { ...novoRastreio("m", 0), id: "r1", ancoras: [{ t: 0, pts: 0, v: { x: 0.1, y: 0.1, w: 0.2, h: 0.2 } }] };
    await abrir((p, c) => {
      p.rastreios = [r1];
      c.efeitos = [{ id: "aj", tipo: "ajustes", ligado: true, nome: "", ativo: null, brilho: 0.1, contraste: 1, gama: 1, saturacao: 1 }, { ...novoDestaque("r1"), id: "dest" }];
    });
    cmd.edicaoOrigemPedir.mockResolvedValue(FAZENDO);
    const { t, v } = await noPalco();
    await espera(ESPERA.mudanca + 80);
    await abrirEfeito("dest");
    expect(marcar(t).disabled).toBe(false);
    await act(async () => pedirMarcacao({ tipo: "caixa", rastreio: "r1", efeito: "dest" }, () => {}, "d"));
    await tecla("ArrowRight", "ArrowRight", { shiftKey: true });
    await chegar(v);
    await tecla("Enter", "Enter");
    expect(ancoras("r1")).toEqual([0, 30 * 512]);
  });
});

describe("busca medida e proxy pedido pelo palco (§7.6)", () => {
  const proxy: PreviaMidia = { relativo: "proxies/abc.720.mp4", tipo: "proxy", desloc_s: 0, de_s: 0, ate_s: 10, largura: 1280, altura: 720, pronta: true };
  const ALVOS = alvosDaMedida(tab, 0);
  const tocando = (t: { raiz: HTMLElement }) => t.raiz.querySelector('button[aria-label="Reproduzir"]')!.getAttribute("aria-pressed") === "true";

  /** Espera a medida começar e responde cada busca depois de `ms`; devolve os quadros buscados. */
  async function medir(v: HTMLVideoElement, ms: number) {
    await espera(MEDIR_APOS_MS + 60);
    const buscados: number[] = [];
    for (let i = 0; i < 3; i++) {
      buscados.push(v.currentTime);
      await espera(ms);
      await act(async () => void v.dispatchEvent(new Event("seeked")));
    }
    await espera(20);
    return buscados;
  }

  it("3 buscas pausadas com a mídia parada; p50 acima de 120 ms pede o proxy; pronto, o palco passa a tocá-lo", async () => {
    await abrir();
    cmd.edicaoProxyPedir.mockResolvedValue(info.previa);
    const { t, v } = await noPalco();
    expect(recursoDe(SHA).busca_p50_ms).toBeUndefined();
    expect(cmd.edicaoProxyPedir).not.toHaveBeenCalled();
    const buscados = await medir(v, BUSCA_P50_MAX_MS + 40);
    // os três alvos, no meio de cada quadro
    expect(ALVOS).toEqual([60, 150, 240]);
    expect(buscados).toEqual(ALVOS.map((j) => meioDoQuadro(tab, j)));
    const ms = recursoDe(SHA).busca_p50_ms!;
    expect(ms).toBeGreaterThan(BUSCA_P50_MAX_MS);
    expect(ms).toBeLessThan(BUSCA_P50_MAX_MS + 40 + 400);
    // pedido uma vez, com a medida; a resposta ainda é o original: nada muda no palco
    expect(cmd.edicaoProxyPedir).toHaveBeenCalledTimes(1);
    expect(cmd.edicaoProxyPedir).toHaveBeenCalledWith(WS, st().doc, "c", ms);
    expect(video(t.raiz).getAttribute("src")).toContain("cam.mp4");
    expect(t.raiz.querySelector('[data-estado="proxy"]')).toBeNull();
    // a medida voltou ao quadro que estava na tela
    expect(v.currentTime).toBe(meioDoQuadro(tab, 0));
    await chegar(v);
    // tocando quando o proxy fica pronto: o arquivo muda e o palco para no quadro mostrado
    await tecla(" ", "Space");
    expect(tocando(t)).toBe(true);
    v.currentTime = meioDoQuadro(tab, 40);
    await espera(40);
    cmd.edicaoProxyPedir.mockResolvedValue(proxy);
    await act(async () => useFundoStore.setState({ tarefas: [{ id: "f1", tipo: "proxy", chave: `proxy:${SHA}`, feito: 1, estado: { tipo: "pronta" } }] }));
    await espera(30);
    expect(cmd.edicaoProxyPedir).toHaveBeenCalledTimes(2);
    const v2 = video(t.raiz);
    expect(v2.getAttribute("src")).toContain("/cache/x/proxies/abc.720.mp4");
    expect(t.raiz.querySelector('[data-estado="proxy"]')).not.toBeNull();
    expect(tocando(t)).toBe(false);
    // o elemento novo vai ao quadro que estava na tela
    await carregar(v2);
    expect(v2.currentTime).toBe(meioDoQuadro(tab, 40));
    // medida uma vez por mídia: parado de novo, nenhuma busca a mais
    await espera(MEDIR_APOS_MS + 100);
    expect(v2.currentTime).toBe(meioDoQuadro(tab, 40));
    expect(cmd.edicaoProxyPedir).toHaveBeenCalledTimes(2);
  }, 15000);

  it("busca rápida: mede, não pede o proxy", async () => {
    await abrir();
    const { v } = await noPalco();
    await medir(v, 5);
    const ms = recursoDe(SHA).busca_p50_ms;
    expect(ms).toBeTypeOf("number");
    expect(ms!).toBeLessThan(BUSCA_P50_MAX_MS);
    await espera(30);
    expect(cmd.edicaoProxyPedir).not.toHaveBeenCalled();
  });

  it("o perito busca no meio da medida: ela é cancelada (sem número) e o quadro pedido vale", async () => {
    await abrir();
    const { t, v } = await noPalco();
    await espera(MEDIR_APOS_MS + 60);
    expect(v.currentTime).toBe(meioDoQuadro(tab, ALVOS[0]!));
    await tecla("ArrowRight", "ArrowRight", { shiftKey: true });
    expect(v.currentTime).toBe(meioDoQuadro(tab, 30));
    await chegar(v);
    expect(t.raiz.querySelector("[data-tempo]")!.textContent).toContain("quadro 30");
    expect(recursoDe(SHA).busca_p50_ms).toBeUndefined();
    expect(cmd.edicaoProxyPedir).not.toHaveBeenCalled();
  });

  it("edição no meio da medida: a medida é cancelada e o elemento volta ao quadro mostrado (a tela não fica parada)", async () => {
    await abrir();
    const { v } = await noPalco();
    await espera(MEDIR_APOS_MS + 60);
    expect(v.currentTime).toBe(meioDoQuadro(tab, ALVOS[0]!));
    // o perito muda o documento (a pintura é pedida)
    await act(async () => st().alterar("Nome", (d) => void ((d.principal[0] as ClipeTrecho).rotulo = "x")));
    expect(v.currentTime).toBe(meioDoQuadro(tab, 0));
    await chegar(v);
    expect(recursoDe(SHA).busca_p50_ms).toBeUndefined();
    // parado de novo, mede outra vez (até MEDIDAS_MAX tentativas)
    await espera(MEDIR_APOS_MS + 60);
    expect(v.currentTime).toBe(meioDoQuadro(tab, ALVOS[0]!));
  });

  it("clipe abaixo de 0,25×: pede o proxy sem esperar medida; a 0,25× não", async () => {
    await abrir((_p, c) => void (c.velocidade = { tipo: "constante", fator: 0.2 }));
    cmd.edicaoProxyPedir.mockResolvedValue(info.previa);
    await noPalco();
    expect(cmd.edicaoProxyPedir).toHaveBeenCalledTimes(1);
    expect(cmd.edicaoProxyPedir).toHaveBeenCalledWith(WS, st().doc, "c", null);
    desmontarTudo();
    await st().fechar({ descartar: true });
    cmd.edicaoProxyPedir.mockClear();
    await abrir((_p, c) => void (c.velocidade = { tipo: "constante", fator: 0.25 }));
    await noPalco();
    expect(cmd.edicaoProxyPedir).not.toHaveBeenCalled();
  });

  it("pedido que falha: ícone no palco com a frase do catálogo, o original segue tocando", async () => {
    await abrir((_p, c) => void (c.velocidade = { tipo: "constante", fator: 0.1 }));
    cmd.edicaoProxyPedir.mockRejectedValue({ kind: "validation", message: "validation error: disco_cheio: Disco cheio" });
    const { t } = await noPalco();
    const icone = t.raiz.querySelector('[data-estado="proxy-falhou"]');
    expect(icone?.getAttribute("title")).toBe("Disco cheio");
    expect(video(t.raiz).getAttribute("src")).toContain("cam.mp4");
  });
});
