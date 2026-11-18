// @vitest-environment happy-dom
/**
 * Fecho do bloco 6.4 (P4.1 × P3.3 × ZS): clique na régua ou no fundo da linha = cursor de programa; logo depois,
 * Ctrl+B corta o trecho sob o cursor e leva os presos, "Chave no cursor" e "Começar no cursor" gravam o instante do
 * slot. Clique num clipe = modo Clipe, sem tirar a cabeça que já está nele.
 */

import { act } from "react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { Caixa, Chave, ClipeTrecho, EfEnquadramento, MidiaInfo, MidiaRef, ProjetoAberto, SobTitulo, TabelaQuadros } from "@domain/edicao";

const cmd = vi.hoisted(() => ({
  edicaoAbrir: vi.fn(), edicaoSalvar: vi.fn(), edicaoAnalisar: vi.fn(), edicaoTabelaQuadros: vi.fn(), edicaoMidiaInfo: vi.fn(), edicaoFila: vi.fn(), edicaoFundo: vi.fn(),
  edicaoSaidas: vi.fn(), edicaoSpriteTexto: vi.fn(), edicaoAtlasCarimbo: vi.fn(), edicaoQuadroFinal: vi.fn(), edicaoEnfileirar: vi.fn(), edicaoProxyPedir: vi.fn(), edicaoOnda: vi.fn(),
  edicaoCapacidades: vi.fn(), edicaoTira: vi.fn(),
}));
vi.mock("@core/commands", () => ({ commands: cmd }));
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn(async () => () => {}) }));
vi.mock("@tauri-apps/api/core", () => ({ convertFileSrc: (p: string) => `asset://${p}`, invoke: vi.fn() }));

import { useEdicaoStore } from "../store/edicaoStore";
import { useDialogosAbertos } from "@components/Dialog/Dialog";
import { EditorEdicao } from "../EditorEdicao";
import { novoProjeto, novoTrecho } from "../modelo/projeto";
import { tempoDoQuadro } from "../modelo/tempo";
import { garantirMidia, useRecursos } from "../palco/recursos";
import { useFundoStore } from "../store/fundoStore";
import { meioDoQuadro } from "../palco/faixa";
import { adicionarEfeito } from "../inspetor/acoes";
import { useInspetorUi } from "../inspetor/ui";
import { desmontarTudo, esperar, montar, type Tela } from "../inspetor/testeTela";
import { CAB_PX } from "./LinhaDoTempo";

const WS = "/casos/regua";
const SHA = "b".repeat(64);
const st = () => useEdicaoStore.getState();
const tab: TabelaQuadros = { tb: { num: 1, den: 15360 }, pts0: 0, passos: Array(299).fill(512), passo_mediano: 512, chaves: [0], vfr: false, origem: "decodificador" };
const info: MidiaInfo = {
  id: "v1", sha256: SHA, nome: "cam.mp4", exibido: { w: 640, h: 360 }, taxa: { num: 30, den: 1 }, vfr: false, passo: { min: 1 / 30, max: 1 / 30 },
  de_s: 0, ate_s: 10, n: 300, tem_audio: false, previa: { relativo: "cam.mp4", tipo: "original", desloc_s: 0, de_s: 0, ate_s: 10, largura: 640, altura: 360, pronta: true },
  tabela: "decodificador", erros: 0, marcavel: true, cor: { matriz: "bt709", faixa: "tv" },
};
const espera = (ms: number) => esperar(() => new Promise((r) => setTimeout(r, ms)));
const tecla = (key: string, code: string, extra: KeyboardEventInit = {}) => act(async () => void window.dispatchEvent(new KeyboardEvent("keydown", { key, code, bubbles: true, ...extra })));
let getContext: typeof HTMLCanvasElement.prototype.getContext;

