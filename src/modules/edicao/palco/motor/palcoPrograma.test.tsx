// @vitest-environment happy-dom
/**
 * Editor inteiro com o palco no modo "Programa" (P3.3, D10): o modo segue o cursor; o seletor "Programa | Clipe"
 * converte o cursor pela linha resolvida (ida e volta no mesmo quadro); tocar e pausar devolvem o cursor ao store;
 * a ferramenta do Inspetor leva ao clipe dela; o Inspetor lê o tempo do clipe no slot; a área visível é publicada
 * ao mudar a vista sem repintar, e nula sem clipe.
 */

import { act } from "react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { Caixa, ClipeTrecho, MidiaInfo, MidiaRef, ProjetoAberto, TabelaQuadros } from "@domain/edicao";

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
  edicaoProxyPedir: vi.fn(),
  edicaoOnda: vi.fn(),
}));
vi.mock("@core/commands", () => ({ commands: cmd }));
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn(async () => () => {}) }));
vi.mock("@tauri-apps/api/core", () => ({ convertFileSrc: (p: string) => `asset://${p}`, invoke: vi.fn() }));
// a área visível de verdade é do pacote PE: aqui ela diz a vista que recebeu
vi.mock("../areaVisivel", async (original) => {
  const m = await original<typeof import("../areaVisivel")>();
  return { ...m, areaVisivelNaOrigem: (v: { s: number; tx: number; ty: number }) => ({ x: v.tx + 0, y: v.ty + 0, w: 1 / v.s, h: 1 / v.s }) };
});

import { useEdicaoStore } from "../../store/edicaoStore";
import { useDialogosAbertos } from "@components/Dialog/Dialog";
import { EditorEdicao } from "../../EditorEdicao";
import { novoProjeto, novoTrecho } from "../../modelo/projeto";
import { tempoDoQuadro } from "../../modelo/tempo";
import { garantirMidia, useRecursos } from "../recursos";
import { useFundoStore } from "../../store/fundoStore";
import { useAreaVisivel } from "../areaVisivel";
import { meioDoQuadro } from "../faixa";
import { tempoNoClipe } from "../../inspetor/acoes";
import { useInspetorUi } from "../../inspetor/ui";
import { botao, clicar, desmontarTudo, esperar, montar } from "../../inspetor/testeTela";
import { PREVIA_VIVA_PRONTA } from "./motor";
import { _paraTeste } from "../Palco";

const WS = "/casos/prog";
const SHA = "e".repeat(64);
const st = () => useEdicaoStore.getState();
const tab: TabelaQuadros = { tb: { num: 1, den: 15360 }, pts0: 0, passos: Array(299).fill(512), passo_mediano: 512, chaves: [0], vfr: false, origem: "decodificador" };
const info: MidiaInfo = {
  id: "v1", sha256: SHA, nome: "cam.mp4", exibido: { w: 640, h: 360 }, taxa: { num: 30, den: 1 }, vfr: false, passo: { min: 1 / 30, max: 1 / 30 },
  de_s: 0, ate_s: 10, n: 300, tem_audio: true, previa: { relativo: "cam.mp4", tipo: "original", desloc_s: 0, de_s: 0, ate_s: 10, largura: 640, altura: 360, pronta: true },
  tabela: "decodificador", erros: 0, marcavel: true, cor: { matriz: "bt709", faixa: "tv" },
};
const espera = (ms: number) => esperar(() => new Promise((r) => setTimeout(r, ms)));
const tecla = (key: string, code: string, extra: KeyboardEventInit = {}) => act(async () => void window.dispatchEvent(new KeyboardEvent("keydown", { key, code, bubbles: true, ...extra })));

