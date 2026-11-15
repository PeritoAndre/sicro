// @vitest-environment happy-dom
/**
 * Holofote e Ocultar no Inspetor (P2.6), montados de verdade: "Adicionar → Holofote" cria o rastreio e o holofote num
 * passo; cada campo do §8.6 é um passo com o rótulo dele; "Polígono" ganha vértices e as alças deles no palco;
 * "Conteúdo" do Ocultar põe o padrão do conteúdo (o mesmo do Rust, pela fixture); só rótulos do §8.6.
 */

import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { Alca, Caixa, ClipeTrecho, EfHolofote, EfOcultar, ProjetoAberto, ProjetoEdicao, QuadroPalco } from "@domain/edicao";

const cmd = vi.hoisted(() => ({
  edicaoAbrir: vi.fn(),
  edicaoSalvar: vi.fn(),
  edicaoAnalisar: vi.fn(),
  edicaoExcluir: vi.fn(),
  edicaoTabelaQuadros: vi.fn(),
  edicaoMidiaInfo: vi.fn(),
  edicaoMidiaImportar: vi.fn(),
  edicaoCapacidades: vi.fn(),
  edicaoGravacaoSalvar: vi.fn(),
}));
vi.mock("@core/commands", () => ({ commands: cmd }));
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn(async () => () => {}) }));
vi.mock("@tauri-apps/plugin-dialog", () => ({ open: vi.fn(async () => null) }));
vi.mock("@components/Dialog/ask", () => ({ askConfirm: vi.fn(async () => true) }));

import { useEdicaoStore } from "../../store/edicaoStore";
import { Inspetor } from "../Inspetor";
import { esquecerLeituras } from "../contexto";
import { ADICIONAR, EDITAVEIS, REGISTRO } from "../registro";
import { useInspetorUi } from "../ui";
import { arrastar, campo, clicar, desmontarTudo, digitar, entradaDe, escolher, esperar, lerFixture as ler, montar, textosVisiveis, type Tela } from "../testeTela";
import { textoPermitido } from "../rotulos.teste";
import { HOLOFOTE_PRONTO, HolofoteEditor } from "./HolofoteEditor";
import { alcasDoAlvo, VERTICES_PADRAO } from "./alvoRegiao";
import { padraoDoConteudo } from "./ocultarPadrao";

const st = () => useEdicaoStore.getState();
const passos = () => st().passado.length;
const ultimo = () => st().passado.at(-1)?.rotulo;
const trecho = () => st().doc!.principal[1] as ClipeTrecho;
const holofote = () => trecho().efeitos.find((e) => (e as EfHolofote).tipo === "holofote") as EfHolofote | undefined;
/** Linha aberta do efeito do tipo (a do efeito selecionado). */
const linha = (tipo: string) => [...document.querySelectorAll<HTMLElement>(`[data-efeito="${tipo}"]`)].find((l) => /efeitoAberto/.test(l.className))!;

let tela: Tela;

