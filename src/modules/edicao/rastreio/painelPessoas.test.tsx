// @vitest-environment happy-dom
/**
 * Painel do rastreio no editor montado (contratos-fase4.md D11, D12), com comandos e eventos falsos: "Motor" antes do
 * "Seguir" (a escolha vai no comando; não instalado desligado com o ícone de baixar; IA desligada esconde os neurais);
 * contador de marcações à mão (ícone de mão + número, fora as do "Simplificar"); "Rastrear pessoas" ao lado do
 * "Da análise", que pede a lista de novo no fim e fica em destaque até ser aberto. Só rótulos de (h) e dados.
 */

import { act } from "react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { MidiaRef, ProjetoAberto, Rastreio } from "@domain/edicao";
import type { PessoasEstado } from "@domain/video_pessoas";
import type { AiSettings } from "@domain/app_settings";

const ev = vi.hoisted(() => ({ ouvintes: new Map<string, (e: { payload: unknown }) => void>() }));
vi.mock("@tauri-apps/api/event", () => ({
  listen: vi.fn(async (nome: string, f: (e: { payload: unknown }) => void) => {
    ev.ouvintes.set(nome, f);
    return () => ev.ouvintes.delete(nome);
  }),
}));
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
  edicaoRastreioSeguir: vi.fn(),
  edicaoRastreioCancelar: vi.fn(),
  edicaoRastreioLer: vi.fn(),
  edicaoRastreioEditar: vi.fn(),
  edicaoRastreioDePessoas: vi.fn(),
  pessoasListar: vi.fn(),
  pessoasRastrear: vi.fn(),
  pessoasCancelar: vi.fn(),
}));
vi.mock("@core/commands", () => ({ commands: cmd }));
vi.mock("@tauri-apps/api/core", () => ({ convertFileSrc: (p: string) => `asset://${p}`, invoke: vi.fn() }));

import { useEdicaoStore } from "../store/edicaoStore";
import { useDialogosAbertos } from "@components/Dialog/Dialog";
import { useSettingsStore } from "@stores/settingsStore";
import { EditorEdicao } from "../EditorEdicao";
import { selecionarClipe } from "../sequencia/acoes";
import { selecionarEfeito } from "../inspetor/acoes";
import { receberMarca, useInspetorUi } from "../inspetor/ui";
import { esquecerLeituras } from "../inspetor/contexto";
import { garantirMidia, useRecursos } from "../palco/recursos";
import { useFundoStore } from "../store/fundoStore";
import { botao, clicar, desmontarTudo, esperar, montar, textosVisiveis, type Tela } from "../inspetor/testeTela";
import { textoPermitido } from "../inspetor/rotulos.teste";
import { esquecerRastreios, useRastreioStore } from "../store/rastreioStore";
import { esquecerPessoasNaEdicao } from "./RastrearPessoas";
import { CAIXA, info, projeto, q, SHA, tab, WS } from "./cenario.teste";
import styles from "../inspetor/Inspetor.module.css";

const st = () => useEdicaoStore.getState();
const r1 = (): Rastreio => st().doc!.rastreios.find((r) => r.id === "r1")!;
const espera = (ms: number) => esperar(() => new Promise((r) => setTimeout(r, ms)));
const ir = (j: number) => act(async () => st().posicionar({ modo: "clipe", clipe: "c", t: q(j).t }));
const ai = (o: Partial<AiSettings>) => act(async () => useSettingsStore.setState((s) => ({ settings: { ...s.settings, ai: { ...s.settings.ai, ...o } } })));
const PRONTO: Partial<AiSettings> = {
  modelos_desligados: false,
  rastreio_dir: "",
  ort_lib_path: "/ai/ort/libonnxruntime.so",
  pessoas_model_path: "/ai/pessoas/yolox_s.onnx",
  sr_ort_lib_path: "",
  gpu_dir: "",
};
const pessoas = (n: number): PessoasEstado =>
  ({ run: null, tamanho: null, pessoas: Array.from({ length: n }, (_, i) => ({ id: `p${i + 1}`, numero: i + 1, nome: "", origem: "auto", estado: "auto", primeiro_s: 10 * i, ultimo_s: 10 * i + 7.5 })) }) as unknown as PessoasEstado;

