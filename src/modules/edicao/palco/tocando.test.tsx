// @vitest-environment happy-dom
/**
 * Editor inteiro com o palco tocando (o `<video>` do happy-dom: `play()` solta o `paused`, o tempo anda pelo teste):
 * "Congelar aqui" e "Dividir no cursor" pegam o quadro MOSTRADO, não o cursor que a store recebe a cada 250 ms; o
 * cursor postado tocando é o tempo do quadro, nunca a posição do elemento.
 */

import { act } from "react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { ClipeCongelar, ClipeTrecho, MidiaInfo, MidiaRef, ProjetoAberto, Rastreio, TabelaQuadros } from "@domain/edicao";

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
  edicaoEnfileirar: vi.fn(),
}));
vi.mock("@core/commands", () => ({ commands: cmd }));
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn(async () => () => {}) }));
vi.mock("@tauri-apps/api/core", () => ({ convertFileSrc: (p: string) => `asset://${p}`, invoke: vi.fn() }));

import { useEdicaoStore } from "../store/edicaoStore";
import { useDialogosAbertos } from "@components/Dialog/Dialog";
import { EditorEdicao } from "../EditorEdicao";
import { novoProjeto, novoTrecho } from "../modelo/projeto";
import { tempoDoQuadro } from "../modelo/tempo";
import { garantirMidia, useRecursos } from "./recursos";
import { useFundoStore } from "../store/fundoStore";
import { useRenderStore } from "../store/renderStore";
import { selecionarClipe } from "../sequencia/acoes";
import { meioDoQuadro } from "./faixa";
import { botao, clicar, desmontarTudo, esperar, montar } from "../inspetor/testeTela";
import { useInspetorUi } from "../inspetor/ui";

const WS = "/casos/toca";
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

beforeEach(async () => {
  for (const f of Object.values(cmd)) f.mockReset();
  cmd.edicaoAnalisar.mockImplementation(() => new Promise(() => {}));
  cmd.edicaoSalvar.mockImplementation(async () => ({ updated_at: new Date().toISOString() }));
  for (const f of [cmd.edicaoTabelaQuadros, cmd.edicaoMidiaInfo, cmd.edicaoSpriteTexto, cmd.edicaoAtlasCarimbo, cmd.edicaoQuadroFinal]) f.mockImplementation(() => new Promise(() => {}));
  cmd.edicaoFila.mockResolvedValue([]);
  cmd.edicaoFundo.mockResolvedValue([]);
  cmd.edicaoSaidas.mockResolvedValue([]);
  const p = novoProjeto();
  p.midias = [{ id: "m", sha256: SHA, tipo: "video", nome: "cam.mp4", relativo: "cam.mp4", vfr: false, ocultacoes: [] } as unknown as MidiaRef];
  p.principal = [{ ...novoTrecho("m", tempoDoQuadro(tab, 0), tempoDoQuadro(tab, 200)), id: "c", inicio_pts: 0, fim_pts: 200 * 512 } as ClipeTrecho];
  cmd.edicaoAbrir.mockResolvedValue({ projeto: p, importado_de: null, so_leitura: false, avisos: [], raiz_cache: "/cache/toca" } satisfies ProjetoAberto);
  await st().abrir(WS, p.id);
  useDialogosAbertos.setState({ pilha: [] });
  // o palco já tem a mídia (o caso dos recursos passa a ser este)
  garantirMidia(WS, SHA);
  useRecursos.setState((s) => ({ midias: { [SHA]: { info, tabela: tab, erro: null, pedindo: false } }, versao: s.versao + 1 }));
});

afterEach(async () => {
  desmontarTudo();
  useFundoStore.setState({ tarefas: [] });
  await st()
    .fechar({ descartar: true })
    .catch(() => {});
});

/** Monta o editor no clipe, com o elemento pronto e parado no primeiro quadro; devolve o `<video>`. */
async function noPalco() {
  const t = await montar(<EditorEdicao ativo />);
  await act(async () => selecionarClipe("c"));
  await espera(0);
  const v = t.raiz.querySelector<HTMLVideoElement>("video[data-elemento]")!;
  expect(v).toBeTruthy();
  await act(async () => void v.dispatchEvent(new Event("loadeddata")));
  await act(async () => void v.dispatchEvent(new Event("seeked")));
  await espera(50);
  return { t, v };
}

