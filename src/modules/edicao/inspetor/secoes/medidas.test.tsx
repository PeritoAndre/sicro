// @vitest-environment happy-dom
/**
 * Régua e velocidade no Inspetor de verdade (P1.3): "Adicionar" busca o retrato da análise pelo comando e cria o
 * efeito num passo; sem calibração ou cálculo, o aviso com a frase do catálogo e nada criado; os campos gravam um
 * passo com o rótulo; só rótulos do §8.6, nenhum texto de exemplo.
 */

import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { ClipeTrecho, EfRegua, EfVelocidadeExibida, ProjetoAberto, ProjetoEdicao, RetratoCalculo, RetratoCalibracao } from "@domain/edicao";

const cmd = vi.hoisted(() => ({
  edicaoAbrir: vi.fn(),
  edicaoSalvar: vi.fn(),
  edicaoAnalisar: vi.fn(),
  edicaoTabelaQuadros: vi.fn(),
  edicaoMidiaInfo: vi.fn(),
  edicaoCapacidades: vi.fn(),
  listSpeedCalibrations: vi.fn(),
  listSpeedCalculations: vi.fn(),
  edicaoRetratoCalibracao: vi.fn(),
  edicaoRetratoCalculo: vi.fn(),
}));
vi.mock("@core/commands", () => ({ commands: cmd }));
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn(async () => () => {}) }));
vi.mock("@tauri-apps/plugin-dialog", () => ({ open: vi.fn(async () => null) }));
const toast = vi.hoisted(() => ({ pushToast: vi.fn() }));
vi.mock("@components/toast/toastStore", () => toast);

import { useEdicaoStore } from "../../store/edicaoStore";
import { Inspetor } from "../Inspetor";
import { esquecerLeituras } from "../contexto";
import { ADICIONAR, descritor } from "../registro";
import { LUPA_PRONTA } from "./LupaEditor";
import { HOLOFOTE_PRONTO } from "./HolofoteEditor";
import { useInspetorUi } from "../ui";
import { botao, campo, clicar, desmontarTudo, digitar, entradaDe, escolher, esperar, lerFixture as ler, montar, textosVisiveis, type Tela } from "../testeTela";
import { textoPermitido } from "../rotulos.teste";
import { pontasConferidas, pontasDaRegua } from "../../palco/desenho/medidas";

const WS = "/casos/c1";
const st = () => useEdicaoStore.getState();
const passos = () => st().passado.length;
const ultimo = () => st().passado.at(-1)?.rotulo;
const doc = () => st().doc!;
const trecho = () => doc().principal[1] as ClipeTrecho;
const SHA = "1".repeat(64);
const ID1 = "a0000000-0000-4000-8000-000000000001";
const ID2 = "a0000000-0000-4000-8000-000000000002";
const ID3 = "a0000000-0000-4000-8000-000000000003";

const calibracao = (id: string, created_at: string, w: number) => ({
  id, occurrence_id: "o", media_hash: SHA, method: "plane", reference_source: "campo", residuals_px: 0.01, distortion_model: null, author: "", created_at,
  control_points: [{ px: 0, py: 0, world_x_m: 0, world_y_m: 0 }, { px: 100, py: 0, world_x_m: w, world_y_m: 0 }, { px: 100, py: 50, world_x_m: w, world_y_m: 3 }, { px: 0, py: 50, world_x_m: 0, world_y_m: 3 }],
  homography: [0.01, 0, 0, 0, 0.01, 0, 0, 0, 1],
});
const retrato = (id: string): RetratoCalibracao => ({
  id, metodo: "plane", h: [0.01, 0, 0, 0, 0.01, 0, 0, 0, 1], espaco: { w: 1920, h: 1080 }, pontos_sha256: "b".repeat(64), residuo_m: 0.01,
  regiao: [{ x: 0.2, y: 0.2 }, { x: 0.8, y: 0.2 }, { x: 0.8, y: 0.6 }, { x: 0.2, y: 0.6 }],
});
const calculo = (id: string, tempos: number[], created_at: string) => ({
  id, occurrence_id: "o", media_hash: SHA, calibration_id: ID1, velocity_kmh: 41.6, vx_m_per_s: 11, vy_m_per_s: 3, se_m_per_s: 0.5, ci_low: 39, ci_high: 44,
  confidence: 0.95, r_squared: 0.99, residuals: [], mc_seed: 7, mc_sigmas: null, mc_n: 10000, mc_failed: 0, mc_mean_kmh: 41.7, mc_median_kmh: 41.6,
  mc_p2_5_kmh: 38.7, mc_p97_5_kmh: 45.2, limitations: [], audit: null, author: "", created_at,
  points: tempos.map((t) => ({ storyboard_frame_id: null, export_id: null, px: 1, py: 2, u_px: 1, actual_timestamp_s: t, delta_s: null, manual: true })),
});
const retratoCalc = (id: string, de_s: number, ate_s: number): RetratoCalculo => ({ id, kmh: 41.6, faixa: [38.7, 45.2], de_s, ate_s, metodo: "monte_carlo" });

