// @vitest-environment happy-dom
/**
 * Store do rastreio (P2.4) com comandos e eventos falsos: o pedido sai com o documento da tela; o fim entra por
 * `alterar()` (um passo: Ctrl+Z devolve o ponteiro anterior); o denso é lido uma vez e guardado; as marcas por quadro,
 * o "Próximo trecho fraco", o "Aceitar ausência", o "Retomar", as edições e o "Da análise" medidos quadro a quadro.
 */

import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { readFileSync } from "node:fs";
import { join } from "node:path";
import type { Corrida, Efeito, EventoRastreio, ProjetoAberto, ProjetoEdicao, Rastreio } from "@domain/edicao";

const cmd = vi.hoisted(() => ({
  edicaoAbrir: vi.fn(),
  edicaoSalvar: vi.fn(),
  edicaoAnalisar: vi.fn(),
  edicaoRastreioSeguir: vi.fn(),
  edicaoRastreioCancelar: vi.fn(),
  edicaoRastreioLer: vi.fn(),
  edicaoRastreioEditar: vi.fn(),
  edicaoRastreioDePessoas: vi.fn(),
}));
vi.mock("@core/commands", () => ({ commands: cmd }));
const ev = vi.hoisted(() => ({ ouvindo: 0 }));
vi.mock("@tauri-apps/api/event", () => ({
  listen: vi.fn(async () => {
    ev.ouvindo++;
    return () => void ev.ouvindo--;
  }),
}));

import { act, createElement } from "react";
import { useToastStore } from "@components/toast/toastStore";
import { useSettingsStore } from "@stores/settingsStore";
import { useEdicaoStore } from "./edicaoStore";
import { useOrigemStore } from "./origemStore";
import { useRecursos } from "../palco/recursos";
import { useInspetorUi } from "../inspetor/ui";
import { desmontarTudo, montar } from "../inspetor/testeTela";
import {
  aceitarAusencia,
  alvoDoEfeito,
  ancoraDeRetomar,
  ancoraDeSeguir,
  apagarDaquiEmDiante,
  atalhosDoRastreio,
  ausenciaNoCursor,
  densoEmCache,
  desatualizado,
  editarRastreio,
  esquecerRastreios,
  FRASE_PERITO_AUSENTE,
  irAoProximoFraco,
  marcas,
  pararCorrida,
  pedidoDe,
  proximoFraco,
  quadrosDaFaixa,
  RASTREIO_PRONTO,
  seguirEfeito,
  TOLERANCIA_SIMPLIFICAR,
  SUAVIZAR_ANOTACAO_S,
  opSimplificar,
  podeSimplificar,
  trazerDaAnalise,
  trechoSemAlvo,
  useOuvirRastreio,
  useRastreioStore,
} from "./rastreioStore";
import { quadroDaCena } from "../palco/cena";
import { aplicarMudanca } from "../modelo/projeto";
import { FerramentaSeguir } from "../rastreio/FerramentaSeguir";
import { CAIXA, densoDe, faixa, info, PASSO, ponteiro, projeto, q, SHA, tab, WS } from "../rastreio/cenario.teste";

const st = () => useEdicaoStore.getState();
const rs = () => useRastreioStore.getState();
const r1 = (): Rastreio => st().doc!.rastreios.find((r) => r.id === "r1")!;
const em = (j: number) => st().posicionar({ modo: "clipe", clipe: "c", t: q(j).t });
const alvo = () => alvoDoEfeito(st().doc!, "e1", "c")!;

/** Denso da primeira corrida: 30..79 medidos, 60..64 incertos (o alvo perdeu-se no 80). */
const A = densoDe("r1", [...faixa(30, 59), ...faixa(60, 64, 1), ...faixa(65, 79)]);
const SHA_A = "a".repeat(64);
const fimA = (tarefa = "t1"): EventoRastreio => ({ tipo: "fim", tarefa_id: tarefa, rastreio_id: "r1", parou: [{ sentido: "frente", motivo: "perdido", pts: 80 * PASSO }], denso: ponteiro(SHA_A, A), silhuetas: null });
const avanco = (j: number, estado = 0, tarefa = "t1"): EventoRastreio => ({
  tipo: "avanco", tarefa_id: tarefa, rastreio_id: "r1", sentido: "frente", pts: j * PASSO, caixa: estado === 2 ? null : CAIXA, conf: 0.9, estado, feitos: j - 30, miniatura: `data:image/jpeg;base64,Q${j}`,
});

beforeEach(async () => {
  vi.useFakeTimers({ toFake: ["Date"], now: new Date("2026-11-15T10:00:00Z") });
  for (const f of Object.values(cmd)) f.mockReset();
  cmd.edicaoSalvar.mockImplementation(async () => ({ updated_at: new Date().toISOString() }));
  cmd.edicaoAnalisar.mockImplementation(() => new Promise(() => {}));
  cmd.edicaoRastreioLer.mockImplementation(async (_w: string, _p: string, _r: string, sha: string) => {
    if (sha === SHA_A) return A;
    throw { kind: "validation", message: "validation error: rastreio_ausente: Rastreio não encontrado" };
  });
  esquecerRastreios();
  useToastStore.setState({ toasts: [] });
  useInspetorUi.setState({ ferramenta: null, aoMarcar: null, chave: null });
  useSettingsStore.setState((s) => ({ settings: { ...s.settings, profile: { ...s.settings.profile, full_name: "Perito Teste" } } }));
  const p = projeto();
  cmd.edicaoAbrir.mockResolvedValue({ projeto: p, importado_de: null, so_leitura: false, avisos: [], raiz_cache: "/cache/r" } satisfies ProjetoAberto);
  await st().abrir(WS, p.id);
  useRecursos.setState((s) => ({ midias: { [SHA]: { info, tabela: tab, erro: null, pedindo: false } }, versao: s.versao + 1 }));
  em(30);
});

