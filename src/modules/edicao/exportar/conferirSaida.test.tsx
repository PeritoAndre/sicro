// @vitest-environment happy-dom
/**
 * "Conferir ocultações" (P2.6) montado de verdade com comandos falsos: nada pedido ao montar; ao abrir, toca o
 * ARQUIVO exportado e lista os trechos do retrato (fracos primeiro, ícone do fraco, inclusive a ausência aceita, tempo
 * como dado); "Próximo trecho
 * fraco", "Quadro anterior" e "Próximo quadro" movem o vídeo; termina em "Conferido" ou "Concluir sem conferir";
 * só rótulos do §8.6.
 */

import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { SaidaRegistrada, TrechoConferencia } from "@domain/edicao";

const cmd = vi.hoisted(() => ({
  edicaoSaidas: vi.fn(),
  edicaoConferenciaTrechos: vi.fn(),
  edicaoConferir: vi.fn(),
}));
vi.mock("@core/commands", () => ({ commands: cmd }));
vi.mock("@tauri-apps/api/core", () => ({ convertFileSrc: (p: string) => `asset://${p}`, invoke: vi.fn() }));
const ask = vi.hoisted(() => ({ askConfirm: vi.fn(async (_o: { title: string; confirmLabel?: string }) => true) }));
vi.mock("@components/Dialog/ask", () => ask);

import { botao, clicar, desmontarTudo, esperar, montar, textosVisiveis } from "../inspetor/testeTela";
import { textoPermitido } from "../inspetor/rotulos.teste";
import { CONFERIR_PRONTO, ConferirSaida, passoDe, proximoFraco, tabelaDaSaida } from "./ConferirSaida";

const WS = "/casos/c1";

function saida(id: string, x: Partial<SaidaRegistrada> = {}): SaidaRegistrada {
  return {
    id, projeto_id: "p1", titulo: "Ocultado", pasta: "Exportações/Vídeos/Edições/Ocultado", arquivo: "Exportações/Vídeos/Edições/Ocultado/Ocultado (ocultado).mp4",
    sha256: "a".repeat(64), composto_sha256: "b".repeat(64), decodificado_sha256: null, audio_sha256: null, tamanho: 1000, duracao_s: 6, n_quadros: 180,
    predefinicao: "juri", codificador: "libx264", tecnico_rel: null, retrato_rel: "Sistema/videos/edicao/p1/exportacoes/s1", projeto_sha256: "c".repeat(64),
    autor: { nome: "Perito", matricula: "", cargo: "" }, refeita_de: null, estado: "a_conferir", exige_conferencia: true, conferida_por: null, conferida_em: null,
    criado_em: "2026-11-15T10:00:00Z", comparacao: null, ...x,
  } as SaidaRegistrada;
}

/** Como o Rust devolve: com fracos primeiro, depois por tempo. */
const TRECHOS: TrechoConferencia[] = [
  { efeito: "o1", conteudo: "placa", de_s: 1 / 3, ate_s: 71 / 30, fracos: [{ de_s: 31 / 30, ate_s: 50 / 30, como: "interpolado" }] },
  { efeito: "o1", conteudo: "placa", de_s: 3, ate_s: 182 / 30, fracos: [{ de_s: 112 / 30, ate_s: 5, como: "incerto" }, { de_s: 170 / 30, ate_s: 182 / 30, como: "interpolado" }] },
  { efeito: "o2", conteudo: "rosto", de_s: 80 / 30, ate_s: 3, fracos: [] },
];

const corpo = () => document.body;
const video = () => corpo().querySelector<HTMLVideoElement>("[data-conferencia] video");
const tempo = () => corpo().querySelector("[data-tempo]")?.textContent;
const semFrase = () => {
  expect(textosVisiveis(corpo()).filter((t) => !textoPermitido(t))).toEqual([]);
  expect(corpo().querySelectorAll("[placeholder]").length).toBe(0);
};

beforeEach(() => {
  for (const f of Object.values(cmd)) f.mockReset();
  ask.askConfirm.mockReset();
  ask.askConfirm.mockResolvedValue(true);
  cmd.edicaoSaidas.mockResolvedValue([saida("s0"), saida("s1")]);
  cmd.edicaoConferenciaTrechos.mockResolvedValue(TRECHOS);
  cmd.edicaoConferir.mockImplementation(async () => {
    cmd.edicaoSaidas.mockResolvedValue([saida("s1", { estado: "concluida" })]);
    return saida("s1", { estado: "concluida" });
  });
});