beforeEach(async () => {
  for (const f of Object.values(cmd)) f.mockReset();
  cmd.edicaoAnalisar.mockImplementation(() => new Promise(() => {}));
  cmd.edicaoSalvar.mockImplementation(async () => ({ updated_at: new Date().toISOString() }));
  for (const f of [cmd.edicaoTabelaQuadros, cmd.edicaoMidiaInfo, cmd.edicaoSpriteTexto, cmd.edicaoAtlasCarimbo, cmd.edicaoQuadroFinal, cmd.edicaoProxyPedir, cmd.edicaoOnda, cmd.edicaoTira, cmd.edicaoCapacidades])
    f.mockImplementation(() => new Promise(() => {}));
  cmd.edicaoFila.mockResolvedValue([]);
  cmd.edicaoFundo.mockResolvedValue([]);
  cmd.edicaoSaidas.mockResolvedValue([]);
  getContext = HTMLCanvasElement.prototype.getContext;
  HTMLCanvasElement.prototype.getContext = (() => new Proxy({}, { get: () => () => undefined, set: () => true })) as unknown as typeof getContext;
  vi.spyOn(HTMLElement.prototype, "clientWidth", "get").mockReturnValue(1000);
  vi.spyOn(HTMLElement.prototype, "clientHeight", "get").mockReturnValue(450);
  const p = novoProjeto();
  p.midias = [{ id: "m", sha256: SHA, tipo: "video", nome: "cam.mp4", relativo: "cam.mp4", vfr: false, tem_audio: false, ocultacoes: [] } as unknown as MidiaRef];
  // a: quadros 0..60 (slots 0..59); b: 100..160 (slots 60..119)
  p.principal = [
    { ...novoTrecho("m", tempoDoQuadro(tab, 0), tempoDoQuadro(tab, 60)), id: "a", rotulo: "Cena 1", inicio_pts: 0, fim_pts: 60 * 512 } as ClipeTrecho,
    { ...novoTrecho("m", tempoDoQuadro(tab, 100), tempoDoQuadro(tab, 160)), id: "b", rotulo: "Cena 2", inicio_pts: 100 * 512, fim_pts: 160 * 512 } as ClipeTrecho,
  ];
  cmd.edicaoAbrir.mockResolvedValue({ projeto: p, importado_de: null, so_leitura: false, avisos: [], raiz_cache: "/cache/regua" } satisfies ProjetoAberto);
  await st().abrir(WS, p.id);
  useDialogosAbertos.setState({ pilha: [] });
  garantirMidia(WS, SHA);
  useRecursos.setState((s) => ({ midias: { [SHA]: { info, tabela: tab, erro: null, pedindo: false } }, versao: s.versao + 1 }));
});

afterEach(async () => {
  desmontarTudo();
  vi.restoreAllMocks();
  HTMLCanvasElement.prototype.getContext = getContext;
  useFundoStore.setState({ tarefas: [] });
  await st()
    .fechar({ descartar: true })
    .catch(() => {});
});

/** px por slot da linha, lido da cabeça (o happy-dom não mede). */
async function escala(t: Tela): Promise<number> {
  await act(async () => st().posicionar({ modo: "programa", q: 100 }));
  await espera(30);
  const tr = t.raiz.querySelector<HTMLElement>("[data-cabeca-topo]")!.style.transform;
  const x = parseFloat(/translateX\(([-\d.]+)px\)/.exec(tr)?.[1] ?? "NaN");
  return x / 100;
}

/** Clique na régua no slot `q`. */
async function naRegua(t: Tela, k: number, q: number) {
  const r = t.raiz.querySelector<HTMLElement>("[data-regua]")!;
  // o happy-dom não herda variável de CSS: a régua lê o `--px` da linha como no navegador
  r.style.setProperty("--px", String(k));
  const ev = (tipo: string) => new PointerEvent(tipo, { clientX: (q + 0.5) * k, clientY: 5, bubbles: true, button: 0, pointerId: 1, isPrimary: true });
  await act(async () => void r.dispatchEvent(ev("pointerdown")));
  await act(async () => void r.dispatchEvent(ev("pointerup")));
  await espera(30);
}
/** Clique (sem arrastar) no clipe `id` da V1, no slot `q`. */
async function noClipe(t: Tela, id: string, k: number, q: number) {
  const el = t.raiz.querySelector<HTMLElement>(`[data-trilha="principal"] [data-item="${id}"]`)!;
  const ev = (tipo: string) => new PointerEvent(tipo, { clientX: CAB_PX + (q + 0.5) * k, clientY: 5, bubbles: true, button: 0, pointerId: 1, isPrimary: true });
  await act(async () => void el.dispatchEvent(ev("pointerdown")));
  await act(async () => void el.dispatchEvent(ev("pointerup")));
  await espera(30);
}
const botaoAcao = (t: Tela, a: string) => t.raiz.querySelector<HTMLButtonElement>(`[data-acao="${a}"]`);

