// @vitest-environment happy-dom
/**
 * Aceite do P2.4 (§11, Fase 2) no editor montado (happy-dom), com o comando e o evento `edicao:rastreio` falsos:
 * marcar → seguir (a miniatura acompanha) → "Incerto" → "Perdido em…" → recolocar → "Retomar" → Ctrl+Z;
 * "Aceitar ausência"; "Próximo trecho fraco" (botão e N); Ctrl+→ e Esc; "Da análise". Só rótulos do §8.6.
 * O palco do happy-dom não tem tamanho: a `FerramentaSeguir` é montada também ao lado, com um quadro de 640×360.
 */

import { act } from "react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { Analise, EventoRastreio, ProjetoAberto, Rastreio } from "@domain/edicao";

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
import { FerramentaSeguir } from "./FerramentaSeguir";
import { exibido } from "../modelo/tempo";
import { CAIXA, densoDe, faixa, info, PASSO, ponteiro, projeto, q, SHA, tab, WS } from "./cenario.teste";

const st = () => useEdicaoStore.getState();
const r1 = (): Rastreio => st().doc!.rastreios.find((r) => r.id === "r1")!;
const espera = (ms: number) => esperar(() => new Promise((r) => setTimeout(r, ms)));
const tecla = (key: string, code: string, x: Partial<KeyboardEventInit> = {}) =>
  act(async () => void window.dispatchEvent(new KeyboardEvent("keydown", { key, code, bubbles: true, ...x })));
const emitir = (e: EventoRastreio) => act(async () => ev.ouvintes.get("edicao:rastreio")!({ payload: e }));
/** Quadro do cursor (o palco põe o cursor no meio do quadro mostrado). */
const quadro = () => {
  const c = st().cursor;
  return c.modo === "clipe" && c.clipe === "c" ? exibido(tab, c.t) : null;
};
const ir = (j: number) => act(async () => st().posicionar({ modo: "clipe", clipe: "c", t: q(j).t }));

const A = densoDe("r1", [...faixa(30, 59), ...faixa(60, 64, 1), ...faixa(65, 79)]);
const B = densoDe("r1", [...faixa(30, 59), ...faixa(60, 64, 1), ...faixa(65, 79), ...faixa(95, 199)]);
const SHA_A = "a".repeat(64);
const SHA_B = "b".repeat(64);
const avanco = (tarefa: string, j: number, estado = 0): EventoRastreio => ({
  tipo: "avanco", tarefa_id: tarefa, rastreio_id: "r1", sentido: "frente", pts: j * PASSO, caixa: { ...CAIXA, x: 0.4 + (j - 30) / 1000 }, conf: estado === 1 ? 0.35 : 0.9, estado, feitos: j - 30, miniatura: `data:image/jpeg;base64,Q${j}`,
});

let editor: Tela;
let palco: Tela;
const painel = () => editor.raiz.querySelector<HTMLElement>('[data-rastreio="r1"]');
const acao = (raiz: ParentNode, a: string) => raiz.querySelector<HTMLButtonElement>(`[data-acao="${a}"]`);

beforeEach(async () => {
  vi.useFakeTimers({ toFake: ["Date"], now: new Date("2026-11-15T10:00:00Z") });
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
  cmd.edicaoRastreioLer.mockImplementation(async (_w: string, _p: string, _r: string, sha: string) => ({ [SHA_A]: A, [SHA_B]: B })[sha]);
  esquecerLeituras();
  esquecerRastreios();
  useInspetorUi.setState({ aba: "clipe", ferramenta: null, aoMarcar: null, chave: null });
  useSettingsStore.setState((s) => ({ settings: { ...s.settings, profile: { ...s.settings.profile, full_name: "Perito Teste" } } }));
  const p = projeto();
  cmd.edicaoAbrir.mockResolvedValue({ projeto: p, importado_de: null, so_leitura: false, avisos: [], raiz_cache: "/cache/r" } satisfies ProjetoAberto);
  await st().abrir(WS, p.id);
  useDialogosAbertos.setState({ pilha: [] });
  garantirMidia(WS, SHA);
  useRecursos.setState((s) => ({ midias: { [SHA]: { info, tabela: tab, erro: null, pedindo: false } }, versao: s.versao + 1 }));
  editor = await montar(<EditorEdicao ativo />);
  palco = await montar(<FerramentaSeguir area={{ x: 0, y: 0, w: 640, h: 360 }} />);
  await act(async () => selecionarClipe("c"));
  await espera(0);
  const v = editor.raiz.querySelector<HTMLVideoElement>("video[data-elemento]");
  if (v) {
    await act(async () => void v.dispatchEvent(new Event("loadeddata")));
    await act(async () => void v.dispatchEvent(new Event("seeked")));
  }
  await act(async () => selecionarEfeito("e1"));
  await ir(30);
  await espera(20);
});

