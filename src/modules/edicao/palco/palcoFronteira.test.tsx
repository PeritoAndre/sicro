// @vitest-environment happy-dom
/**
 * Palco com intermediário de origem e busca medida, nas bordas (§7.1, §7.5, §7.6):
 * - o passo que cruza a faixa do clipe troca de arquivo e o elemento novo vai ao quadro PEDIDO (nunca pinta um quadro
 *   com o número de outro, nem deixa marcar ali);
 * - o transporte cobre a mídia inteira também com o intermediário no palco;
 * - depois da medida da busca o canvas só é repintado com a imagem do quadro mostrado.
 */

import { act } from "react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { ClipePrincipal, ClipeTrecho, EfLente, EstadoOrigem, MidiaInfo, MidiaRef, PreviaMidia, ProjetoAberto, ProjetoEdicao, TabelaQuadros } from "@domain/edicao";

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
vi.mock("./Transporte", async (original) => {
  const m = await original<typeof import("./Transporte")>();
  return { ...m, Transporte: vi.fn(m.Transporte) };
});

import { useEdicaoStore } from "../store/edicaoStore";
import { useDialogosAbertos } from "@components/Dialog/Dialog";
import { EditorEdicao } from "../EditorEdicao";
import { novoProjeto, novoTrecho } from "../modelo/projeto";
import { tempoDoQuadro } from "../modelo/tempo";
import { alvosDaMedida, garantirMidia, recursoDe, useRecursos } from "./recursos";
import { MEDIR_APOS_MS, esquecerMedidas } from "./Palco";
import { Buscador, type Agenda } from "./motor/busca";
import type { ElementoBusca } from "./motor/elemento";
import { Transporte } from "./Transporte";
import { useFundoStore } from "../store/fundoStore";
import { ESPERA, esquecerOrigens } from "../inspetor/origemDoClipe";
import { selecionarClipe } from "../sequencia/acoes";
import { meioDoQuadro } from "./faixa";
import { desmontarTudo, esperar, montar } from "../inspetor/testeTela";
import { useInspetorUi } from "../inspetor/ui";
import { novoDestaque, novoRastreio } from "../inspetor/novos";

const SHA = "e".repeat(64);
const st = () => useEdicaoStore.getState();
/** 300 quadros a 30 qps em 1/15360. */
const tab: TabelaQuadros = { tb: { num: 1, den: 15360 }, pts0: 0, passos: Array(299).fill(512), passo_mediano: 512, chaves: [0], vfr: false, origem: "decodificador" };
const info: MidiaInfo = {
  id: "v1", sha256: SHA, nome: "cam.mp4", exibido: { w: 640, h: 360 }, taxa: { num: 30, den: 1 }, vfr: false, passo: { min: 1 / 30, max: 1 / 30 },
  de_s: 0, ate_s: 10, n: 300, tem_audio: false, previa: { relativo: "cam.mp4", tipo: "original", desloc_s: 0, de_s: 0, ate_s: 10, largura: 640, altura: 360, pronta: true },
  tabela: "decodificador", erros: 0, marcavel: true, cor: { matriz: "bt709", faixa: "tv" },
};
const espera = (ms: number) => esperar(() => new Promise((r) => setTimeout(r, ms)));
const tecla = (tipo: "keydown" | "keyup", key: string, x: Partial<KeyboardEventInit> = {}) =>
  act(async () => void window.dispatchEvent(new KeyboardEvent(tipo, { key, code: key, bubbles: true, cancelable: true, ...x })));
const toque = async (key: "ArrowRight" | "ArrowLeft") => {
  await tecla("keydown", key);
  await tecla("keyup", key);
};

let WS = "";
let caso = 0;