beforeEach(async () => {
  for (const f of Object.values(cmd)) f.mockReset();
  cmd.edicaoAnalisar.mockImplementation(() => new Promise(() => {}));
  cmd.edicaoSalvar.mockImplementation(async () => ({ updated_at: new Date().toISOString() }));
  for (const f of [cmd.edicaoTabelaQuadros, cmd.edicaoMidiaInfo, cmd.edicaoSpriteTexto, cmd.edicaoAtlasCarimbo, cmd.edicaoQuadroFinal, cmd.edicaoProxyPedir, cmd.edicaoOnda]) f.mockImplementation(() => new Promise(() => {}));
  cmd.edicaoFila.mockResolvedValue([]);
  cmd.edicaoFundo.mockResolvedValue([]);
  cmd.edicaoSaidas.mockResolvedValue([]);
  const p = novoProjeto();
  p.midias = [{ id: "m", sha256: SHA, tipo: "video", nome: "cam.mp4", relativo: "cam.mp4", vfr: false, tem_audio: true, ocultacoes: [] } as unknown as MidiaRef];
  // a: quadros 0..60; b: 100..160 (corte para outro ponto do mesmo arquivo)
  p.principal = [
    { ...novoTrecho("m", tempoDoQuadro(tab, 0), tempoDoQuadro(tab, 60)), id: "a", rotulo: "Cena 1", inicio_pts: 0, fim_pts: 60 * 512 } as ClipeTrecho,
    { ...novoTrecho("m", tempoDoQuadro(tab, 100), tempoDoQuadro(tab, 160)), id: "b", rotulo: "Cena 2", inicio_pts: 100 * 512, fim_pts: 160 * 512 } as ClipeTrecho,
  ];
  cmd.edicaoAbrir.mockResolvedValue({ projeto: p, importado_de: null, so_leitura: false, avisos: [], raiz_cache: "/cache/prog" } satisfies ProjetoAberto);
  await st().abrir(WS, p.id);
  useDialogosAbertos.setState({ pilha: [] });
  garantirMidia(WS, SHA);
  useRecursos.setState((s) => ({ midias: { [SHA]: { info, tabela: tab, erro: null, pedindo: false } }, versao: s.versao + 1 }));
});

afterEach(async () => {
  desmontarTudo();
  vi.restoreAllMocks();
  useFundoStore.setState({ tarefas: [] });
  await st()
    .fechar({ descartar: true })
    .catch(() => {});
});

/** O happy-dom não carrega mídia: os elementos do pool com arquivo ficam com dados e cada busca pedida "chega". */
async function chegar(raiz: HTMLElement) {
  for (let vez = 0; vez < 3; vez++) {
    for (const v of [...raiz.querySelectorAll<HTMLVideoElement>("[data-pool-host] video")].filter((x) => !!x.src)) {
      Object.defineProperty(v, "readyState", { value: 4, configurable: true });
      await act(async () => void v.dispatchEvent(new Event("loadedmetadata")));
      await act(async () => void v.dispatchEvent(new Event("seeked")));
    }
    await espera(40);
  }
}

const modo = (raiz: HTMLElement) => raiz.querySelector("[data-modo-palco]")?.getAttribute("data-modo-palco");