let editor: Tela;
const painel = () => editor.raiz.querySelector<HTMLElement>('[data-rastreio="r1"]')!;
const acao = (a: string) => painel().querySelector<HTMLButtonElement>(`[data-acao="${a}"]`);
const chip = (m: string) => painel().querySelector<HTMLButtonElement>(`[data-campo="Motor"] [data-valor="${m}"]`);
const contador = () => painel().querySelector<HTMLElement>("[data-marcadas-a-mao]");
const conferirTextos = () => {
  expect(textosVisiveis(painel()).filter((t) => !textoPermitido(t))).toEqual([]);
  expect(painel().querySelector("[placeholder]")).toBeNull();
};

beforeEach(async () => {
  vi.useFakeTimers({ toFake: ["Date"], now: new Date("2026-11-17T10:00:00Z") });
  for (const f of Object.values(cmd)) f.mockReset();
  ev.ouvintes.clear();
  cmd.edicaoAnalisar.mockImplementation(() => new Promise(() => {}));
  cmd.edicaoSalvar.mockImplementation(async () => ({ updated_at: new Date().toISOString() }));
  for (const f of [cmd.edicaoSpriteTexto, cmd.edicaoAtlasCarimbo, cmd.edicaoQuadroFinal]) f.mockImplementation(() => new Promise(() => {}));
  cmd.edicaoTabelaQuadros.mockResolvedValue(tab);
  cmd.edicaoMidiaInfo.mockResolvedValue(info);
  cmd.edicaoFila.mockResolvedValue([]);
  cmd.edicaoFundo.mockResolvedValue([]);
  cmd.edicaoSaidas.mockResolvedValue([]);
  cmd.pessoasCancelar.mockResolvedValue(undefined);
  vi.stubGlobal("localStorage", { getItem: () => null, setItem: () => {}, removeItem: () => {} });
  esquecerLeituras();
  esquecerRastreios();
  esquecerPessoasNaEdicao();
  useInspetorUi.setState({ aba: "clipe", ferramenta: null, aoMarcar: null, chave: null });
  useSettingsStore.setState((s) => ({ settings: { ...s.settings, profile: { ...s.settings.profile, full_name: "Perito Teste" }, ai: { ...s.settings.ai, ...PRONTO } } }));
  const p = projeto();
  // mídia do caso (o cenário do P2.4 não diz de onde veio)
  (p.midias[0] as MidiaRef).origem = "caso";
  cmd.edicaoAbrir.mockResolvedValue({ projeto: p, importado_de: null, so_leitura: false, avisos: [], raiz_cache: "/cache/r" } satisfies ProjetoAberto);
  await st().abrir(WS, p.id);
  useDialogosAbertos.setState({ pilha: [] });
  garantirMidia(WS, SHA);
  useRecursos.setState((s) => ({ midias: { [SHA]: { info, tabela: tab, erro: null, pedindo: false } }, versao: s.versao + 1 }));
  editor = await montar(<EditorEdicao ativo />);
  await act(async () => selecionarClipe("c"));
  await espera(0);
  await act(async () => selecionarEfeito("e1"));
  await ir(30);
  await espera(20);
});

afterEach(async () => {
  desmontarTudo();
  vi.unstubAllGlobals();
  useFundoStore.setState({ tarefas: [] });
  await st()
    .fechar({ descartar: true })
    .catch(() => {});
  vi.useRealTimers();
});

