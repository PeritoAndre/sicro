// @vitest-environment happy-dom
/** "Trecho…" montado de verdade: Início e Fim digitados não somem quando a MidiaInfo chega depois; "Evento" leva o
 *  tempo de fonte do evento (contratos (h) 27). */

import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { MidiaInfo, TabelaQuadros, Titulo } from "@domain/edicao";
import type { VideoEvent, VideoMedia } from "@domain/video";

const cmd = vi.hoisted(() => ({ listVideoMedia: vi.fn(), openVideoMedia: vi.fn(), edicaoMidiaInfo: vi.fn(), edicaoTabelaQuadros: vi.fn(), videoProveniencia: vi.fn() }));
vi.mock("@core/commands", () => ({ commands: cmd }));
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn(async () => () => {}) }));

import { NovoClipe, type TrechoNovo } from "./NovoClipe";
import { tempoDoEvento, tituloDaFase0 } from "./novos";
import { useEdicaoStore } from "../store/edicaoStore";
import { novoProjeto } from "../modelo/projeto";
import { arranjo, escalaDoCanvas } from "../modelo/titulos";
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
  // proveniência da análise: nada gravado (a coleta e o "Trecho…" leem sempre)
  cmd.videoProveniencia.mockResolvedValue({ recebido_nativo: null, recebido_obs: "", relogio_desvio_s: null, gravador: null });
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

  // verificação 6.4: no app, "Adicionar" clicado logo depois de escolher um vídeo que começa depois de 0 (gravador)
  // gravava o Fim = duração em tempo de fonte (perdia o fim da mídia)
  it("sem a faixa da mídia, 'Adicionar' espera; chegando, o trecho é a mídia inteira (1 a 41 s)", async () => {
    const recebidos: TrechoNovo[][] = [];
    const t = await montar(<NovoClipe qual="trecho" ws="/c4" onFechar={() => {}} onTrechos={(x) => recebidos.push(x)} onTitulo={() => {}} />);
    await esperar(() => new Promise((r) => setTimeout(r, 0)));
    const raiz = document.body;
    await escolher(raiz.querySelector("select")!, "v1");
    await esperar(() => new Promise((r) => setTimeout(r, 0)));
    expect(botao(raiz, "Adicionar").disabled).toBe(true);
    await clicar(botao(raiz, "Adicionar"));
    expect(recebidos).toEqual([]);
    await esperar(async () => {
      soltar();
      await new Promise((r) => setTimeout(r, 0));
    });
    expect(botao(raiz, "Adicionar").disabled).toBe(false);
    await clicar(botao(raiz, "Adicionar"));
    expect(recebidos[0]).toEqual([expect.objectContaining({ inicio_s: 1, inicio_pts: 30, fim_s: 41, fim_pts: null })]);
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

  it("erro de leitura da proveniência: 'Trecho…' e 'Dos eventos…' mostram a frase no diálogo e não adicionam; lida, vai no MidiaRef", async () => {
    soltar();
    const recebidos: TrechoNovo[][] = [];
    const erroDe = () => document.body.querySelector("[data-erro]")?.textContent ?? null;
    const falha = { kind: "validation", message: "validation error: midia_ausente: Mídia ausente" };
    const pausa = () => esperar(() => new Promise((r) => setTimeout(r, 0)));
    cmd.videoProveniencia.mockRejectedValue(falha);
    const t = await montar(<NovoClipe qual="trecho" ws="/c3" onFechar={() => {}} onTrechos={(x) => recebidos.push(x)} onTitulo={() => {}} />);
    await pausa();
    await escolher(document.body.querySelector("select")!, "v1");
    await pausa();
    expect(erroDe()).toBeNull();
    await clicar(botao(document.body, "Adicionar"));
    await pausa();
    expect(recebidos).toEqual([]);
    expect(erroDe()).toBe("Mídia ausente");
    // a leitura volta: o erro some e o trecho leva a proveniência
    cmd.videoProveniencia.mockResolvedValue({ recebido_nativo: false, recebido_obs: "cópia", relogio_desvio_s: 12, gravador: "DVR" });
    await clicar(botao(document.body, "Adicionar"));
    await pausa();
    expect(erroDe()).toBeNull();
    expect(recebidos[0]![0]!.midia).toMatchObject({ recebido: { nativo: false, obs: "cópia" }, relogio_desvio_s: 12, gravador: "DVR" });
    t.desmontar();
    // Dos eventos: erro fora do catálogo aparece com a mensagem crua
    cmd.videoProveniencia.mockRejectedValue({ kind: "database", message: "database error: database is locked" });
    const e = await montar(<NovoClipe qual="eventos" ws="/c3" onFechar={() => {}} onTrechos={(x) => recebidos.push(x)} onTitulo={() => {}} />);
    await pausa();
    await escolher(document.body.querySelector("select")!, "v1");
    await pausa();
    for (const c of document.body.querySelectorAll<HTMLInputElement>('input[type="checkbox"]')) await clicar(c);
    await clicar(botao(document.body, "Adicionar"));
    await pausa();
    expect(recebidos).toHaveLength(1);
    expect(erroDe()).toBe("database is locked");
    e.desmontar();
  });

  it("Título…: cria no modelo escolhido (P1.7), para o canvas da saída do projeto aberto", async () => {
    const canvas = { w: 1080, h: 1920 };
    useEdicaoStore.setState({ doc: { ...novoProjeto(), saida: { ...novoProjeto().saida, largura: canvas.w, altura: canvas.h } } });
    const recebidos: Titulo[] = [];
    const t = await montar(<NovoClipe qual="titulo" ws="/c" onFechar={() => {}} onTrechos={() => {}} onTitulo={(x) => recebidos.push(x)} />);
    const raiz = document.body;
    await escolher(raiz.querySelector("select")!, "encerramento");
    const [campoTitulo, campoSub] = [...raiz.querySelectorAll<HTMLInputElement>("input")];
    await digitar(campoTitulo!, "Fim");
    await digitar(campoSub!, "Laudo 77");
    await clicar(botao(raiz, "Adicionar"));
    const feito = recebidos[0]!;
    expect(feito.modelo).toBe("encerramento");
    expect(feito.blocos.map((b) => [b.papel, b.conteudo, b.lugar])).toEqual([
      ["titulo", "Fim", arranjo("encerramento", "titulo").lugar],
      ["subtitulo", "Laudo 77", arranjo("encerramento", "subtitulo").lugar],
    ]);
    // vertical: o texto do modelo encolhe com a largura
    expect(feito.blocos[0]!.estilo.tamanho).toBe(arranjo("encerramento", "titulo").tamanho * escalaDoCanvas(canvas));
    expect(escalaDoCanvas(canvas)).toBeLessThan(1);
    // o arranjo da Fase 0 para o mesmo pedido era outro (cartão centrado, igual à cartela)
    expect(feito.blocos[0]!.lugar).not.toEqual(tituloDaFase0("encerramento", "Fim", "Laudo 77").blocos[0]!.lugar);
    useEdicaoStore.setState({ doc: null });
    t.desmontar();
  });

  it("tempoDoEvento: pts × time_base quando há; senão timestamp_s", () => {
    expect(tempoDoEvento({ timestamp_s: 3, pts: 9000, time_base: "1/90000" })).toBe(0.1);
    expect(tempoDoEvento({ timestamp_s: 3, pts: null, time_base: "1/90000" })).toBe(3);
    expect(tempoDoEvento({ timestamp_s: 3, pts: 10, time_base: null })).toBe(3);
    expect(tempoDoEvento({ timestamp_s: 3, pts: 10, time_base: "x" })).toBe(3);
  });
});