afterEach(async () => {
  desmontarTudo();
  useOrigemStore.setState({ porClipe: {} });
  await st()
    .fechar({ descartar: true })
    .catch(() => {});
  vi.useRealTimers();
});

function Ouvir() {
  useOuvirRastreio();
  return null;
}

const espera = () => new Promise((r) => setTimeout(r, 0));

describe("bandeira", () => {
  it("RASTREIO_PRONTO ligada (entrega do P2.4)", () => {
    expect(RASTREIO_PRONTO).toBe(true);
  });
});

describe("Seguir: pedido e eventos", () => {
  it("o pedido sai com o documento da tela, a âncora do quadro, até o último quadro do efeito e motor auto; nada grava", async () => {
    cmd.edicaoRastreioSeguir.mockResolvedValue("t1");
    const rev = st().revisao;
    await seguirEfeito("e1", "c", "frente");
    expect(cmd.edicaoRastreioSeguir).toHaveBeenCalledTimes(1);
    const [ws, doc, rid, clipe, ancora, sentido, ate, motor] = cmd.edicaoRastreioSeguir.mock.calls[0]!;
    expect([ws, rid, clipe, sentido, motor]).toEqual([WS, "r1", "c", "frente", "auto"]);
    expect(doc).toBe(st().doc);
    expect(ancora).toEqual({ ...q(30), v: CAIXA });
    // trecho [0, 200): o último quadro do efeito é o 199
    expect(ate).toBe(q(199).t);
    expect(st().revisao).toBe(rev);
    expect(rs().corrida).toMatchObject({ tarefa: "t1", rastreio: "r1", ultimo: null });
    // uma corrida por vez: o segundo pedido nem chega ao backend
    await seguirEfeito("e1", "c", "tras");
    expect(cmd.edicaoRastreioSeguir).toHaveBeenCalledTimes(1);
  });

  it("para trás: âncora do quadro (ou a primeira depois), até o primeiro quadro do efeito; sem quadro antes, não pede", async () => {
    em(40);
    const x = alvo();
    expect(pedidoDe(x, "tras", "seguir", {})).toEqual({ rastreio_id: "r1", clipe_id: "c", ancora: { ...q(30), v: CAIXA }, sentido: "tras", ate_s: q(0).t, motor: "auto" });
    // efeito que começa no 30: para trás do 30 não há o que seguir
    useEdicaoStore.getState().alterar("De", (d) => void ((d.principal[0] as { efeitos: { ativo: unknown }[] }).efeitos[0]!.ativo = { de: q(30).t, ate: null }));
    expect(pedidoDe(alvo(), "tras", "seguir", {})).toBeNull();
    expect(pedidoDe(alvo(), "frente", "seguir", {})?.ate_s).toBe(q(199).t);
  });

  it("andamento na tela; fim por alterar() (um passo), origem assistida, cursor onde perdeu; Ctrl+Z e Ctrl+Y trocam o ponteiro", async () => {
    cmd.edicaoRastreioSeguir.mockResolvedValue("t1");
    await seguirEfeito("e1", "c", "frente");
    const passos = st().passado.length;
    rs().receber(avanco(31));
    rs().receber(avanco(45, 1));
    expect(rs().corrida?.ultimo).toMatchObject({ pts: 45 * PASSO, estado: 1, miniatura: "data:image/jpeg;base64,Q45" });
    // evento de outra tarefa: ignorado
    rs().receber(avanco(50, 0, "outra"));
    expect(rs().corrida?.ultimo?.pts).toBe(45 * PASSO);
    expect(st().passado.length).toBe(passos);
    rs().receber(fimA());
    expect(rs().corrida).toBeNull();
    expect(r1().denso).toEqual(ponteiro(SHA_A, A));
    expect(r1().origem).toEqual({ tipo: "assistido" });
    expect(st().passado.length).toBe(passos + 1);
    expect(st().passado.at(-1)?.rotulo).toBe("Seguir");
    expect(rs().paradas[SHA_A]).toEqual([{ sentido: "frente", motivo: "perdido", pts: 80 * PASSO }]);
    expect(st().cursor).toEqual({ modo: "clipe", clipe: "c", t: q(80).t });
    // andamento atrasado da tarefa que acabou: nada
    rs().receber(avanco(70));
    expect(rs().corrida).toBeNull();
    st().desfazer();
    expect(r1().denso).toBeNull();
    expect(r1().origem).toEqual({ tipo: "manual" });
    st().refazer();
    expect(r1().denso?.sha256).toBe(SHA_A);
  });

  it("o efeito aberto perdeu o alvo: a ferramenta de marcar fica ligada nele e o que se marca vira âncora", async () => {
    st().selecionar({ ...st().selecao, efeito: "e1" });
    cmd.edicaoRastreioSeguir.mockResolvedValue("t1");
    await seguirEfeito("e1", "c", "frente");
    rs().receber(fimA());
    const ui = useInspetorUi.getState();
    expect(ui.ferramenta).toEqual({ tipo: "caixa", rastreio: "r1", efeito: "e1" });
    expect(ui.chave).toBe("e1:caixa");
    ui.aoMarcar!({ x: 0.5, y: 0.4, w: 0.1, h: 0.2 }, q(95).t, q(95).pts);
    expect(r1().ancoras.map((k) => k.pts)).toEqual([30 * PASSO, 95 * PASSO]);
  });

  it("eventos antes da resposta do comando: adotados pelo rastreio; fim antes da resposta não ressuscita a corrida", async () => {
    let responder: (t: string) => void = () => {};
    cmd.edicaoRastreioSeguir.mockImplementation(() => new Promise<string>((r) => (responder = r)));
    const pedido = seguirEfeito("e1", "c", "frente");
    await espera();
    rs().receber(avanco(31));
    expect(rs().corrida?.tarefa).toBe("t1");
    rs().receber(fimA());
    responder("t1");
    await pedido;
    expect(rs().corrida).toBeNull();
    expect(r1().denso?.sha256).toBe(SHA_A);
  });

  it("falhou: a frase do catálogo no aviso do app; o comando recusado também", async () => {
    cmd.edicaoRastreioSeguir.mockResolvedValueOnce("t1");
    await seguirEfeito("e1", "c", "frente");
    rs().receber({ tipo: "falhou", tarefa_id: "t1", rastreio_id: "r1", mensagem: "modelo_ausente: Modelo não instalado" });
    expect(rs().corrida).toBeNull();
    expect(useToastStore.getState().toasts.at(-1)).toMatchObject({ kind: "error", message: "Modelo não instalado" });
    cmd.edicaoRastreioSeguir.mockRejectedValueOnce({ kind: "validation", message: "validation error: rastreio_em_curso: Outro rastreio em andamento" });
    await seguirEfeito("e1", "c", "frente");
    expect(rs().corrida).toBeNull();
    expect(useToastStore.getState().toasts.at(-1)?.message).toBe("Outro rastreio em andamento");
    expect(r1().denso).toBeNull();
  });

  it("ambos com o segundo lado em falha: o fim aplica o que foi seguido e a falha que vem depois vai para o aviso, uma vez", async () => {
    cmd.edicaoRastreioSeguir.mockResolvedValue("t1");
    await seguirEfeito("e1", "c", "frente");
    rs().receber(fimA());
    expect(r1().denso?.sha256).toBe(SHA_A);
    const n = useToastStore.getState().toasts.length;
    const falha: EventoRastreio = { tipo: "falhou", tarefa_id: "t1", rastreio_id: "r1", mensagem: "gpu_sem_memoria: Memória de vídeo insuficiente" };
    rs().receber(falha);
    expect(useToastStore.getState().toasts.at(-1)).toMatchObject({ kind: "error", message: "Memória de vídeo insuficiente" });
    rs().receber(falha);
    expect(useToastStore.getState().toasts.length).toBe(n + 1);
    expect(rs().corrida).toBeNull();
    expect(r1().denso?.sha256).toBe(SHA_A);
  });

  it("Parar (e Esc): cancela pela tarefa; antes da resposta, cancela quando o id chega; tarefa que já acabou solta a tela", async () => {
    expect(pararCorrida()).toBe(false);
    let responder: (t: string) => void = () => {};
    cmd.edicaoRastreioSeguir.mockImplementation(() => new Promise<string>((r) => (responder = r)));
    cmd.edicaoRastreioCancelar.mockResolvedValue(undefined);
    const pedido = seguirEfeito("e1", "c", "frente");
    await espera();
    expect(pararCorrida()).toBe(true);
    expect(cmd.edicaoRastreioCancelar).not.toHaveBeenCalled();
    responder("t9");
    await pedido;
    expect(cmd.edicaoRastreioCancelar).toHaveBeenCalledWith("t9");
    cmd.edicaoRastreioCancelar.mockRejectedValueOnce({ kind: "validation", message: "validation error: tarefa_ausente: Tarefa não encontrada" });
    await rs().parar();
    expect(rs().corrida).toBeNull();
  });

  it("editor desmontado no meio da corrida (projeto fechado ou trocado): solta o ouvinte, cancela pela tarefa e solta a tela", async () => {
    cmd.edicaoRastreioCancelar.mockResolvedValue(undefined);
    const ed = await montar(createElement(Ouvir));
    await act(async () => {});
    expect(ev.ouvindo).toBe(1);
    cmd.edicaoRastreioSeguir.mockResolvedValueOnce("t1");
    await seguirEfeito("e1", "c", "frente");
    ed.desmontar();
    expect(ev.ouvindo).toBe(0);
    expect(rs().corrida).toBeNull();
    await espera();
    expect(cmd.edicaoRastreioCancelar).toHaveBeenCalledWith("t1");
    expect(rs().corrida).toBeNull();
    // evento atrasado da tarefa largada não volta
    rs().receber(avanco(40));
    expect(rs().corrida).toBeNull();
    // largada antes da resposta: cancela quando o id chega; a corrida seguinte segue livre
    const ed2 = await montar(createElement(Ouvir));
    let responder: (t: string) => void = () => {};
    cmd.edicaoRastreioSeguir.mockImplementationOnce(() => new Promise<string>((r) => (responder = r)));
    const pedido = seguirEfeito("e1", "c", "frente");
    await espera();
    ed2.desmontar();
    expect(rs().corrida).toBeNull();
    responder("t2");
    await pedido;
    await espera();
    expect(cmd.edicaoRastreioCancelar).toHaveBeenLastCalledWith("t2");
    expect(rs().corrida).toBeNull();
  });

  it("Seguir para trás: o fim entra no histórico como 'Seguir para trás'", async () => {
    em(40);
    cmd.edicaoRastreioSeguir.mockResolvedValue("t1");
    await seguirEfeito("e1", "c", "tras");
    expect(cmd.edicaoRastreioSeguir.mock.calls[0]![5]).toBe("tras");
    rs().receber({ tipo: "fim", tarefa_id: "t1", rastreio_id: "r1", parou: [{ sentido: "tras", motivo: "fim", pts: 0 }], denso: ponteiro(SHA_A, A), silhuetas: null });
    expect(st().passado.at(-1)?.rotulo).toBe("Seguir para trás");
    expect(st().cursor).toEqual({ modo: "clipe", clipe: "c", t: q(0).t });
  });

  it("só leitura: nada pede ao backend", async () => {
    useEdicaoStore.setState({ soLeitura: true });
    await seguirEfeito("e1", "c", "frente");
    expect(cmd.edicaoRastreioSeguir).not.toHaveBeenCalled();
    expect(rs().corrida).toBeNull();
  });

  it("intermediário de origem em preparo (§7.5): Seguir do atalho e do palco não sai; pronto, sai", async () => {
    st().alterar("Desentrelaçar", (d) =>
      void (d.principal[0] as { efeitos: unknown[] }).efeitos.unshift({ id: "dz", ligado: true, nome: "", ativo: null, tipo: "desentrelacar", modo: "quadro", ordem: "auto" }),
    );
    cmd.edicaoRastreioSeguir.mockResolvedValue("t1");
    st().selecionar({ ...st().selecao, efeito: "e1" });
    useInspetorUi.setState({ ferramenta: { tipo: "caixa", rastreio: "r1", efeito: "e1" } });
    const palco = await montar(createElement(FerramentaSeguir, { area: { x: 0, y: 0, w: 640, h: 360 } }));
    const botoes = () => [...palco.raiz.querySelectorAll<HTMLButtonElement>("[data-seguir-palco] button")].map((b) => b.disabled);
    expect(botoes()).toEqual([true, true]);
    atalhosDoRastreio()["edicao.seguirFrente"]!();
    await seguirEfeito("e1", "c", "frente");
    expect(cmd.edicaoRastreioSeguir).not.toHaveBeenCalled();
    const previa = { relativo: "o.mp4", tipo: "origem" as const, desloc_s: 0, de_s: 0, ate_s: 10, largura: 640, altura: 360, pronta: true };
    await act(async () => useOrigemStore.setState({ porClipe: { c: { tipo: "pronto", previa, trf: null } } }));
    expect(botoes()).toEqual([false, false]);
    await seguirEfeito("e1", "c", "frente");
    expect(cmd.edicaoRastreioSeguir).toHaveBeenCalledTimes(1);
  });
});

