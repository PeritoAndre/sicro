// @vitest-environment happy-dom
/**
 * Clipe · Enquadramento (Fase 4, ZS) montado de verdade: chaves do zoom no mesmo trecho ("Chave no cursor", lista com
 * Cena inteira, Aproximar, Segurar, Afastar), a segunda chave diferente grava "Preencher", a chave aberta põe o palco
 * na vista de origem (efeito selecionado), "Acompanhar rastreio" e "Suavizar"; só rótulos de (h).
 */

import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { Caixa, Chave, ClipeCongelar, ClipeTrecho, EfEnquadramento, ProjetoAberto, ProjetoEdicao } from "@domain/edicao";

const cmd = vi.hoisted(() => ({
  edicaoAbrir: vi.fn(),
  edicaoSalvar: vi.fn(),
  edicaoAnalisar: vi.fn(),
  edicaoTabelaQuadros: vi.fn(),
  edicaoMidiaInfo: vi.fn(),
  edicaoCapacidades: vi.fn(),
}));
vi.mock("@core/commands", () => ({ commands: cmd }));
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn(async () => () => {}) }));

import { useEdicaoStore } from "../../store/edicaoStore";
import { aplicarMudanca } from "../../modelo/projeto";
import { comRegiao, proporcaoDaSaida } from "../../palco/alcas";
import { Inspetor } from "../Inspetor";
import { esquecerLeituras } from "../contexto";
import { removerEfeito } from "../acoes";
import { useInspetorUi } from "../ui";
import { clicar, desmontarTudo, digitar, esperar, escolher, lerFixture, montar, sair, textosVisiveis, type Tela } from "../testeTela";
import { textoPermitido } from "../rotulos.teste";

const st = () => useEdicaoStore.getState();
const doc = () => st().doc!;
const ultimo = () => st().passado.at(-1)?.rotulo;
const trecho = () => doc().principal[1] as ClipeTrecho;
const enq = () => trecho().efeitos.find((e) => (e as EfEnquadramento).tipo === "enquadramento") as EfEnquadramento | undefined;
const chaves = () => enq()!.regiao as Chave<Caixa>[];
const rotulos = (t: Tela) => [...t.raiz.querySelectorAll("[data-chave]")].map((li) => li.querySelector("button span:nth-child(2)")!.textContent);
const acao = (t: Tela, a: string) => t.raiz.querySelector<HTMLButtonElement>(`[data-acao="${a}"]`);

let tela: Tela;

beforeEach(async () => {
  vi.useFakeTimers({ toFake: ["Date"], now: new Date("2026-11-17T10:00:00Z") });
  for (const f of Object.values(cmd)) f.mockReset();
  cmd.edicaoSalvar.mockImplementation(async () => ({ updated_at: new Date().toISOString() }));
  cmd.edicaoAnalisar.mockImplementation(async () => ({ problemas: [], linha: { fps: { num: 30, den: 1 }, total_q: 0, clipes: [], livres: [] }, midias: [], prender: [], saida_auto: null }));
  cmd.edicaoTabelaQuadros.mockRejectedValue({ kind: "validation", message: "validation error: tabela_pendente: ainda não" });
  cmd.edicaoMidiaInfo.mockRejectedValue({ kind: "validation", message: "validation error: tabela_pendente: ainda não" });
  esquecerLeituras();
  useInspetorUi.setState({ aba: "clipe", ferramenta: null, aoMarcar: null, chave: null });
  const projeto = lerFixture<ProjetoEdicao>("projeto-do-store.json");
  const r: ProjetoAberto = { projeto, importado_de: null, so_leitura: false, avisos: [], raiz_cache: "/cache/c1" };
  cmd.edicaoAbrir.mockResolvedValueOnce(r);
  await st().abrir("/casos/c1", projeto.id);
  tela = await montar(<Inspetor />);
  const it = { k: "clipe" as const, id: trecho().id };
  await esperar(() => st().selecionar({ itens: [it], foco: it, efeito: null }));
});

