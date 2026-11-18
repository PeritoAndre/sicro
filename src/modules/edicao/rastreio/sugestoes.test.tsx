// @vitest-environment happy-dom
/**
 * Aceite do P4.5 na tela (happy-dom, comandos e evento `edicao:deteccao` falsos): "Sugestões" → alvo pede a detecção
 * do trecho; o fim entra no projeto como rastreio SEM confirmação + ocultação da mídia; "Confirmar" só depois de a
 * folha de contato CARREGAR e grava quem, quando, o conteúdo e o intervalo; "Descartar" tira rastreio e ocultação;
 * Ctrl+Z desfaz; "Trechos fracos" vai ao intervalo fraco; "Parar"; com "Usar modelos de IA" desligado a seção some.
 */

import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { ArquivoDenso, ClipeTrecho, ContextoClipe, EventoDeteccao, ProjetoAberto, ProjetoEdicao, Rastreio, TabelaQuadros } from "@domain/edicao";

const ev = vi.hoisted(() => ({ ouvintes: new Map<string, (e: { payload: unknown }) => void>() }));
vi.mock("@tauri-apps/api/event", () => ({
  listen: vi.fn(async (nome: string, f: (e: { payload: unknown }) => void) => {
    ev.ouvintes.set(nome, f);
    return () => ev.ouvintes.delete(nome);
  }),
}));
vi.mock("@tauri-apps/api/core", () => ({ convertFileSrc: (p: string) => `asset://${p}`, invoke: vi.fn() }));
const cmd = vi.hoisted(() => ({
  edicaoAbrir: vi.fn(),
  edicaoSalvar: vi.fn(),
  edicaoAnalisar: vi.fn(),
  edicaoTabelaQuadros: vi.fn(),
  edicaoMidiaInfo: vi.fn(),
  edicaoDetectar: vi.fn(),
  edicaoDetectarCancelar: vi.fn(),
  edicaoRastreioTira: vi.fn(),
  edicaoRastreioLer: vi.fn(),
}));
vi.mock("@core/commands", () => ({ commands: cmd }));

import { useEdicaoStore } from "../store/edicaoStore";
import { esquecerRastreios } from "../store/rastreioStore";
import { useSettingsStore } from "@stores/settingsStore";
import { botao, clicar, desmontarTudo, esperar, lerFixture, montar, type Tela } from "../inspetor/testeTela";
import { confirmada, confirmarSugestao, esquecerSugestoes, proximoFraco, resumoDaSugestao, SugestoesSecao, trechosFracos, useSugestoesStore } from "./Sugestoes";
import { CELULAS } from "./FolhaContato";

/** Rótulos do §8.6 que a seção pode mostrar (sugestões, F4) e os do "Conteúdo" do Ocultar no menu. */
const ROTULOS = ["Sugestões", "Trechos fracos", "Confirmar", "Descartar", "Parar", "Rosto", "Placa", "Corpo"];
/** Botão pelo texto ou nome acessível; null quando não há. */
const talvez = (de: ParentNode, texto: string) => [...de.querySelectorAll<HTMLButtonElement>("button")].find((x) => (x.textContent ?? "").trim() === texto || x.getAttribute("aria-label") === texto) ?? null;

const WS = "/casos/c1";
const st = () => useEdicaoStore.getState();
const doc = () => st().doc!;
const trecho = () => doc().principal[1] as ClipeTrecho;
const TABELA: TabelaQuadros = { tb: { num: 1, den: 15360 }, pts0: 0, passos: Array<number>(599).fill(512), passo_mediano: 512, chaves: [0, 30, 60], vfr: false, origem: "decodificador" };
const SHA = (c: string) => c.repeat(64);
/** Instante dentro do trecho do clipe (as propostas cruzam o trecho). */
const T = (s: number) => trecho().inicio_s + s;

let tela: Tela;
let irPara: ReturnType<typeof vi.fn>;
let agora = 0;

