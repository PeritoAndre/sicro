// @vitest-environment happy-dom
/**
 * P4.4 Fila visível: várias exportações na ordem pedida, cada uma com predefinição, estado e progresso; cancelar uma
 * não mexe nas outras; fim avisa com "Abrir pasta"; reaproveitada por ícone; app reaberto = fila vazia e saídas listadas.
 */

import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { ProjetoAberto, ProjetoEdicao, SaidaRegistrada, Tarefa } from "@domain/edicao";

const cmd = vi.hoisted(() => ({
  edicaoAbrir: vi.fn(),
  edicaoSalvar: vi.fn(),
  edicaoAnalisar: vi.fn(),
  edicaoExcluir: vi.fn(),
  edicaoEnfileirar: vi.fn(),
  edicaoFila: vi.fn(),
  edicaoCancelar: vi.fn(),
  edicaoConferir: vi.fn(),
  edicaoSaidas: vi.fn(),
  edicaoSaidaExcluir: vi.fn(),
  edicaoRefazer: vi.fn(),
  revealPathInExplorer: vi.fn(),
}));
vi.mock("@core/commands", () => ({ commands: cmd }));
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn(async () => () => {}) }));

import { useEdicaoStore } from "../store/edicaoStore";
import { useRenderStore, wsDaTarefa } from "../store/renderStore";
import { botao, clicar, desmontarTudo, esperar, lerFixture, montar, textosVisiveis } from "../inspetor/testeTela";
import { textoPermitido } from "../inspetor/rotulos.teste";
import { useExportarUi } from "./avisos";
import { Fila } from "./Fila";
import { Toaster } from "@components/toast/Toaster";
import { useToastStore } from "@components/toast/toastStore";

const WS = "/casos/c1";
const st = () => useEdicaoStore.getState();
const corpo = () => document.body;
const toasts = () => useToastStore.getState().toasts;
const tick = () => esperar(() => new Promise((r) => setTimeout(r, 0)));

const tarefa = (id: string, estado: Tarefa["estado"], x: Partial<Tarefa> = {}): Tarefa => ({
  id,
  tipo: "exportar",
  caso: "c1",
  projeto_id: "p",
  titulo: `Edição ${id}`,
  estado,
  criado_em: "2026-11-17T10:00:00Z",
  predefinicao: "juri",
  reaproveitada: false,
  ...x,
});
const rodando = (feito: number, total: number, blocos: [number, number] = [1, 2]): Tarefa["estado"] => ({
  tipo: "rodando",
  fase: { tipo: "blocos", feitos: blocos[0], total: blocos[1] },
  feito_q: feito,
  total_q: total,
  qps: 60,
  restante_s: 3,
});
const saida = (id: string, x: Partial<SaidaRegistrada> = {}): SaidaRegistrada => ({
  id,
  projeto_id: "p",
  titulo: "Edição",
  pasta: `Exportações/Vídeos/Edições/${id}`,
  arquivo: `Exportações/Vídeos/Edições/${id}/edicao.mp4`,
  sha256: "a".repeat(64),
  composto_sha256: "b".repeat(64),
  decodificado_sha256: null,
  audio_sha256: null,
  tamanho: 1,
  duracao_s: 12.5,
  n_quadros: 375,
  predefinicao: "juri",
  codificador: "libx264",
  tecnico_rel: null,
  retrato_rel: "",
  projeto_sha256: "",
  autor: { nome: "", matricula: "", cargo: "" },
  refeita_de: null,
  estado: "concluida",
  exige_conferencia: false,
  conferida_por: null,
  conferida_em: null,
  criado_em: "2026-11-17T10:00:00Z",
  comparacao: null,
  ...x,
});

const linhas = () => [...corpo().querySelectorAll<HTMLElement>("[data-tarefas] > li")];
const ids = () => linhas().map((l) => l.dataset.tarefa);
const rotulos = () => linhas().map((l) => l.querySelector("[data-rotulo]")?.textContent);
const predefinicoes = () => linhas().map((l) => l.querySelector("[data-predefinicao]")?.textContent ?? null);
const linha = (id: string) => corpo().querySelector<HTMLElement>(`[data-tarefa="${id}"]`)!;
const barra = (id: string) => linha(id).querySelector<HTMLElement>("[style]")?.style.width ?? null;
const receber = (ts: Tarefa[]) => esperar(() => useRenderStore.getState().receberFila(ts));
const abrirPainel = async () => {
  await clicar(corpo().querySelector<HTMLButtonElement>('[data-acao="fila"]')!);
  await tick();
};
const semFrase = () => {
  expect(textosVisiveis(corpo()).filter((t) => !textoPermitido(t))).toEqual([]);
  expect(corpo().querySelectorAll("[placeholder]").length).toBe(0);
};