describe("régua = cursor de programa; o que vem logo depois", () => {
  it("Ctrl+B depois da régua: corta o trecho sob o cursor no quadro do slot e leva o texto preso depois do corte", async () => {
    await act(async () =>
      st().alterar("Texto", (d) => {
        d.sobreposicoes = [{ id: "T1", nome: "", oculta: false, clipes: [{ tipo: "titulo", id: "s1", rotulo: "", pos: { ref: "clipe", clipe: "b", desloc: 1.5 }, duracao_s: 0.5, entrada_s: 0, saida_s: 0, efeitos: [],
          transformacao: { x: 0.5, y: 0.5, escala: 1, rotacao: 0, opacidade: 1 }, titulo: { modelo: "livre", fundo: { tipo: "transparente" }, blocos: [] } } as unknown as SobTitulo] }];
      }),
    );
    const t = await montar(<EditorEdicao ativo />);
    await espera(40);
    const k = await escala(t);
    await naRegua(t, k, 75);
    expect(st().cursor).toEqual({ modo: "programa", q: 75 });
    await tecla("b", "KeyB", { ctrlKey: true });
    await espera(20);
    const [, b1, b2] = st().doc!.principal as ClipeTrecho[];
    expect([b1!.fim_pts, b2!.inicio_pts]).toEqual([115 * 512, 115 * 512]);
    // o texto estava 1,5 s depois do começo de b: fica a 1,0 s do começo do pedaço da direita
    const pos = (st().doc!.sobreposicoes[0]!.clipes[0] as SobTitulo).pos as { ref: string; clipe: string; desloc: number };
    expect([pos.clipe, pos.desloc]).toEqual([b2!.id, expect.closeTo(1, 9)]);
  });

  it("Chave no cursor e Começar no cursor depois da régua: o instante do slot no clipe", async () => {
    const t = await montar(<EditorEdicao ativo />);
    await espera(40);
    const k = await escala(t);
    await act(async () => st().selecionar({ itens: [{ k: "clipe", id: "a" }], foco: { k: "clipe", id: "a" }, efeito: null }));
    await act(async () => useInspetorUi.getState().abrirAba("clipe"));
    await naRegua(t, k, 30);
    expect(st().cursor).toEqual({ modo: "programa", q: 30 });
    await act(async () => botaoAcao(t, "chave-no-cursor")!.click());
    const enq = (st().doc!.principal[0] as ClipeTrecho).efeitos.find((e) => (e as EfEnquadramento).tipo === "enquadramento") as EfEnquadramento;
    expect((enq.regiao as Chave<Caixa>[]).map((c) => c.t)).toEqual([tempoDoQuadro(tab, 30)]);
    // um texto no clipe, aberto; a régua no slot 40; "Começar no cursor" grava o quadro 40
    await act(async () => void adicionarEfeito("a", "texto", "Texto"));
    await espera(20);
    await naRegua(t, k, 40);
    expect(st().cursor).toEqual({ modo: "programa", q: 40 });
    await act(async () => botaoAcao(t, "comecar-no-cursor")!.click());
    const tx = (st().doc!.principal[0] as ClipeTrecho).efeitos.at(-1)!;
    expect((tx as { ativo: { de: number } }).ativo.de).toBeCloseTo(tempoDoQuadro(tab, 40), 9);
  });

  it("clique num clipe: modo Clipe; com a cabeça já nele, no mesmo quadro; fora dele, no quadro clicado", async () => {
    const t = await montar(<EditorEdicao ativo />);
    await espera(40);
    const k = await escala(t);
    await naRegua(t, k, 75);
    // a cabeça já está em b (slot 75 = quadro 115): o clique noutro ponto de b não a tira
    await noClipe(t, "b", k, 90);
    expect(st().cursor).toEqual({ modo: "clipe", clipe: "b", t: meioDoQuadro(tab, 115) });
    // em a, onde a cabeça não está: vai ao quadro clicado
    await noClipe(t, "a", k, 20);
    expect(st().cursor).toEqual({ modo: "clipe", clipe: "a", t: meioDoQuadro(tab, 20) });
  });
});