describe("Motor", () => {
  it("antes do Seguir; as quatro escolhas; não instalado desligado com o ícone de baixar; marcado = o que vai", async () => {
    const motor = painel().querySelector<HTMLElement>('[data-campo="Motor"]')!;
    const seguir = acao("seguir")!;
    expect(motor.compareDocumentPosition(seguir) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
    expect([...motor.querySelectorAll("button")].map((b) => b.textContent?.trim())).toEqual(["Automático", "EdgeTAM", "VitTrack", "Mancha"]);
    // sem o item Rastreador: EdgeTAM desligado com o ícone; VitTrack tem o onnxruntime das Pessoas
    expect(chip("edgetam")!.disabled).toBe(true);
    expect(chip("edgetam")!.dataset.estado).toBe("ausente");
    expect(chip("edgetam")!.querySelector("svg.lucide-download")).not.toBeNull();
    expect(chip("vittrack")!.disabled).toBe(false);
    expect(chip("auto")!.getAttribute("aria-pressed")).toBe("true");
    conferirTextos();
  });

  it("a escolha vai no comando do Seguir e fica guardada; desligada durante a corrida", async () => {
    await clicar(chip("vittrack")!);
    expect(useRastreioStore.getState().motor).toBe("vittrack");
    expect(chip("vittrack")!.getAttribute("aria-pressed")).toBe("true");
    expect(chip("auto")!.getAttribute("aria-pressed")).toBe("false");
    cmd.edicaoRastreioSeguir.mockResolvedValueOnce("t1");
    await clicar(acao("seguir")!);
    await espera(0);
    expect(cmd.edicaoRastreioSeguir.mock.calls[0]!.slice(2)).toEqual(["r1", "c", { ...q(30), v: CAIXA }, "frente", q(199).t, "vittrack"]);
    expect(chip("mancha")!.disabled).toBe(true);
  });

  it("instalado depois: o EdgeTAM liga; IA desligada: os neurais somem e o pedido sai automático", async () => {
    await ai({ rastreio_dir: "/ai/rastreio" });
    expect(chip("edgetam")!.disabled).toBe(false);
    await clicar(chip("edgetam")!);
    await ai({ modelos_desligados: true });
    expect(chip("edgetam")).toBeNull();
    expect(chip("vittrack")).toBeNull();
    expect(chip("auto")!.getAttribute("aria-pressed")).toBe("true");
    cmd.edicaoRastreioSeguir.mockResolvedValueOnce("t2");
    await clicar(acao("seguir")!);
    expect(cmd.edicaoRastreioSeguir.mock.calls[0]![7]).toBe("auto");
    // "Rastrear pessoas" é IA: some também
    expect(acao("rastrear-pessoas")).toBeNull();
  });
});

describe("contador de marcações à mão", () => {
  it("ícone de mão + número; acompanha as marcações; as do Simplificar não contam; mexer numa delas conta", async () => {
    const c = contador()!;
    expect(c.querySelector("svg.lucide-hand")).not.toBeNull();
    expect(c.textContent?.trim()).toBe("1");
    expect(c.dataset.marcadasAMao).toBe("1");
    // "Marcar" no palco: a ferramenta ligada recebe a caixa do quadro
    await clicar(editor.raiz.querySelector<HTMLButtonElement>('[data-acao="marcar"]')!);
    await ir(60);
    await act(async () => receberMarca({ ...CAIXA, x: 0.5 }, q(60).t, q(60).pts));
    await act(async () => useInspetorUi.getState().cancelarMarcacao());
    expect(r1().ancoras.map((k) => k.pts)).toEqual([q(30).pts, q(60).pts]);
    expect(contador()!.textContent?.trim()).toBe("2");
    // o Simplificar devolveu 3 âncoras, uma no mesmo quadro de uma do perito (fica do perito)
    await act(async () =>
      st().alterar("Simplificar", (d) => {
        const r = d.rastreios[0]!;
        r.ancoras = [{ ...q(30), v: CAIXA }, { ...q(45), v: CAIXA }, { ...q(60), v: { ...CAIXA, x: 0.5 } }, { ...q(90), v: CAIXA }];
        r.ancoras_do_programa = [q(45).t, q(90).t];
      }),
    );
    expect(contador()!.textContent?.trim()).toBe("2");
    // o perito ajusta a do 90: vira dele (a store tira da lista do programa)
    await act(async () => st().alterar("Marcar", (d) => void (d.rastreios[0]!.ancoras[3]!.v = { ...CAIXA, y: 0.3 })));
    expect(r1().ancoras_do_programa).toEqual([q(45).t]);
    expect(contador()!.textContent?.trim()).toBe("3");
    conferirTextos();
  });
});

describe("Rastrear pessoas no painel", () => {
  it("ao lado do Da análise; no fim a lista é pedida de novo e o Da análise fica em destaque até abrir", async () => {
    cmd.pessoasListar.mockResolvedValueOnce(pessoas(0));
    // primeiro "Da análise": sem trilha, desliga
    await clicar(acao("da-analise")!);
    await espera(0);
    expect(acao("da-analise")!.disabled).toBe(true);
    // Rastrear pessoas: lista (nada detectado ainda), roda, termina
    let terminar!: (e: PessoasEstado) => void;
    cmd.pessoasListar.mockResolvedValueOnce(pessoas(0));
    cmd.pessoasRastrear.mockImplementationOnce(() => new Promise<PessoasEstado>((r) => (terminar = r)));
    expect(acao("rastrear-pessoas")!.nextElementSibling).toBe(acao("da-analise"));
    await clicar(acao("rastrear-pessoas")!);
    await espera(0);
    expect(cmd.pessoasRastrear).toHaveBeenCalledWith(WS, SHA, expect.objectContaining({ classes: expect.any(Array) }));
    await act(async () => ev.ouvintes.get("pessoas-progress")!({ payload: { media_hash: SHA, fase: "rastrear", feitos: 40, total: 300, pessoas: 2 } }));
    expect(painel().textContent).toContain("40/300");
    expect(acao("da-analise")!.disabled).toBe(true);
    conferirTextos();
    cmd.pessoasListar.mockResolvedValue(pessoas(2));
    await act(async () => terminar(pessoas(2)));
    await espera(0);
    expect(cmd.pessoasListar).toHaveBeenCalledTimes(3);
    const da = acao("da-analise")!;
    expect(da.disabled).toBe(false);
    expect(da.className).toContain(styles.miniAtivo);
    await clicar(da);
    await espera(0);
    const menu = editor.raiz.querySelector<HTMLElement>('[role="menu"]')!;
    expect([...menu.querySelectorAll('[role="menuitem"]')].map((b) => b.textContent?.trim())).toEqual(["#1 0:00–0:07", "#2 0:10–0:17"]);
    expect(acao("da-analise")!.className).not.toContain(styles.miniAtivo);
    // a trilha automática entra sem confirmação (regra da Fase 2)
    cmd.edicaoRastreioDePessoas.mockResolvedValue({ ...r1(), id: "r9", ancoras: [], origem: { tipo: "pessoas", pessoa_id: "p2", estado: "auto", copiado_em: "2026-11-17T10:00:00Z" } });
    await clicar(botao(menu, "#2 0:10–0:17"));
    await espera(0);
    expect(cmd.edicaoRastreioDePessoas).toHaveBeenCalledWith(WS, st().aberta!.projetoId, "m", SHA, "p2");
    expect(st().doc!.rastreios.map((r) => [r.id, r.confirmacao])).toEqual([["r9", null]]);
    expect(editor.raiz.querySelector('[data-rastreio="r9"] [data-marcadas-a-mao="0"]')).not.toBeNull();
  });

  it("terminou sem ninguém: o Da análise segue desligado", async () => {
    cmd.pessoasListar.mockResolvedValue(pessoas(0));
    cmd.pessoasRastrear.mockResolvedValue(pessoas(0));
    await clicar(acao("rastrear-pessoas")!);
    await espera(0);
    expect(cmd.pessoasRastrear).toHaveBeenCalledTimes(1);
    expect(acao("da-analise")!.disabled).toBe(true);
    expect(acao("da-analise")!.className).not.toContain(styles.miniAtivo);
  });
});