afterEach(async () => {
  desmontarTudo();
  await st()
    .fechar({ descartar: true })
    .catch(() => {});
  vi.useRealTimers();
});

const naoHaFrase = () => expect(textosVisiveis(tela.raiz).filter((t) => !textoPermitido(t))).toEqual([]);
const cursor = (t: number) => esperar(() => st().posicionar({ modo: "clipe", clipe: trecho().id, t }));
/** O arrasto da alça da região no palco, no instante do cursor (a mesma porta do palco). */
const alca = (t: number, v: Caixa) => esperar(() => st().alterar("Marcar", (d) => void aplicarMudanca(d, { efeito: comRegiao(enq()!, v, t, null) })));

describe("Clipe · Enquadramento: zoom com chaves", () => {
  it("aproxima, segura e afasta no mesmo trecho; a segunda chave diferente grava Preencher; o Ajuste some", async () => {
    await esperar(() => removerEfeito(enq()!.id, "Remover"));
    expect(enq()).toBeUndefined();
    const nClipes = doc().principal.length;
    await cursor(12.5);
    await clicar(acao(tela, "chave-no-cursor")!);
    expect(chaves()).toEqual([{ t: 12.5, v: { x: 0, y: 0, w: 1, h: 1 } }]);
    expect([ultimo(), st().selecao.efeito]).toEqual(["Chave no cursor", enq()!.id]);
    expect(rotulos(tela)).toEqual(["Cena inteira"]);
    // uma chave só vale como fixa: o Ajuste aparece
    expect(tela.raiz.querySelector('[data-campo="Ajuste"]')).not.toBeNull();
    await cursor(13.5);
    await clicar(acao(tela, "chave-no-cursor")!);
    expect(rotulos(tela)).toEqual(["Cena inteira", "Cena inteira"]);
    expect(enq()!.ajuste).toBe("conter");
    // a alça do palco no cursor troca a chave do instante: aproximar
    await alca(13.5, { x: 0.25, y: 0.25, w: 0.5, h: 0.5 });
    expect(chaves().map((k) => k.t)).toEqual([12.5, 13.5]);
    expect(enq()!.ajuste).toBe("cobrir");
    expect(tela.raiz.querySelector('[data-campo="Ajuste"]')).toBeNull();
    await cursor(14.5);
    await clicar(acao(tela, "chave-no-cursor")!);
    await cursor(15.2);
    await clicar(acao(tela, "chave-no-cursor")!);
    await alca(15.2, { x: 0.1, y: 0.1, w: 0.8, h: 0.8 });
    expect(rotulos(tela)).toEqual(["Cena inteira", "Aproximar", "Segurar", "Afastar"]);
    const dados = [...tela.raiz.querySelectorAll("[data-chave] [data-dado]")].map((x) => x.textContent);
    expect(dados).toEqual(["1,0×", "00:00:12,500", "2,0×", "00:00:13,500", "2,0×", "00:00:14,500", "1,3×", "00:00:15,200"]);
    // o trecho não foi dividido
    expect(doc().principal.length).toBe(nClipes);
    // a chave do cursor fica acesa; clicar noutra leva o cursor a ela e abre a vista de origem
    expect(tela.raiz.querySelector('[data-chave="3"] button')!.getAttribute("aria-current")).toBe("true");
    await esperar(() => st().selecionar({ ...st().selecao, efeito: null }));
    await clicar(tela.raiz.querySelector<HTMLButtonElement>('[data-chave="1"] button')!);
    expect(st().cursor).toEqual({ modo: "clipe", clipe: trecho().id, t: 13.5 });
    expect(st().selecao.efeito).toBe(enq()!.id);
    // "Enquadramento" (ícone) liga e desliga a vista de origem
    await clicar(acao(tela, "editar-regiao")!);
    expect(st().selecao.efeito).toBeNull();
    naoHaFrase();
  });

  it("Marcar no palco grava a chave do instante na proporção da saída", async () => {
    await cursor(13);
    await clicar(acao(tela, "marcar-regiao")!);
    const f = useInspetorUi.getState().ferramenta!;
    expect(f).toMatchObject({ tipo: "regiao", efeito: enq()!.id, proporcao: 16 / 9 });
    // a caixa desenhada passou da borda e fora da proporção: entra na proporção e dentro da imagem
    await esperar(() => useInspetorUi.getState().marcado({ x: 0.8, y: 0.1, w: 0.4, h: 0.3 }, 13, null));
    const r = enq()!.regiao as Caixa;
    expect(r.x + r.w).toBeLessThanOrEqual(1 + 1e-12);
    // proporção da saída na origem girada (o trecho tem `girar`)
    expect(r.w / r.h).toBeCloseTo(proporcaoDaSaida(trecho().efeitos, { w: 1920, h: 1080 }, { w: 1920, h: 1080 })!, 9);
    expect(ultimo()).toBe("Marcar");
  });
});

