// @vitest-environment happy-dom
/**
 * "Rastrear pessoas" na Edição (contratos-fase4.md D11), com os comandos e o evento `pessoas-progress` falsos: o mesmo
 * `pessoas_rastrear` da análise na mídia do clipe, com as opções dela; andamento só da mesma mídia; "Cancelar" pelo
 * `pessoas_cancelar`; fim (bem ou cancelado) avisa o painel; trilhas detectadas pedem confirmação antes; a tarefa
 * sobrevive ao painel sair da tela. Só rótulos de (h) e dados.
 */

import { act } from "react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { MidiaRef } from "@domain/edicao";
import type { PessoasEstado, PessoasProgresso } from "@domain/video_pessoas";
import type { AiSettings } from "@domain/app_settings";

const ev = vi.hoisted(() => ({ ouvintes: new Map<string, (e: { payload: unknown }) => void>(), pedidos: 0 }));
vi.mock("@tauri-apps/api/event", () => ({
  listen: vi.fn(async (nome: string, f: (e: { payload: unknown }) => void) => {
    ev.pedidos++;
    ev.ouvintes.set(nome, f);
    return () => ev.ouvintes.delete(nome);
  }),
}));
const cmd = vi.hoisted(() => ({ pessoasListar: vi.fn(), pessoasRastrear: vi.fn(), pessoasCancelar: vi.fn() }));
vi.mock("@core/commands", () => ({ commands: cmd }));
const ask = vi.hoisted(() => ({ askConfirm: vi.fn(async (_o: { title: string; message?: string; confirmLabel?: string; danger?: boolean }) => true) }));
vi.mock("@components/Dialog/ask", () => ask);

import { useToastStore } from "@components/toast/toastStore";
import { useSettingsStore } from "@stores/settingsStore";
import { usePessoasStore } from "@modules/video/editor/pessoas/pessoasStore";
import { useEdicaoStore } from "../store/edicaoStore";
import { novoProjeto } from "../modelo/projeto";
import { clicar, desmontarTudo, esperar, montar, textosVisiveis, type Tela } from "../inspetor/testeTela";
import { textoPermitido } from "../inspetor/rotulos.teste";
import { esquecerPessoasNaEdicao, EVENTO_PESSOAS, opcoesDaAnalise, PESSOAS_NA_EDICAO_PRONTO, RastrearPessoas, usePessoasNaEdicao } from "./RastrearPessoas";

const WS = "/casos/pessoas";
const SHA = "e".repeat(64);
const OUTRA = "f".repeat(64);
const midia = (o: Partial<MidiaRef> = {}): MidiaRef =>
  ({ id: "m", tipo: "video", origem: "caso", media_id: "v1", sha256: SHA, nome: "cam.mp4", relativo: "cam.mp4", ocultacoes: [], ...o }) as MidiaRef;
const ai = (o: Partial<AiSettings>) => useSettingsStore.setState((s) => ({ settings: { ...s.settings, ai: { ...s.settings.ai, ...o } } }));
const INSTALADO: Partial<AiSettings> = { modelos_desligados: false, ort_lib_path: "/ai/ort/libonnxruntime.so", pessoas_model_path: "/ai/pessoas/yolox_s.onnx" };
const estado = (pessoas: { id: string; origem: "auto" | "manual"; estado: "auto" | "editado" }[] = []): PessoasEstado =>
  ({ run: null, tamanho: null, pessoas: pessoas.map((p, i) => ({ ...p, numero: i + 1, nome: "" })) }) as unknown as PessoasEstado;
const progresso = (p: Partial<PessoasProgresso>) =>
  act(async () => ev.ouvintes.get(EVENTO_PESSOAS)!({ payload: { media_hash: SHA, fase: "rastrear", feitos: 0, total: 0, pessoas: 0, ...p } }));
/** Promessa controlada pelo teste (o comando longo). */
function adiada<T>() {
  let resolver!: (v: T) => void;
  let rejeitar!: (e: unknown) => void;
  const p = new Promise<T>((a, b) => {
    resolver = a;
    rejeitar = b;
  });
  return { p, resolver, rejeitar };
}

