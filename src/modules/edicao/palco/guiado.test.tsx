// @vitest-environment happy-dom
/**
 * Prévia da rampa em modo guiado (§7.3, P3.4): as contas puras contra o mapa do espelho e o palco de verdade tocando
 * um trecho em rampa (o elemento fica parado; o quadro pedido segue o mapa no ritmo do relógio).
 */

import { act } from "react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { ClipeTrecho, MidiaInfo, MidiaRef, ProjetoAberto, TabelaQuadros } from "@domain/edicao";

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
}));
vi.mock("@core/commands", () => ({ commands: cmd }));
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn(async () => () => {}) }));
vi.mock("@tauri-apps/api/core", () => ({ convertFileSrc: (p: string) => `asset://${p}`, invoke: vi.fn() }));

import { useEdicaoStore } from "../store/edicaoStore";
import { useDialogosAbertos } from "@components/Dialog/Dialog";
import { EditorEdicao } from "../EditorEdicao";
import { novoProjeto, novoTrecho } from "../modelo/projeto";
import { mapaTrecho, tempoDoQuadro } from "../modelo/tempo";
import { garantirMidia, useRecursos } from "./recursos";
import { useFundoStore } from "../store/fundoStore";
import { selecionarClipe } from "../sequencia/acoes";
import { desmontarTudo, esperar, montar } from "../inspetor/testeTela";
import { quadroGuiado, slotDoQuadro, tocaGuiado } from "./guiado";

const WS = "/casos/rampa";
const SHA = "e".repeat(64);
const st = () => useEdicaoStore.getState();
/** 300 quadros a 30 qps em 1/15360. */
const tab: TabelaQuadros = { tb: { num: 1, den: 15360 }, pts0: 0, passos: Array(299).fill(512), passo_mediano: 512, chaves: [0], vfr: false, origem: "decodificador" };
const t = (j: number) => tempoDoQuadro(tab, j);
const f30 = { num: 30, den: 1 };

describe("contas do modo guiado", () => {
  const rampa = { tipo: "rampa" as const, pontos: [ { t: t(30), v: 1 }, { t: t(60), v: 0.25 }, { t: t(90), v: 1 } ] };
  const mapa = mapaTrecho(tab, 0, 120, rampa, f30);

  it("só a rampa toca guiada", () => {
    const c = { ...novoTrecho("m", 0, 4), velocidade: rampa } as ClipeTrecho;
    expect([tocaGuiado(c), tocaGuiado({ ...c, velocidade: { tipo: "constante", fator: 0.1 } }), tocaGuiado(null)]).toEqual([true, false, false]);
  });

  it("o quadro de cada slot do relógio é o do mapa; do quadro 60 em diante 4 slots por quadro; null no fim", () => {
    const q0 = slotDoQuadro(mapa, 60, 1);
    expect(mapa.quadro_do_slot[q0]).toBe(60);
    // no mais lento: 1 quadro a cada 4 slots de 1/30 s
    expect([0, 1 / 30, 3 / 30, 4 / 30, 8 / 30].map((s) => quadroGuiado(mapa, 1, q0, s + 1e-6, f30))).toEqual([60, 60, 60, 61, 62]);
    // do começo: o mesmo que o render mostra em cada slot
    for (let q = 0; q < mapa.n_q; q++) expect(quadroGuiado(mapa, 1, 0, (q + 0.5) / 30, f30)).toBe(mapa.quadro_do_slot[q]);
    expect(quadroGuiado(mapa, 1, 0, mapa.n_q / 30 + 0.01, f30)).toBeNull();
    // por campo: o mapa conta campos (2 por quadro)
    expect(quadroGuiado({ ...mapa, quadro_do_slot: [0, 1, 2, 3, 4, 5], n_q: 6 }, 2, 0, 3.5 / 30, f30)).toBe(1);
    expect(slotDoQuadro({ ...mapa, quadro_do_slot: [0, 0, 1, 1, 2, 3], n_q: 6 }, 1, 2)).toBe(4);
  });
});