function proposta(id: string, sha: string, de: number, ate: number): Rastreio {
  return {
    id,
    nome: `Placa ${id}`,
    cor: "#4fc3f7",
    midia: trecho().midia,
    base: "fonte",
    tipo: "caixa",
    espaco: "original",
    ancoras: [],
    denso: { sha256: sha, n: 30, de_s: de, ate_s: ate, ok: 30, incertos: 0, ausentes: 0 },
    silhuetas: null,
    ausencias: [],
    origem: { tipo: "deteccao", classe: "placa" },
    confirmacao: null,
  };
}

/** Denso de 30 quadros a 30 qps, a partir de `de`; os quadros 10..14 interpolados, o 20 incerto. */
function denso(rid: string, de: number): ArquivoDenso {
  const n = 30;
  const p0 = Math.round(de * 15360);
  return {
    esquema: 2, rastreio: rid, midia_sha256: SHA("a"), espaco: "original", pixel: { w: 1920, h: 1080 },
    pts: Array.from({ length: n }, (_, i) => p0 + 512 * i),
    x: Array<number>(n).fill(0.1), y: Array<number>(n).fill(0.1), w: Array<number>(n).fill(0.02), h: Array<number>(n).fill(0.05), conf: Array<number>(n).fill(0.9),
    estado: Array.from({ length: n }, (_, i) => (i === 20 ? 1 : 0)), tipo: Array.from({ length: n }, (_, i) => (i >= 10 && i < 15 ? 1 : 0)), corrida: Array<number>(n).fill(0),
    corridas: [],
  };
}

const emitir = async (e: EventoDeteccao) => esperar(() => ev.ouvintes.get("edicao:deteccao")?.({ payload: e }));
const linhas = () => [...tela.raiz.querySelectorAll<HTMLElement>("[data-sugestao]")];
const linha = (id: string) => tela.raiz.querySelector<HTMLElement>(`[data-sugestao="${id}"]`)!;
const menuItem = (texto: string) => [...document.querySelectorAll<HTMLElement>("[role=menuitem], button, li")].find((e) => (e.textContent ?? "").trim() === texto);

async function montarSecao() {
  const ctx = { tempo: () => agora, irPara } as unknown as ContextoClipe;
  tela = await montar(<SugestoesSecao clipe={trecho()} ctx={ctx} info={null} tabela={TABELA} />);
}

async function detectarPlacas(rastreios: Rastreio[]) {
  await clicar(botao(tela.raiz, "Sugestões"));
  const item = menuItem("Placa");
  expect(item, "item Placa no menu").toBeTruthy();
  await clicar(item!);
  expect(cmd.edicaoDetectar).toHaveBeenCalledTimes(1);
  expect(cmd.edicaoDetectar.mock.calls[0]![2]).toEqual({ clipe_id: trecho().id, classes: ["placa"] });
  await emitir({ tipo: "avanco", tarefa_id: "t1", feitos: 3, total: 10 });
  expect(tela.raiz.textContent).toContain("3/10");
  await emitir({ tipo: "fim", tarefa_id: "t1", clipe_id: trecho().id, midia_id: trecho().midia, rastreios, cancelada: false });
}

beforeEach(async () => {
  for (const f of Object.values(cmd)) f.mockReset();
  ev.ouvintes.clear();
  esquecerSugestoes();
  esquecerRastreios();
  agora = 0;
  irPara = vi.fn();
  cmd.edicaoSalvar.mockImplementation(async () => ({ updated_at: new Date().toISOString(), raiz_cache: "/cache/c1" }));
  cmd.edicaoAnalisar.mockImplementation(async () => ({ problemas: [], linha: { fps: { num: 30, den: 1 }, total_q: 0, clipes: [], livres: [] }, midias: [], prender: [], saida_auto: null }));
  cmd.edicaoTabelaQuadros.mockResolvedValue(TABELA);
  cmd.edicaoMidiaInfo.mockRejectedValue({ kind: "validation", message: "validation error: tabela_pendente: ainda não" });
  cmd.edicaoDetectar.mockResolvedValue("t1");
  cmd.edicaoDetectarCancelar.mockResolvedValue(undefined);
  cmd.edicaoRastreioTira.mockImplementation(async (_ws: string, _p: string, _m: string, sha: string, n: number) => ({ relativo: `tiras/${sha.slice(0, 8)}.jpg`, colunas: 6, tempos: Array.from({ length: Math.min(n, 12) }, (_, i) => T(i / 30)) }));
  cmd.edicaoRastreioLer.mockImplementation(async (_ws: string, _p: string, rid: string) => denso(rid, rid === "r1" ? T(0) : T(1)));
  const s = useSettingsStore.getState().settings;
  useSettingsStore.setState({ settings: { ...s, profile: { ...s.profile, full_name: "Perito Teste" }, ai: { ...s.ai, modelos_desligados: false } } });
  const projeto = lerFixture<ProjetoEdicao>("projeto-do-store.json");
  const r: ProjetoAberto = { projeto, importado_de: null, so_leitura: false, avisos: [], raiz_cache: "/cache/c1" };
  cmd.edicaoAbrir.mockResolvedValueOnce(r);
  await st().abrir(WS, projeto.id);
  await montarSecao();
});

