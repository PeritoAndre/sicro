// @vitest-environment happy-dom
/**
 * Lupa no Inspetor (P1.2), montado de verdade: "Adicionar → Lupa" cria o rastreio e a lupa num passo; cada campo
 * do §8.6 muda o documento num passo com o rótulo dele; "Marcar" marca o alvo e o centro da lupa fixa; só rótulos
 * do §8.6, nenhum texto de exemplo.
 */

import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { ClipeTrecho, EfLupa, ProjetoAberto, ProjetoEdicao } from "@domain/edicao";

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
import { ADICIONAR, REGISTRO } from "../registro";
import { useInspetorUi } from "../ui";
import { campo, clicar, desmontarTudo, digitar, entradaDe, esperar, lerFixture as ler, montar, textosVisiveis, type Tela } from "../testeTela";
import { textoPermitido } from "../rotulos.teste";
import { recursoDe, renovar, useRecursos } from "../../palco/recursos";
import { LUPA_PRONTA } from "./LupaEditor";

const st = () => useEdicaoStore.getState();
const passos = () => st().passado.length;
const ultimo = () => st().passado.at(-1)?.rotulo;
const trecho = () => st().doc!.principal[1] as ClipeTrecho;
const lupa = () => trecho().efeitos.find((e) => (e as EfLupa).tipo === "lupa") as EfLupa | undefined;
const linha = () => document.querySelector<HTMLElement>('[data-efeito="lupa"]')!;

let tela: Tela;