/** Projeto de um clipe (quadros 0 a 200) num caso novo. */
async function abrir(mudar?: (p: ProjetoEdicao, c: ClipeTrecho) => void) {
  WS = `/casos/fronteira${++caso}`;
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

/** Imagem que o elemento mostra: o tempo da última busca que CHEGOU (no navegador o `currentTime` muda na hora do
 *  pedido, a imagem só no `seeked`). */
const imagem = new WeakMap<HTMLVideoElement, number>();
/** Imagem do elemento em cada `drawImage(video)` do canvas do palco. */
let pintadas: (number | undefined)[] = [];
const getContext = HTMLCanvasElement.prototype.getContext;

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
  pintadas = [];
  // contexto 2D falso (o happy-dom não tem): só interessa o que foi desenhado do elemento
  const ctx = new Proxy({} as Record<string, unknown>, {
    get: (alvo, k: string) =>
      k in alvo
        ? alvo[k]
        : (...a: unknown[]) => {
            if (k === "drawImage" && a[0] instanceof HTMLVideoElement) pintadas.push(imagem.get(a[0]));
            if (k === "getImageData") return { data: new Uint8ClampedArray(4), width: 1, height: 1 };
          },
    set: (alvo, k: string, v) => ((alvo[k] = v), true),
  });
  HTMLCanvasElement.prototype.getContext = (() => ctx) as unknown as typeof getContext;
});

afterEach(async () => {
  HTMLCanvasElement.prototype.getContext = getContext;
  desmontarTudo();
  useFundoStore.setState({ tarefas: [] });
  await st()
    .fechar({ descartar: true })
    .catch(() => {});
});

const video = (raiz: HTMLElement) => raiz.querySelector<HTMLVideoElement>("video[data-elemento]")!;
const chegar = async (v: HTMLVideoElement) => {
  imagem.set(v, v.currentTime);
  await act(async () => void v.dispatchEvent(new Event("seeked")));
  await espera(50);
};
/** Elemento carregado (com imagem) e o quadro pedido na tela. */
const carregar = async (v: HTMLVideoElement) => {
  Object.defineProperty(v, "readyState", { value: 4, configurable: true });
  await act(async () => void v.dispatchEvent(new Event("loadeddata")));
  await chegar(v);
};
const quadro = (t: { raiz: HTMLElement }) => Number(/quadro (\d+)/.exec(t.raiz.querySelector("[data-tempo]")!.textContent ?? "")?.[1]);

async function noPalco() {
  const t = await montar(<EditorEdicao ativo />);
  await act(async () => selecionarClipe("c"));
  await espera(0);
  const v = video(t.raiz);
  await carregar(v);
  return { t, v };
}