beforeEach(async () => {
  vi.useFakeTimers({ toFake: ["Date"], now: new Date("2026-11-15T10:00:00Z") });
  for (const f of Object.values(cmd)) f.mockReset();
  cmd.edicaoSalvar.mockImplementation(async () => ({ updated_at: new Date().toISOString() }));
  cmd.edicaoAnalisar.mockImplementation(async () => ({ problemas: [], linha: { fps: { num: 30, den: 1 }, total_q: 0, clipes: [], livres: [] }, midias: [], prender: [], saida_auto: null }));
  cmd.edicaoTabelaQuadros.mockRejectedValue({ kind: "validation", message: "validation error: tabela_pendente: ainda não" });
  cmd.edicaoMidiaInfo.mockRejectedValue({ kind: "validation", message: "validation error: tabela_pendente: ainda não" });
  esquecerLeituras();
  useInspetorUi.setState({ aba: "clipe", ferramenta: null, aoMarcar: null, chave: null });
  const projeto = ler<ProjetoEdicao>("projeto-do-store.json");
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

async function adicionar(rotulo: string) {
  await clicar(tela.raiz.querySelector<HTMLButtonElement>('[data-acao="adicionar"]')!);
  const item = [...document.querySelectorAll<HTMLElement>("button, [role=menuitem]")].find((x) => x.textContent?.trim() === rotulo);
  expect(item).toBeTruthy();
  await clicar(item!);
  await esperar();
}

const semFrase = (raiz: HTMLElement) => {
  expect(textosVisiveis(raiz).filter((t) => !textoPermitido(t))).toEqual([]);
  expect(raiz.querySelectorAll("[placeholder]").length).toBe(0);
};

/** Quadro do palco com A = identidade e o rastreio `r1` na caixa dada. */
function quadro(caixa: Caixa): QuadroPalco {
  return {
    q: 0, t: 0, pts: 0, indice: 0, programa_s: 0, A: [1, 0, 0, 1, 0, 0], canvas: { w: 1920, h: 1080 },
    rastreio: (id) => (id === "r1" ? caixa : null), sprite: () => null, atlas: () => null, midia: () => null,
  };
}

describe("Holofote", () => {
  it("ligado: editor próprio e no Adicionar, entre Ocultar e Lupa (§8.6)", () => {
    expect(HOLOFOTE_PRONTO).toBe(true);
    expect(ADICIONAR.slice(0, 3)).toEqual(["destaque", "ocultar", "holofote"]);
    expect(REGISTRO.holofote.rotulo).toBe("Holofote");
    expect(REGISTRO.holofote.Editor).toBe(HolofoteEditor);
    expect(EDITAVEIS.has("holofote")).toBe(true);
  });

  it("Adicionar → Holofote: rastreio novo com a âncora no cursor e o holofote apontando para ele, um passo, aberto", async () => {
    const n = passos();
    const nr = st().doc!.rastreios.length;
    await adicionar("Holofote");
    expect([passos(), ultimo()]).toEqual([n + 1, "Holofote"]);
    const h = holofote()!;
    expect(h).toMatchObject({ tipo: "holofote", ligado: true, ativo: null, escurecer: 0.6, dessaturar: 0, alvo: { forma: "elipse", vertices: null, margem: 0.1, borda_suave: 0.1, inverter: false } });
    const r = st().doc!.rastreios.find((x) => x.id === h.alvo.rastreio)!;
    expect(st().doc!.rastreios.length).toBe(nr + 1);
    expect(r).toMatchObject({ midia: trecho().midia, base: "fonte", origem: { tipo: "manual" } });
    expect(r.ancoras.map((a) => a.t)).toEqual([12]);
    expect(st().selecao.efeito).toBe(h.id);
    expect(linha("holofote").className).toMatch(/efeitoAberto/);
    // desfazer tira o rastreio e o holofote juntos
    await esperar(() => st().desfazer());
    expect([holofote(), st().doc!.rastreios.length]).toEqual([undefined, nr]);
  });

  it("cada campo é um passo com o rótulo do §8.6; Polígono nasce com os cantos da caixa", async () => {
    await adicionar("Holofote");
    const l = () => linha("holofote");
    await clicar(campo(l(), "Forma").querySelector<HTMLButtonElement>('[data-valor="poligono"]')!);
    expect([holofote()!.alvo.forma, holofote()!.alvo.vertices, ultimo()]).toEqual(["poligono", VERTICES_PADRAO, "Forma"]);
    await clicar(campo(l(), "Forma").querySelector<HTMLButtonElement>('[data-valor="retangulo"]')!);
    // os vértices ficam (voltar ao Polígono não perde o desenho)
    expect([holofote()!.alvo.forma, holofote()!.alvo.vertices]).toEqual(["retangulo", VERTICES_PADRAO]);
    await digitar(entradaDe(l(), "Margem") as HTMLInputElement, "25");
    expect([holofote()!.alvo.margem, ultimo()]).toEqual([0.25, "Margem"]);
    await clicar(campo(l(), "Inverter").querySelector("input")!);
    expect([holofote()!.alvo.inverter, ultimo()]).toEqual([true, "Inverter"]);
    for (const [rotulo, chave, v] of [["Borda suave", "borda_suave", 0.3], ["Escurecer", "escurecer", 0.85], ["Dessaturar", "dessaturar", 1]] as const) {
      const n = passos();
      await arrastar(campo(l(), rotulo).querySelector<HTMLInputElement>("input[type=range]")!, [v / 2, v]);
      const h = holofote()!;
      expect([chave === "borda_suave" ? h.alvo.borda_suave : h[chave], passos(), ultimo()]).toEqual([v, n + 1, rotulo]);
    }
    semFrase(l());
  });

  it("Polígono: uma alça por vértice, no canvas; arrastar move só aquele vértice, em fração da caixa com a margem", () => {
    const c: Caixa = { x: 0.2, y: 0.3, w: 0.4, h: 0.2 };
    const daCaixa: Alca[] = [{ id: "r1:mover", tipo: "mover", em: { x: 0.4, y: 0.4 }, cursor: "move", arrastar: () => ({ rastreio: "r1", ancora: { t: 0, pts: 0, v: c } }) }];
    const base: EfHolofote = {
      id: "h1", ligado: true, nome: "", ativo: null, tipo: "holofote", escurecer: 0.6, dessaturar: 0,
      alvo: { rastreio: "r1", forma: "poligono", vertices: [{ x: 0, y: 0 }, { x: 1, y: 0 }, { x: 0.5, y: 1 }], margem: 0, borda_suave: 0, inverter: false },
    };
    const a = alcasDoAlvo(base, quadro(c), daCaixa);
    expect(a.slice(0, 1)).toEqual(daCaixa);
    expect(a.slice(1).map((x) => [x.tipo, x.em])).toEqual([["ponto", { x: 0.2, y: 0.3 }], ["ponto", { x: 0.6, y: 0.3 }], ["ponto", { x: 0.4, y: 0.5 }]].map(([t, p]) => [t, expect.objectContaining({ x: expect.closeTo((p as Caixa).x, 12), y: expect.closeTo((p as Caixa).y, 12) })]));
    const m = a[3]!.arrastar({ x: 0.4, y: 0.5 }, { x: 0.5, y: 0.45 }, { shift: false, alt: false }) as { efeito: EfHolofote };
    expect(m.efeito.alvo.vertices![0]).toEqual({ x: 0, y: 0 });
    expect(m.efeito.alvo.vertices![2]!.x).toBeCloseTo(0.75, 12);
    expect(m.efeito.alvo.vertices![2]!.y).toBeCloseTo(0.75, 12);
    // fora da caixa: preso a 0..1
    const fora = a[1]!.arrastar({ x: 0.2, y: 0.3 }, { x: -1, y: -1 }, { shift: false, alt: false }) as { efeito: EfHolofote };
    expect(fora.efeito.alvo.vertices![0]).toEqual({ x: 0, y: 0 });
    // com margem, a fração é da caixa com a margem (a do desenho)
    const comMargem = alcasDoAlvo({ ...base, alvo: { ...base.alvo, margem: 0.5 } }, quadro(c), []);
    expect(comMargem[0]!.em.x).toBeCloseTo(0.0, 12);
    expect(comMargem[0]!.em.y).toBeCloseTo(0.2, 12);
    // outras formas: só as da caixa
    expect(alcasDoAlvo({ ...base, alvo: { ...base.alvo, forma: "elipse" } }, quadro(c), daCaixa)).toEqual(daCaixa);
    // sem caixa no quadro: só as da caixa (que também ficam vazias)
    expect(alcasDoAlvo(base, { ...quadro(c), rastreio: () => null }, [])).toEqual([]);
  });

  it("registro: as alças do holofote e do ocultar passam por alcasDoAlvo", () => {
    const q = quadro({ x: 0.1, y: 0.1, w: 0.2, h: 0.2 });
    const h: EfHolofote = {
      id: "h1", ligado: true, nome: "", ativo: null, tipo: "holofote", escurecer: 0.6, dessaturar: 0,
      alvo: { rastreio: "r1", forma: "poligono", vertices: [...VERTICES_PADRAO], margem: 0, borda_suave: 0, inverter: false },
    };
    const daCaixa = REGISTRO.holofote.alcas!({ ...h, alvo: { ...h.alvo, forma: "retangulo" } }, q).length;
    expect(REGISTRO.holofote.alcas!(h, q).length).toBe(daCaixa + 4);
    const o = { ...h, tipo: "ocultar", modo: "solido", intensidade: 1, cor: "#000000ff", conteudo: "rosto", motivo: "" } as unknown as EfOcultar;
    expect(REGISTRO.ocultar.alcas!(o, q).length).toBe(daCaixa + 4);
  });
});

describe("Ocultar", () => {
  const ocultar = () => st().doc!.midias.flatMap((m) => m.ocultacoes).at(-1)!;

  it("padrão por conteúdo = o do Rust (fixture conferencia-padrao.json)", () => {
    const casos = ler<{ nome: string; entrada: EfOcultar["conteudo"]; saida: Pick<EfOcultar, "modo" | "intensidade"> }[]>("conferencia-padrao.json");
    expect(casos.map((c) => c.entrada).sort()).toEqual(["corpo", "documento", "outro", "placa", "rosto", "tela"]);
    for (const c of casos) expect([c.nome, padraoDoConteudo(c.entrada)]).toEqual([c.nome, c.saida]);
  });

  it("Conteúdo põe o modo e a intensidade do padrão quando ainda estavam no padrão, num passo; Polígono aparece e nasce com os cantos", async () => {
    await adicionar("Ocultar");
    const l = () => linha("ocultar");
    await escolher(entradaDe(l(), "Conteúdo") as HTMLSelectElement, "placa");
    expect([ocultar().conteudo, ocultar().modo, ocultar().intensidade, ultimo()]).toEqual(["placa", "solido", 1, "Conteúdo"]);
    const n = passos();
    await escolher(entradaDe(l(), "Conteúdo") as HTMLSelectElement, "rosto");
    expect([ocultar().conteudo, ocultar().modo, ocultar().intensidade, passos()]).toEqual(["rosto", "mosaico", 0.75, n + 1]);
    // escolha do perito fica: trocar o conteúdo depois de escolher o modo não volta ao padrão
    await clicar(campo(l(), "Modo").querySelector<HTMLButtonElement>('[data-valor="desfoque"]')!);
    await escolher(entradaDe(l(), "Conteúdo") as HTMLSelectElement, "placa");
    expect([ocultar().conteudo, ocultar().modo, ocultar().intensidade]).toEqual(["placa", "desfoque", 0.75]);
    const formas = [...campo(l(), "Forma").querySelectorAll<HTMLButtonElement>("button")].map((b) => b.dataset.valor);
    expect(formas).toEqual(["retangulo", "elipse", "poligono"]);
    await clicar(campo(l(), "Forma").querySelector<HTMLButtonElement>('[data-valor="poligono"]')!);
    expect([ocultar().alvo.forma, ocultar().alvo.vertices, ultimo()]).toEqual(["poligono", VERTICES_PADRAO, "Forma"]);
    semFrase(l());
  });
});