beforeEach(async () => {
  for (const f of Object.values(cmd)) f.mockReset();
  cmd.edicaoSalvar.mockImplementation(async () => ({ updated_at: new Date().toISOString() }));
  cmd.edicaoAnalisar.mockImplementation(() => new Promise(() => {}));
  cmd.edicaoFila.mockResolvedValue([]);
  cmd.edicaoSaidas.mockResolvedValue([]);
  cmd.edicaoCancelar.mockResolvedValue(undefined);
  cmd.revealPathInExplorer.mockResolvedValue(undefined);
  const projeto = lerFixture<ProjetoEdicao>("projeto-do-store.json");
  const r: ProjetoAberto = { projeto, importado_de: null, so_leitura: false, avisos: [], raiz_cache: "/cache/c1" };
  cmd.edicaoAbrir.mockResolvedValueOnce(r);
  await st().abrir(WS, projeto.id);
  useExportarUi.setState({ dialogo: false, fila: false });
  useToastStore.getState().clear();
  wsDaTarefa.clear();
});

afterEach(async () => {
  desmontarTudo();
  useRenderStore.setState({ tarefas: [], progresso: {} });
  await st()
    .fechar({ descartar: true })
    .catch(() => {});
});

describe("Fila visível (P4.4)", () => {
  // primeiro do arquivo: é aqui que a tela lê a fila pela primeira vez, como ao abrir o app
  it("reabrir o app: fila vazia (a interrompida não volta), saídas listadas, nenhum aviso do que já tinha terminado", async () => {
    cmd.edicaoSaidas.mockResolvedValue([saida("s1"), saida("s2", { predefinicao: "whatsapp" })]);
    await montar(<Fila />);
    await abrirPainel();
    expect(cmd.edicaoFila).toHaveBeenCalledTimes(1);
    expect(useRenderStore.getState().tarefas).toEqual([]);
    expect(corpo().querySelector("[data-tarefas]")).toBeNull();
    expect([...corpo().querySelectorAll<HTMLElement>("[data-saidas] > li")].map((l) => l.querySelector("[title]")?.getAttribute("title"))).toEqual([
      "Exportações/Vídeos/Edições/s1/edicao.mp4",
      "Exportações/Vídeos/Edições/s2/edicao.mp4",
    ]);
    // nada girando no botão
    expect(corpo().querySelector('[data-acao="fila"]')?.textContent).toBe("Fila");
    semFrase();
    // lista que chega já com fins (tela recarregada com o app aberto): mostra, não avisa
    await receber([tarefa("9", { tipo: "concluida", saida_id: "s9" }), tarefa("8", { tipo: "falhou", mensagem: "Disco cheio", log_rel: "" })]);
    expect(ids()).toEqual(["9", "8"]);
    expect(toasts()).toEqual([]);
  });

  it("Júri, WhatsApp e Preservação (dois projetos): todas na ordem pedida, cada uma com predefinição, estado e o próprio progresso", async () => {
    await montar(<Fila />);
    await receber([
      tarefa("1", rodando(300, 700, [3, 7])),
      tarefa("2", { tipo: "na_fila" }, { predefinicao: "whatsapp" }),
      tarefa("3", { tipo: "na_fila" }, { predefinicao: "preservacao", projeto_id: "q", titulo: "Outra edição" }),
    ]);
    await abrirPainel();
    expect(ids()).toEqual(["1", "2", "3"]);
    expect(predefinicoes()).toEqual(["Júri", "WhatsApp", "Preservação"]);
    expect(rotulos()).toEqual(["Vídeo 3/7", "Na fila", "Na fila"]);
    expect([barra("1"), barra("2"), barra("3")]).toEqual([`${(300 / 700) * 100}%`, null, null]);
    expect(corpo().querySelector('[data-acao="fila"]')?.textContent).toBe("Fila3");
    semFrase();
    // o progresso de uma não mexe nas outras
    await esperar(() => useRenderStore.getState().receberProgresso({ tarefa_id: "1", fase: { tipo: "blocos", feitos: 6, total: 7 }, feito_q: 600, total_q: 700, qps: 90, restante_s: 1 }));
    expect([rotulos(), barra("1"), barra("2")]).toEqual([["Vídeo 6/7", "Na fila", "Na fila"], `${(600 / 700) * 100}%`, null]);
    // a primeira acaba, a segunda começa: a ordem na lista é a pedida, não a de chegada ao fim
    await receber([
      tarefa("1", { tipo: "concluida", saida_id: "s1" }),
      tarefa("2", rodando(10, 40), { predefinicao: "whatsapp" }),
      tarefa("3", { tipo: "na_fila" }, { predefinicao: "preservacao", projeto_id: "q", titulo: "Outra edição" }),
    ]);
    expect([ids(), rotulos()]).toEqual([
      ["1", "2", "3"],
      ["Pronto", "Vídeo 1/2", "Na fila"],
    ]);
    expect([barra("1"), barra("2"), barra("3")]).toEqual([null, "25%", null]);
    expect(corpo().querySelector('[data-acao="fila"]')?.textContent).toBe("Fila2");
  });

  it("cancelar a do meio (na espera) e depois a que roda: só a pedida sai; as outras seguem", async () => {
    await montar(<Fila />);
    const t2 = tarefa("2", { tipo: "na_fila" }, { predefinicao: "whatsapp" });
    const t3 = tarefa("3", { tipo: "na_fila" }, { predefinicao: "preservacao" });
    await receber([tarefa("1", rodando(100, 400)), t2, t3]);
    await abrirPainel();
    const cancelar = (id: string) => linha(id).querySelector<HTMLButtonElement>('[aria-label="Cancelar"]');
    expect(["1", "2", "3"].map((id) => cancelar(id) !== null)).toEqual([true, true, true]);
    await clicar(cancelar("2")!);
    expect(cmd.edicaoCancelar.mock.calls).toEqual([["2"]]);
    await receber([tarefa("1", rodando(200, 400)), { ...t2, estado: { tipo: "cancelada" } }, t3]);
    expect(rotulos()).toEqual(["Vídeo 1/2", "Cancelado", "Na fila"]);
    expect(["1", "2", "3"].map((id) => cancelar(id) !== null)).toEqual([true, false, true]);
    expect(barra("1")).toBe("50%");
    await clicar(cancelar("1")!);
    expect(cmd.edicaoCancelar.mock.calls).toEqual([["2"], ["1"]]);
    await receber([tarefa("1", { tipo: "cancelada" }), { ...t2, estado: { tipo: "cancelada" } }, { ...t3, estado: rodando(5, 50) }]);
    expect([ids(), rotulos(), predefinicoes()]).toEqual([
      ["1", "2", "3"],
      ["Cancelado", "Cancelado", "Vídeo 1/2"],
      ["Júri", "WhatsApp", "Preservação"],
    ]);
    expect([barra("3"), cancelar("3") !== null]).toEqual(["10%", true]);
    // cancelada não avisa
    expect(toasts()).toEqual([]);
    semFrase();
  });

  it("fim de cada uma: aviso discreto com Abrir pasta (pasta do caso de quando foi pedida); reaproveitada só por ícone", async () => {
    await montar(
      <>
        <Fila />
        <Toaster />
      </>,
    );
    // pedida com outro caso aberto: o aviso abre a pasta daquele caso
    cmd.edicaoEnfileirar.mockResolvedValueOnce("1");
    const doc = st().doc!;
    await esperar(async () => void (await useRenderStore.getState().enfileirar("/casos/outro", doc, { tipo: "final", exportacao: doc.saida.exportacao }, [])));
    await receber([tarefa("1", rodando(1, 30)), tarefa("2", { tipo: "na_fila" }, { predefinicao: "whatsapp" })]);
    expect(toasts()).toEqual([]);
    await receber([tarefa("1", { tipo: "concluida", saida_id: "s1" }), tarefa("2", rodando(1, 30), { predefinicao: "whatsapp" })]);
    expect(toasts().map((t) => [t.kind, t.title, t.message, t.acao?.rotulo])).toEqual([["success", "Edição 1", "Júri · Pronto", "Abrir pasta"]]);
    // a mesma lista de novo (evento repetido): nenhum aviso a mais
    await receber([tarefa("1", { tipo: "concluida", saida_id: "s1" }), tarefa("2", rodando(2, 30), { predefinicao: "whatsapp" })]);
    expect(toasts().length).toBe(1);
    cmd.edicaoSaidas.mockResolvedValue([saida("s0"), saida("s1")]);
    await clicar(botao(corpo().querySelector('[role="region"]')!, "Abrir pasta"));
    await tick();
    expect(cmd.edicaoSaidas).toHaveBeenLastCalledWith("/casos/outro", "p");
    expect(cmd.revealPathInExplorer).toHaveBeenLastCalledWith("/casos/outro/Exportações/Vídeos/Edições/s1/edicao.mp4");
    expect(toasts()).toEqual([]);

    // a segunda reaproveitou tudo: o fim chega "na hora" e a fila diz por ícone, sem frase
    await receber([
      tarefa("1", { tipo: "concluida", saida_id: "s1" }),
      tarefa("2", { tipo: "concluida", saida_id: "s2" }, { predefinicao: "whatsapp", reaproveitada: true }),
      tarefa("3", { tipo: "na_fila" }, { tipo: "refazer" }),
      tarefa("4", { tipo: "na_fila" }, { predefinicao: "preservacao" }),
      { ...tarefa("5", { tipo: "rodando", fase: { tipo: "camadas" }, feito_q: 0, total_q: 9, qps: 0, restante_s: 0 }, { predefinicao: null }), tipo: "previa" },
    ]);
    expect(toasts().map((t) => [t.kind, t.message, t.acao?.rotulo])).toEqual([["success", "WhatsApp · Pronto", "Abrir pasta"]]);
    await abrirPainel();
    expect(linhas().map((l) => l.querySelector("[data-reaproveitada]") !== null)).toEqual([false, true, false, false]);
    expect(linha("2").querySelector("[data-reaproveitada]")?.getAttribute("aria-label")).toBe("Reaproveitada");
    // sem ws guardado: a pasta do caso aberto na Edição
    cmd.edicaoSaidas.mockResolvedValue([saida("s2")]);
    await clicar(botao(corpo().querySelector('[role="region"]')!, "Abrir pasta"));
    await tick();
    expect(cmd.revealPathInExplorer).toHaveBeenLastCalledWith(`${WS}/Exportações/Vídeos/Edições/s2/edicao.mp4`);

    // refeita a conferir, exportação que falha, prévia pronta
    await receber([
      tarefa("1", { tipo: "concluida", saida_id: "s1" }),
      tarefa("2", { tipo: "concluida", saida_id: "s2" }, { predefinicao: "whatsapp", reaproveitada: true }),
      tarefa("3", { tipo: "a_conferir", saida_id: "s3" }, { tipo: "refazer" }),
      tarefa("4", { tipo: "falhou", mensagem: "disco_cheio: Disco cheio", log_rel: "render/4/falha.log" }, { predefinicao: "preservacao" }),
      { ...tarefa("5", { tipo: "pronta", relativo: "render/5/previa.mp4" }, { predefinicao: null }), tipo: "previa" },
    ]);
    expect(toasts().map((t) => [t.kind, t.title, t.message, t.acao?.rotulo ?? null])).toEqual([
      ["warn", "Edição 3", "Júri · A conferir", "Abrir pasta"],
      ["error", "Edição 4", "Preservação · Falhou", null],
    ]);
    // o texto do aviso: só rótulos (o título é dado)
    for (const t of toasts()) expect(t.message.split(" · ").filter((x) => !textoPermitido(x))).toEqual([]);
    await esperar(() => useToastStore.getState().clear());
    semFrase();
  });

  it("sem a pasta de quando foi pedida e de outro caso que o da Edição aberta: aviso sem Abrir pasta", async () => {
    await montar(<Fila />);
    await receber([tarefa("1", rodando(1, 30), { caso: "c2" }), tarefa("2", rodando(1, 30))]);
    await receber([tarefa("1", { tipo: "concluida", saida_id: "s1" }, { caso: "c2" }), tarefa("2", { tipo: "concluida", saida_id: "s2" })]);
    expect(toasts().map((t) => [t.message, t.acao?.rotulo ?? null])).toEqual([
      ["Júri · Pronto", null],
      ["Júri · Pronto", "Abrir pasta"],
    ]);
  });
});