describe("intermediário de origem: a borda da faixa do clipe", () => {
  const lente: EfLente = { id: "lente", tipo: "lente", ligado: true, nome: "", ativo: null, k1: -0.2, k2: 0, cx: 0.5, cy: 0.5, perfil: "cam" };
  const daOrigem: PreviaMidia = { relativo: "origem/abc.mp4", tipo: "origem", desloc_s: 0, de_s: 0, ate_s: tempoDoQuadro(tab, 200), largura: 640, altura: 360, pronta: true };
  const PRONTO: EstadoOrigem = { tipo: "pronto", previa: daOrigem, trf: null };
  const ancoras = () => st().doc!.rastreios.find((x) => x.id === "r1")!.ancoras.map((a) => a.pts);

  /** Clipe com Lente e um destaque, intermediário pronto, cursor no último quadro do clipe (199). */
  async function noUltimoQuadro() {
    const r1 = { ...novoRastreio("m", 0), id: "r1", ancoras: [{ t: 0, pts: 0, v: { x: 0.1, y: 0.1, w: 0.2, h: 0.2 } }] };
    await abrir((p, c) => {
      p.rastreios = [r1];
      c.efeitos = [lente, { ...novoDestaque("r1"), id: "dest" }];
    });
    cmd.edicaoOrigemPedir.mockResolvedValue(PRONTO);
    const { t } = await noPalco();
    await espera(ESPERA.mudanca + 80);
    await act(async () => st().selecionar({ ...st().selecao, efeito: "dest" }));
    let v = video(t.raiz);
    expect(v.getAttribute("src")).toContain("origem/abc.mp4");
    await carregar(v);
    await act(async () => t.raiz.querySelector<HTMLButtonElement>('button[aria-label="Fim do clipe"]')!.click());
    v = video(t.raiz);
    await chegar(v);
    expect(quadro(t)).toBe(199);
    expect(v.getAttribute("src")).toContain("origem/abc.mp4");
    return t;
  }

  it("→ no último quadro do clipe: o original entra e vai ao quadro pedido (200), não ao que estava na tela", async () => {
    const t = await noUltimoQuadro();
    await toque("ArrowRight");
    const v = video(t.raiz);
    expect(v.getAttribute("src")).toContain("cam.mp4");
    expect(v.getAttribute("src")).not.toContain("origem/");
    await carregar(v);
    await chegar(v);
    expect(v.currentTime).toBe(meioDoQuadro(tab, 200));
    expect(quadro(t)).toBe(200);
  });

  it("← de fora para dentro do clipe: o intermediário entra no quadro pedido (199); nunca mostra nem marca o 200 sobre ele", async () => {
    const t = await noUltimoQuadro();
    await toque("ArrowRight");
    let v = video(t.raiz);
    await carregar(v);
    await chegar(v);
    expect(quadro(t)).toBe(200);
    await toque("ArrowLeft");
    v = video(t.raiz);
    expect(v.getAttribute("src")).toContain("origem/abc.mp4");
    // ferramenta de anotação ligada: com o arquivo novo ainda sem o quadro, Enter não grava
    await act(async () => useInspetorUi.getState().pedir({ tipo: "caixa", rastreio: "r1", efeito: "dest" }, () => {}, "b"));
    await tecla("keydown", "Enter");
    expect(ancoras()).toEqual([0]);
    await carregar(v);
    await chegar(v);
    // o arquivo do intermediário acaba no quadro 199: é ele que foi pedido e é o número mostrado
    expect(v.currentTime).toBe(meioDoQuadro(tab, 199));
    expect(quadro(t)).toBe(199);
    await tecla("keydown", "Enter");
    expect(ancoras()).toEqual([0, 199 * 512]);
  });

  it("tocando até o fim do arquivo intermediário (ele para sozinho): o palco fica no último quadro do clipe, no intermediário", async () => {
    const t = await noUltimoQuadro();
    await act(async () => t.raiz.querySelector<HTMLButtonElement>('button[aria-label="Início do clipe"]')!.click());
    const v = video(t.raiz);
    await chegar(v);
    await act(async () => t.raiz.querySelector<HTMLButtonElement>('button[aria-label="Reproduzir"]')!.click());
    expect(v.paused).toBe(false);
    // fim do arquivo: a posição é a duração (o começo do quadro 200) e o elemento para
    v.currentTime = tempoDoQuadro(tab, 200);
    v.pause();
    await espera(60);
    const v2 = video(t.raiz);
    expect(v2.getAttribute("src")).toContain("origem/abc.mp4");
    await chegar(v2);
    expect(quadro(t)).toBe(199);
    expect(v2.currentTime).toBe(meioDoQuadro(tab, 199));
  });

  it("o transporte cobre a mídia inteira com o intermediário no palco (a faixa do clipe é só a marca)", async () => {
    await noUltimoQuadro();
    const p = vi.mocked(Transporte).mock.calls.at(-1)![0];
    expect([p.de, p.ate]).toEqual([0, 10]);
    expect(p.faixa).toEqual([0, tempoDoQuadro(tab, 200)]);
  });
});