afterEach(async () => {
  desmontarTudo();
  useFundoStore.setState({ tarefas: [] });
  await st()
    .fechar({ descartar: true })
    .catch(() => {});
  vi.useRealTimers();
});

/** Todo texto visível do painel é rótulo do §8.6 ou dado. */
const conferirTextos = () => {
  const fora = textosVisiveis(painel()!).filter((t) => !textoPermitido(t));
  expect(fora).toEqual([]);
  expect(painel()!.querySelector("[placeholder]")).toBeNull();
};

/** marcar → Seguir no palco → andamentos (miniatura e caixa) → "Incerto" → fim perdido no 80. */
async function ateOPerdido() {
  expect(painel()).not.toBeNull();
  // marcar: a ferramenta liga; o palco marca a caixa no quadro 30 e segue ligada
  await clicar(acao(editor.raiz, "marcar")!);
  expect(useInspetorUi.getState().chave).toBe("e1:caixa");
  const marcada = { x: 0.42, y: 0.4, w: 0.1, h: 0.2 };
  await act(async () => receberMarca(marcada, q(30).t, q(30).pts));
  expect(r1().ancoras).toEqual([{ ...q(30), v: marcada }]);
  expect(useInspetorUi.getState().ferramenta).not.toBeNull();
  // "Seguir" no próprio palco
  cmd.edicaoRastreioSeguir.mockResolvedValueOnce("t1");
  const seguir = palco.raiz.querySelector<HTMLButtonElement>('[data-seguir-palco] [data-acao="seguir"]')!;
  expect(seguir.disabled).toBe(false);
  await clicar(seguir);
  expect(cmd.edicaoRastreioSeguir.mock.calls[0]!.slice(2)).toEqual(["r1", "c", { ...q(30), v: marcada }, "frente", q(199).t, "auto"]);
  expect(cmd.edicaoRastreioSeguir.mock.calls[0]![1]).toBe(st().doc);
  // corrida: "Parar" no painel e no palco; Seguir some
  expect(acao(painel()!, "parar-rastreio")).not.toBeNull();
  expect(acao(painel()!, "seguir")).toBeNull();
  // a miniatura acompanha: cada andamento troca a imagem e a caixa
  await emitir(avanco("t1", 31));
  const img = () => palco.raiz.querySelector<HTMLImageElement>("img[data-miniatura]")!;
  const caixa = () => palco.raiz.querySelector<HTMLElement>("[data-caixa-seguida]")!;
  expect(img().getAttribute("src")).toBe("data:image/jpeg;base64,Q31");
  expect([caixa().style.left, caixa().style.top, caixa().style.width, caixa().style.height]).toEqual([`${(0.4 + 1 / 1000) * 640}px`, "144px", "64px", "72px"]);
  await emitir(avanco("t1", 45));
  expect(img().getAttribute("src")).toBe("data:image/jpeg;base64,Q45");
  expect(caixa().style.left).toBe(`${(0.4 + 15 / 1000) * 640}px`);
  expect(palco.raiz.textContent).toContain("00:00:01,500");
  expect(painel()!.textContent).toContain("00:00:01,500");
  // "Incerto": ícone, com o rótulo só no title
  await emitir(avanco("t1", 61, 1));
  for (const raiz of [palco.raiz, painel()!]) expect(raiz.querySelector('[data-estado-rastreio="incerto"]')?.getAttribute("title")).toBe("Incerto");
  conferirTextos();
  // fim: perdeu no 80
  await emitir({ tipo: "fim", tarefa_id: "t1", rastreio_id: "r1", parou: [{ sentido: "frente", motivo: "perdido", pts: 80 * PASSO }], denso: ponteiro(SHA_A, A), silhuetas: null });
  await espera(0);
  expect(palco.raiz.querySelector("[data-seguindo]")).toBeNull();
  expect(r1().denso?.sha256).toBe(SHA_A);
  expect(r1().origem).toEqual({ tipo: "assistido" });
  expect(quadro()).toBe(80);
}

