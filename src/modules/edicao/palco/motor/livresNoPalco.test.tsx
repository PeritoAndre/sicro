// @vitest-environment happy-dom
/**
 * Texto livre no palco (P3.3 × TL): o pintor põe o sobreposto pela transformação do Rust (objeto = canvas inteiro,
 * centro em (x, y), escala da largura, giro horário, opacidade do grupo); nos dois modos o palco liga as alças de
 * `alcasDoLivre` (giro redondo com haste), o contorno girado e o clique dentro da caixa, que seleciona e arrasta pela
 * alça "mover" num passo do histórico. `palco/livres.ts` aqui é falso; o de verdade, em `palcoPrograma.test.tsx`.
 */

import { act } from "react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { Alca, ClipeTrecho, MidiaInfo, MidiaRef, ProjetoAberto, SobTitulo, TabelaQuadros } from "@domain/edicao";

const cmd = vi.hoisted(() => ({
  edicaoAbrir: vi.fn(), edicaoSalvar: vi.fn(), edicaoAnalisar: vi.fn(), edicaoTabelaQuadros: vi.fn(), edicaoMidiaInfo: vi.fn(), edicaoFila: vi.fn(), edicaoFundo: vi.fn(),
  edicaoSaidas: vi.fn(), edicaoSpriteTexto: vi.fn(), edicaoAtlasCarimbo: vi.fn(), edicaoQuadroFinal: vi.fn(), edicaoEnfileirar: vi.fn(), edicaoProxyPedir: vi.fn(), edicaoOnda: vi.fn(),
}));
vi.mock("@core/commands", () => ({ commands: cmd }));
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn(async () => () => {}) }));
vi.mock("@tauri-apps/api/core", () => ({ convertFileSrc: (p: string) => `asset://${p}`, invoke: vi.fn() }));
// falso: um texto no centro, caixa de 0,4 × 0,2, com mover, um canto e o giro
vi.mock("../livres", () => {
  const item = (doc: { sobreposicoes: { id: string; clipes: SobTitulo[] }[] }, q: number) => {
    const c = doc.sobreposicoes[0]?.clipes[0];
    if (!c || q < 10 || q >= 70) return null;
    const x = c.transformacao.x as number;
    return { trilha: doc.sobreposicoes[0]!.id, clipe: c, tau: (q - 10) / 30, transformacao: { x, y: 0.5, escala: 1, rotacao: 0, opacidade: 1 }, primitivas: [] };
  };
  const alcas = (it: { trilha: string; clipe: SobTitulo; transformacao: { x: number } }): Alca[] => {
    const id = it.clipe.id;
    const mover: Alca = {
      id: `livre:${id}:mover`, tipo: "mover", em: { x: it.transformacao.x, y: 0.5 }, cursor: "move",
      arrastar: (de, para) => ({ livre: id, trilha: it.trilha, sobreposto: { ...it.clipe, transformacao: { ...it.clipe.transformacao, x: it.transformacao.x + (para.x - de.x) } } }),
    };
    const giro: Alca = { id: `livre:${id}:giro`, tipo: "giro", em: { x: it.transformacao.x, y: 0.3 }, cursor: "grab", arrastar: mover.arrastar };
    const canto: Alca = { id: `livre:${id}:se`, tipo: "canto", em: { x: it.transformacao.x + 0.2, y: 0.6 }, cursor: "nwse-resize", arrastar: mover.arrastar };
    return [mover, canto, giro];
  };
  return {
    livresNoQuadro: (doc: never, _l: unknown, q: number) => {
      const i = item(doc, q);
      return i ? [i] : [];
    },
    alcasDoLivre: (_d: unknown, it: never) => alcas(it),
    dentroDoTexto: (it: { transformacao: { x: number } }, _c: unknown, p: { x: number; y: number }) => Math.abs(p.x - it.transformacao.x) <= 0.2 && Math.abs(p.y - 0.5) <= 0.1,
    cantosDoTexto: (it: { transformacao: { x: number } }) => [
      { x: it.transformacao.x - 0.2, y: 0.4 }, { x: it.transformacao.x + 0.2, y: 0.4 }, { x: it.transformacao.x + 0.2, y: 0.6 }, { x: it.transformacao.x - 0.2, y: 0.6 },
    ],
  };
});

import { useEdicaoStore } from "../../store/edicaoStore";
import { useDialogosAbertos } from "@components/Dialog/Dialog";
import { EditorEdicao } from "../../EditorEdicao";
import { novoProjeto, novoTrecho } from "../../modelo/projeto";
import { tempoDoQuadro } from "../../modelo/tempo";
import { garantirMidia, useRecursos } from "../recursos";
import { useFundoStore } from "../../store/fundoStore";
import { desmontarTudo, esperar, montar } from "../../inspetor/testeTela";
import { pintarLivres } from "./pintor";
import type { LivreNoQuadro } from "../livres";