describe("arquivo trocado tocando, depois de um passo", () => {
  const proxy: PreviaMidia = { relativo: "proxies/abc.720.mp4", tipo: "proxy", desloc_s: 0, de_s: 0, ate_s: 10, largura: 1280, altura: 720, pronta: true };

  it.each([
    ["chegado", true],
    ["ainda em busca quando tocou", false],
  ])("o elemento novo vai ao quadro que estava na tela, não ao último passo dado (%s)", async (_nome, chegou) => {
    await abrir((_p, c) => void (c.velocidade = { tipo: "constante", fator: 0.2 }));
    cmd.edicaoProxyPedir.mockResolvedValue(info.previa);
    const { t, v } = await noPalco();
    await toque("ArrowRight");
    if (chegou) {
      await chegar(v);
      expect(quadro(t)).toBe(1);
    }
    await act(async () => t.raiz.querySelector<HTMLButtonElement>('button[aria-label="Reproduzir"]')!.click());
    v.currentTime = meioDoQuadro(tab, 40);
    await espera(40);
    cmd.edicaoProxyPedir.mockResolvedValue(proxy);
    await act(async () => useFundoStore.setState({ tarefas: [{ id: `f${caso}`, tipo: "proxy", chave: `proxy:${SHA}`, feito: 1, estado: { tipo: "pronta" } }] }));
    await espera(30);
    const v2 = video(t.raiz);
    expect(v2.getAttribute("src")).toContain("proxies/abc.720.mp4");
    await carregar(v2);
    expect(v2.currentTime).toBe(meioDoQuadro(tab, 40));
    expect(quadro(t)).toBe(40);
  });
});

describe("busca medida: o canvas nunca mostra o quadro de uma busca da medida", () => {
  const ALVOS = alvosDaMedida(tab, 0);
  const daMedida = () => pintadas.filter((x) => x !== undefined && ALVOS.some((j) => x === meioDoQuadro(tab, j)));

  it("medida completa: a repintura que o resultado provoca espera o elemento voltar ao quadro mostrado", async () => {
    await abrir();
    const { v } = await noPalco();
    await espera(MEDIR_APOS_MS + 60);
    for (let i = 0; i < 3; i++) {
      expect(v.currentTime).toBe(meioDoQuadro(tab, ALVOS[i]!));
      imagem.set(v, v.currentTime);
      await act(async () => void v.dispatchEvent(new Event("seeked")));
      await espera(10);
    }
    // o resultado chegou (repinta o palco) e o elemento ainda não voltou: a imagem dele é a da última busca
    await espera(30);
    expect(recursoDe(SHA).busca_p50_ms).toBeTypeOf("number");
    expect(v.currentTime).toBe(meioDoQuadro(tab, 0));
    expect(daMedida()).toEqual([]);
    // voltou: pinta de novo, com a imagem do quadro mostrado
    const antes = pintadas.length;
    await chegar(v);
    expect(pintadas.length).toBeGreaterThan(antes);
    expect(pintadas.at(-1)).toBe(meioDoQuadro(tab, 0));
    expect(daMedida()).toEqual([]);
  });

  it("medida cancelada por uma edição: as repinturas seguintes esperam o elemento voltar", async () => {
    await abrir();
    const { v } = await noPalco();
    await espera(MEDIR_APOS_MS + 60);
    expect(v.currentTime).toBe(meioDoQuadro(tab, ALVOS[0]!));
    imagem.set(v, v.currentTime);
    await act(async () => void v.dispatchEvent(new Event("seeked")));
    await espera(10);
    // segunda busca da medida em curso (a imagem é a da primeira): duas edições seguidas
    await act(async () => st().alterar("Nome", (d) => void ((d.principal[0] as ClipeTrecho).rotulo = "x")));
    await act(async () => st().alterar("Nome", (d) => void ((d.principal[0] as ClipeTrecho).rotulo = "xy")));
    expect(v.currentTime).toBe(meioDoQuadro(tab, 0));
    expect(daMedida()).toEqual([]);
    await chegar(v);
    expect(pintadas.at(-1)).toBe(meioDoQuadro(tab, 0));
    expect(daMedida()).toEqual([]);
  });
});