let tela: Tela;

beforeEach(async () => {
  vi.useFakeTimers({ toFake: ["Date"], now: new Date("2026-11-14T10:00:00Z") });
  for (const f of Object.values(cmd)) f.mockReset();
  toast.pushToast.mockReset();
  cmd.edicaoSalvar.mockImplementation(async () => ({ updated_at: new Date().toISOString() }));
  cmd.edicaoAnalisar.mockImplementation(async () => ({ problemas: [], linha: { fps: { num: 30, den: 1 }, total_q: 0, clipes: [], livres: [] }, midias: [], prender: [], saida_auto: null }));
  cmd.edicaoTabelaQuadros.mockRejectedValue({ kind: "validation", message: "validation error: tabela_pendente: ainda não" });
  cmd.edicaoMidiaInfo.mockRejectedValue({ kind: "validation", message: "validation error: tabela_pendente: ainda não" });
  cmd.listSpeedCalibrations.mockResolvedValue([calibracao(ID1, "2026-11-13T10:00:00Z", 4), calibracao(ID2, "2026-11-12T09:30:00Z", 6)]);
  cmd.edicaoRetratoCalibracao.mockImplementation(async (_ws: string, id: string) => retrato(id));
  // o mais novo fora do trecho (12 a 15,5 s); o que cruza vem depois na lista
  cmd.listSpeedCalculations.mockResolvedValue([calculo(ID3, [40, 41], "2026-11-13T11:00:00Z"), calculo(ID2, [12.5, 13.1, 14], "2026-11-12T11:00:00Z")]);
  cmd.edicaoRetratoCalculo.mockImplementation(async (_ws: string, id: string) => (id === ID2 ? retratoCalc(ID2, 12.5, 14) : retratoCalc(id, 40, 41)));
  esquecerLeituras();
  useInspetorUi.setState({ aba: "clipe", ferramenta: null, aoMarcar: null, chave: null });
  const r: ProjetoAberto = { projeto: ler<ProjetoEdicao>("projeto-do-store.json"), importado_de: null, so_leitura: false, avisos: [], raiz_cache: "/cache/c1" };
  cmd.edicaoAbrir.mockResolvedValueOnce(r);
  await st().abrir(WS, r.projeto.id);
  tela = await montar(<Inspetor />);
  await esperar(() => {
    const it = { k: "clipe" as const, id: trecho().id };
    st().selecionar({ itens: [it], foco: it, efeito: null });
  });
});

afterEach(async () => {
  desmontarTudo();
  await st()
    .fechar({ descartar: true })
    .catch(() => {});
  vi.useRealTimers();
});

const assentar = () => esperar(() => new Promise((r) => setTimeout(r, 0)));

async function adicionar(rotulo: string) {
  await clicar(tela.raiz.querySelector<HTMLButtonElement>('[data-acao="adicionar"]')!);
  await clicar([...document.querySelectorAll<HTMLElement>("button")].find((x) => x.textContent?.trim() === rotulo)!);
  for (let i = 0; i < 4; i++) await assentar();
}

function conferirTextos() {
  expect(textosVisiveis(tela.raiz).filter((t) => !textoPermitido(t))).toEqual([]);
  expect(tela.raiz.querySelectorAll("[placeholder]").length).toBe(0);
}

