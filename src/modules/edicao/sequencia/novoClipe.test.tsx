// @vitest-environment happy-dom
/** "Trecho…" montado de verdade: Início e Fim digitados não somem quando a MidiaInfo chega depois; "Evento" leva o
 *  tempo de fonte do evento (contratos (h) 27). */

import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { MidiaInfo, TabelaQuadros } from "@domain/edicao";
import type { VideoEvent, VideoMedia } from "@domain/video";

const cmd = vi.hoisted(() => ({ listVideoMedia: vi.fn(), openVideoMedia: vi.fn(), edicaoMidiaInfo: vi.fn(), edicaoTabelaQuadros: vi.fn() }));
vi.mock("@core/commands", () => ({ commands: cmd }));
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn(async () => () => {}) }));

import { NovoClipe, type TrechoNovo } from "./NovoClipe";
import { tempoDoEvento } from "./novos";
import { botao, clicar, desmontarTudo, digitar, entradaDe, escolher, esperar, montar, tecla } from "../inspetor/testeTela";

const SHA = "a".repeat(64);
const video = { id: "v1", sha256: SHA, filename: "cam1.mp4", duration_s: 60 } as unknown as VideoMedia;
const ev = (id: string, ts: number, x: Partial<VideoEvent> = {}): VideoEvent =>
  ({ id, occurrence_id: "o", media_hash: SHA, timestamp_s: ts, timestamp_label: "", frame_observed: null, pts: null, time_base: null, category: "outro", title: id, description: "", reviewed: false, source: "manual", created_at: "", updated_at: "", ...x }) as VideoEvent;
const info: MidiaInfo = {
  id: "v1", sha256: SHA, nome: "cam1.mp4", exibido: { w: 1280, h: 720 }, taxa: { num: 30, den: 1 }, vfr: false, passo: { min: 1 / 30, max: 1 / 30 },
  de_s: 1, ate_s: 41, n: 1200, tem_audio: false, previa: { relativo: "x", tipo: "original", desloc_s: 0, de_s: 1, ate_s: 41, largura: 1280, altura: 720, pronta: true },
  tabela: "decodificador", erros: 0, marcavel: true, cor: { matriz: "bt709", faixa: "tv" },
};
const tabela: TabelaQuadros = { tb: { num: 1, den: 30 }, pts0: 30, passos: Array(1199).fill(1), passo_mediano: 1, chaves: [0], vfr: false, origem: "decodificador" };

let soltar: () => void;
beforeEach(() => {
  for (const f of Object.values(cmd)) f.mockReset();
  cmd.listVideoMedia.mockResolvedValue([video]);
  // o evento de 12 s gravado com pts na base 1/1000: o tempo de fonte é o do pts (12,5)
  cmd.openVideoMedia.mockResolvedValue({ events: [ev("e2", 20), ev("e1", 12, { pts: 12500, time_base: "1/1000" })] });
  // a MidiaInfo e a tabela chegam só quando o teste solta
  const espera = new Promise<void>((r) => (soltar = r));
  cmd.edicaoMidiaInfo.mockImplementation(async () => (await espera, info));
  cmd.edicaoTabelaQuadros.mockImplementation(async () => (await espera, tabela));
});
afterEach(() => desmontarTudo());