const WS = "/casos/livre";
const SHA = "f".repeat(64);
const st = () => useEdicaoStore.getState();
const tab: TabelaQuadros = { tb: { num: 1, den: 30 }, pts0: 0, passos: Array(299).fill(1), passo_mediano: 1, chaves: [0], vfr: false, origem: "decodificador" };
const info: MidiaInfo = {
  id: "v", sha256: SHA, nome: "cam.mp4", exibido: { w: 1920, h: 1080 }, taxa: { num: 30, den: 1 }, vfr: false, passo: { min: 1 / 30, max: 1 / 30 }, de_s: 0, ate_s: 10, n: 300, tem_audio: false,
  previa: { relativo: "cam.mp4", tipo: "original", desloc_s: 0, de_s: 0, ate_s: 10, largura: 1920, altura: 1080, pronta: true }, tabela: "decodificador", erros: 0, marcavel: true, cor: { matriz: "bt709", faixa: "tv" },
};
const espera = (ms: number) => esperar(() => new Promise((r) => setTimeout(r, ms)));
const sob = (): SobTitulo => ({
  tipo: "titulo", id: "s1", rotulo: "", pos: { ref: "programa", t: 10 / 30 }, duracao_s: 2, transformacao: { x: 0.5, y: 0.5, escala: 1, rotacao: 0, opacidade: 1 }, entrada_s: 0, saida_s: 0, efeitos: [],
  titulo: { modelo: "livre", fundo: { tipo: "transparente" }, blocos: [] },
});

beforeEach(async () => {
  for (const f of Object.values(cmd)) f.mockReset();
  cmd.edicaoAnalisar.mockImplementation(() => new Promise(() => {}));
  cmd.edicaoSalvar.mockImplementation(async () => ({ updated_at: new Date().toISOString() }));
  for (const f of [cmd.edicaoTabelaQuadros, cmd.edicaoMidiaInfo, cmd.edicaoSpriteTexto, cmd.edicaoAtlasCarimbo, cmd.edicaoQuadroFinal, cmd.edicaoProxyPedir, cmd.edicaoOnda]) f.mockImplementation(() => new Promise(() => {}));
  cmd.edicaoFila.mockResolvedValue([]);
  cmd.edicaoFundo.mockResolvedValue([]);
  cmd.edicaoSaidas.mockResolvedValue([]);
  const p = novoProjeto();
  p.midias = [{ id: "m", sha256: SHA, tipo: "video", nome: "cam.mp4", relativo: "cam.mp4", vfr: false, tem_audio: false, ocultacoes: [] } as unknown as MidiaRef];
  p.principal = [{ ...novoTrecho("m", 0, tempoDoQuadro(tab, 90)), id: "a", inicio_pts: 0, fim_pts: 90 } as ClipeTrecho];
  p.sobreposicoes = [{ id: "T1", nome: "", oculta: false, clipes: [sob()] }];
  cmd.edicaoAbrir.mockResolvedValue({ projeto: p, importado_de: null, so_leitura: false, avisos: [], raiz_cache: "/cache/livre" } satisfies ProjetoAberto);
  await st().abrir(WS, p.id);
  useDialogosAbertos.setState({ pilha: [] });
  garantirMidia(WS, SHA);
  useRecursos.setState((s) => ({ midias: { [SHA]: { info, tabela: tab, erro: null, pedindo: false } }, versao: s.versao + 1 }));
  // contexto 2D falso (o happy-dom não tem): o palco pinta e calcula as alças como no navegador
  const ctx = new Proxy({} as Record<string, unknown>, {
    get: (alvo, k: string) => (k in alvo ? alvo[k] : (...a: unknown[]) => (k === "getImageData" ? { data: new Uint8ClampedArray(4), width: 1, height: 1 } : void a)),
    set: (alvo, k: string, v) => ((alvo[k] = v), true),
  });
  HTMLCanvasElement.prototype.getContext = (() => ctx) as unknown as typeof getContext;
  // palco de 800 × 450 px na tela (a saída é 1920 × 1080: o canvas cabe inteiro)
  vi.spyOn(HTMLElement.prototype, "clientWidth", "get").mockReturnValue(800);
  vi.spyOn(HTMLElement.prototype, "clientHeight", "get").mockReturnValue(450);
});

const getContext = HTMLCanvasElement.prototype.getContext;

afterEach(async () => {
  HTMLCanvasElement.prototype.getContext = getContext;
  desmontarTudo();
  vi.restoreAllMocks();
  useFundoStore.setState({ tarefas: [] });
  await st()
    .fechar({ descartar: true })
    .catch(() => {});
});

const ponteiro = (el: Element, tipo: string, x: number, y: number) =>
  act(async () => void el.dispatchEvent(new PointerEvent(tipo, { clientX: x, clientY: y, bubbles: true, button: 0, pointerId: 1, isPrimary: true })));