describe("palco no modo Programa", () => {
  it("o projeto abre no programa; o pool fica dentro do palco; o nome é o do clipe sob o cursor", async () => {
    expect(PREVIA_VIVA_PRONTA).toBe(true);
    const t = await montar(<EditorEdicao ativo />);
    await espera(20);
    expect(modo(t.raiz)).toBe("programa");
    expect(botao(t.raiz, "Programa").getAttribute("aria-pressed")).toBe("true");
    const host = t.raiz.querySelector("[data-caixa] [data-pool-host]")!;
    expect(host.querySelectorAll("video").length).toBeGreaterThanOrEqual(4);
    expect(t.raiz.querySelector("[data-nome-clipe]")!.textContent).toBe("Cena 1");
    await act(async () => st().posicionar({ modo: "programa", q: 75 }));
    await espera(20);
    expect(t.raiz.querySelector("[data-nome-clipe]")!.textContent).toBe("Cena 2");
    // o transporte mostra o tempo do programa e o da fonte
    expect(t.raiz.querySelector("[data-tempo-fonte]")!.textContent).toContain("quadro 115");
  });

  it("seletor: programa → clipe no meio do quadro do slot, e de volta ao mesmo slot", async () => {
    const t = await montar(<EditorEdicao ativo />);
    await act(async () => st().posicionar({ modo: "programa", q: 77 }));
    await espera(20);
    await clicar(botao(t.raiz, "Clipe"));
    await espera(20);
    expect(st().cursor).toEqual({ modo: "clipe", clipe: "b", t: meioDoQuadro(tab, 117) });
    expect(st().selecao.foco).toEqual({ k: "clipe", id: "b" });
    expect(modo(t.raiz)).toBe("clipe");
    await clicar(botao(t.raiz, "Programa"));
    await espera(20);
    expect(st().cursor).toEqual({ modo: "programa", q: 77 });
    expect(modo(t.raiz)).toBe("programa");
  });

  it("Espaço toca pelo elemento do clipe e, pausado, o cursor volta ao store no slot do relógio", async () => {
    const t = await montar(<EditorEdicao ativo />);
    await act(async () => st().posicionar({ modo: "programa", q: 10 }));
    await espera(20);
    const tocar = t.raiz.querySelector<HTMLButtonElement>('button[aria-label="Reproduzir"]')!;
    await tecla(" ", "Space");
    await chegar(t.raiz);
    expect(tocar.getAttribute("aria-pressed")).toBe("true");
    const tocando = [...t.raiz.querySelectorAll<HTMLVideoElement>("[data-pool-host] video")].filter((v) => !v.paused);
    expect(tocando).toHaveLength(1);
    expect(tocando[0]!.src).toContain("cam.mp4");
    // o elemento que toca é o do quadro do slot (meio do quadro 10); o do clipe b fica parado na entrada dele
    const parados = [...t.raiz.querySelectorAll<HTMLVideoElement>("[data-pool-host] video")].filter((v) => v.paused && !!v.src);
    expect(parados.map((v) => v.currentTime)).toEqual([meioDoQuadro(tab, 100)]);
    await tecla(" ", "Space");
    await espera(30);
    expect(tocar.getAttribute("aria-pressed")).toBe("false");
    expect(st().cursor.modo).toBe("programa");
    expect([...t.raiz.querySelectorAll<HTMLVideoElement>("[data-pool-host] video")].every((v) => v.paused)).toBe(true);
  });

  it("aberto com o cursor no meio do programa, toca dali (o plano chega ao relógio antes do cursor)", async () => {
    await act(async () => st().posicionar({ modo: "programa", q: 75 }));
    const t = await montar(<EditorEdicao ativo />);
    await espera(20);
    await tecla(" ", "Space");
    await chegar(t.raiz);
    const tocando = [...t.raiz.querySelectorAll<HTMLVideoElement>("[data-pool-host] video")].filter((v) => !v.paused);
    expect(tocando.map((v) => v.currentTime)).toEqual([meioDoQuadro(tab, 115)]);
    await tecla(" ", "Space");
    await espera(30);
    expect(st().cursor).toEqual({ modo: "programa", q: 75 });
  });

  it("texto livre arrastado no palco, nos dois modos: um passo cada (livres.ts do TL)", async () => {
    const getContext = HTMLCanvasElement.prototype.getContext;
    HTMLCanvasElement.prototype.getContext = (() => new Proxy({}, { get: () => () => undefined, set: () => true })) as unknown as typeof getContext;
    vi.spyOn(HTMLElement.prototype, "clientWidth", "get").mockReturnValue(800);
    vi.spyOn(HTMLElement.prototype, "clientHeight", "get").mockReturnValue(450);
    cmd.edicaoSpriteTexto.mockImplementation(async () => ({ relativo: "sprites/t.png", w: 400, h: 60 }));
    const estilo = { fonte: "sans" as const, negrito: false, italico: false, tamanho: 0.052, cor: "#ffffff", alinhamento: "centro" as const, caixa: null, contorno: null };
    await act(async () =>
      st().alterar("Texto livre", (d) => {
        d.saida.largura = 1600;
        d.saida.altura = 900;
        d.sobreposicoes = [{ id: "T1", nome: "", oculta: false, clipes: [{ tipo: "titulo", id: "s1", rotulo: "", pos: { ref: "programa", t: 0 }, duracao_s: 3, entrada_s: 0, saida_s: 0, efeitos: [],
          transformacao: { x: 0.5, y: 0.5, escala: 1, rotacao: 0, opacidade: 1 },
          titulo: { modelo: "livre", fundo: { tipo: "transparente" }, blocos: [{ id: "b", papel: "corpo", conteudo: "Placa", estilo, lugar: { x: 0.5, y: 0.5, h: "centro", v: "meio" }, largura_max: 0.3 }] } }] }];
      }),
    );
    const t = await montar(<EditorEdicao ativo />);
    await act(async () => st().posicionar({ modo: "programa", q: 20 }));
    await espera(60);
    const arrastar = async (de: number, para: number) => {
      const camada = t.raiz.querySelector("[data-camada]")!;
      const ponteiro = (tipo: string, x: number) => act(async () => void camada.dispatchEvent(new PointerEvent(tipo, { clientX: x, clientY: 225, bubbles: true, button: 0, pointerId: 1, isPrimary: true })));
      await ponteiro("pointerdown", de);
      await ponteiro("pointermove", para);
      await ponteiro("pointerup", para);
      await espera(40);
    };
    const passos = st().passado.length;
    await arrastar(400, 480);
    expect(st().selecao.foco).toEqual({ k: "livre", id: "s1", trilha: "T1" });
    expect(st().doc!.sobreposicoes[0]!.clipes[0]).toMatchObject({ transformacao: { x: 0.6 } });
    expect(st().passado.length).toBe(passos + 1);
    expect(t.raiz.querySelector('[data-alca="livre:s1:giro"]')).toBeTruthy();
    // modo Clipe, mesmo quadro: o texto continua clicável e arrastável
    await clicar(botao(t.raiz, "Clipe"));
    await espera(60);
    expect(modo(t.raiz)).toBe("clipe");
    await arrastar(480, 360);
    expect(st().doc!.sobreposicoes[0]!.clipes[0]).toMatchObject({ transformacao: { x: 0.45 } });
    expect(st().passado.length).toBe(passos + 2);
    HTMLCanvasElement.prototype.getContext = getContext;
  });

  it("vista de origem do zoom que acompanha: a caixa anda com o rastreio (ZS)", async () => {
    const getContext = HTMLCanvasElement.prototype.getContext;
    HTMLCanvasElement.prototype.getContext = (() => new Proxy({}, { get: () => () => undefined, set: () => true })) as unknown as typeof getContext;
    vi.spyOn(HTMLElement.prototype, "clientWidth", "get").mockReturnValue(800);
    vi.spyOn(HTMLElement.prototype, "clientHeight", "get").mockReturnValue(450);
    // a pessoa anda da esquerda (quadro 0) à direita (quadro 60)
    const caixa = (x: number) => ({ x, y: 0.4, w: 0.1, h: 0.1 });
    await act(async () =>
      st().alterar("Acompanhar rastreio", (d) => {
        d.rastreios.push({ id: "r1", nome: "", cor: "#ffcc00", midia: "m", base: "fonte", tipo: "caixa", espaco: "original", denso: null, silhuetas: null, ausencias: [],
          origem: { tipo: "manual" }, confirmacao: null, ancoras: [{ t: tempoDoQuadro(tab, 0), pts: 0, v: caixa(0.1) }, { t: tempoDoQuadro(tab, 60), pts: 60 * 512, v: caixa(0.7) }] });
        (d.principal[0] as ClipeTrecho).efeitos.push({ id: "z1", ligado: true, nome: "", ativo: null, tipo: "enquadramento", regiao: { x: 0.25, y: 0.25, w: 0.5, h: 0.5 },
          ajuste: "cobrir", interpolacao: "bilinear", segue: { rastreio: "r1", suavizar_s: 0 } });
      }),
    );
    const t = await montar(<EditorEdicao ativo />);
    await act(async () => st().selecionar({ itens: [{ k: "clipe", id: "a" }], foco: { k: "clipe", id: "a" }, efeito: "z1" }));
    // o happy-dom não busca: o palco mostra o quadro do cursor ao (re)entrar no clipe
    const esquerda = async (j: number) => {
      await act(async () => st().posicionar({ modo: "clipe", clipe: "b", t: meioDoQuadro(tab, 100) }));
      await espera(20);
      await act(async () => st().posicionar({ modo: "clipe", clipe: "a", t: meioDoQuadro(tab, j) }));
      await espera(60);
      const xs = [...t.raiz.querySelectorAll<HTMLElement>('[data-alca^="regiao:z1"]')].map((a) => parseFloat(a.style.left));
      expect(xs.length).toBeGreaterThan(0);
      return Math.min(...xs);
    };
    const x10 = await esquerda(10);
    const x50 = await esquerda(50);
    // a região sai de x = 0 (encostada) a x = 0,4 da origem: a borda esquerda anda para a direita
    expect(x50 - x10).toBeGreaterThan(100);
    HTMLCanvasElement.prototype.getContext = getContext;
  });

  it("Ctrl+B e Congelar aqui no Programa: o trecho sob o cursor, no quadro do slot (revisão P3.3)", async () => {
    const t = await montar(<EditorEdicao ativo />);
    await act(async () => st().posicionar({ modo: "programa", q: 75 }));
    await espera(20);
    expect(botao(t.raiz, "Congelar aqui").disabled).toBe(false);
    await tecla("b", "KeyB", { ctrlKey: true });
    await espera(20);
    const [a, b1, b2] = st().doc!.principal as ClipeTrecho[];
    expect(st().doc!.principal).toHaveLength(3);
    expect(a!.id).toBe("a");
    // b = quadros 100..160; o slot 75 mostra o 115: [100, 115) e [115, 160)
    expect(b1!.id).toBe("b");
    expect(b1!.fim_pts).toBe(115 * 512);
    expect(b2!.inicio_pts).toBe(115 * 512);
    expect(b2!.fim_pts).toBe(160 * 512);
  });

  it("J dá ré (guiado, elementos parados) e L toca no Programa; K para (revisão P3.3)", async () => {
    const t = await montar(<EditorEdicao ativo />);
    await act(async () => st().posicionar({ modo: "programa", q: 30 }));
    await espera(20);
    const tocar = t.raiz.querySelector<HTMLButtonElement>('button[aria-label="Reproduzir"]')!;
    await tecla("l", "KeyL");
    await chegar(t.raiz);
    expect(tocar.getAttribute("aria-pressed")).toBe("true");
    expect([...t.raiz.querySelectorAll<HTMLVideoElement>("[data-pool-host] video")].filter((v) => !v.paused)).toHaveLength(1);
    await tecla("k", "KeyK");
    await espera(30);
    expect(tocar.getAttribute("aria-pressed")).toBe("false");
    await tecla("j", "KeyJ");
    await espera(30);
    expect(tocar.getAttribute("aria-pressed")).toBe("true");
    expect([...t.raiz.querySelectorAll<HTMLVideoElement>("[data-pool-host] video")].every((v) => v.paused)).toBe(true);
    await tecla("k", "KeyK");
    await espera(30);
    expect(tocar.getAttribute("aria-pressed")).toBe("false");
  });

  it("J e L aceleram no palco (Relogio.velocidade): L L a 2×, J ré guiada, K para, L de novo a 1×; no Clipe também", async () => {
    const t = await montar(<EditorEdicao ativo />);
    await act(async () => st().posicionar({ modo: "programa", q: 20 }));
    await espera(20);
    const tocando = () => [...t.raiz.querySelectorAll<HTMLVideoElement>("[data-pool-host] video")].filter((v) => !v.paused);
    await tecla("l", "KeyL");
    await chegar(t.raiz);
    await tecla("l", "KeyL");
    await chegar(t.raiz);
    expect(tocando().map((v) => v.playbackRate)).toEqual([2]);
    await tecla("j", "KeyJ");
    await espera(30);
    expect(tocando()).toHaveLength(0);
    await tecla("k", "KeyK");
    await espera(30);
    await tecla("l", "KeyL");
    await chegar(t.raiz);
    expect(tocando().map((v) => v.playbackRate)).toEqual([1]);
    await tecla("k", "KeyK");
    await espera(30);
    // Clipe: o elemento do palco
    await clicar(botao(t.raiz, "Clipe"));
    await espera(20);
    const el = t.raiz.querySelector<HTMLVideoElement>("video[data-elemento]")!;
    Object.defineProperty(el, "readyState", { value: 4, configurable: true });
    let parado = true;
    Object.defineProperty(el, "paused", { get: () => parado, configurable: true });
    const play = vi.spyOn(el, "play").mockImplementation(async () => void (parado = false));
    vi.spyOn(el, "pause").mockImplementation(() => void (parado = true));
    await tecla("l", "KeyL");
    await espera(20);
    await tecla("l", "KeyL");
    await espera(20);
    expect(play).toHaveBeenCalledTimes(2);
    expect(el.playbackRate).toBe(2);
  });

  it("Clipe toca o som do clipe na conta do Programa (volume até 1; mudo 0); parado, volta a 1", async () => {
    await act(async () => st().alterar("Som", (d) => void ((d.principal[0] as ClipeTrecho).audio.volume = [{ t: tempoDoQuadro(tab, 0), v: 0.5 }, { t: tempoDoQuadro(tab, 59), v: 1.6 }])));
    const t = await montar(<EditorEdicao ativo />);
    await act(async () => st().posicionar({ modo: "clipe", clipe: "a", t: meioDoQuadro(tab, 0) }));
    await espera(20);
    const el = t.raiz.querySelector<HTMLVideoElement>("video[data-elemento]")!;
    Object.defineProperty(el, "readyState", { value: 4, configurable: true });
    let parado = true;
    Object.defineProperty(el, "paused", { get: () => parado, configurable: true });
    vi.spyOn(el, "play").mockImplementation(async () => void (parado = false));
    vi.spyOn(el, "pause").mockImplementation(() => void (parado = true));
    await tecla(" ", "Space");
    await espera(20);
    // linear de 0,5 (quadro 0) a 1,6 (quadro 59), no meio do quadro 0
    expect(el.volume).toBeCloseTo(0.5 + 1.1 * (0.5 / 59), 9);
    await tecla(" ", "Space");
    await espera(20);
    expect(el.volume).toBe(1);
    // acima de 1 a prévia fica em 1 (o render é a verdade); mudo = 0
    const { ganhoNoClipe } = _paraTeste;
    const a = () => st().doc!.principal[0] as ClipeTrecho;
    expect(ganhoNoClipe(st().doc!, a(), tempoDoQuadro(tab, 59), true)).toBe(1);
    expect(ganhoNoClipe(st().doc!, { ...a(), audio: { ...a().audio, mudo: true } }, tempoDoQuadro(tab, 10), true)).toBe(0);
    expect(ganhoNoClipe(st().doc!, a(), tempoDoQuadro(tab, 80), false)).toBe(1);
  });

  it("o Inspetor lê o tempo do clipe no slot do cursor (Começar/Terminar no cursor no modo Programa)", async () => {
    await montar(<EditorEdicao ativo />);
    await act(async () => st().posicionar({ modo: "programa", q: 70 }));
    const b = st().doc!.principal[1] as ClipeTrecho;
    const a = st().doc!.principal[0] as ClipeTrecho;
    expect(tempoNoClipe(b)).toBe(tempoDoQuadro(tab, 110));
    // fora do slot: o começo, como antes
    expect(tempoNoClipe(a)).toBe(a.inicio_s);
  });

  it("ferramenta do Inspetor ligada no programa: o palco vai ao clipe dela, no mesmo quadro", async () => {
    const t = await montar(<EditorEdicao ativo />);
    await act(async () => st().posicionar({ modo: "programa", q: 64 }));
    await act(async () => st().selecionar({ itens: [{ k: "clipe", id: "b" }], foco: { k: "clipe", id: "b" }, efeito: null }));
    await espera(20);
    await act(async () => useInspetorUi.getState().pedir({ tipo: "caixa" }, () => {}, "t"));
    await espera(20);
    expect(st().cursor).toEqual({ modo: "clipe", clipe: "b", t: meioDoQuadro(tab, 104) });
    expect(modo(t.raiz)).toBe("clipe");
    await act(async () => useInspetorUi.getState().cancelarMarcacao());
  });

  it("área visível: mudar a vista (sem repintar) publica de novo; sem clipe sob o cursor, nula", async () => {
    vi.spyOn(HTMLElement.prototype, "clientWidth", "get").mockReturnValue(800);
    vi.spyOn(HTMLElement.prototype, "clientHeight", "get").mockReturnValue(450);
    const pub = vi.spyOn(useAreaVisivel.getState(), "publicar");
    const t = await montar(<EditorEdicao ativo />);
    await act(async () => st().posicionar({ modo: "programa", q: 5 }));
    await espera(20);
    const ultimo = () => pub.mock.calls.at(-1) as [string | null, Caixa | null];
    expect(ultimo()).toEqual(["a", { x: 0, y: 0, w: 1, h: 1 }]);
    const caixa = t.raiz.querySelector<HTMLElement>("[data-caixa]")!;
    await act(async () => void caixa.dispatchEvent(new WheelEvent("wheel", { deltaY: -100, clientX: 400, clientY: 225, bubbles: true })));
    await espera(20);
    expect(ultimo()[0]).toBe("a");
    expect(ultimo()[1]!.w).toBeCloseTo(1 / 1.25, 9);
    // no modo clipe também (o mesmo efeito do palco)
    await clicar(botao(t.raiz, "Clipe"));
    await espera(20);
    const n = pub.mock.calls.length;
    await act(async () => void t.raiz.querySelector<HTMLElement>("[data-caixa]")!.dispatchEvent(new WheelEvent("wheel", { deltaY: -100, clientX: 400, clientY: 225, bubbles: true })));
    await espera(20);
    expect(pub.mock.calls.length).toBeGreaterThan(n);
    expect(ultimo()[1]!.w).toBeCloseTo(1 / 1.25, 9);
    // programa vazio: nada sob o cursor
    await clicar(botao(t.raiz, "Programa"));
    await act(async () => st().alterar("Remover", (d) => void (d.principal = [])));
    await espera(20);
    expect(ultimo()).toEqual([null, null]);
  });
});