describe("denso em cache", () => {
  it("lido uma vez por SHA-256 e guardado; leitura que falha não repete", async () => {
    const r = { ...r1(), denso: ponteiro(SHA_A, A) };
    expect(densoEmCache(r)).toBeNull();
    expect(densoEmCache(r)).toBeNull();
    await espera();
    expect(cmd.edicaoRastreioLer).toHaveBeenCalledTimes(1);
    expect(cmd.edicaoRastreioLer).toHaveBeenCalledWith(WS, st().aberta!.projetoId, "r1", SHA_A);
    expect(densoEmCache(r)).toBe(A);
    const ruim = { ...r1(), denso: ponteiro("b".repeat(64), A) };
    densoEmCache(ruim);
    await espera();
    densoEmCache(ruim);
    await espera();
    expect(cmd.edicaoRastreioLer).toHaveBeenCalledTimes(2);
  });

  it("leitura que falhou (pasta fora do ar) volta a ser tentada quando o editor abre de novo", async () => {
    const r = { ...r1(), denso: ponteiro(SHA_A, A) };
    cmd.edicaoRastreioLer.mockRejectedValueOnce({ kind: "filesystem", message: "filesystem error: indisponível" });
    densoEmCache(r);
    await espera();
    densoEmCache(r);
    await espera();
    expect(cmd.edicaoRastreioLer).toHaveBeenCalledTimes(1);
    await montar(createElement(Ouvir));
    expect(densoEmCache(r)).toBeNull();
    await espera();
    expect(cmd.edicaoRastreioLer).toHaveBeenCalledTimes(2);
    expect(densoEmCache(r)).toBe(A);
  });
});