describe("aceite P2.4: marcar → seguir → incerto → perdido → recolocar → retomar → Ctrl+Z", () => {
  it("o fluxo inteiro, pela tela", async () => {
    await ateOPerdido();
    // "Perdido em…" (tempo do quadro onde perdeu) e "Retomar" desligado até recolocar
    expect(botao(painel()!, "Perdido em 00:00:02,667")).toBeTruthy();
    expect(acao(painel()!, "retomar")!.disabled).toBe(true);
    conferirTextos();
    // recolocar: a ferramenta ficou ligada; o perito avança até o alvo reaparecer e marca no 95
    expect(useInspetorUi.getState().chave).toBe("e1:caixa");
    await ir(95);
    const nova = { x: 0.6, y: 0.42, w: 0.1, h: 0.2 };
    await act(async () => receberMarca(nova, q(95).t, q(95).pts));
    expect(r1().ancoras.map((k) => k.pts)).toEqual([30 * PASSO, 95 * PASSO]);
    expect(acao(painel()!, "retomar")!.disabled).toBe(false);
    // Retomar: da âncora recolocada, até o fim do efeito
    cmd.edicaoRastreioSeguir.mockResolvedValueOnce("t2");
    await clicar(acao(painel()!, "retomar")!);
    expect(cmd.edicaoRastreioSeguir.mock.calls[1]!.slice(2)).toEqual(["r1", "c", { ...q(95), v: nova }, "frente", q(199).t, "auto"]);
    await emitir(avanco("t2", 120));
    await emitir({ tipo: "fim", tarefa_id: "t2", rastreio_id: "r1", parou: [{ sentido: "frente", motivo: "fim", pts: 199 * PASSO }], denso: ponteiro(SHA_B, B), silhuetas: null });
    await espera(0);
    expect(r1().denso?.sha256).toBe(SHA_B);
    expect(painel()!.textContent).not.toContain("Perdido em");
    // Ctrl+Z: o ponteiro volta ao da primeira corrida, e com ele o "Perdido em…"
    const passos = st().passado.length;
    await tecla("z", "KeyZ", { ctrlKey: true });
    expect(st().passado.length).toBe(passos - 1);
    expect(r1().denso?.sha256).toBe(SHA_A);
    expect(botao(painel()!, "Perdido em 00:00:02,667")).toBeTruthy();
    // Ctrl+Y: de novo o da segunda
    await tecla("y", "KeyY", { ctrlKey: true });
    expect(r1().denso?.sha256).toBe(SHA_B);
  });

  it('"Perdido em…" leva o cursor ao quadro onde perdeu', async () => {
    await ateOPerdido();
    await ir(40);
    await clicar(botao(painel()!, "Perdido em 00:00:02,667"));
    expect(quadro()).toBe(80);
  });
});

describe('aceite P2.4: "Aceitar ausência" e "Próximo trecho fraco"', () => {
  it("Aceitar ausência → Oculto: o trecho sem caixa em volta do cursor, com quem e quando; onde há caixa, nada a aceitar", async () => {
    await ateOPerdido();
    // num quadro medido não há o que aceitar
    await ir(70);
    expect(acao(painel()!, "aceitar-ausencia")!.disabled).toBe(true);
    await ir(85);
    await clicar(acao(painel()!, "aceitar-ausencia")!);
    const menu = editor.raiz.querySelector<HTMLElement>('[role="menu"]')!;
    expect([...menu.querySelectorAll('[role="menuitem"]')].map((b) => b.textContent?.trim())).toEqual(["Oculto", "Fora de cena", "Perdido"]);
    await clicar(botao(menu, "Oculto"));
    expect(r1().ausencias).toEqual([{ de_s: q(80).t, ate_s: q(199).t, motivo: "oculto", aceita_em: "2026-11-15T10:00:00.000Z", por: "Perito Teste" }]);
    expect(st().passado.at(-1)?.rotulo).toBe("Aceitar ausência");
    expect(acao(painel()!, "aceitar-ausencia")!.disabled).toBe(true);
    // recolocado no 95: a interpolação 80..94 tem caixa (o motor desenha); ali não se aceita ausência
    await act(async () => receberMarca(CAIXA, q(95).t, q(95).pts));
    await ir(85);
    expect(acao(painel()!, "aceitar-ausencia")!.disabled).toBe(true);
  });

  it("Próximo trecho fraco (botão e N): o incerto do 60, depois o que ficou sem o alvo depois da âncora 95, e volta", async () => {
    await ateOPerdido();
    await act(async () => receberMarca(CAIXA, q(95).t, q(95).pts));
    await act(async () => useInspetorUi.getState().cancelarMarcacao());
    await ir(30);
    await clicar(acao(painel()!, "trecho-fraco")!);
    expect(quadro()).toBe(60);
    // 80..94 interpolado entre as âncoras (fraco); depois da 95, sem denso, nada
    await tecla("n", "KeyN");
    expect(quadro()).toBe(80);
    await tecla("n", "KeyN");
    expect(quadro()).toBe(96);
    await tecla("n", "KeyN");
    expect(quadro()).toBe(0);
  });
});