describe("registro (P1.3)", () => {
  it("Adicionar oferece Régua e Velocidade na ordem do §8.6, com editor", () => {
    expect(ADICIONAR).toEqual(["destaque", "ocultar", ...(HOLOFOTE_PRONTO ? ["holofote"] : []), ...(LUPA_PRONTA ? ["lupa"] : []), "regua", "velocidade_exibida", "texto", "carimbo", "imagem"]);
    for (const tipo of ["regua", "velocidade_exibida"] as const) {
      const { d, editavel } = descritor({ tipo, id: "x" } as unknown as EfRegua);
      expect([d.tipo, editavel]).toEqual([tipo, true]);
    }
  });
});

describe("Régua", () => {
  it("Adicionar: retrato da calibração mais nova pelo comando, âncora no centro da região, um passo", async () => {
    const n = passos();
    await adicionar("Régua");
    expect(cmd.listSpeedCalibrations).toHaveBeenCalledWith(WS, SHA);
    expect(cmd.edicaoRetratoCalibracao).toHaveBeenCalledWith(WS, ID1);
    const e = trecho().efeitos.at(-1) as EfRegua;
    expect(e.tipo).toBe("regua");
    expect(e.calibracao).toEqual(retrato(ID1));
    expect(e.ancora).toEqual({ x: 0.5, y: 0.4 });
    expect([e.comprimento_m, e.divisoes, e.direcao_graus, e.rotulo]).toEqual([1, 2, 0, true]);
    expect(pontasDaRegua(e)).toHaveLength(3);
    expect([passos(), ultimo(), st().selecao.efeito]).toEqual([n + 1, "Régua", e.id]);
    const linha = tela.raiz.querySelector<HTMLElement>('[data-efeito="regua"]')!;
    for (const r of ["Calibração", "Comprimento (m)", "Divisões", "Direção", "Cor", "Espessura", "Contorno", "Rótulo"]) expect(() => campo(linha, r), r).not.toThrow();
    conferirTextos();
    // a lista mostra as calibrações da mídia (dados) e a escolhida
    const sel = entradaDe(linha, "Calibração") as HTMLSelectElement;
    expect([...sel.options].map((o) => o.value)).toEqual([ID1, ID2]);
    expect(sel.value).toBe(ID1);
  });

  it("Calibração troca o retrato, Rótulo e Comprimento gravam um passo cada; Marcar põe a âncora no ponto do palco", async () => {
    await adicionar("Régua");
    const id = st().selecao.efeito!;
    const linha = () => tela.raiz.querySelector<HTMLElement>('[data-efeito="regua"]')!;
    const n = passos();
    await escolher(entradaDe(linha(), "Calibração") as HTMLSelectElement, ID2);
    await assentar();
    const e = () => trecho().efeitos.find((x) => (x as EfRegua).id === id) as EfRegua;
    expect(e().calibracao.id).toBe(ID2);
    expect([passos(), ultimo()]).toEqual([n + 1, "Calibração"]);
    await clicar(entradaDe(linha(), "Rótulo") as HTMLInputElement);
    expect([e().rotulo, ultimo()]).toEqual([false, "Rótulo"]);
    await digitar(entradaDe(linha(), "Comprimento (m)") as HTMLInputElement, "2,5");
    expect([e().comprimento_m, ultimo()]).toEqual([2.5, "Comprimento (m)"]);
    await digitar(entradaDe(linha(), "Divisões") as HTMLInputElement, "4");
    expect([e().divisoes, ultimo()]).toEqual([4, "Divisões"]);
    await clicar(linha().querySelector<HTMLButtonElement>('[data-acao="marcar-ancora"]')!);
    expect(useInspetorUi.getState().ferramenta).toEqual({ tipo: "ponto", efeito: id });
    await esperar(() => useInspetorUi.getState().marcado({ x: 0.3, y: 0.35, w: 0, h: 0 }, 12.5, 6400));
    expect([e().ancora, ultimo(), useInspetorUi.getState().ferramenta]).toEqual([{ x: 0.3, y: 0.35 }, "Marcar", null]);
    // o retrato de novo pelo ícone: mesma calibração, pedida ao backend, e a lista também (a análise pode ter criado outra)
    cmd.edicaoRetratoCalibracao.mockClear();
    cmd.listSpeedCalibrations.mockClear();
    cmd.listSpeedCalibrations.mockResolvedValue([calibracao(ID3, "2026-11-14T09:00:00Z", 5), calibracao(ID1, "2026-11-13T10:00:00Z", 4), calibracao(ID2, "2026-11-12T09:30:00Z", 6)]);
    await clicar(linha().querySelector<HTMLButtonElement>('[data-acao="retrato"]')!);
    await assentar();
    expect(cmd.edicaoRetratoCalibracao).toHaveBeenCalledWith(WS, ID2);
    expect(cmd.listSpeedCalibrations).toHaveBeenCalledTimes(1);
    const sel = entradaDe(linha(), "Calibração") as HTMLSelectElement;
    expect([[...sel.options].map((o) => o.value), sel.value]).toEqual([[ID3, ID1, ID2], ID2]);
    conferirTextos();
  });

  it("Adicionar em região calibrada estreita: a régua nasce dentro dela (a 90°, ou do primeiro ponto)", async () => {
    // 1,92 m de largura (100 px/m): do centro, 1 m a 0° sai da região; a 90° cabe
    const estreita = [{ x: 0.45, y: 0.2 }, { x: 0.55, y: 0.2 }, { x: 0.55, y: 0.6 }, { x: 0.45, y: 0.6 }];
    cmd.edicaoRetratoCalibracao.mockImplementation(async (_ws: string, id: string) => ({ ...retrato(id), regiao: estreita }));
    await adicionar("Régua");
    const e = trecho().efeitos.at(-1) as EfRegua;
    expect([e.ancora, e.direcao_graus]).toEqual([{ x: 0.5, y: 0.4 }, 90]);
    expect(() => pontasConferidas(e)).not.toThrow();
    // 1,92 m × 0,97 m: do centro não cabe em nenhuma das duas; do primeiro ponto, pela borda, cabe
    const baixa = [{ x: 0.45, y: 0.2 }, { x: 0.55, y: 0.2 }, { x: 0.55, y: 0.29 }, { x: 0.45, y: 0.29 }];
    cmd.edicaoRetratoCalibracao.mockImplementation(async (_ws: string, id: string) => ({ ...retrato(id), regiao: baixa }));
    await adicionar("Régua");
    const e2 = trecho().efeitos.at(-1) as EfRegua;
    expect([e2.ancora, e2.direcao_graus]).toEqual([{ x: 0.45, y: 0.2 }, 0]);
    expect(() => pontasConferidas(e2)).not.toThrow();
  });

  it("cross_ratio (1D): Direção só com os dois sentidos", async () => {
    cmd.edicaoRetratoCalibracao.mockImplementation(async (_ws: string, id: string) => ({ ...retrato(id), metodo: "cross_ratio", regiao: [{ x: 0.1, y: 0.8 }, { x: 0.6, y: 0.4 }] }));
    await adicionar("Régua");
    const linha = tela.raiz.querySelector<HTMLElement>('[data-efeito="regua"]')!;
    const d = campo(linha, "Direção");
    expect([...d.querySelectorAll("button")].map((b) => b.getAttribute("data-valor"))).toEqual(["0", "180"]);
    await clicar(d.querySelector<HTMLButtonElement>('[data-valor="180"]')!);
    expect([(trecho().efeitos.at(-1) as EfRegua).direcao_graus, ultimo()]).toEqual([180, "Direção"]);
  });

  it("sem calibração: nada criado e o aviso com a frase do catálogo", async () => {
    cmd.listSpeedCalibrations.mockResolvedValue([]);
    cmd.edicaoRetratoCalibracao.mockRejectedValue({ kind: "validation", message: "validation error: calibracao_ausente: Calibração não encontrada" });
    const n = [passos(), trecho().efeitos.length];
    await adicionar("Régua");
    expect(cmd.edicaoRetratoCalibracao).toHaveBeenCalledWith(WS, "");
    expect(toast.pushToast).toHaveBeenCalledWith("error", "Calibração não encontrada");
    expect([passos(), trecho().efeitos.length]).toEqual(n);
  });
});