describe("marcas por quadro", () => {
  const rComDenso = (): Rastreio => ({ ...r1(), origem: { tipo: "assistido" }, ancoras: [{ ...q(30), v: CAIXA }, { ...q(95), v: CAIXA }], denso: ponteiro(SHA_A, A) });

  it("âncora vence o denso; linha do denso pelo estado e pelo tipo; sem linha: interpolado entre âncoras, nada depois da última (assistido)", () => {
    const D = densoDe("r1", [{ j: 30, estado: 1 }, { j: 31 }, { j: 32, estado: 1 }, { j: 33, estado: 2 }, { j: 34, tipo: 1 }, { j: 35, tipo: 2 }]);
    const m = marcas(rComDenso(), D, tab, 29, 97);
    expect(m.slice(0, 7)).toEqual(["nada", "ancora", "ok", "incerto", "ausente", "interpolado", "interpolado"]);
    // 36..94 sem linha, entre as âncoras 30 e 95
    expect(new Set(m.slice(7, 66))).toEqual(new Set(["interpolado"]));
    expect(m.slice(66)).toEqual(["ancora", "nada", "nada"]);
    // manual segura a última âncora
    expect(marcas({ ...rComDenso(), origem: { tipo: "manual" } }, D, tab, 96, 97)).toEqual(["ok", "ok"]);
    // ausência aceita só desculpa o quadro sem caixa (§6.1: o motor desenha a caixa que houver), nas duas pontas
    const aceita = { ...rComDenso(), ausencias: [{ de_s: q(31).t, ate_s: q(97).t, motivo: "oculto" as const, aceita_em: "", por: "" }] };
    expect(marcas(aceita, D, tab, 30, 36)).toEqual(["ancora", "ok", "incerto", "aceita", "interpolado", "interpolado", "interpolado"]);
    expect(marcas(aceita, D, tab, 95, 98)).toEqual(["ancora", "aceita", "aceita", "nada"]);
  });

  it("faixa do efeito em quadros: fim de trecho fica fora; `ate` do efeito é fechado", () => {
    expect(quadrosDaFaixa(tab, q(0).t, q(200).t, true)).toEqual([0, 199]);
    expect(quadrosDaFaixa(tab, q(10).t, q(20).t, false)).toEqual([10, 20]);
    // começo no meio de um quadro: o primeiro é o seguinte
    expect(quadrosDaFaixa(tab, q(10).t + 0.01, q(20).t, false)).toEqual([11, 20]);
    expect(quadrosDaFaixa(tab, q(20).t, q(20).t, true)).toBeNull();
  });

  it("próximo trecho fraco: começo do trecho seguinte, de dentro de um trecho pula para o outro, e volta ao primeiro", () => {
    const m = ["ok", "incerto", "incerto", "ok", "ancora", "nada", "ausente", "ok", "interpolado"] as const;
    expect(proximoFraco(m, 0)).toBe(1);
    expect(proximoFraco(m, 1)).toBe(5);
    expect(proximoFraco(m, 3)).toBe(5);
    expect(proximoFraco(m, 5)).toBe(8);
    expect(proximoFraco(m, 8)).toBe(1);
    expect(proximoFraco(m, -1)).toBe(1);
    expect(proximoFraco(["ok", "ancora", "aceita"], 0)).toBeNull();
  });

  it("trecho sem o alvo em volta do quadro: ausente e nada juntos; quadro com caixa (ok, incerto, interpolado) para", () => {
    const m = ["ok", "interpolado", "nada", "ausente", "incerto", "nada", "ok"] as const;
    expect(trechoSemAlvo(m, 2)).toEqual([2, 3]);
    expect(trechoSemAlvo(m, 5)).toEqual([5, 5]);
    expect(trechoSemAlvo(m, 1)).toBeNull();
    expect(trechoSemAlvo(m, 4)).toBeNull();
    expect(trechoSemAlvo(m, 0)).toBeNull();
  });
});