describe("atalhos e Da análise", () => {
  it("Ctrl+→ segue do efeito aberto; Esc para a corrida pela tarefa", async () => {
    cmd.edicaoRastreioSeguir.mockResolvedValueOnce("t3");
    cmd.edicaoRastreioCancelar.mockResolvedValue(undefined);
    await ir(50);
    await tecla("ArrowRight", "ArrowRight", { ctrlKey: true });
    await espera(0);
    expect(cmd.edicaoRastreioSeguir.mock.calls[0]!.slice(2, 6)).toEqual(["r1", "c", { ...q(30), v: CAIXA }, "frente"]);
    expect(useRastreioStore.getState().corrida?.tarefa).toBe("t3");
    await tecla("Escape", "Escape");
    expect(cmd.edicaoRastreioCancelar).toHaveBeenCalledWith("t3");
    // Ctrl+← com corrida em andamento: nada
    await tecla("ArrowLeft", "ArrowLeft", { ctrlKey: true });
    expect(cmd.edicaoRastreioSeguir).toHaveBeenCalledTimes(1);
  });

  it("Da análise: lista as trilhas (rótulo é dado) e aponta o efeito para o rastreio novo", async () => {
    cmd.pessoasListar.mockResolvedValue({ run: null, tamanho: null, pessoas: [{ id: "p3", numero: 3, nome: "" }, { id: "p7", numero: 7, nome: "Motociclista" }] });
    const novo: Rastreio = { ...r1(), id: "r9", ancoras: [], origem: { tipo: "pessoas", pessoa_id: "p7", estado: "revisada", copiado_em: "2026-11-15T09:00:00Z" }, denso: ponteiro(SHA_A, A) };
    cmd.edicaoRastreioDePessoas.mockResolvedValue(novo);
    await clicar(acao(painel()!, "da-analise")!);
    await espera(0);
    expect(cmd.pessoasListar).toHaveBeenCalledWith(WS, SHA);
    const menu = editor.raiz.querySelector<HTMLElement>('[role="menu"]')!;
    expect([...menu.querySelectorAll('[role="menuitem"]')].map((b) => b.textContent?.trim())).toEqual(["#3", "Motociclista"]);
    await clicar(botao(menu, "Motociclista"));
    await espera(0);
    expect(cmd.edicaoRastreioDePessoas).toHaveBeenCalledWith(WS, st().aberta!.projetoId, "m", SHA, "p7");
    expect(st().doc!.rastreios.map((r) => r.id)).toEqual(["r9"]);
    expect(editor.raiz.querySelector('[data-rastreio="r9"]')).not.toBeNull();
    expect(st().passado.at(-1)?.rotulo).toBe("Da análise");
    // a trilha mudou na análise: "Atualizar" traz de novo a mesma pessoa
    expect(acao(editor.raiz, "atualizar-rastreio")).toBeNull();
    const problema = { nivel: "aviso", codigo: "retrato_desatualizado", mensagem: "", clipe: "c", efeito: "r9", trilha: null, midia: null, campo: null, n: null };
    await act(async () => useEdicaoStore.setState({ analise: { problemas: [problema], linha: { clipes: [], livres: [] }, midias: [], prender: [], saida_auto: null } as unknown as Analise }));
    cmd.edicaoRastreioDePessoas.mockResolvedValue({ ...novo, origem: { ...novo.origem, copiado_em: "2026-11-15T10:00:00Z" } });
    await clicar(acao(editor.raiz, "atualizar-rastreio")!);
    await espera(0);
    expect(cmd.edicaoRastreioDePessoas).toHaveBeenLastCalledWith(WS, st().aberta!.projetoId, "m", SHA, "p7");
    expect(st().passado.at(-1)?.rotulo).toBe("Atualizar");
  });
});