describe("Velocidade", () => {
  it("Adicionar: o cálculo que cruza o trecho, ativo só no intervalo medido, texto com faixa", async () => {
    const n = passos();
    await adicionar("Velocidade");
    expect(cmd.edicaoRetratoCalculo).toHaveBeenCalledWith(WS, ID2);
    const e = trecho().efeitos.at(-1) as EfVelocidadeExibida;
    expect([e.tipo, e.calculo, e.ativo]).toEqual(["velocidade_exibida", retratoCalc(ID2, 12.5, 14), { de: 12.5, ate: 14 }]);
    expect(e.onde).toEqual({ modo: "fixa", lugar: { x: 0.5, y: 0.06, h: "centro", v: "topo" } });
    expect([passos(), ultimo()]).toEqual([n + 1, "Velocidade"]);
    const linha = tela.raiz.querySelector<HTMLElement>('[data-efeito="velocidade_exibida"]')!;
    expect(linha.querySelector("[data-texto-velocidade]")!.textContent).toBe("42 km/h (38 a 46)");
    conferirTextos();
  });

  it("Acompanha cria o rastreio no quadro (com Marcar na linha); Fixa de novo tira o rastreio que ninguém usa", async () => {
    await adicionar("Velocidade");
    const id = st().selecao.efeito!;
    const e = () => trecho().efeitos.find((x) => (x as EfVelocidadeExibida).id === id) as EfVelocidadeExibida;
    const linha = () => tela.raiz.querySelector<HTMLElement>('[data-efeito="velocidade_exibida"]')!;
    const rastreios = doc().rastreios.length;
    await clicar(botao(linha(), "Acompanha"));
    const onde = e().onde;
    expect(onde.modo).toBe("segue");
    const r = doc().rastreios.at(-1)!;
    expect([doc().rastreios.length, onde.modo === "segue" && onde.rastreio, r.midia, ultimo()]).toEqual([rastreios + 1, r.id, "m1", "Posição"]);
    expect(linha().querySelector('[data-acao="marcar"]')).toBeTruthy();
    await clicar(linha().querySelector<HTMLButtonElement>('[data-valor="dir"]')!);
    expect(e().onde).toEqual({ modo: "segue", rastreio: r.id, dx: 0.15, dy: 0 });
    conferirTextos();
    await clicar(botao(linha(), "Fixa"));
    expect([e().onde.modo, doc().rastreios.length]).toEqual(["fixa", rastreios]);
    // Cálculo troca o retrato e, no trecho, o ativo
    await escolher(entradaDe(linha(), "Cálculo") as HTMLSelectElement, ID3);
    await assentar();
    expect([e().calculo.id, e().ativo, ultimo()]).toEqual([ID3, { de: 40, ate: 41 }, "Cálculo"]);
  });

  it("Fixa de novo não tira o rastreio que o zoom acompanha", async () => {
    await adicionar("Velocidade");
    const linha = () => tela.raiz.querySelector<HTMLElement>('[data-efeito="velocidade_exibida"]')!;
    await clicar(botao(linha(), "Acompanha"));
    const r = doc().rastreios.at(-1)!.id;
    const z = { id: "z1", ligado: true, nome: "", ativo: null, tipo: "enquadramento", regiao: { x: 0, y: 0, w: 0.5, h: 0.5 }, ajuste: "cobrir",
      interpolacao: "bilinear", segue: { rastreio: r, suavizar_s: 1 } };
    st().alterar("Acompanhar rastreio", (d) => void (d.principal[1] as ClipeTrecho).efeitos.push(z as unknown as ClipeTrecho["efeitos"][number]));
    await clicar(botao(linha(), "Fixa"));
    expect(doc().rastreios.some((x) => x.id === r)).toBe(true);
  });

  it("sem cálculo: nada criado e o aviso com a frase do catálogo", async () => {
    cmd.listSpeedCalculations.mockResolvedValue([]);
    cmd.edicaoRetratoCalculo.mockRejectedValue({ kind: "validation", message: "validation error: calculo_ausente: Cálculo de velocidade não encontrado" });
    const n = [passos(), trecho().efeitos.length];
    await adicionar("Velocidade");
    expect(cmd.edicaoRetratoCalculo).toHaveBeenCalledWith(WS, "");
    expect(toast.pushToast).toHaveBeenCalledWith("error", "Cálculo de velocidade não encontrado");
    expect([passos(), trecho().efeitos.length]).toEqual(n);
  });
});