describe("ações da linha do efeito", () => {
  async function primeiraCorrida() {
    cmd.edicaoRastreioSeguir.mockResolvedValueOnce("t1");
    await seguirEfeito("e1", "c", "frente");
    rs().receber(fimA());
    await espera();
    expect(rs().densos[SHA_A]).toBe(A);
  }

  it("Retomar: da âncora recolocada depois de onde parou, no sentido da corrida; sem ela, não há pedido", async () => {
    await primeiraCorrida();
    const p = rs().paradas[SHA_A]![0]!;
    expect(ancoraDeRetomar(r1(), p, q(80).t)).toBeNull();
    expect(pedidoDe(alvo(), "frente", "retomar", rs().paradas)).toBeNull();
    useEdicaoStore.getState().alterar("Marcar", (d) => void d.rastreios[0]!.ancoras.push({ ...q(95), v: CAIXA }));
    expect(ancoraDeRetomar(r1(), p, q(80).t)).toEqual({ ...q(95), v: CAIXA });
    // cursor em outro lugar: Retomar parte do 95 mesmo assim; Seguir, da âncora antes do cursor
    em(40);
    expect(pedidoDe(alvo(), "tras", "retomar", rs().paradas)).toMatchObject({ sentido: "frente", ancora: { pts: 95 * PASSO }, ate_s: q(199).t });
    expect(ancoraDeSeguir(r1(), q(40).t, q(40).pts, "frente")?.pts).toBe(30 * PASSO);
    // Ctrl+Z da corrida: o ponteiro volta e a parada deixa de valer
    st().desfazer();
    st().desfazer();
    expect(r1().denso).toBeNull();
    expect(pedidoDe(alvo(), "frente", "retomar", rs().paradas)).toBeNull();
  });

  it("Aceitar ausência: o trecho sem caixa em volta do cursor, com quem e quando; vira 'aceita' nas marcas só onde segue sem caixa", async () => {
    await primeiraCorrida();
    em(70);
    expect(ausenciaNoCursor(alvo())).toBeNull();
    // perdeu no 80 e não voltou: do 80 ao fim do efeito
    em(85);
    const a = ausenciaNoCursor(alvo())!;
    expect(a).toEqual({ de_s: q(80).t, ate_s: q(199).t });
    aceitarAusencia("r1", a.de_s, a.ate_s, "fora_de_cena");
    expect(r1().ausencias).toEqual([{ de_s: q(80).t, ate_s: q(199).t, motivo: "fora_de_cena", aceita_em: "2026-11-15T10:00:00.000Z", por: "Perito Teste" }]);
    expect(st().passado.at(-1)?.rotulo).toBe("Aceitar ausência");
    expect(marcas(r1(), A, tab, 79, 96)).toEqual(["ok", ...Array(17).fill("aceita")]);
    expect(ausenciaNoCursor(alvo())).toBeNull();
    // recolocado no 95: a interpolação 80..94 tem caixa e o motor a desenha; a ausência só segue valendo depois da âncora
    useEdicaoStore.getState().alterar("Marcar", (d) => void d.rastreios[0]!.ancoras.push({ ...q(95), v: CAIXA }));
    expect(marcas(r1(), A, tab, 79, 96)).toEqual(["ok", ...Array(15).fill("interpolado"), "ancora", "aceita"]);
    em(85);
    expect(ausenciaNoCursor(alvo())).toBeNull();
  });

  it("Próximo trecho fraco (botão e atalho N): incerto 60, depois o fim perdido 80, e volta", async () => {
    await primeiraCorrida();
    em(30);
    expect(irAoProximoFraco(alvo())).toBe(true);
    expect(st().cursor).toEqual({ modo: "clipe", clipe: "c", t: q(60).t });
    st().selecionar({ ...st().selecao, efeito: "e1" });
    atalhosDoRastreio()["edicao.trechoFraco"]!();
    expect(st().cursor).toMatchObject({ t: q(80).t });
    atalhosDoRastreio()["edicao.trechoFraco"]!();
    expect(st().cursor).toMatchObject({ t: q(0).t });
  });

  it("atalhos Seguir e Seguir para trás: do efeito aberto, no clipe do cursor", async () => {
    cmd.edicaoRastreioSeguir.mockResolvedValue("t1");
    st().selecionar({ ...st().selecao, efeito: "e1" });
    em(50);
    atalhosDoRastreio()["edicao.seguirTras"]!();
    await espera();
    expect(cmd.edicaoRastreioSeguir.mock.calls[0]!.slice(2, 7)).toEqual(["r1", "c", { ...q(30), v: CAIXA }, "tras", q(0).t]);
  });

  it("Apagar daqui em diante e Simplificar: o pedido pelo pts do quadro; âncoras quando vêm e o denso sempre, um passo", async () => {
    await primeiraCorrida();
    em(70);
    const ate69 = densoDe("r1", faixa(30, 69));
    cmd.edicaoRastreioEditar.mockResolvedValueOnce({ ancoras: null, denso: ponteiro("c".repeat(64), ate69) });
    await editarRastreio("r1", { tipo: "apagar_faixa", de_pts: 70 * PASSO, ate_pts: null }, "Apagar daqui em diante");
    expect(cmd.edicaoRastreioEditar.mock.calls[0]!.slice(2)).toEqual(["r1", { tipo: "apagar_faixa", de_pts: 70 * PASSO, ate_pts: null }]);
    expect(r1().denso?.sha256).toBe("c".repeat(64));
    expect(r1().ancoras).toHaveLength(1);
    const ks = [{ ...q(30), v: CAIXA }, { ...q(69), v: { ...CAIXA, x: 0.5 } }];
    cmd.edicaoRastreioEditar.mockResolvedValueOnce({ ancoras: ks, denso: null });
    await editarRastreio("r1", { tipo: "simplificar", tolerancia: TOLERANCIA_SIMPLIFICAR }, "Simplificar");
    expect(cmd.edicaoRastreioEditar.mock.calls[1]![3]).toEqual({ tipo: "simplificar", tolerancia: 0.04 });
    expect(r1().ancoras).toEqual(ks);
    expect(r1().denso).toBeNull();
    expect(st().passado.slice(-2).map((p) => p.rotulo)).toEqual(["Apagar daqui em diante", "Simplificar"]);
    st().desfazer();
    st().desfazer();
    expect(r1().denso?.sha256).toBe(SHA_A);
  });

  it("Apagar daqui em diante pede do pts do quadro do cursor até o fim (cursor no meio do quadro)", async () => {
    await primeiraCorrida();
    st().posicionar({ modo: "clipe", clipe: "c", t: q(70).t + 0.4 / 30 });
    cmd.edicaoRastreioEditar.mockResolvedValueOnce({ ancoras: null, denso: ponteiro("c".repeat(64), densoDe("r1", faixa(30, 69))) });
    const doc = st().doc;
    await apagarDaquiEmDiante(alvo());
    expect(cmd.edicaoRastreioEditar).toHaveBeenCalledWith(WS, doc, "r1", { tipo: "apagar_faixa", de_pts: 70 * PASSO, ate_pts: null });
    expect(st().passado.at(-1)?.rotulo).toBe("Apagar daqui em diante");
  });

  it("Atualizar: só no rastreio de Pessoas com retrato_desatualizado dele (pelo id do rastreio ou pelo efeito + campo)", () => {
    const p = (efeito: string | null, campo: string | null, codigo = "retrato_desatualizado") => ({ codigo, efeito, campo });
    const pessoas: Rastreio = { ...r1(), id: "r9", origem: { tipo: "pessoas", pessoa_id: "p3", estado: "auto", copiado_em: "" } };
    expect(desatualizado(pessoas, "e1", [p("r9", null)])).toBe(true);
    expect(desatualizado(pessoas, "e1", [p(null, "r9")])).toBe(true);
    expect(desatualizado(pessoas, "e1", [p("e1", "rastreio")])).toBe(true);
    expect(desatualizado(pessoas, "e1", [p("e2", "rastreio"), p("r8", null), p("r9", null, "sem_cobertura")])).toBe(false);
    expect(desatualizado(r1(), "e1", [p("r1", null)])).toBe(false);
  });

  it("Aceitar ausência sem o nome do perito nas Configurações: nada gravado, a frase do perito ausente (Fase 1)", async () => {
    await primeiraCorrida();
    useSettingsStore.setState((s) => ({ settings: { ...s.settings, profile: { ...s.settings.profile, full_name: "  " } } }));
    const passos = st().passado.length;
    aceitarAusencia("r1", q(80).t, q(199).t, "oculto");
    expect(r1().ausencias).toEqual([]);
    expect(st().passado.length).toBe(passos);
    expect(useToastStore.getState().toasts.at(-1)).toMatchObject({ kind: "error", message: FRASE_PERITO_AUSENTE });
  });

  it("a frase do perito ausente é a do catálogo (`edicao/mensagens.rs`)", () => {
    const fonte = readFileSync(join(process.cwd(), "src-tauri/src/edicao/mensagens.rs"), "utf8");
    expect(fonte).toContain(`e(PERITO_AUSENTE, Erro, "${FRASE_PERITO_AUSENTE}")`);
    expect(fonte).toContain('pub const PERITO_AUSENTE: &str = "perito_ausente";');
  });

  it("Da análise de trilha revisada: a confirmação leva o perito das Configurações; sem nome, nada entra", async () => {
    const revisada: Rastreio = {
      ...r1(), id: "r9", origem: { tipo: "pessoas", pessoa_id: "p3", estado: "revisada", copiado_em: "2026-11-15T09:00:00Z" }, denso: ponteiro(SHA_A, A), ancoras: [],
      confirmacao: { por: "", em: "2026-11-15T08:00:00Z", denso_sha256: SHA_A, de_s: q(30).t, ate_s: q(79).t },
    };
    cmd.edicaoRastreioDePessoas.mockResolvedValue(revisada);
    useSettingsStore.setState((s) => ({ settings: { ...s.settings, profile: { ...s.settings.profile, full_name: "" } } }));
    const passos = st().passado.length;
    await trazerDaAnalise("e1", "m", SHA, "p3", "Da análise");
    expect(st().doc!.rastreios.map((r) => r.id)).toEqual(["r1"]);
    expect(st().passado.length).toBe(passos);
    expect(useToastStore.getState().toasts.at(-1)?.message).toBe(FRASE_PERITO_AUSENTE);
    useSettingsStore.setState((s) => ({ settings: { ...s.settings, profile: { ...s.settings.profile, full_name: "Perito Teste" } } }));
    await trazerDaAnalise("e1", "m", SHA, "p3", "Da análise");
    expect(st().doc!.rastreios.find((r) => r.id === "r9")?.confirmacao).toEqual({ ...revisada.confirmacao, por: "Perito Teste" });
  });

  it("Da análise: o efeito passa ao rastreio novo; o antigo, sem outro uso, sai; um passo", async () => {
    const novo: Rastreio = { ...r1(), id: "r9", origem: { tipo: "pessoas", pessoa_id: "p3", estado: "auto", copiado_em: "2026-11-15T09:00:00Z" }, denso: ponteiro(SHA_A, A), ancoras: [] };
    cmd.edicaoRastreioDePessoas.mockResolvedValue(novo);
    await trazerDaAnalise("e1", "m", SHA, "p3", "Da análise");
    expect(cmd.edicaoRastreioDePessoas).toHaveBeenCalledWith(WS, st().aberta!.projetoId, "m", SHA, "p3");
    const d = st().doc!;
    expect(d.rastreios.map((r) => r.id)).toEqual(["r9"]);
    expect((d.principal[0] as { efeitos: { rastreio: string }[] }).efeitos[0]!.rastreio).toBe("r9");
    st().desfazer();
    expect(st().doc!.rastreios.map((r) => r.id)).toEqual(["r1"]);
  });

  it("Da análise: o rastreio que o zoom acompanha fica", async () => {
    const z = { id: "z1", ligado: true, nome: "", ativo: null, tipo: "enquadramento", regiao: { x: 0, y: 0, w: 0.5, h: 0.5 }, ajuste: "cobrir",
      interpolacao: "bilinear", segue: { rastreio: "r1", suavizar_s: 1 } } as unknown as Efeito;
    st().alterar("Acompanhar rastreio", (x) => void (x.principal[0] as unknown as { efeitos: Efeito[] }).efeitos.push(z));
    cmd.edicaoRastreioDePessoas.mockResolvedValue({ ...r1(), id: "r9", origem: { tipo: "pessoas", pessoa_id: "p3", estado: "auto", copiado_em: "2026-11-15T09:00:00Z" }, ancoras: [] });
    await trazerDaAnalise("e1", "m", SHA, "p3", "Da análise");
    expect(st().doc!.rastreios.map((r) => r.id)).toEqual(["r1", "r9"]);
  });
});


