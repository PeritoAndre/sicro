// @vitest-environment happy-dom
/**
 * Saída · selos (P1.R): projeto novo nasce com os três ligados; cada marca muda só o selo dela, em UM passo com o
 * rótulo do §8.6; só os três rótulos na tela; projeto só para leitura não muda.
 */

import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { ProjetoAberto, ProjetoEdicao } from "@domain/edicao";

const cmd = vi.hoisted(() => ({
  edicaoAbrir: vi.fn(),
  edicaoSalvar: vi.fn(),
  edicaoAnalisar: vi.fn(),
  edicaoTabelaQuadros: vi.fn(),
  edicaoMidiaInfo: vi.fn(),
}));
vi.mock("@core/commands", () => ({ commands: cmd }));
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn(async () => () => {}) }));

import { novoProjeto } from "../../modelo/projeto";
import { useEdicaoStore } from "../../store/edicaoStore";
import { Inspetor } from "../Inspetor";
import { useInspetorUi } from "../ui";
import { botao, clicar, desmontarTudo, entradaDe, esperar, lerFixture, montar, textosVisiveis, type Tela } from "../testeTela";
import { ROTULOS } from "../rotulos.teste";
import { SELOS, SELOS_PRONTOS, SelosSecao } from "./SelosSecao";

const WS = "/casos/c1";
const st = () => useEdicaoStore.getState();
const selos = () => st().doc!.saida.selos;
const marca = (de: ParentNode, r: string) => entradaDe(de, r) as HTMLInputElement;

async function abrir(so_leitura = false) {
  const projeto = lerFixture<ProjetoEdicao>("projeto-do-store.json");
  const r: ProjetoAberto = { projeto, importado_de: null, so_leitura, avisos: [], raiz_cache: "/cache/c1" };
  cmd.edicaoAbrir.mockResolvedValueOnce(r);
  await st().abrir(WS, projeto.id);
}

let tela: Tela;

beforeEach(() => {
  vi.useFakeTimers({ toFake: ["Date"], now: new Date("2026-11-14T10:00:00Z") });
  for (const f of Object.values(cmd)) f.mockReset();
  cmd.edicaoSalvar.mockImplementation(async () => ({ updated_at: new Date().toISOString(), raiz_cache: "/cache/c1" }));
  cmd.edicaoAnalisar.mockImplementation(async () => ({ problemas: [], linha: { fps: { num: 30, den: 1 }, total_q: 0, clipes: [], livres: [] }, midias: [], prender: [], saida_auto: null }));
  cmd.edicaoTabelaQuadros.mockRejectedValue({ kind: "validation", message: "validation error: tabela_pendente: ainda não" });
  cmd.edicaoMidiaInfo.mockRejectedValue({ kind: "validation", message: "validation error: tabela_pendente: ainda não" });
  useInspetorUi.setState({ aba: "clipe", ferramenta: null, aoMarcar: null, chave: null });
});

afterEach(async () => {
  desmontarTudo();
  await st()
    .fechar({ descartar: true })
    .catch(() => {});
  vi.useRealTimers();
});

describe("Saída · selos", () => {
  it("projeto novo nasce com os três selos ligados", () => {
    expect(SELOS_PRONTOS).toBe(true);
    expect(novoProjeto().saida.selos).toEqual({ velocidade: true, ampliacao: true, realce: true });
    expect(SELOS.map((s) => s.r)).toEqual(["Selo de velocidade", "Selo de ampliação", "Selo de realce"]);
    for (const s of SELOS) expect(ROTULOS.has(s.r), s.r).toBe(true);
  });

  it("na aba Saída, depois de Áudio: três marcas ligadas, só os rótulos do §8.6", async () => {
    await abrir();
    tela = await montar(<Inspetor />);
    await clicar(botao(tela.raiz, "Saída"));
    const painel = tela.raiz.querySelector<HTMLElement>('[data-painel="saida"]')!;
    const textos = textosVisiveis(painel);
    expect(textos.slice(textos.indexOf("Áudio") + 1, textos.indexOf("Áudio") + 4)).toEqual(["Selo de velocidade", "Selo de ampliação", "Selo de realce"]);
    for (const s of SELOS) {
      const m = marca(painel, s.r);
      expect([m.type, m.checked, m.disabled], s.r).toEqual(["checkbox", true, false]);
    }
    expect(painel.querySelectorAll("[placeholder]").length).toBe(0);
  });

  it("cada marca muda só o selo dela, em um passo com o rótulo; desfazer volta", async () => {
    await abrir();
    tela = await montar(<SelosSecao />);
    const n = st().passado.length;
    await clicar(marca(tela.raiz, "Selo de ampliação"));
    expect(selos()).toEqual({ velocidade: true, ampliacao: false, realce: true });
    expect([st().passado.length, st().passado.at(-1)?.rotulo]).toEqual([n + 1, "Selo de ampliação"]);
    expect(marca(tela.raiz, "Selo de ampliação").checked).toBe(false);
    await clicar(marca(tela.raiz, "Selo de velocidade"));
    await clicar(marca(tela.raiz, "Selo de realce"));
    expect(selos()).toEqual({ velocidade: false, ampliacao: false, realce: false });
    expect(st().passado.slice(n).map((p) => p.rotulo)).toEqual(["Selo de ampliação", "Selo de velocidade", "Selo de realce"]);
    await clicar(marca(tela.raiz, "Selo de velocidade"));
    expect(selos()).toEqual({ velocidade: true, ampliacao: false, realce: false });
    // desfazer os quatro passos devolve os três ligados, sem tocar no resto da saída
    const antes = lerFixture<ProjetoEdicao>("projeto-do-store.json").saida;
    await esperar(() => {
      for (let i = 0; i < 4; i++) st().desfazer();
    });
    expect(st().doc!.saida).toEqual(antes);
    // todo texto da seção é rótulo do §8.6
    expect(textosVisiveis(tela.raiz)).toEqual(["Selo de velocidade", "Selo de ampliação", "Selo de realce"]);
  });

  it("projeto só para leitura: marcas desligadas, nada muda", async () => {
    await abrir(true);
    tela = await montar(<SelosSecao />);
    const m = marca(tela.raiz, "Selo de realce");
    expect(m.disabled).toBe(true);
    await clicar(m);
    expect(selos().realce).toBe(true);
  });

  it("sem projeto aberto não desenha nada", async () => {
    tela = await montar(<SelosSecao />);
    expect(tela.raiz.textContent).toBe("");
  });
});