let tela: Tela;
const aoTerminar = vi.fn();
const acao = (a: string) => tela.raiz.querySelector<HTMLButtonElement>(`[data-acao="${a}"]`);
const conferirTextos = () => expect(textosVisiveis(tela.raiz).filter((t) => !textoPermitido(t))).toEqual([]);

beforeEach(() => {
  for (const f of Object.values(cmd)) f.mockReset();
  ask.askConfirm.mockReset();
  ask.askConfirm.mockResolvedValue(true);
  aoTerminar.mockReset();
  ev.ouvintes.clear();
  ev.pedidos = 0;
  esquecerPessoasNaEdicao();
  useToastStore.setState({ toasts: [] });
  usePessoasStore.setState({ mediaHash: null, tarefa: null, limiar: 0.5, passo: 1, classes: [0] });
  ai(INSTALADO);
  // o painel só existe com um projeto aberto (os botões pequenos desligam sem documento)
  useEdicaoStore.setState({ doc: novoProjeto("Pessoas"), soLeitura: false });
  cmd.pessoasListar.mockResolvedValue(estado());
  cmd.pessoasCancelar.mockResolvedValue(undefined);
});

afterEach(() => {
  desmontarTudo();
  useEdicaoStore.setState({ doc: null });
});

describe("bandeira e onde aparece", () => {
  it("PESSOAS_NA_EDICAO_PRONTO ligada (entrega do pacote)", () => {
    expect(PESSOAS_NA_EDICAO_PRONTO).toBe(true);
  });

  it("só em vídeo do caso, com a IA ligada; sem o item de Pessoas, desligado com o ícone de baixar", async () => {
    tela = await montar(<RastrearPessoas ws={WS} midia={midia()} aoTerminar={aoTerminar} />);
    expect(acao("rastrear-pessoas")!.disabled).toBe(false);
    expect(acao("rastrear-pessoas")!.textContent?.trim()).toBe("Rastrear pessoas");
    conferirTextos();
    for (const m of [midia({ origem: "projeto" }), midia({ tipo: "imagem" }), midia({ tipo: "audio" })]) {
      desmontarTudo();
      tela = await montar(<RastrearPessoas ws={WS} midia={m} aoTerminar={aoTerminar} />);
      expect(acao("rastrear-pessoas")).toBeNull();
    }
    desmontarTudo();
    ai({ ...INSTALADO, modelos_desligados: true });
    tela = await montar(<RastrearPessoas ws={WS} midia={midia()} aoTerminar={aoTerminar} />);
    expect(tela.raiz.textContent).toBe("");
    desmontarTudo();
    ai({ ...INSTALADO, pessoas_model_path: "" });
    tela = await montar(<RastrearPessoas ws={WS} midia={midia()} aoTerminar={aoTerminar} />);
    expect(acao("rastrear-pessoas")!.disabled).toBe(true);
    expect(acao("rastrear-pessoas")!.dataset.estado).toBe("ausente");
    expect(acao("rastrear-pessoas")!.querySelector("svg.lucide-download")).not.toBeNull();
  });
});