describe("Simplificar: suaviza só o rastreio de anotação", () => {
  it("destaque suaviza; com ocultação ou holofote no mesmo rastreio, não", () => {
    const d = st().doc! as ProjetoEdicao;
    expect(opSimplificar(d, "r1")).toEqual({ tipo: "simplificar", tolerancia: TOLERANCIA_SIMPLIFICAR, suavizar_s: SUAVIZAR_ANOTACAO_S });
    const oc = { id: "o1", ligado: true, nome: "", ativo: null, tipo: "ocultar", modo: "solido", cor: "#000000ff", intensidade: 1, conteudo: "outro", motivo: "",
      alvo: { rastreio: "r1", forma: "retangulo", vertices: null, margem: 0, borda_suave: 0, inverter: false } } as unknown as Efeito;
    st().alterar("Ocultar", (x) => void (x.midias[0]!.ocultacoes as unknown as Efeito[]).push(oc));
    expect(opSimplificar(st().doc! as ProjetoEdicao, "r1")).toEqual({ tipo: "simplificar", tolerancia: TOLERANCIA_SIMPLIFICAR });
  });

  it("sem denso: o botão vale com 3 ou mais âncoras do programa, só no rastreio de anotação", () => {
    const d = () => st().doc! as ProjetoEdicao;
    expect(podeSimplificar(d(), r1())).toBe(false);
    st().alterar("x", (x) => void (x.rastreios[0]!.ancoras_do_programa = [q(31).t, q(32).t, q(33).t]));
    expect(podeSimplificar(d(), r1())).toBe(true);
    st().alterar("x", (x) => void (x.rastreios[0]!.ancoras_do_programa = [q(31).t, q(32).t]));
    expect(podeSimplificar(d(), r1())).toBe(false);
  });
});