describe("busca medida num trecho que toca", () => {
  it("tocar no meio da medida: o laço pinta os quadros que o elemento mostra (a tela não fica parada)", async () => {
    await abrir();
    const { t, v } = await noPalco();
    await espera(MEDIR_APOS_MS + 60);
    expect(v.currentTime).toBe(meioDoQuadro(tab, alvosDaMedida(tab, 0)[0]!));
    await act(async () => t.raiz.querySelector<HTMLButtonElement>('button[aria-label="Reproduzir"]')!.click());
    // a medida foi cancelada e o elemento toca a partir do quadro mostrado
    expect(v.currentTime).toBe(meioDoQuadro(tab, 0));
    pintadas = [];
    v.currentTime = meioDoQuadro(tab, 5);
    await espera(40);
    expect(quadro(t)).toBe(0);
    expect(pintadas.length).toBeGreaterThan(0);
    await act(async () => t.raiz.querySelector<HTMLButtonElement>('button[aria-label="Reproduzir"]')!.click());
    await chegar(v);
    expect(quadro(t)).toBe(5);
  });
});

describe("busca medida num quadro congelado que toca pelo relógio", () => {
  it("tocar no meio da medida: os quadros do relógio não pintam a imagem da busca medida", async () => {
    await abrir((p) => {
      p.principal = [{ id: "c", tipo: "congelar", rotulo: "", desligado: false, transicao_entrada: null, efeitos: [], midia: "m", instante_s: tempoDoQuadro(tab, 10), instante_pts: 10 * 512, duracao_s: 2 } as ClipePrincipal];
    });
    const { t, v } = await noPalco();
    expect(v.currentTime).toBe(meioDoQuadro(tab, 10));
    const ALVOS = alvosDaMedida(tab, 10);
    await espera(MEDIR_APOS_MS + 60);
    expect(v.currentTime).toBe(meioDoQuadro(tab, ALVOS[0]!));
    imagem.set(v, v.currentTime);
    await act(async () => void v.dispatchEvent(new Event("seeked")));
    // segunda busca da medida em curso: a imagem do elemento é a da primeira
    pintadas = [];
    await act(async () => t.raiz.querySelector<HTMLButtonElement>('button[aria-label="Reproduzir"]')!.click());
    await espera(80);
    // a medida foi cancelada e o elemento volta ao quadro congelado; enquanto não chega, nada do elemento é pintado
    expect(v.currentTime).toBe(meioDoQuadro(tab, 10));
    expect(pintadas).toEqual([]);
    await chegar(v);
    expect(pintadas.length).toBeGreaterThan(0);
    expect(pintadas.every((x) => x === meioDoQuadro(tab, 10))).toBe(true);
  });
});

describe("busca medida: não atrapalha quem vai marcar", () => {
  const ALVOS = alvosDaMedida(tab, 0);

  it("com a ferramenta de marcação ligada a medida espera (Enter marca); desligada, mede", async () => {
    const r1 = { ...novoRastreio("m", 0), id: "r1", ancoras: [{ t: 0, pts: 0, v: { x: 0.1, y: 0.1, w: 0.2, h: 0.2 } }] };
    await abrir((p, c) => {
      p.rastreios = [r1];
      c.efeitos = [{ ...novoDestaque("r1"), id: "dest" }];
    });
    const { v } = await noPalco();
    await act(async () => useInspetorUi.getState().pedir({ tipo: "caixa", rastreio: "r1", efeito: "dest" }, () => {}, "m"));
    await toque("ArrowRight");
    await chegar(v);
    await espera(MEDIR_APOS_MS + 100);
    expect(v.currentTime).toBe(meioDoQuadro(tab, 1));
    await tecla("keydown", "Enter");
    expect(st().doc!.rastreios[0]!.ancoras.map((a) => a.pts)).toEqual([0, 512]);
    // ferramenta desligada: a medida que ficou para depois acontece
    await act(async () => useInspetorUi.getState().cancelarMarcacao());
    await espera(2 * MEDIR_APOS_MS + 100);
    expect(v.currentTime).toBe(meioDoQuadro(tab, ALVOS[0]!));
  }, 10000);

  it("com o ponteiro sobre o palco a medida espera; fora dele, mede", async () => {
    await abrir();
    const { t, v } = await noPalco();
    const caixa = t.raiz.querySelector<HTMLElement>("[data-caixa]")!;
    await act(async () => void caixa.dispatchEvent(new MouseEvent("pointerover", { bubbles: true, relatedTarget: document.body })));
    await espera(MEDIR_APOS_MS + 100);
    expect(v.currentTime).toBe(meioDoQuadro(tab, 0));
    await act(async () => void caixa.dispatchEvent(new MouseEvent("pointerout", { bubbles: true, relatedTarget: document.body })));
    await espera(2 * MEDIR_APOS_MS + 100);
    expect(v.currentTime).toBe(meioDoQuadro(tab, ALVOS[0]!));
  }, 10000);
});