describe("rodar, andamento e fim", () => {
  it("o comando da análise na mídia do clipe, com as opções dela; andamento só da mesma mídia; fim avisa o painel", async () => {
    usePessoasStore.setState({ limiar: 0.35, passo: 2, classes: [0, 2] });
    const d = adiada<PessoasEstado>();
    cmd.pessoasRastrear.mockReturnValue(d.p);
    tela = await montar(<RastrearPessoas ws={WS} midia={midia()} aoTerminar={aoTerminar} />);
    await clicar(acao("rastrear-pessoas")!);
    await esperar();
    expect(cmd.pessoasListar).toHaveBeenCalledWith(WS, SHA);
    expect(ask.askConfirm).not.toHaveBeenCalled();
    expect(cmd.pessoasRastrear).toHaveBeenCalledWith(WS, SHA, { limiar: 0.35, passo: 2, classes: [0, 2] });
    expect(opcoesDaAnalise()).toEqual({ limiar: 0.35, passo: 2, classes: [0, 2] });
    // rodando: Cancelar no lugar do botão; sem total ainda, só o ícone
    expect(acao("rastrear-pessoas")).toBeNull();
    expect(acao("cancelar-pessoas")!.textContent?.trim()).toBe("Cancelar");
    expect(tela.raiz.querySelector("[data-andamento]")).toBeNull();
    // andamento: outra mídia e outra fase não entram
    await progresso({ media_hash: OUTRA, feitos: 50, total: 100 });
    await progresso({ fase: "exportar", feitos: 70, total: 100 });
    expect(tela.raiz.textContent).not.toContain("50/100");
    expect(tela.raiz.textContent).not.toContain("70/100");
    await progresso({ feitos: 120, total: 900, pessoas: 3 });
    expect(tela.raiz.textContent).toContain("120/900");
    expect(Number(tela.raiz.querySelector<HTMLElement>("[data-andamento]")!.dataset.andamento)).toBeCloseTo(120 / 900, 9);
    conferirTextos();
    expect(aoTerminar).not.toHaveBeenCalled();
    await act(async () => d.resolver(estado([{ id: "p1", origem: "auto", estado: "auto" }])));
    await esperar();
    expect(aoTerminar).toHaveBeenCalledTimes(1);
    expect(acao("rastrear-pessoas")!.disabled).toBe(false);
    expect(useToastStore.getState().toasts).toEqual([]);
    // evento atrasado depois do fim não ressuscita nada
    await progresso({ feitos: 900, total: 900 });
    expect(acao("cancelar-pessoas")).toBeNull();
  });

  it("Cancelar: pessoas_cancelar; o comando volta com cancelado (sem aviso) e o painel é avisado", async () => {
    const d = adiada<PessoasEstado>();
    cmd.pessoasRastrear.mockReturnValue(d.p);
    tela = await montar(<RastrearPessoas ws={WS} midia={midia()} aoTerminar={aoTerminar} />);
    await clicar(acao("rastrear-pessoas")!);
    await esperar();
    await progresso({ feitos: 10, total: 900 });
    await clicar(acao("cancelar-pessoas")!);
    await esperar();
    expect(cmd.pessoasCancelar).toHaveBeenCalledTimes(1);
    expect(cmd.pessoasCancelar).toHaveBeenCalledWith();
    await act(async () => d.rejeitar({ kind: "validation", message: "validation error: cancelado" }));
    await esperar();
    expect(useToastStore.getState().toasts).toEqual([]);
    expect(aoTerminar).toHaveBeenCalledTimes(1);
    expect(acao("rastrear-pessoas")!.disabled).toBe(false);
  });

  it("erro do backend vai para o aviso do app; o botão volta", async () => {
    cmd.pessoasRastrear.mockRejectedValue({ kind: "validation", message: "validation error: já existe uma tarefa de pessoas em andamento" });
    tela = await montar(<RastrearPessoas ws={WS} midia={midia()} aoTerminar={aoTerminar} />);
    await clicar(acao("rastrear-pessoas")!);
    await esperar();
    expect(useToastStore.getState().toasts.map((t) => t.message)).toEqual(["já existe uma tarefa de pessoas em andamento"]);
    expect(acao("rastrear-pessoas")!.disabled).toBe(false);
  });

  it("com trilhas detectadas: confirma antes (vermelho se o perito editou alguma); não confirmou, nada roda", async () => {
    cmd.pessoasListar.mockResolvedValue(
      estado([
        { id: "a", origem: "auto", estado: "auto" },
        { id: "b", origem: "auto", estado: "editado" },
        { id: "c", origem: "manual", estado: "auto" },
      ]),
    );
    ask.askConfirm.mockResolvedValueOnce(false);
    cmd.pessoasRastrear.mockResolvedValue(estado());
    tela = await montar(<RastrearPessoas ws={WS} midia={midia()} aoTerminar={aoTerminar} />);
    await clicar(acao("rastrear-pessoas")!);
    await esperar();
    expect(ask.askConfirm).toHaveBeenCalledWith({ title: "Rastrear pessoas", message: "2 pessoas", confirmLabel: "Rastrear pessoas", danger: true });
    expect(cmd.pessoasRastrear).not.toHaveBeenCalled();
    expect(aoTerminar).not.toHaveBeenCalled();
    // só detectadas sem edição: confirma sem vermelho; confirmou, roda
    cmd.pessoasListar.mockResolvedValue(estado([{ id: "a", origem: "auto", estado: "auto" }]));
    await clicar(acao("rastrear-pessoas")!);
    await esperar();
    expect(ask.askConfirm).toHaveBeenLastCalledWith({ title: "Rastrear pessoas", message: "1 pessoa", confirmLabel: "Rastrear pessoas", danger: false });
    expect(cmd.pessoasRastrear).toHaveBeenCalledTimes(1);
    expect(aoTerminar).toHaveBeenCalledTimes(1);
  });

  it("a tarefa vive fora da tela: o painel sai e volta no meio e mostra o andamento; o fim com o painel fora não avisa ninguém", async () => {
    const d = adiada<PessoasEstado>();
    cmd.pessoasRastrear.mockReturnValue(d.p);
    tela = await montar(<RastrearPessoas ws={WS} midia={midia()} aoTerminar={aoTerminar} />);
    await clicar(acao("rastrear-pessoas")!);
    await esperar();
    tela.desmontar();
    await progresso({ feitos: 300, total: 900 });
    tela = await montar(<RastrearPessoas ws={WS} midia={midia()} aoTerminar={aoTerminar} />);
    expect(tela.raiz.textContent).toContain("300/900");
    // outra mídia na tela enquanto roda: botão desligado (uma tarefa por vez)
    const outra = await montar(<RastrearPessoas ws={WS} midia={midia({ id: "m2", sha256: OUTRA })} aoTerminar={vi.fn()} />);
    expect(outra.raiz.querySelector<HTMLButtonElement>('[data-acao="rastrear-pessoas"]')!.disabled).toBe(true);
    tela.desmontar();
    await act(async () => d.resolver(estado()));
    await esperar();
    expect(aoTerminar).not.toHaveBeenCalled();
    expect(usePessoasNaEdicao.getState().tarefa).toBeNull();
    expect(outra.raiz.querySelector<HTMLButtonElement>('[data-acao="rastrear-pessoas"]')!.disabled).toBe(false);
    // o evento é ouvido uma vez por sessão
    expect(ev.pedidos).toBe(1);
  });

  it("tarefa da própria análise em andamento: desligado; a aba Pessoas com a mesma mídia recarrega no fim", async () => {
    usePessoasStore.setState({ tarefa: { fase: "rastrear", feitos: 0, total: 0, pessoas: 0 } });
    tela = await montar(<RastrearPessoas ws={WS} midia={midia()} aoTerminar={aoTerminar} />);
    expect(acao("rastrear-pessoas")!.disabled).toBe(true);
    await act(async () => usePessoasStore.setState({ tarefa: null, mediaHash: SHA }));
    expect(acao("rastrear-pessoas")!.disabled).toBe(false);
    const original = usePessoasStore.getState().carregar;
    const carregar = vi.fn(async () => {});
    usePessoasStore.setState({ carregar });
    try {
      cmd.pessoasRastrear.mockResolvedValue(estado());
      await clicar(acao("rastrear-pessoas")!);
      await esperar();
      expect(carregar).toHaveBeenCalledWith(WS, SHA);
      // outra mídia na análise: não recarrega
      carregar.mockClear();
      usePessoasStore.setState({ mediaHash: OUTRA });
      await clicar(acao("rastrear-pessoas")!);
      await esperar();
      expect(cmd.pessoasRastrear).toHaveBeenCalledTimes(2);
      expect(carregar).not.toHaveBeenCalled();
    } finally {
      usePessoasStore.setState({ carregar: original });
    }
  });
});