describe("âncoras do programa (Simplificar)", () => {
  const k = (j: number, x = 0.4) => ({ ...q(j), v: { ...CAIXA, x } });
  const programa = () => r1().ancoras_do_programa;

  /** Corrida, depois "Simplificar" devolvendo a âncora do perito (30) e as do programa. */
  async function simplificar(devolvidas: ReturnType<typeof k>[]) {
    cmd.edicaoRastreioEditar.mockResolvedValueOnce({ ancoras: devolvidas, denso: null });
    await editarRastreio("r1", { tipo: "simplificar", tolerancia: TOLERANCIA_SIMPLIFICAR }, "Simplificar");
  }

  it("o Simplificar grava os t das âncoras dele (não as do perito); mexer à mão tira; Ctrl+Z e Ctrl+Y levam a lista", async () => {
    await simplificar([k(30), k(45, 0.45), k(60, 0.5), k(79, 0.6)]);
    expect(r1().ancoras.map((a) => a.pts)).toEqual([30, 45, 60, 79].map((j) => j * PASSO));
    expect(programa()).toEqual([q(45).t, q(60).t, q(79).t]);
    // acrescentar uma âncora noutro quadro: a lista fica
    st().alterar("Marcar", (d) => void d.rastreios[0]!.ancoras.push(k(90)));
    expect(programa()).toEqual([q(45).t, q(60).t, q(79).t]);
    // ajustar a caixa de uma âncora do programa (alça do palco): sai
    st().alterar("Mover", (d) => void aplicarMudanca(d as ProjetoEdicao, { rastreio: "r1", ancora: k(60, 0.52) }));
    expect(programa()).toEqual([q(45).t, q(79).t]);
    // marcar por cima, no mesmo quadro: sai
    st().alterar("Marcar", (d) => void aplicarMudanca(d as ProjetoEdicao, { rastreio: "r1", ancora: k(79, 0.7) }));
    expect(programa()).toEqual([q(45).t]);
    // mover (outro quadro): sai; a lista vazia some do documento
    st().alterar("Mover", (d) => void (d.rastreios[0]!.ancoras[1] = k(46, 0.45)));
    expect(programa()).toBeUndefined();
    expect("ancoras_do_programa" in r1()).toBe(false);
    st().desfazer();
    expect(programa()).toEqual([q(45).t]);
    st().desfazer();
    st().desfazer();
    expect(programa()).toEqual([q(45).t, q(60).t, q(79).t]);
    st().refazer();
    expect(programa()).toEqual([q(45).t, q(79).t]);
    // apagar à mão (e o "Apagar daqui em diante", que tira as âncoras da faixa): sai
    cmd.edicaoRastreioEditar.mockResolvedValueOnce({ ancoras: [k(30), k(45, 0.45)], denso: null });
    await editarRastreio("r1", { tipo: "apagar_faixa", de_pts: 70 * PASSO, ate_pts: null }, "Apagar daqui em diante");
    expect(programa()).toEqual([q(45).t]);
  });

  it("âncora do programa intocada continua do programa num Simplificar seguinte; a do perito, nunca", async () => {
    await simplificar([k(30), k(45, 0.45)]);
    expect(programa()).toEqual([q(45).t]);
    // o perito marca no 60; o Simplificar seguinte devolve as de antes e uma nova no 70
    st().alterar("Marcar", (d) => void d.rastreios[0]!.ancoras.push(k(60, 0.5)));
    await simplificar([k(30), k(45, 0.45), k(60, 0.5), k(70, 0.55)]);
    expect(programa()).toEqual([q(45).t, q(70).t]);
    // devolvida no quadro do perito com outra caixa: é do programa (a do perito foi trocada)
    await simplificar([k(30, 0.31), k(45, 0.45), k(60, 0.5), k(70, 0.55)]);
    expect(programa()).toEqual([q(30).t, q(45).t, q(70).t]);
  });

  it("o registro das corridas do Simplificar vai ao documento no mesmo passo; sem o campo fica; vazio sai; sem âncora do programa sai junto", async () => {
    const registro = [{ motor: "vittrack" }, null] as unknown as Corrida[];
    cmd.edicaoRastreioEditar.mockResolvedValueOnce({ ancoras: [k(30), k(45, 0.45), k(60, 0.5)], denso: null, corridas_do_programa: registro });
    await editarRastreio("r1", { tipo: "simplificar", tolerancia: TOLERANCIA_SIMPLIFICAR }, "Simplificar");
    expect(r1().corridas_do_programa).toEqual(registro);
    st().desfazer();
    expect("corridas_do_programa" in r1()).toBe(false);
    st().refazer();
    expect(r1().corridas_do_programa).toEqual(registro);
    // edição que não mexe nas âncoras (o backend não manda o campo): fica
    cmd.edicaoRastreioEditar.mockResolvedValueOnce({ ancoras: null, denso: null });
    await editarRastreio("r1", { tipo: "apagar_faixa", de_pts: 90 * PASSO, ate_pts: null }, "Apagar daqui em diante");
    expect(r1().corridas_do_programa).toEqual(registro);
    // o perito mexe nas duas âncoras do programa: sem nenhuma, o registro sai junto (Ctrl+Z devolve)
    st().alterar("Mover", (d) => void aplicarMudanca(d as ProjetoEdicao, { rastreio: "r1", ancora: k(45, 0.46) }));
    expect(r1().corridas_do_programa).toEqual(registro);
    st().alterar("Mover", (d) => void aplicarMudanca(d as ProjetoEdicao, { rastreio: "r1", ancora: k(60, 0.51) }));
    expect(programa()).toBeUndefined();
    expect("corridas_do_programa" in r1()).toBe(false);
    st().desfazer();
    expect(r1().corridas_do_programa).toEqual(registro);
    // vazio do backend: sai
    cmd.edicaoRastreioEditar.mockResolvedValueOnce({ ancoras: [k(30)], denso: null, corridas_do_programa: [] });
    await editarRastreio("r1", { tipo: "apagar_faixa", de_pts: 40 * PASSO, ate_pts: null }, "Apagar daqui em diante");
    expect("corridas_do_programa" in r1()).toBe(false);
  });
});