describe("pintor: o sobreposto pela transformação do Rust", () => {
  it("objeto = canvas inteiro: translada ao centro, gira no sentido horário, escala a largura, opacidade do grupo", () => {
    const chamadas: string[] = [];
    const ctx = new Proxy({} as Record<string, unknown>, {
      get: (_o, k: string) => (k === "canvas" ? {} : (...a: unknown[]) => void chamadas.push(`${k}(${a.map((x) => (typeof x === "number" ? Math.round(x * 1000) / 1000 : typeof x)).join(",")})`)),
      set: (_o, k: string, v: unknown) => {
        chamadas.push(`${k}=${v}`);
        return true;
      },
    }) as unknown as CanvasRenderingContext2D;
    const apoio = { dataset: {} as DOMStringMap, width: 0, height: 0, getContext: () => ({ setTransform() {}, clearRect() {}, fillRect() {}, save() {}, restore() {}, drawImage() {} }) } as unknown as HTMLCanvasElement;
    const l = { trilha: "T1", clipe: sob(), tau: 0, transformacao: { x: 0.25, y: 0.75, escala: 0.5, rotacao: 90, opacidade: 0.4 }, primitivas: [] } as LivreNoQuadro;
    pintarLivres(ctx, [l], { imagem: () => null, cena: { sprite: () => null, atlas: () => null, midia: () => null }, apoio: () => apoio }, 640, 360);
    const i = chamadas.indexOf("globalAlpha=0.4");
    expect(i).toBeGreaterThan(-1);
    expect(chamadas.slice(i + 1, i + 5)).toEqual(["translate(160,270)", `rotate(${Math.round((Math.PI / 2) * 1000) / 1000})`, "scale(0.5,0.5)", "drawImage(object,-320,-180,640,360)"]);
  });
});

describe("texto livre nos dois modos do palco", () => {
  it("programa: clicar dentro da caixa seleciona; arrastar move num passo do histórico; giro redondo com haste; contorno girado", async () => {
    const t = await montar(<EditorEdicao ativo />);
    await act(async () => st().posicionar({ modo: "programa", q: 20 }));
    await espera(40);
    const camada = t.raiz.querySelector("[data-camada]")!;
    const passos = st().passado.length;
    // centro do canvas na tela: (400, 225)
    await ponteiro(camada, "pointerdown", 400, 225);
    expect(st().selecao.foco).toEqual({ k: "livre", id: "s1", trilha: "T1" });
    await ponteiro(camada, "pointermove", 480, 225);
    await ponteiro(camada, "pointerup", 480, 225);
    await espera(40);
    expect((st().doc!.sobreposicoes[0]!.clipes[0] as SobTitulo).transformacao.x).toBeCloseTo(0.6, 9);
    expect(st().passado.length).toBe(passos + 1);
    // selecionado e parado: alças, giro redondo com a haste até a caixa, e o contorno
    const giro = t.raiz.querySelector('[data-alca="livre:s1:giro"]')!;
    expect(giro.getAttribute("data-tipo")).toBe("giro");
    expect(t.raiz.querySelectorAll("[data-contorno-livre] line")).toHaveLength(1);
    expect(t.raiz.querySelector("[data-contorno-livre] polygon")?.getAttribute("points")?.split(" ")).toHaveLength(4);
    // fora da caixa: a vista desloca, nada muda no documento
    await ponteiro(camada, "pointerdown", 20, 20);
    await ponteiro(camada, "pointerup", 30, 30);
    expect(st().passado.length).toBe(passos + 1);
  });

  it("clipe: o sobreposto do slot do quadro mostrado também se pega e se arrasta", async () => {
    const t = await montar(<EditorEdicao ativo />);
    await act(async () => {
      st().selecionar({ itens: [{ k: "clipe", id: "a" }], foco: { k: "clipe", id: "a" }, efeito: null });
      st().posicionar({ modo: "clipe", clipe: "a", t: tempoDoQuadro(tab, 30) + 1 / 60 });
    });
    await espera(40);
    expect(t.raiz.querySelector("[data-modo-palco]")?.getAttribute("data-modo-palco")).toBe("clipe");
    const camada = t.raiz.querySelector("[data-camada]")!;
    await ponteiro(camada, "pointerdown", 400, 225);
    expect(st().selecao.foco).toEqual({ k: "livre", id: "s1", trilha: "T1" });
    await ponteiro(camada, "pointermove", 360, 225);
    await ponteiro(camada, "pointerup", 360, 225);
    await espera(40);
    expect((st().doc!.sobreposicoes[0]!.clipes[0] as SobTitulo).transformacao.x).toBeCloseTo(0.45, 9);
    expect(t.raiz.querySelector('[data-alca="livre:s1:mover"]')).toBeTruthy();
  });
});
