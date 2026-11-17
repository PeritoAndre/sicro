// @vitest-environment happy-dom
/**
 * Velocidade do trecho (P3.4) no Inspetor de verdade: ligar a rampa (pontos no início, no cursor e no fim; som mudo),
 * editar tempo e fator de cada ponto, adicionar e remover, voltar à constante; cada mudança é um passo com o rótulo, e
 * só os rótulos do §8.6 aparecem.
 */

import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { ClipeTitulo, ClipeTrecho, ProjetoAberto, ProjetoEdicao } from "@domain/edicao";

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
import { useInspetorUi } from "../ui";
import { botao, campo, clicar, desmontarTudo, digitar, entradaDe, esperar, lerFixture, montar, tecla, textosVisiveis, type Tela } from "../testeTela";
import { textoPermitido } from "../rotulos.teste";
import { fatorEm, instanteNovo, rampaNova } from "./VelocidadeSecao";

const st = () => useEdicaoStore.getState();
const doc = () => st().doc!;
const passos = () => st().passado.length;
const ultimo = () => st().passado.at(-1)?.rotulo;
const principal = () => doc().principal as [ClipeTitulo, ClipeTrecho, ClipeTrecho];
const trecho = () => principal()[1];
const pontos = () => {
  const v = trecho().velocidade;
  return v.tipo === "rampa" ? v.pontos.map((p) => [p.t, p.v]) : null;
};
const selecionar = (id: string) => {
  const it = { k: "clipe" as const, id };
  st().selecionar({ itens: [it], foco: it, efeito: null });
};
const linha = (i: number) => tela.raiz.querySelector<HTMLElement>(`[data-ponto="${i}"]`)!;
const depois = async () => vi.setSystemTime(Date.now() + 1500);

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
  await esperar(() => selecionar(trecho().id));
});

afterEach(async () => {
  desmontarTudo();
  await st()
    .fechar({ descartar: true })
    .catch(() => {});
  vi.useRealTimers();
});

const conferirRotulos = () => {
  const fora = textosVisiveis(tela.raiz).filter((t) => !textoPermitido(t));
  expect(fora).toEqual([]);
  expect(tela.raiz.querySelectorAll("[placeholder]").length).toBe(0);
};

