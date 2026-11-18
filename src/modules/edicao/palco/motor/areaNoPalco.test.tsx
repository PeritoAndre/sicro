// @vitest-environment happy-dom
/**
 * Fecho do bloco 6.4 (PE + TL + P3.3): com o palco a 3× num canto, destaque, ocultar, lupa e texto livre novos nascem
 * dentro da área que o perito vê (a publicada pelo palco, `areaVisivel.ts` de verdade), nos dois modos.
 */

import { act } from "react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { Caixa, ClipeTrecho, EfDestaque, EfLupa, EfOcultar, MidiaInfo, MidiaRef, ProjetoAberto, SobTitulo, TabelaQuadros } from "@domain/edicao";

const cmd = vi.hoisted(() => ({
  edicaoAbrir: vi.fn(), edicaoSalvar: vi.fn(), edicaoAnalisar: vi.fn(), edicaoTabelaQuadros: vi.fn(), edicaoMidiaInfo: vi.fn(), edicaoFila: vi.fn(), edicaoFundo: vi.fn(),
  edicaoSaidas: vi.fn(), edicaoSpriteTexto: vi.fn(), edicaoAtlasCarimbo: vi.fn(), edicaoQuadroFinal: vi.fn(), edicaoEnfileirar: vi.fn(), edicaoProxyPedir: vi.fn(), edicaoOnda: vi.fn(),
}));
vi.mock("@core/commands", () => ({ commands: cmd }));
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn(async () => () => {}) }));
vi.mock("@tauri-apps/api/core", () => ({ convertFileSrc: (p: string) => `asset://${p}`, invoke: vi.fn() }));

import { useEdicaoStore } from "../../store/edicaoStore";
import { useDialogosAbertos } from "@components/Dialog/Dialog";
import { EditorEdicao } from "../../EditorEdicao";
import { novoProjeto, novoTrecho } from "../../modelo/projeto";
import { tempoDoQuadro } from "../../modelo/tempo";
import { garantirMidia, useRecursos } from "../recursos";
import { useFundoStore } from "../../store/fundoStore";
import { useAreaVisivel } from "../areaVisivel";
import { meioDoQuadro } from "../faixa";
import { adicionarEfeito } from "../../inspetor/acoes";
import { adicionarLupa } from "../../inspetor/secoes/LupaEditor";
import { adicionarTextoLivre } from "../../inspetor/secoes/textoLivre";
import { botao, clicar, desmontarTudo, esperar, montar } from "../../inspetor/testeTela";

const WS = "/casos/area";
const SHA = "c".repeat(64);
const st = () => useEdicaoStore.getState();
const tab: TabelaQuadros = { tb: { num: 1, den: 15360 }, pts0: 0, passos: Array(299).fill(512), passo_mediano: 512, chaves: [0], vfr: false, origem: "decodificador" };
const info: MidiaInfo = {
  id: "v1", sha256: SHA, nome: "cam.mp4", exibido: { w: 1920, h: 1080 }, taxa: { num: 30, den: 1 }, vfr: false, passo: { min: 1 / 30, max: 1 / 30 },
  de_s: 0, ate_s: 10, n: 300, tem_audio: false, previa: { relativo: "cam.mp4", tipo: "original", desloc_s: 0, de_s: 0, ate_s: 10, largura: 1920, altura: 1080, pronta: true },
  tabela: "decodificador", erros: 0, marcavel: true, cor: { matriz: "bt709", faixa: "tv" },
};
const espera = (ms: number) => esperar(() => new Promise((r) => setTimeout(r, ms)));
let getContext: typeof HTMLCanvasElement.prototype.getContext;