describe("Clipe · Enquadramento: Acompanhar rastreio", () => {
  it("liga com o rastreio do destaque, Preencher e a suavização padrão; Suavizar; o rastreio fica enquanto o zoom o segue", async () => {
    const destaque = trecho().efeitos.find((e) => (e as { tipo: string }).tipo === "destaque") as { id: string; rastreio: string };
    await clicar(acao(tela, "acompanhar")!);
    expect(enq()!.segue).toEqual({ rastreio: destaque.rastreio, suavizar_s: 1 });
    expect([enq()!.ajuste, enq()!.regiao, ultimo()]).toEqual(["cobrir", { x: 0.25, y: 0.25, w: 0.5, h: 0.5 }, "Acompanhar rastreio"]);
    expect(acao(tela, "acompanhar")!.getAttribute("aria-pressed")).toBe("true");
    const sel = tela.raiz.querySelector<HTMLSelectElement>('select[aria-label="Acompanhar rastreio"]')!;
    expect(sel.value).toBe(destaque.rastreio);
    const suavizar = tela.raiz.querySelector<HTMLInputElement>('[data-campo="Suavizar"] input')!;
    await digitar(suavizar, "2,5");
    await sair(suavizar);
    expect(enq()!.segue!.suavizar_s).toBe(2.5);
    // tirar o destaque não leva o rastreio que o zoom segue
    await esperar(() => removerEfeito(destaque.id, "Remover"));
    expect(doc().rastreios.some((r) => r.id === destaque.rastreio)).toBe(true);
    await escolher(sel, destaque.rastreio);
    naoHaFrase();
    // desliga
    await clicar(acao(tela, "acompanhar")!);
    expect(enq()!.segue).toBeUndefined();
    expect(tela.raiz.querySelector('[data-campo="Suavizar"]')).toBeNull();
  });

  it("sem rastreio no trecho, desligado; no quadro congelado, não aparece", async () => {
    await esperar(() => removerEfeito((trecho().efeitos.find((e) => (e as { tipo: string }).tipo === "destaque") as { id: string }).id, "Remover"));
    expect(acao(tela, "acompanhar")!.disabled).toBe(true);
    const cong: ClipeCongelar = { tipo: "congelar", id: "cg1", rotulo: "", desligado: false, transicao_entrada: null, efeitos: [], midia: "m1", instante_s: 13, instante_pts: null, duracao_s: 2 };
    await esperar(() => st().alterar("x", (d) => void d.principal.push(cong)));
    const it = { k: "clipe" as const, id: "cg1" };
    await esperar(() => st().selecionar({ itens: [it], foco: it, efeito: null }));
    expect(acao(tela, "chave-no-cursor")).not.toBeNull();
    expect(acao(tela, "acompanhar")).toBeNull();
  });
});