const info: MidiaInfo = {
  id: "v1", sha256: SHA, nome: "cam.mp4", exibido: { w: 640, h: 360 }, taxa: { num: 30, den: 1 }, vfr: false, passo: { min: 1 / 30, max: 1 / 30 },
  de_s: 0, ate_s: 10, n: 300, tem_audio: false, previa: { relativo: "cam.mp4", tipo: "proxy", desloc_s: 0, de_s: 0, ate_s: 10, largura: 640, altura: 360, pronta: true },
  tabela: "decodificador", erros: 0, marcavel: true, cor: { matriz: "bt709", faixa: "tv" },
};
const espera = (ms: number) => esperar(() => new Promise((r) => setTimeout(r, ms)));
const tecla = (key: string, code: string) => act(async () => void window.dispatchEvent(new KeyboardEvent("keydown", { key, code, bubbles: true })));

describe("palco tocando um trecho em rampa", () => {
  beforeEach(async () => {
    for (const f of Object.values(cmd)) f.mockReset();
    cmd.edicaoAnalisar.mockImplementation(() => new Promise(() => {}));
    cmd.edicaoSalvar.mockImplementation(async () => ({ updated_at: new Date().toISOString() }));
    for (const f of [cmd.edicaoTabelaQuadros, cmd.edicaoMidiaInfo, cmd.edicaoSpriteTexto, cmd.edicaoAtlasCarimbo, cmd.edicaoQuadroFinal, cmd.edicaoProxyPedir]) f.mockImplementation(() => new Promise(() => {}));
    cmd.edicaoFila.mockResolvedValue([]);
    cmd.edicaoFundo.mockResolvedValue([]);
    cmd.edicaoSaidas.mockResolvedValue([]);
    const p = novoProjeto();
    p.midias = [{ id: "m", sha256: SHA, tipo: "video", nome: "cam.mp4", relativo: "cam.mp4", vfr: false, ocultacoes: [] } as unknown as MidiaRef];
    // 0,25× do começo ao fim do trecho (a rampa de um ponto): 7,5 quadros por segundo de relógio
    const c = { ...novoTrecho("m", t(0), t(200)), id: "c", inicio_pts: 0, fim_pts: 200 * 512, velocidade: { tipo: "rampa", pontos: [{ t: t(0), v: 0.25 }] } } as ClipeTrecho;
    p.principal = [c];
    cmd.edicaoAbrir.mockResolvedValue({ projeto: p, importado_de: null, so_leitura: false, avisos: [], raiz_cache: "/cache/rampa" } satisfies ProjetoAberto);
    await st().abrir(WS, p.id);
    useDialogosAbertos.setState({ pilha: [] });
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

  it("o elemento fica parado e o quadro anda a 0,25× do relógio, pedido por busca", async () => {
    const tela = await montar(<EditorEdicao ativo />);
    await act(async () => selecionarClipe("c"));
    await espera(0);
    const v = tela.raiz.querySelector<HTMLVideoElement>("video[data-elemento]")!;
    await act(async () => void v.dispatchEvent(new Event("loadeddata")));
    await act(async () => void v.dispatchEvent(new Event("seeked")));
    await espera(50);
    const quadro = () => Number(/quadro (\d+)/.exec(tela.raiz.querySelector("[data-tempo]")!.textContent ?? "")?.[1]);
    const tocando = () => tela.raiz.querySelector('button[aria-label="Reproduzir"]')!.getAttribute("aria-pressed") === "true";
    expect(quadro()).toBe(0);
    const inicio = performance.now();
    await tecla(" ", "Space");
    expect(tocando()).toBe(true);
    expect(v.paused).toBe(true);
    const vistos: number[] = [];
    for (let i = 0; i < 6; i++) {
      await espera(120);
      await act(async () => void v.dispatchEvent(new Event("seeked")));
      await espera(20);
      vistos.push(quadro());
    }
    const decorrido = (performance.now() - inicio) / 1000;
    await tecla(" ", "Space");
    expect(tocando()).toBe(false);
    expect(v.paused).toBe(true);
    // andou, sem voltar, perto de 7,5 quadros por segundo (a 1× seriam 30)
    for (let i = 1; i < vistos.length; i++) expect(vistos[i]!).toBeGreaterThanOrEqual(vistos[i - 1]!);
    expect(vistos.at(-1)!).toBeGreaterThan(0);
    expect(vistos.at(-1)!).toBeLessThanOrEqual(Math.ceil(decorrido * 7.5) + 1);
  });
});