afterEach(async () => {
  desmontarTudo();
  await st()
    .fechar({ descartar: true })
    .catch(() => {});
});

describe("Sugestões (ocultação assistida)", () => {
  it("propor → nasce sem confirmação → folha carregada → Confirmar; Descartar; Ctrl+Z", async () => {
    const m = trecho().midia;
    const antes = doc().midias.find((x) => x.id === m)!.ocultacoes.length;
    await detectarPlacas([proposta("r1", SHA("1"), T(0), T(0.9667)), proposta("r2", SHA("2"), T(1), T(1.9667))]);
    // no projeto: rastreios sem confirmação e uma ocultação da mídia por proposta (sólido, placa, no intervalo)
    const rs = doc().rastreios.filter((r) => r.origem.tipo === "deteccao");
    expect(rs.map((r) => [r.id, r.confirmacao])).toEqual([["r1", null], ["r2", null]]);
    expect(rs.every((r) => !confirmada(r))).toBe(true);
    const oc = doc().midias.find((x) => x.id === m)!.ocultacoes.slice(antes) as { tipo: string; modo: string; conteudo: string; ativo: unknown; alvo: { rastreio: string; margem: number } }[];
    expect(oc.map((o) => [o.tipo, o.modo, o.conteudo, o.alvo.rastreio, o.alvo.margem])).toEqual([["ocultar", "solido", "placa", "r1", 0], ["ocultar", "solido", "placa", "r2", 0]]);
    expect(oc[0]!.ativo).toEqual({ de: T(0), ate: T(0.9667) });
    expect(linhas().map((l) => l.getAttribute("data-sugestao"))).toEqual(["r1", "r2"]);
    expect(linhas().every((l) => l.querySelector('[data-estado-sugestao="pendente"]'))).toBe(true);

    // abrir a linha: a folha é pedida; "Confirmar" só depois de a imagem carregar
    await clicar(linha("r1").querySelector<HTMLElement>("button[aria-expanded]")!);
    await esperar();
    expect(cmd.edicaoRastreioTira).toHaveBeenCalledWith(WS, doc().id, expect.any(String), SHA("1"), CELULAS);
    const confirmar = () => botao(linha("r1"), "Confirmar") as HTMLButtonElement;
    expect(confirmar().disabled).toBe(true);
    await clicar(confirmar());
    expect(doc().rastreios.find((r) => r.id === "r1")!.confirmacao).toBeNull();
    const img = linha("r1").querySelector("img")!;
    expect(img.getAttribute("src")).toBe(`asset:///cache/c1/tiras/${SHA("1").slice(0, 8)}.jpg`);
    // célula leva ao instante dela
    await clicar(linha("r1").querySelector<HTMLElement>('[data-celula="2"]')!);
    expect(irPara).toHaveBeenLastCalledWith(T(2 / 30));
    await esperar(() => void img.dispatchEvent(new Event("load")));
    expect(confirmar().disabled).toBe(false);
    await clicar(confirmar());
    const r1 = doc().rastreios.find((r) => r.id === "r1")!;
    expect(r1.confirmacao).toMatchObject({ por: "Perito Teste", denso_sha256: SHA("1"), de_s: T(0), ate_s: T(0.9667) });
    expect(confirmada(r1)).toBe(true);
    expect(linha("r1").querySelector('[data-estado-sugestao="confirmada"]')).toBeTruthy();
    expect(talvez(linha("r1"), "Confirmar")).toBeNull();
    // a folha vista é a deste conteúdo: outro denso no mesmo rastreio volta a pedir
    expect(useSugestoesStore.getState().vistas[SHA("1")]).toBe(true);
    expect(useSugestoesStore.getState().vistas[SHA("2")]).toBeUndefined();

    // "Descartar" tira o rastreio e a ocultação dele; Ctrl+Z devolve
    await clicar(botao(linha("r2"), "Descartar"));
    expect(doc().rastreios.some((r) => r.id === "r2")).toBe(false);
    expect(doc().midias.find((x) => x.id === m)!.ocultacoes.some((o) => (o as { alvo?: { rastreio: string } }).alvo?.rastreio === "r2")).toBe(false);
    expect(linhas().map((l) => l.getAttribute("data-sugestao"))).toEqual(["r1"]);
    await esperar(() => st().desfazer());
    expect(doc().rastreios.some((r) => r.id === "r2")).toBe(true);
    // desfazer a confirmação volta ao estado sem ela; desfazer de novo tira as sugestões
    await esperar(() => st().desfazer());
    expect(doc().rastreios.find((r) => r.id === "r1")!.confirmacao).toBeNull();
    await esperar(() => st().desfazer());
    expect(doc().rastreios.some((r) => r.origem.tipo === "deteccao")).toBe(false);
    expect(doc().midias.find((x) => x.id === m)!.ocultacoes.length).toBe(antes);
  });

  it("Descartar tira a ocultação; o rastreio que o zoom acompanha fica", async () => {
    await detectarPlacas([proposta("r1", SHA("1"), T(0), T(0.9667)), proposta("r2", SHA("2"), T(1), T(1.9667))]);
    const z = { id: "z1", ligado: true, nome: "", ativo: null, tipo: "enquadramento", regiao: { x: 0, y: 0, w: 0.5, h: 0.5 }, ajuste: "cobrir",
      interpolacao: "bilinear", segue: { rastreio: "r2", suavizar_s: 1 } };
    await esperar(() => st().alterar("Acompanhar rastreio", (d) => void (d.principal[1] as ClipeTrecho).efeitos.push(z as unknown as ClipeTrecho["efeitos"][number])));
    await clicar(botao(linha("r2"), "Descartar"));
    expect(doc().rastreios.some((r) => r.id === "r2")).toBe(true);
    expect(doc().midias.flatMap((x) => x.ocultacoes).some((o) => (o as { alvo?: { rastreio: string } }).alvo?.rastreio === "r2")).toBe(false);
  });

  it("conteúdo trocado depois de confirmar: a folha vista de outro conteúdo não habilita; sem folha nada grava", async () => {
    await detectarPlacas([proposta("r1", SHA("1"), T(0), T(0.9667)), proposta("r2", SHA("2"), T(1), T(1.9667))]);
    const r = (id: string) => doc().rastreios.find((x) => x.id === id)!;
    // a guarda não é só o botão
    confirmarSugestao("r2");
    expect(r("r2").confirmacao).toBeNull();
    await clicar(linha("r1").querySelector<HTMLElement>("button[aria-expanded]")!);
    await esperar();
    await esperar(() => void linha("r1").querySelector("img")!.dispatchEvent(new Event("load")));
    await clicar(botao(linha("r1"), "Confirmar"));
    expect(confirmada(r("r1"))).toBe(true);
    // nova corrida ou edição: outro denso
    await esperar(() => st().alterar("Simplificar", (d) => void Object.assign(d.rastreios.find((x) => x.id === "r1")!.denso!, { sha256: SHA("3") })));
    expect(confirmada(r("r1"))).toBe(false);
    expect(linha("r1").querySelector('[data-estado-sugestao="pendente"]')).toBeTruthy();
    const confirmar = () => botao(linha("r1"), "Confirmar") as HTMLButtonElement;
    expect(confirmar().disabled).toBe(true);
    confirmarSugestao("r1");
    expect(r("r1").confirmacao!.denso_sha256).toBe(SHA("1"));
    await esperar();
    expect(cmd.edicaoRastreioTira).toHaveBeenLastCalledWith(WS, doc().id, expect.any(String), SHA("3"), CELULAS);
    await esperar(() => void linha("r1").querySelector("img")!.dispatchEvent(new Event("load")));
    expect(confirmar().disabled).toBe(false);
    await clicar(confirmar());
    expect(r("r1").confirmacao).toMatchObject({ denso_sha256: SHA("3") });
    expect(confirmada(r("r1"))).toBe(true);
  });

  it("Trechos fracos vai ao intervalo de baixa confiança; resumo da linha", async () => {
    await detectarPlacas([proposta("r1", SHA("1"), T(0), T(0.9667))]);
    await clicar(linha("r1").querySelector<HTMLElement>("button[aria-expanded]")!);
    await esperar();
    await esperar();
    // denso lido: 1 s, 0,02 × 1920 = 38 px de lado menor, 5 de 30 interpolados
    expect(linha("r1").querySelector('[data-dado="menor"]')!.textContent).toBe("38 px");
    expect(linha("r1").querySelector('[data-dado="interpolada"]')!.textContent).toBe("17 %");
    await clicar(botao(linha("r1"), "Trechos fracos"));
    const perto = (v: number) => (x: number) => Math.abs(x - v) < 1e-6;
    expect(perto(T(10 / 30))(irPara.mock.lastCall![0])).toBe(true);
    agora = T(0.5);
    await clicar(botao(linha("r1"), "Trechos fracos"));
    expect(perto(T(20 / 30))(irPara.mock.lastCall![0])).toBe(true);
    // puros
    const d = denso("x", 0);
    expect(trechosFracos(d, TABELA.tb)).toEqual([[10 / 30, 14 / 30], [20 / 30, 20 / 30]]);
    expect(proximoFraco(trechosFracos(d, TABELA.tb), 1)).toBe(10 / 30);
    expect(resumoDaSugestao(proposta("x", SHA("9"), 0, 1), d)).toEqual({ duracao: 1, menor: 38, interpolada: 5 / 30 });
  });

  it("Parar cancela; fim cancelado não grava; falha vai ao aviso; outro projeto não recebe", async () => {
    const n0 = doc().rastreios.length;
    await clicar(botao(tela.raiz, "Sugestões"));
    await clicar(menuItem("Rosto")!);
    expect(cmd.edicaoDetectar.mock.calls[0]![2]).toEqual({ clipe_id: trecho().id, classes: ["rosto"] });
    await esperar();
    expect(talvez(tela.raiz, "Sugestões")).toBeNull();
    await clicar(botao(tela.raiz, "Parar"));
    expect(cmd.edicaoDetectarCancelar).toHaveBeenCalledWith("t1");
    await emitir({ tipo: "fim", tarefa_id: "t1", clipe_id: trecho().id, midia_id: trecho().midia, rastreios: [], cancelada: true });
    expect(doc().rastreios.length).toBe(n0);
    expect(botao(tela.raiz, "Sugestões").hasAttribute("disabled")).toBe(false);
    // evento de outra tarefa não muda nada
    await emitir({ tipo: "fim", tarefa_id: "outra", clipe_id: trecho().id, midia_id: trecho().midia, rastreios: [proposta("rx", SHA("3"), T(0), T(1))], cancelada: false });
    expect(doc().rastreios.length).toBe(n0);
  });

  it("com \"Usar modelos de IA\" desligado a seção some; só rótulos do §8.6", async () => {
    expect(tela.raiz.textContent).toContain("Sugestões");
    for (const t of [...tela.raiz.querySelectorAll("button, h3")].map((b) => (b.textContent ?? "").trim()).filter(Boolean)) expect(ROTULOS).toContain(t);
    expect(tela.raiz.querySelector("[placeholder]")).toBeNull();
    const s = useSettingsStore.getState().settings;
    await esperar(() => useSettingsStore.setState({ settings: { ...s, ai: { ...s.ai, modelos_desligados: true } } }));
    expect(tela.raiz.textContent).not.toContain("Sugestões");
  });
});