describe("Buscador: imagem do elemento depois da medida", () => {
  function falso() {
    const ouvintes = new Set<() => void>();
    const pedidos: number[] = [];
    const estado = { buscando: false };
    const el: ElementoBusca = {
      buscar: (j) => void pedidos.push(j),
      quadro: () => pedidos.at(-1) ?? 0,
      agora: () => 0,
      buscando: () => estado.buscando,
      aoBuscado: (f) => (ouvintes.add(f), () => ouvintes.delete(f)),
      tocar: async () => {},
      pausar: () => {},
      pausado: () => true,
    };
    const rafs: (() => void)[] = [];
    const agenda: Agenda = { raf: (f) => void rafs.push(f), setTimeout: () => 0, clearTimeout: () => {}, agora: () => 0 };
    const buscado = () => ouvintes.forEach((f) => f());
    const quadro = () => rafs.splice(0).forEach((f) => f());
    return { el, pedidos, agenda, buscado, quadro, estado };
  }
  function pintadoEm(f: ReturnType<typeof falso>, j: number) {
    const pintados: number[] = [];
    const alheiaAoPintar: boolean[] = [];
    const b: Buscador = new Buscador(f.el, (q) => (pintados.push(q), void alheiaAoPintar.push(b.imagemAlheia)), f.agenda);
    b.ir(j);
    f.buscado();
    f.quadro();
    f.quadro();
    return { b, pintados, alheiaAoPintar };
  }

  it("`imagemAlheia` da primeira busca da medida até o quadro de volta ser pintado", async () => {
    const f = falso();
    const { b, alheiaAoPintar } = pintadoEm(f, 7);
    expect(b.imagemAlheia).toBe(false);
    const medida = b.medir([100, 200, 300]);
    expect(b.imagemAlheia).toBe(true);
    f.buscado();
    f.buscado();
    f.buscado();
    await medida;
    // voltando ao 7: a imagem ainda é a do 300
    expect(b.medindo).toBe(false);
    expect(b.imagemAlheia).toBe(true);
    f.buscado();
    f.quadro();
    expect(b.imagemAlheia).toBe(false);
    // quem pinta no aviso do quadro já vê a imagem certa
    expect(alheiaAoPintar.every((x) => !x)).toBe(true);
  });

  it("medida cancelada: `imagemAlheia` até o quadro pedido chegar; o `seeked` da busca substituída não conta como chegada", async () => {
    const f = falso();
    const { b, pintados } = pintadoEm(f, 7);
    const n = pintados.length;
    const medida = b.medir([100, 200, 300]);
    b.cancelarMedida();
    expect(await medida).toBeNull();
    expect(b.imagemAlheia).toBe(true);
    b.ir(8);
    // o `seeked` da busca da medida chega com o elemento ainda buscando o 8: nada é pintado
    f.estado.buscando = true;
    f.buscado();
    f.quadro();
    f.quadro();
    expect(pintados).toHaveLength(n);
    expect(b.alvo).toBe(8);
    expect(b.imagemAlheia).toBe(true);
    f.estado.buscando = false;
    f.buscado();
    f.quadro();
    f.quadro();
    expect(pintados.slice(n)).toEqual([8, 8]);
    expect(b.imagemAlheia).toBe(false);
    expect(b.alvo).toBeNull();
  });
});