describe("palco com o denso", () => {
  it("o palco usa a caixa do denso em cache (medida e incerta; ausente = nada)", () => {
    const caixa = (j: number) => ({ x: 0.1 + j / 1000, y: 0.2, w: 0.1, h: 0.2 });
    const linhas = Array.from({ length: 50 }, (_, i) => 30 + i).map((j) => ({ j, caixa: caixa(j), estado: j >= 60 && j <= 64 ? 1 : j === 70 ? 2 : 0 }));
    const D = densoDe("r1", linhas);
    const sha = "e".repeat(64);
    useRastreioStore.setState({ densos: { [sha]: D } });
    st().alterar("Seguir", (d) => {
      d.rastreios[0]!.denso = ponteiro(sha, D);
      d.rastreios[0]!.origem = { tipo: "assistido" };
    });
    const doc = st().doc!;
    const fontes = { sprite: () => null, atlas: () => null, midia: () => null };
    const no = (j: number) => quadroDaCena(doc, [], { ...q(j), indice: j, q: j, programa_s: q(j).t }, { w: 640, h: 360 }, fontes).rastreio("r1");
    expect(no(30)).toEqual(CAIXA);
    expect(no(50)).toEqual(caixa(50));
    expect(no(62)).toEqual(caixa(62));
    expect(no(70)).toBeNull();
    expect(no(85)).toBeNull();
  });
});