describe("botões da linha do efeito e do palco", () => {
  it("Parar pelo palco e pelo painel cancela pela tarefa", async () => {
    cmd.edicaoRastreioSeguir.mockResolvedValueOnce("t5");
    cmd.edicaoRastreioCancelar.mockResolvedValue(undefined);
    await clicar(acao(painel()!, "seguir")!);
    await espera(0);
    await clicar(acao(palco.raiz, "parar-rastreio")!);
    expect(cmd.edicaoRastreioCancelar).toHaveBeenCalledTimes(1);
    expect(cmd.edicaoRastreioCancelar).toHaveBeenLastCalledWith("t5");
    await clicar(acao(painel()!, "parar-rastreio")!);
    expect(cmd.edicaoRastreioCancelar).toHaveBeenCalledTimes(2);
    expect(cmd.edicaoRastreioCancelar).toHaveBeenLastCalledWith("t5");
  });

  it("Seguir para trás no palco, com a ferramenta de marcar ligada", async () => {
    await clicar(acao(editor.raiz, "marcar")!);
    await ir(120);
    await act(async () => receberMarca(CAIXA, q(120).t, q(120).pts));
    cmd.edicaoRastreioSeguir.mockResolvedValueOnce("t6");
    await clicar(palco.raiz.querySelector<HTMLButtonElement>('[data-seguir-palco] [data-acao="seguir-tras"]')!);
    expect(cmd.edicaoRastreioSeguir.mock.calls[0]!.slice(2)).toEqual(["r1", "c", { ...q(120), v: CAIXA }, "tras", q(0).t, "auto"]);
  });

  it("Apagar daqui em diante e Simplificar pelos botões: o pts do quadro do cursor e a tolerância", async () => {
    await ateOPerdido();
    await ir(70);
    cmd.edicaoRastreioEditar.mockResolvedValueOnce({ ancoras: null, denso: ponteiro(SHA_B, B) });
    await clicar(acao(painel()!, "apagar-daqui")!);
    await espera(0);
    expect(cmd.edicaoRastreioEditar.mock.calls[0]!.slice(2)).toEqual(["r1", { tipo: "apagar_faixa", de_pts: 70 * PASSO, ate_pts: null }]);
    expect(r1().denso?.sha256).toBe(SHA_B);
    cmd.edicaoRastreioEditar.mockResolvedValueOnce({ ancoras: [{ ...q(30), v: CAIXA }], denso: null });
    await clicar(acao(painel()!, "simplificar")!);
    await espera(0);
    expect(cmd.edicaoRastreioEditar.mock.calls[1]!.slice(2)).toEqual(["r1", { tipo: "simplificar", tolerancia: 0.04 }]);
    expect(st().passado.slice(-2).map((p) => p.rotulo)).toEqual(["Apagar daqui em diante", "Simplificar"]);
  });

  it("Fora de cena: ícone (só title) no andamento; parada fora do quadro mostra o tempo, sem 'Perdido em'", async () => {
    cmd.edicaoRastreioSeguir.mockResolvedValueOnce("t7");
    await clicar(acao(painel()!, "seguir")!);
    await emitir(avanco("t7", 70, 2));
    for (const raiz of [palco.raiz, painel()!]) expect(raiz.querySelector('[data-estado-rastreio="fora"]')?.getAttribute("title")).toBe("Fora de cena");
    conferirTextos();
    await emitir({ tipo: "fim", tarefa_id: "t7", rastreio_id: "r1", parou: [{ sentido: "frente", motivo: "saiu_do_quadro", pts: 80 * PASSO }], denso: ponteiro(SHA_A, A), silhuetas: null });
    await espera(0);
    const onde = acao(painel()!, "onde-parou")!;
    expect(onde.getAttribute("title")).toBe("Fora de cena");
    expect(onde.textContent?.trim()).toBe("00:00:02,667");
    expect(painel()!.textContent).not.toContain("Perdido em");
    expect(acao(painel()!, "retomar")).not.toBeNull();
    conferirTextos();
  });
});