beforeEach(async () => {
  for (const f of Object.values(cmd)) f.mockReset();
  cmd.edicaoAnalisar.mockImplementation(() => new Promise(() => {}));
  cmd.edicaoSalvar.mockImplementation(async () => ({ updated_at: new Date().toISOString() }));
  for (const f of [cmd.edicaoTabelaQuadros, cmd.edicaoMidiaInfo, cmd.edicaoSpriteTexto, cmd.edicaoAtlasCarimbo, cmd.edicaoQuadroFinal, cmd.edicaoProxyPedir, cmd.edicaoOnda]) f.mockImplementation(() => new Promise(() => {}));
  cmd.edicaoFila.mockResolvedValue([]);
  cmd.edicaoFundo.mockResolvedValue([]);
  cmd.edicaoSaidas.mockResolvedValue([]);
  getContext = HTMLCanvasElement.prototype.getContext;
  HTMLCanvasElement.prototype.getContext = (() => new Proxy({}, { get: () => () => undefined, set: () => true })) as unknown as typeof getContext;
  vi.spyOn(HTMLElement.prototype, "clientWidth", "get").mockReturnValue(800);
  vi.spyOn(HTMLElement.prototype, "clientHeight", "get").mockReturnValue(450);
  const p = novoProjeto();
  p.saida.largura = 1920;
  p.saida.altura = 1080;
  p.midias = [{ id: "m", sha256: SHA, tipo: "video", nome: "cam.mp4", relativo: "cam.mp4", vfr: false, tem_audio: false, ocultacoes: [] } as unknown as MidiaRef];
  p.principal = [{ ...novoTrecho("m", tempoDoQuadro(tab, 0), tempoDoQuadro(tab, 120)), id: "a", inicio_pts: 0, fim_pts: 120 * 512 } as ClipeTrecho];
  cmd.edicaoAbrir.mockResolvedValue({ projeto: p, importado_de: null, so_leitura: false, avisos: [], raiz_cache: "/cache/area" } satisfies ProjetoAberto);
  await st().abrir(WS, p.id);
  useDialogosAbertos.setState({ pilha: [] });
  garantirMidia(WS, SHA);
  useRecursos.setState((s) => ({ midias: { [SHA]: { info, tabela: tab, erro: null, pedindo: false } }, versao: s.versao + 1 }));
  useAreaVisivel.setState({ clipe: null, caixa: null });
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

const dentro = (k: Caixa, a: Caixa) => {
  const e = 1e-9;
  expect(k.x).toBeGreaterThanOrEqual(a.x - e);
  expect(k.y).toBeGreaterThanOrEqual(a.y - e);
  expect(k.x + k.w).toBeLessThanOrEqual(a.x + a.w + e);
  expect(k.y + k.h).toBeLessThanOrEqual(a.y + a.h + e);
};
const caixaDoRastreio = (id: string) => st().doc!.rastreios.find((r) => r.id === id)!.ancoras[0]!.v as Caixa;

/** Cinco giros da roda no canto de cima à esquerda: 1,25⁵ ≈ 3,05×. */
async function zoomNoCanto(raiz: HTMLElement) {
  for (let i = 0; i < 5; i++) {
    const caixa = raiz.querySelector<HTMLElement>("[data-caixa]")!;
    await act(async () => void caixa.dispatchEvent(new WheelEvent("wheel", { deltaY: -100, clientX: 120, clientY: 70, bubbles: true })));
  }
  await espera(30);
  const a = useAreaVisivel.getState();
  expect(a.clipe).toBe("a");
  expect(a.caixa!.w).toBeCloseTo(1 / 1.25 ** 5, 6);
  // longe do centro: a caixa de sempre (0,4..0,6) ficaria fora
  expect(a.caixa!.x + a.caixa!.w).toBeLessThan(0.4);
  return a.caixa!;
}

describe("área visível do palco × itens novos", () => {
  for (const modo of ["programa", "clipe"] as const)
    it(`zoom 3× num canto (${modo}): destaque, ocultar, lupa e texto livre nascem dentro da vista`, async () => {
      const t = await montar(<EditorEdicao ativo />);
      await act(async () => st().posicionar({ modo: "programa", q: 30 }));
      await espera(20);
      if (modo === "clipe") {
        await clicar(botao(t.raiz, "Clipe"));
        await espera(20);
        expect(st().cursor).toEqual({ modo: "clipe", clipe: "a", t: meioDoQuadro(tab, 30) });
      }
      const area = await zoomNoCanto(t.raiz);
      const efeitos = () => (st().doc!.principal[0] as ClipeTrecho).efeitos;

      const idD = await act(async () => adicionarEfeito("a", "destaque", "Destaque"));
      dentro(caixaDoRastreio((efeitos().find((e) => (e as EfDestaque).id === idD) as EfDestaque).rastreio), area);

      const idO = await act(async () => adicionarEfeito("a", "ocultar", "Ocultar"));
      const oc = st().doc!.midias[0]!.ocultacoes.find((e) => (e as EfOcultar).id === idO) as EfOcultar;
      dentro(caixaDoRastreio(oc.alvo.rastreio!), area);

      const idL = await act(async () => adicionarLupa("a", "Lupa"));
      dentro(caixaDoRastreio((efeitos().find((e) => (e as EfLupa).id === idL) as EfLupa).rastreio), area);

      // texto livre: centro da área (origem = canvas aqui: mesma proporção, sem enquadramento)
      const idT = await act(async () => adicionarTextoLivre());
      const livre = st().doc!.sobreposicoes.flatMap((s) => s.clipes).find((c) => c.id === idT) as SobTitulo;
      const { x, y } = livre.transformacao as { x: number; y: number };
      expect(x).toBeCloseTo(area.x + area.w / 2, 3);
      expect(y).toBeCloseTo(area.y + area.h / 2, 3);
    });
});