describe("Trecho…", () => {
  it("o que o perito digitou fica quando a MidiaInfo chega; Evento usa o tempo de fonte", async () => {
    const recebidos: TrechoNovo[][] = [];
    const t = await montar(<NovoClipe qual="trecho" ws="/c" onFechar={() => {}} onTrechos={(x) => recebidos.push(x)} onTitulo={() => {}} />);
    await esperar(() => new Promise((r) => setTimeout(r, 0)));
    const raiz = document.body;
    await escolher(raiz.querySelector("select")!, "v1");
    await esperar(() => new Promise((r) => setTimeout(r, 0)));
    const ini = entradaDe(raiz, "Início") as HTMLInputElement;
    const fim = entradaDe(raiz, "Fim") as HTMLInputElement;
    await digitar(ini, "5", { aos: 1 });
    await tecla(ini, "Enter");
    await digitar(fim, "9", { aos: 1 });
    await tecla(fim, "Enter");
    expect([ini.value, fim.value]).toEqual(["00:00:05,000", "00:00:09,000"]);
    // chega a faixa da mídia (1 a 41 s): o digitado continua
    await esperar(async () => {
      soltar();
      await new Promise((r) => setTimeout(r, 0));
    });
    expect([ini.value, fim.value]).toEqual(["00:00:05,000", "00:00:09,000"]);
    // evento com pts: Início vai para 12,5 s (o de fonte), não para o timestamp_s
    const sel = [...raiz.querySelectorAll("select")][1] as HTMLSelectElement;
    expect([...sel.options].map((o) => o.value)).toEqual(["", "e1", "e2"]);
    await escolher(sel, "e1");
    expect(ini.value).toBe("00:00:12,500");
    await clicar(botao(raiz, "Adicionar"));
    expect(recebidos[0]![0]).toMatchObject({ inicio_s: 12.5, inicio_pts: 375 });
    t.desmontar();
  });

  it("mídia não marcável (h 17): Início, Fim e Evento desligados, nenhum pts da tabela de pacotes; Dos eventos não adiciona", async () => {
    const recebidos: TrechoNovo[][] = [];
    cmd.edicaoMidiaInfo.mockResolvedValue({ ...info, tabela: "pacotes", marcavel: false });
    cmd.edicaoTabelaQuadros.mockResolvedValue({ ...tabela, origem: "pacotes" });
    const t = await montar(<NovoClipe qual="trecho" ws="/c2" onFechar={() => {}} onTrechos={(x) => recebidos.push(x)} onTitulo={() => {}} />);
    await esperar(() => new Promise((r) => setTimeout(r, 0)));
    const raiz = document.body;
    await escolher(raiz.querySelector("select")!, "v1");
    await esperar(() => new Promise((r) => setTimeout(r, 0)));
    await esperar(() => new Promise((r) => setTimeout(r, 0)));
    expect((entradaDe(raiz, "Início") as HTMLInputElement).disabled).toBe(true);
    expect((entradaDe(raiz, "Fim") as HTMLInputElement).disabled).toBe(true);
    expect(([...raiz.querySelectorAll("select")][1] as HTMLSelectElement).disabled).toBe(true);
    // a mídia inteira, presa depois pelo Rust (pts nulos)
    await clicar(botao(raiz, "Adicionar"));
    expect(recebidos[0]).toEqual([expect.objectContaining({ inicio_s: 1, inicio_pts: null, fim_s: 41, fim_pts: null })]);
    t.desmontar();
    const e = await montar(<NovoClipe qual="eventos" ws="/c2" onFechar={() => {}} onTrechos={(x) => recebidos.push(x)} onTitulo={() => {}} />);
    await esperar(() => new Promise((r) => setTimeout(r, 0)));
    await escolher(document.body.querySelector("select")!, "v1");
    await esperar(() => new Promise((r) => setTimeout(r, 0)));
    const caixas = [...document.body.querySelectorAll<HTMLInputElement>('input[type="checkbox"]')];
    expect(caixas).toHaveLength(2);
    expect(caixas.every((c) => c.disabled)).toBe(true);
    expect(botao(document.body, "Adicionar").disabled).toBe(true);
    e.desmontar();
  });

  it("tempoDoEvento: pts × time_base quando há; senão timestamp_s", () => {
    expect(tempoDoEvento({ timestamp_s: 3, pts: 9000, time_base: "1/90000" })).toBe(0.1);
    expect(tempoDoEvento({ timestamp_s: 3, pts: null, time_base: "1/90000" })).toBe(3);
    expect(tempoDoEvento({ timestamp_s: 3, pts: 10, time_base: null })).toBe(3);
    expect(tempoDoEvento({ timestamp_s: 3, pts: 10, time_base: "x" })).toBe(3);
  });
});