/** Toca e leva o elemento ao quadro j (o tempo do elemento anda), sem passar dos 250 ms do aviso à store. */
async function tocarAte(v: HTMLVideoElement, j: number) {
  await tecla(" ", "Space");
  expect(v.paused).toBe(false);
  v.currentTime = meioDoQuadro(tab, j);
  await espera(40);
}

describe("trava de marcação do palco (livre())", () => {
  it("I e Enter durante a busca não gravam o quadro velho; com o quadro na tela, gravam o dele (contratos (i))", async () => {
    const { t, v } = await noPalco();
    const inicio = () => (st().doc!.principal[0] as ClipeTrecho).inicio_pts;
    const proximo = async (chegou: boolean) => {
      await clicar(botao(t.raiz, "Próximo quadro"));
      if (chegou) {
        await act(async () => void v.dispatchEvent(new Event("seeked")));
        await espera(50);
      }
    };
    // quadro 1 na tela; o início segue no 0
    await proximo(true);
    expect(inicio()).toBe(0);
    // pedido o 2, ainda o 1 na tela: I não grava o 1
    await proximo(false);
    await tecla("i", "KeyI");
    expect(inicio()).toBe(0);
    await act(async () => void v.dispatchEvent(new Event("seeked")));
    await espera(50);
    await tecla("i", "KeyI");
    expect(inicio()).toBe(2 * 512);
    // Enter ("Marcar neste quadro") com a ferramenta de caixa do rastreio: âncora só no quadro 0
    const r = { id: "r1", nome: "", midia: "m", base: "fonte", tipo: "caixa", espaco: "original", ancoras: [{ t: tempoDoQuadro(tab, 0), pts: 0, v: { x: 0.1, y: 0.1, w: 0.2, h: 0.2 }, interp: "linear" }],
      denso: null, silhuetas: null, ausencias: [], origem: { tipo: "manual" }, confirmacao: null } as unknown as Rastreio;
    await act(async () => st().alterar("Rastreio", (d) => void d.rastreios.push(r)));
    await act(async () => useInspetorUi.getState().pedir({ tipo: "caixa", rastreio: "r1" }, () => {}, "teste"));
    const pts = () => st().doc!.rastreios[0]!.ancoras.map((a) => a.pts);
    // pedido o 3, ainda o 2 na tela: Enter não grava no 2
    await proximo(false);
    await tecla("Enter", "Enter");
    expect(pts()).toEqual([0]);
    await act(async () => void v.dispatchEvent(new Event("seeked")));
    await espera(50);
    await tecla("Enter", "Enter");
    expect(pts()).toEqual([0, 3 * 512]);
    // clique no palco com a ferramenta de lugar (o `onMarcar`): o mesmo
    const marcou = vi.fn();
    await act(async () => useInspetorUi.getState().pedir({ tipo: "lugar" }, marcou, "lugar"));
    const camada = t.raiz.querySelector<HTMLElement>("[data-camada]")!;
    const ponteiro = (tipo: string) => act(async () => void camada.dispatchEvent(new PointerEvent(tipo, { bubbles: true, button: 0, pointerId: 1, clientX: 0, clientY: 0 })));
    await clicar(botao(t.raiz, "Próximo quadro"));
    await ponteiro("pointerdown");
    await ponteiro("pointerup");
    expect(marcou).not.toHaveBeenCalled();
    await act(async () => void v.dispatchEvent(new Event("seeked")));
    await espera(50);
    await ponteiro("pointerdown");
    await ponteiro("pointerup");
    expect(marcou).toHaveBeenCalledTimes(1);
  });
});