afterEach(() => desmontarTudo());

async function abrir() {
  await montar(<ConferirSaida ws={WS} saidaId="s1" />);
  await clicar(botao(corpo(), "Conferir ocultações"));
  await esperar(() => new Promise((r) => setTimeout(r, 0)));
}

describe("Conferir ocultações", () => {
  it("ligado; montado não pede nada: o comando sai só quando o diálogo abre", async () => {
    expect(CONFERIR_PRONTO).toBe(true);
    await montar(<ConferirSaida ws={WS} saidaId="s1" />);
    expect(cmd.edicaoConferenciaTrechos).not.toHaveBeenCalled();
    expect(cmd.edicaoSaidas).not.toHaveBeenCalled();
    expect(textosVisiveis(corpo())).toEqual(["Conferir ocultações"]);
    await clicar(botao(corpo(), "Conferir ocultações"));
    await esperar(() => new Promise((r) => setTimeout(r, 0)));
    expect(cmd.edicaoConferenciaTrechos).toHaveBeenCalledWith(WS, "s1");
  });

  it("toca o arquivo exportado e lista os trechos na ordem do retrato, com o ícone do fraco", async () => {
    await abrir();
    expect(video()!.getAttribute("src")).toBe(`file://${WS}/Exportações/Vídeos/Edições/Ocultado/Ocultado (ocultado).mp4`.split("/").map((p, i) => (i < 2 ? p : encodeURIComponent(p))).join("/"));
    const linhas = [...corpo().querySelectorAll<HTMLElement>("[data-trechos] > li")];
    expect(linhas.map((l) => [l.dataset.trecho, l.dataset.fraco ?? null])).toEqual([["o1", "true"], ["o1", "true"], ["o2", null]]);
    expect(linhas.map((l) => l.querySelector("[data-dado]")!.textContent)).toEqual(["00:00:00,333 – 00:00:02,367", "00:00:03,000 – 00:00:06,067", "00:00:02,667 – 00:00:03,000"]);
    expect([...linhas[1]!.querySelectorAll("[data-faixa-fraca]")].map((b) => (b as HTMLElement).dataset.faixaFraca)).toEqual(["incerto", "interpolado"]);
    expect(linhas[0]!.querySelector('[data-fraco="interpolado"]')).toBeTruthy();
    expect(linhas[1]!.querySelector('[data-fraco="incerto"]')).toBeTruthy();
    semFrase();
  });

  it("Próximo trecho fraco, Quadro anterior e Próximo quadro movem o vídeo, no meio do quadro", async () => {
    await abrir();
    const v = video()!;
    const passo = 6 / 180;
    await clicar(botao(corpo(), "Próximo trecho fraco"));
    expect(v.currentTime).toBeCloseTo(31 / 30 + passo / 2, 9);
    expect(tempo()).toBe("00:00:01,033");
    await clicar(botao(corpo(), "Próximo trecho fraco"));
    expect(v.currentTime).toBeCloseTo(112 / 30 + passo / 2, 9);
    await clicar(corpo().querySelector<HTMLButtonElement>('[aria-label="Próximo quadro"]')!);
    expect(v.currentTime).toBeCloseTo(113 / 30 + passo / 2, 9);
    await clicar(corpo().querySelector<HTMLButtonElement>('[aria-label="Quadro anterior"]')!);
    await clicar(corpo().querySelector<HTMLButtonElement>('[aria-label="Quadro anterior"]')!);
    expect(v.currentTime).toBeCloseTo(111 / 30 + passo / 2, 9);
    // a linha do trecho leva ao primeiro fraco dele; o sem fraco, ao começo
    await clicar(corpo().querySelectorAll<HTMLButtonElement>("[data-trechos] > li > button:first-child")[2]!);
    expect(v.currentTime).toBeCloseTo(80 / 30 + passo / 2, 9);
    // depois do último fraco volta ao primeiro
    await clicar([...corpo().querySelectorAll<HTMLButtonElement>("[data-faixa-fraca]")].at(-1)!);
    expect(v.currentTime).toBeCloseTo(170 / 30 + passo / 2, 9);
    await clicar(botao(corpo(), "Próximo trecho fraco"));
    expect(v.currentTime).toBeCloseTo(31 / 30 + passo / 2, 9);
    expect(v.paused).toBe(true);
  });

  it("Concluir sem conferir: edicao_conferir sem a conferência e fecha", async () => {
    await abrir();
    await clicar(botao(corpo(), "Concluir sem conferir"));
    await esperar(() => new Promise((r) => setTimeout(r, 0)));
    expect(cmd.edicaoConferir).toHaveBeenCalledWith(WS, "s1", false);
    expect(ask.askConfirm).not.toHaveBeenCalled();
    expect(corpo().querySelector("[data-conferencia]")).toBeNull();
  });

  it("Conferido pede confirmação; recusada, nada é gravado e o diálogo fica", async () => {
    await abrir();
    ask.askConfirm.mockResolvedValueOnce(false);
    await clicar(botao(corpo(), "Conferido"));
    await esperar(() => new Promise((r) => setTimeout(r, 0)));
    expect(ask.askConfirm).toHaveBeenCalledWith(expect.objectContaining({ title: "Conferido" }));
    expect(cmd.edicaoConferir).not.toHaveBeenCalled();
    expect(corpo().querySelector("[data-conferencia]")).toBeTruthy();
    await clicar(botao(corpo(), "Conferido"));
    await esperar(() => new Promise((r) => setTimeout(r, 0)));
    expect(cmd.edicaoConferir).toHaveBeenCalledWith(WS, "s1", true);
    expect(corpo().querySelector("[data-conferencia]")).toBeNull();
  });

  it("erro do comando: a frase do catálogo, sem vídeo; Fechar fecha", async () => {
    cmd.edicaoConferenciaTrechos.mockRejectedValue({ kind: "validation", message: "validation error: midia_trocada: Arquivo diferente do registrado" });
    await abrir();
    expect(corpo().querySelector("[data-erro]")!.textContent).toContain("Arquivo diferente do registrado");
    expect(video()).toBeNull();
    expect((botao(corpo(), "Conferido") as HTMLButtonElement).disabled).toBe(true);
    semFrase();
    await clicar(botao(corpo(), "Fechar"));
    expect(corpo().querySelector("[data-conferencia]")).toBeNull();
  });

  it("ausência aceita: faixa fraca com o ícone dela, e o Próximo trecho fraco leva até lá", async () => {
    const comAusencia: TrechoConferencia[] = [
      { efeito: "o1", conteudo: "rosto", de_s: 1 / 3, ate_s: 2, fracos: [{ de_s: 1, ate_s: 4 / 3, como: "incerto" }, { de_s: 4 / 3, ate_s: 5 / 3, como: "ausencia" }] },
    ];
    cmd.edicaoConferenciaTrechos.mockResolvedValue(comAusencia);
    await abrir();
    const linha = corpo().querySelector<HTMLElement>("[data-trechos] > li")!;
    expect([...linha.querySelectorAll<HTMLElement>("[data-faixa-fraca]")].map((b) => b.dataset.faixaFraca)).toEqual(["incerto", "ausencia"]);
    expect(linha.querySelector('[data-faixa-fraca="ausencia"] [data-fraco="ausencia"]')).toBeTruthy();
    const passo = 6 / 180;
    await clicar(botao(corpo(), "Próximo trecho fraco"));
    await clicar(botao(corpo(), "Próximo trecho fraco"));
    expect(video()!.currentTime).toBeCloseTo(4 / 3 + passo / 2, 9);
    semFrase();
  });

  it("proximoFraco e passoDe", () => {
    expect(proximoFraco(TRECHOS, 0, 1 / 30)).toBeCloseTo(31 / 30, 12);
    expect(proximoFraco(TRECHOS, 31 / 30 + 1 / 60, 1 / 30)).toBeCloseTo(112 / 30, 12);
    expect(proximoFraco(TRECHOS, 6, 1 / 30)).toBeCloseTo(31 / 30, 12);
    expect(proximoFraco([TRECHOS[2]!], 0, 1 / 30)).toBeNull();
    expect(passoDe({ duracao_s: 6, n_quadros: 180 })).toBeCloseTo(1 / 30, 12);
    expect(passoDe(null)).toBeCloseTo(1 / 30, 12);
  });

  it("tabelaDaSaida: passo exato em 29,97, 23,976 e 25 (1 h sem deriva)", () => {
    for (const [num, den] of [[30000, 1001], [24000, 1001], [25, 1]] as const) {
      const n = Math.round((3600 * num) / den);
      const t = tabelaDaSaida({ duracao_s: (n * den) / num, n_quadros: n });
      expect(t.passo_mediano * num).toBe(t.tb.den * den);
      expect(t.passos.every((p) => p === t.passo_mediano)).toBe(true);
    }
  });
});