describe("Clipe · Trecho · Velocidade (P3.4)", () => {
  it("Rampa: 1× no início e no fim, 0,25× no meio, som mudo; um passo; volta à constante pelo primeiro ponto", async () => {
    // trecho de 12 s a 15,5 s, a 1×, com som
    expect([trecho().inicio_s, trecho().fim_s, trecho().audio.mudo]).toEqual([12, 15.5, false]);
    conferirRotulos();
    const n = passos();
    const b = botao(tela.raiz, "Rampa");
    expect(b.getAttribute("aria-pressed")).toBe("false");
    await clicar(b);
    expect(pontos()).toEqual([
      [12, 1],
      [13.75, 0.25],
      [15.5, 1],
    ]);
    expect(trecho().audio.mudo).toBe(true);
    expect([passos(), ultimo()]).toEqual([n + 1, "Rampa"]);
    expect(tela.raiz.querySelectorAll("[data-ponto]").length).toBe(3);
    conferirRotulos();
    // fator do ponto 2: um passo; 4 casas não entram
    const f = entradaDe(linha(1), "Velocidade") as HTMLInputElement;
    await digitar(f, "0,5");
    expect(pontos()![1]).toEqual([13.75, 0.5]);
    expect([passos(), ultimo()]).toEqual([n + 2, "Velocidade"]);
    await depois();
    await digitar(f, "0,2501", { aos: 6 });
    expect(pontos()![1]).toEqual([13.75, 0.5]);
    expect(campo(linha(1), "Velocidade").className).toMatch(/ruim/);
    // de volta à constante: o fator do primeiro ponto
    await clicar(botao(tela.raiz, "Rampa"));
    expect(trecho().velocidade).toEqual({ tipo: "constante", fator: 1 });
    expect(ultimo()).toBe("Rampa");
    // desfazer volta à rampa inteira
    await esperar(() => st().desfazer());
    expect(pontos()).toEqual([
      [12, 1],
      [13.75, 0.5],
      [15.5, 1],
    ]);
  });

  it("o ponto do meio vai no cursor do palco quando ele está no trecho", async () => {
    await esperar(() => st().posicionar({ modo: "clipe", clipe: trecho().id, t: 14.2 }));
    await clicar(botao(tela.raiz, "Rampa"));
    expect(pontos()![1]).toEqual([14.2, 0.25]);
  });

  it("tempo do ponto: dentro do trecho, e a lista fica em ordem; Adicionar e Remover", async () => {
    await clicar(botao(tela.raiz, "Rampa"));
    const t = entradaDe(linha(1), "Tempo") as HTMLInputElement;
    // fora do trecho: não entra
    await digitar(t, "16", { aos: 5 });
    await tecla(t, "Enter");
    expect(pontos()![1]![0]).toBe(13.75);
    expect(campo(linha(1), "Tempo").className).toMatch(/ruim/);
    // o primeiro ponto vai para depois do segundo: entra e a lista se reordena
    const t0 = entradaDe(linha(0), "Tempo") as HTMLInputElement;
    await digitar(t0, "15", { aos: 5 });
    await tecla(t0, "Enter");
    expect(pontos()).toEqual([
      [13.75, 0.25],
      [15, 1],
      [15.5, 1],
    ]);
    const n = passos();
    // Adicionar: no cursor, com o fator da rampa ali (antes do primeiro ponto vale o dele; entre 13,75 e 15, linear)
    await esperar(() => st().posicionar({ modo: "clipe", clipe: trecho().id, t: 14.5 }));
    await clicar(botao(tela.raiz, "Adicionar"));
    expect(pontos()).toEqual([
      [13.75, 0.25],
      [14.5, 0.7],
      [15, 1],
      [15.5, 1],
    ]);
    expect([passos(), ultimo()]).toEqual([n + 1, "Adicionar"]);
    // Remover tira o ponto da linha; o último não sai
    await clicar(botao(linha(1), "Remover"));
    expect(pontos()!.length).toBe(3);
    await clicar(botao(linha(0), "Remover"));
    await clicar(botao(linha(0), "Remover"));
    expect(pontos()!.length).toBe(1);
    expect(botao(linha(0), "Remover").disabled).toBe(true);
    conferirRotulos();
  });
});

describe("contas da rampa na tela", () => {
  const c = { inicio_s: 2, fim_s: 6, velocidade: { tipo: "constante" as const, fator: 0.5 } };
  it("rampa nova: o fator de agora nas pontas, no máximo 0,25× no cursor ou no meio", () => {
    expect(rampaNova(c, null).map((p) => [p.t, p.v])).toEqual([
      [2, 0.5],
      [4, 0.25],
      [6, 0.5],
    ]);
    expect(rampaNova({ ...c, velocidade: { tipo: "constante", fator: 0.1 } }, 3).map((p) => [p.t, p.v])).toEqual([
      [2, 0.1],
      [3, 0.1],
      [6, 0.1],
    ]);
    // cursor fora do trecho: o meio
    expect(rampaNova(c, 9)[1]!.t).toBe(4);
  });

  it("fator em t: linear, degrau segura, pontas fora, milésimos dentro de 0,05..16", () => {
    const p = [
      { t: 2, v: 1 },
      { t: 4, v: 0.25, interp: "degrau" as const },
      { t: 5, v: 2 },
    ];
    expect([fatorEm(p, 1), fatorEm(p, 3), fatorEm(p, 4.5), fatorEm(p, 9)]).toEqual([1, 0.625, 0.25, 2]);
    expect(fatorEm([{ t: 0, v: 1 }, { t: 3, v: 0.05 }], 1)).toBe(0.683);
  });

  it("instante novo: o cursor livre; senão o meio do maior vão", () => {
    const p = [
      { t: 2, v: 1 },
      { t: 3, v: 1 },
    ];
    expect(instanteNovo(p, c, 5)).toBe(5);
    expect(instanteNovo(p, c, 3)).toBe(4.5);
    expect(instanteNovo(p, c, null)).toBe(4.5);
  });
});