beforeEach(async () => {
  vi.useFakeTimers({ toFake: ["Date"], now: new Date("2026-11-14T10:00:00Z") });
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

/** "Adicionar → Lupa" pelo menu da seção. */
async function adicionar() {
  await clicar(tela.raiz.querySelector<HTMLButtonElement>('[data-acao="adicionar"]')!);
  const item = [...document.querySelectorAll<HTMLElement>("button, [role=menuitem]")].find((x) => x.textContent?.trim() === "Lupa");
  expect(item).toBeTruthy();
  await clicar(item!);
  await esperar();
}

describe("Lupa", () => {
  it("ligada: editor próprio e no Adicionar, entre Ocultar e Texto (§8.6)", () => {
    expect(LUPA_PRONTA).toBe(true);
    expect(ADICIONAR.slice(0, 3)).toEqual(["destaque", "ocultar", "lupa"]);
    expect(ADICIONAR.indexOf("lupa")).toBeLessThan(ADICIONAR.indexOf("texto"));
    expect(REGISTRO.lupa.rotulo).toBe("Lupa");
  });

  it("Adicionar → Lupa: rastreio novo e a lupa apontando para ele, um passo, aberta", async () => {
    const n = passos();
    const nr = st().doc!.rastreios.length;
    await adicionar();
    expect([passos(), ultimo()]).toEqual([n + 1, "Lupa"]);
    const l = lupa()!;
    expect(l).toMatchObject({ tipo: "lupa", ligado: true, janela: 0.15, fator: 2, forma: "circulo", interpolacao: "bilinear", onde: { modo: "fixa" }, ligacao: true });
    const r = st().doc!.rastreios.find((x) => x.id === l.rastreio)!;
    expect(st().doc!.rastreios.length).toBe(nr + 1);
    // trecho: rastreio da mídia, em tempo de fonte, com a âncora no cursor (o começo do clipe)
    expect(r).toMatchObject({ midia: trecho().midia, base: "fonte" });
    expect(r.ancoras.map((a) => a.t)).toEqual([12]);
    expect(st().selecao.efeito).toBe(l.id);
    expect(linha().className).toMatch(/efeitoAberto/);
  });

  it("com a tabela da mídia, a âncora do alvo nasce no quadro mostrado, com o pts ((i) 2)", async () => {
    const info = {
      id: "vm1", sha256: "1".repeat(64), nome: "cam1.mp4", exibido: { w: 1920, h: 1080 }, taxa: { num: 30, den: 1 }, vfr: false, passo: { min: 1 / 30, max: 1 / 30 },
      de_s: 0, ate_s: 60, n: 1800, tem_audio: false, previa: { relativo: "", tipo: "original", desloc_s: 0, de_s: 0, ate_s: 60, largura: 1920, altura: 1080, pronta: true },
      tabela: "decodificador", erros: 0, marcavel: true,
    };
    cmd.edicaoMidiaInfo.mockResolvedValue(info);
    cmd.edicaoTabelaQuadros.mockResolvedValue({ tb: { num: 1, den: 15360 }, pts0: 0, passos: Array(1799).fill(512), passo_mediano: 512, chaves: [0], vfr: false, origem: "decodificador" });
    // quem pede a tabela no app é o palco
    renovar("/casos/c1", "1".repeat(64));
    await esperar(() => new Promise((r) => setTimeout(r, 0)));
    expect(recursoDe("1".repeat(64)).tabela).toBeTruthy();
    // cursor no meio do quadro 365 (12,1667 s): a âncora vai para o instante e o pts do quadro
    await esperar(() => st().posicionar({ modo: "clipe", clipe: trecho().id, t: 365 / 30 + 0.01 }));
    await adicionar();
    const r = st().doc!.rastreios.find((x) => x.id === lupa()!.rastreio)!;
    expect(r.ancoras).toHaveLength(1);
    expect(r.ancoras[0]!.pts).toBe(365 * 512);
    expect(r.ancoras[0]!.t).toBeCloseTo(365 / 30, 9);
    useRecursos.setState({ midias: {}, versao: 0 });
  });

  it("cada campo é um passo com o rótulo do §8.6", async () => {
    await adicionar();
    const id = lupa()!.id;
    const n = passos();
    await clicar(campo(linha(), "Forma").querySelector<HTMLButtonElement>('[data-valor="retangulo"]')!);
    expect([lupa()!.forma, ultimo()]).toEqual(["retangulo", "Forma"]);
    await digitar(entradaDe(linha(), "Ampliação") as HTMLInputElement, "3,5");
    expect([lupa()!.fator, ultimo()]).toEqual([3.5, "Ampliação"]);
    await digitar(entradaDe(linha(), "Janela") as HTMLInputElement, "25");
    expect(lupa()!.janela).toBeCloseTo(0.25, 12);
    expect(ultimo()).toBe("Janela");
    // grade: fixa no canto de baixo à esquerda; Acompanha leva a casa para o lado do alvo
    await clicar(campo(linha(), "Posição").querySelector<HTMLButtonElement>('[data-valor="esq-base"]')!);
    expect([lupa()!.onde, ultimo()]).toEqual([{ modo: "fixa", x: 0.2, y: 0.75 }, "Posição"]);
    expect(campo(linha(), "Posição").querySelector('[data-valor="esq-base"]')!.getAttribute("aria-pressed")).toBe("true");
    const acompanha = [...linha().querySelectorAll<HTMLButtonElement>("button")].find((b) => b.textContent === "Acompanha")!;
    await clicar(acompanha);
    expect([lupa()!.onde, ultimo()]).toEqual([{ modo: "segue", dx: -0.25, dy: 0.3 }, "Posição"]);
    expect(campo(linha(), "Posição").querySelector('[data-valor="esq-base"]')!.getAttribute("aria-pressed")).toBe("true");
    await clicar(campo(linha(), "Posição").querySelector<HTMLButtonElement>('[data-valor="dir-topo"]')!);
    expect(lupa()!.onde).toEqual({ modo: "segue", dx: 0.25, dy: -0.3 });
    await clicar(campo(linha(), "Interpolação").querySelector<HTMLButtonElement>('[data-valor="vizinho"]')!);
    expect([lupa()!.interpolacao, ultimo()]).toEqual(["vizinho", "Interpolação"]);
    await clicar(entradaDe(linha(), "Ligação") as HTMLInputElement);
    expect([lupa()!.ligacao, ultimo()]).toEqual([false, "Ligação"]);
    await digitar(entradaDe(linha(), "Espessura") as HTMLInputElement, "0,8");
    expect(lupa()!.moldura.espessura).toBeCloseTo(0.008, 12);
    await clicar(entradaDe(linha(), "Contorno") as HTMLInputElement);
    expect([lupa()!.moldura.contorno, ultimo()]).toEqual([false, "Contorno"]);
    expect(passos()).toBe(n + 10);
    expect(lupa()!.id).toBe(id);
  });

  it("Ampliação fora de 1,5..8 não entra", async () => {
    await adicionar();
    const n = passos();
    await digitar(entradaDe(linha(), "Ampliação") as HTMLInputElement, "9");
    await digitar(entradaDe(linha(), "Ampliação") as HTMLInputElement, "1");
    expect([lupa()!.fator, passos()]).toEqual([2, n]);
  });

  it("Espessura zero não entra (o canvas ignora largura 0 e a prévia mostraria a moldura que o render não tem)", async () => {
    await adicionar();
    const n = passos();
    await digitar(entradaDe(linha(), "Espessura") as HTMLInputElement, "0");
    expect([lupa()!.moldura.espessura, passos()]).toEqual([0.004, n]);
  });

  it("Marcar: o alvo (caixa do rastreio) e o centro da lupa fixa (lugar no canvas)", async () => {
    await adicionar();
    const l = lupa()!;
    await clicar(linha().querySelector<HTMLButtonElement>('[data-acao="marcar"]')!);
    expect(useInspetorUi.getState().ferramenta).toMatchObject({ tipo: "caixa", rastreio: l.rastreio, efeito: l.id });
    await esperar(() => useInspetorUi.getState().marcado({ x: 0.5, y: 0.5, w: 0.05, h: 0.08 }, 14, 14 * 30 * 512));
    expect(st().doc!.rastreios.find((x) => x.id === l.rastreio)!.ancoras.map((a) => a.t)).toEqual([12, 14]);
    const n = passos();
    await clicar(linha().querySelector<HTMLButtonElement>('[data-acao="marcar-lupa"]')!);
    expect(useInspetorUi.getState().ferramenta).toMatchObject({ tipo: "lugar", efeito: l.id });
    await esperar(() => useInspetorUi.getState().marcado({ x: 0.3, y: 0.6 }, 14, null));
    expect([lupa()!.onde, passos(), ultimo()]).toEqual([{ modo: "fixa", x: 0.3, y: 0.6 }, n + 1, "Marcar"]);
    // Acompanha não marca no palco (o lado vem da grade)
    await clicar([...linha().querySelectorAll<HTMLButtonElement>("button")].find((b) => b.textContent === "Acompanha")!);
    expect(linha().querySelector('[data-acao="marcar-lupa"]')).toBeNull();
  });

  it("Remover tira a lupa e o rastreio dela", async () => {
    await adicionar();
    const l = lupa()!;
    await clicar([...linha().querySelectorAll<HTMLButtonElement>("button")].find((b) => b.getAttribute("aria-label") === "Remover")!);
    expect(lupa()).toBeUndefined();
    expect(st().doc!.rastreios.some((r) => r.id === l.rastreio)).toBe(false);
  });

  it("só rótulos do §8.6, nenhum texto de exemplo", async () => {
    await adicionar();
    expect(textosVisiveis(linha()).filter((t) => !textoPermitido(t))).toEqual([]);
    for (const r of ["Forma", "Ampliação", "Janela", "Posição", "Interpolação", "Ligação", "Cor", "Espessura", "Contorno", "Fixa", "Acompanha", "Marcar", "De", "Até"]) {
      expect(textosVisiveis(linha())).toContain(r);
    }
    expect(linha().querySelectorAll("[placeholder]").length).toBe(0);
  });
});