describe("palco tocando", () => {
  it("Congelar aqui (atalho e botão) e Dividir no cursor pegam o quadro mostrado", async () => {
    const { t, v } = await noPalco();
    await tocarAte(v, 120);
    await tecla("B", "KeyB", { ctrlKey: true, shiftKey: true });
    const cong = st().doc!.principal.find((c) => c.tipo === "congelar") as ClipeCongelar;
    expect(cong.instante_pts).toBe(120 * 512);
    expect(v.paused).toBe(true);
    // pelo botão da Sequência
    st().desfazer();
    await act(async () => selecionarClipe("c"));
    await espera(0);
    await tocarAte(v, 90);
    await clicar(botao(t.raiz, "Congelar aqui"));
    expect((st().doc!.principal.find((c) => c.tipo === "congelar") as ClipeCongelar).instante_pts).toBe(90 * 512);
    // dividir
    st().desfazer();
    await act(async () => selecionarClipe("c"));
    await espera(0);
    await tocarAte(v, 60);
    await tecla("b", "KeyB", { ctrlKey: true });
    const partes = st().doc!.principal as ClipeTrecho[];
    expect(partes.map((c) => c.inicio_pts)).toEqual([0, 60 * 512]);
  });

  it("cópia de prévia que falhou na fila de fundo: ícone de vídeo ausente, não a espera sem fim", async () => {
    const semCopia: MidiaInfo = { ...info, previa: { ...info.previa, tipo: "copia", pronta: false } };
    useRecursos.setState((s) => ({ midias: { [SHA]: { info: semCopia, tabela: tab, erro: null, pedindo: false } }, versao: s.versao + 1 }));
    useFundoStore.setState({ tarefas: [{ id: "f1", tipo: "copia_previa", chave: `copia_previa:${SHA}`, feito: 0.4, estado: { tipo: "rodando" } }] });
    const t = await montar(<EditorEdicao ativo />);
    await act(async () => selecionarClipe("c"));
    await espera(0);
    expect(t.raiz.querySelector('[data-estado="preparando"]')).toBeTruthy();
    await act(async () => useFundoStore.setState({ tarefas: [{ id: "f1", tipo: "copia_previa", chave: `copia_previa:${SHA}`, feito: 0.4, estado: { tipo: "falhou", mensagem: "outra: remux" } }] }));
    expect(t.raiz.querySelector('[data-estado="preparando"]')).toBeNull();
    expect(t.raiz.querySelector('[data-estado="sem-video"]')).toBeTruthy();
  });

  it("Prévia do render que falha: ícone de falha no botão até pedir de novo (não volta calada à prévia viva)", async () => {
    cmd.edicaoEnfileirar.mockResolvedValue("t1");
    const t = await noPalco();
    const botaoPrevia = () => t.t.raiz.querySelector<HTMLButtonElement>('[data-acao="previa-render"]')!;
    await clicar(botaoPrevia());
    await espera(0);
    expect(cmd.edicaoEnfileirar).toHaveBeenCalledTimes(1);
    expect(cmd.edicaoEnfileirar.mock.calls[0]![2]).toMatchObject({ tipo: "previa" });
    const tarefa = (estado: unknown) => ({ id: "t1", tipo: "previa", caso: "o", projeto_id: st().doc!.id, titulo: "", criado_em: "", estado });
    await act(async () => useRenderStore.setState({ tarefas: [tarefa({ tipo: "falhou", mensagem: "outra: x", log_rel: "" })] as never }));
    expect(botaoPrevia().querySelector("[data-render-falhou]")).toBeTruthy();
    // pedir de novo tira o ícone
    cmd.edicaoEnfileirar.mockResolvedValue("t2");
    await clicar(botaoPrevia());
    await espera(0);
    expect(botaoPrevia().querySelector("[data-render-falhou]")).toBeNull();
    await act(async () => useRenderStore.setState({ tarefas: [] }));
  });

  it("o cursor que a store recebe tocando é o tempo do quadro mostrado (meio dele), não a posição do elemento", async () => {
    const { v } = await noPalco();
    await tecla(" ", "Space");
    // posição do elemento perto do fim do quadro 45
    v.currentTime = tempoDoQuadro(tab, 46) - 0.001;
    await espera(320);
    const c = st().cursor;
    expect(c).toEqual({ modo: "clipe", clipe: "c", t: meioDoQuadro(tab, 45) });
    await tecla(" ", "Space");
  });
});
