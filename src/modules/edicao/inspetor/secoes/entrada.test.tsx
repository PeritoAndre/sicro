// @vitest-environment happy-dom
/**
 * Clipe · Entrada e Saída · Fecho (P1.1), montados no Inspetor de verdade: o catálogo é o do motor
 * (`juncao-transicoes.json`), cada mudança é um passo com o rótulo do campo, e só os rótulos do §8.6 aparecem.
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
import { botao, campo, clicar, desmontarTudo, digitar, entradaDe, esperar, lerFixture, montar, textosVisiveis, type Tela } from "../testeTela";
import { textoPermitido } from "../rotulos.teste";
import { FECHO_PRONTO } from "./EntradaSecao";

const st = () => useEdicaoStore.getState();
const doc = () => st().doc!;
const passos = () => st().passado.length;
const ultimo = () => st().passado.at(-1)?.rotulo;
const principal = () => doc().principal as [ClipeTitulo, ClipeTrecho, ClipeTrecho];
const selecionar = (id: string) => {
  const it = { k: "clipe" as const, id };
  st().selecionar({ itens: [it], foco: it, efeito: null });
};
/** Mais de 1 s depois: a próxima mudança do mesmo campo é outro passo. */
const depois = async () => vi.setSystemTime(Date.now() + 1500);

/** Catálogo do motor, na ordem (`juncao::TRANSICOES`). */
const CATALOGO = lerFixture<{ nome: string; saida: { sobrepoe: boolean; xfade: string | null } }[]>("juncao-transicoes.json");

/** Botões de "Transição" do painel: valor, texto e se é só ícone. */
const opcoes = (raiz: HTMLElement) =>
  [...raiz.querySelectorAll<HTMLButtonElement>('[data-campo="Transição"] button')].map((b) => ({
    v: b.getAttribute("data-valor"),
    texto: (b.textContent ?? "").trim(),
    icone: !!b.querySelector("svg"),
    marcado: b.getAttribute("aria-pressed") === "true",
  }));

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
  const projeto = lerFixture<ProjetoEdicao>("projeto-do-store.json");
  const r: ProjetoAberto = { projeto, importado_de: null, so_leitura: false, avisos: [], raiz_cache: "/cache/c1" };
  cmd.edicaoAbrir.mockResolvedValueOnce(r);
  await st().abrir("/casos/c1", projeto.id);
  tela = await montar(<Inspetor />);
});

afterEach(async () => {
  desmontarTudo();
  await st()
    .fechar({ descartar: true })
    .catch(() => {});
  vi.useRealTimers();
});

describe("Clipe · Entrada", () => {
  it("catálogo do motor: Corte e Escurecer com nome, as que sobrepõem só com ícone", async () => {
    await esperar(() => selecionar(principal()[1].id));
    const os = opcoes(tela.raiz);
    expect(os.map((o) => o.v)).toEqual(["corte", ...CATALOGO.map((c) => c.nome)]);
    expect(CATALOGO.filter((c) => !c.saida.sobrepoe).map((c) => c.nome)).toEqual(["escurecer"]);
    for (const o of os) {
      if (o.v === "corte" || o.v === "escurecer") expect([o.texto, o.icone]).toEqual([o.v === "corte" ? "Corte" : "Escurecer", false]);
      else expect([o.texto, o.icone], o.v!).toEqual(["", true]);
    }
    // corte: só "Transição"; sem "Duração (s)" nem "Som" da entrada
    expect(os.filter((o) => o.marcado).map((o) => o.v)).toEqual(["corte"]);
    expect(tela.raiz.querySelector('[data-campo="Duração (s)"]')).toBeNull();
  });

  it("ícone grava o tipo; Duração (s), Som e Corte: um passo cada", async () => {
    const id = principal()[1].id;
    await esperar(() => selecionar(id));
    const n = passos();
    for (const c of CATALOGO.filter((x) => x.saida.sobrepoe)) {
      await clicar(tela.raiz.querySelector<HTMLButtonElement>(`[data-campo="Transição"] button[data-valor="${c.nome}"]`)!);
      expect(principal()[1].transicao_entrada).toEqual({ tipo: c.nome, duracao_s: 0.5, audio: "acompanhar" });
      expect(ultimo()).toBe("Transição");
      expect(opcoes(tela.raiz).filter((o) => o.marcado).map((o) => o.v)).toEqual([c.nome]);
    }
    const tipos = CATALOGO.filter((x) => x.saida.sobrepoe).length;
    expect(passos()).toBe(n + tipos);
    await digitar(entradaDe(tela.raiz, "Duração (s)") as HTMLInputElement, "1,2");
    expect(principal()[1].transicao_entrada?.duracao_s).toBe(1.2);
    expect([passos(), ultimo()]).toEqual([n + tipos + 1, "Duração (s)"]);
    // "Som" da entrada vem depois do "Som" do trecho (volume e mudo)
    const som = [...tela.raiz.querySelectorAll<HTMLElement>('[data-campo="Som"]')].find((x) => x.querySelector('button[data-valor="cortar"]'))!;
    await clicar(botao(som, "Cortar"));
    expect(principal()[1].transicao_entrada?.audio).toBe("cortar");
    expect([passos(), ultimo()]).toEqual([n + tipos + 2, "Som"]);
    // trocar o tipo mantém duração e som
    await clicar(botao(campo(tela.raiz, "Transição"), "Escurecer"));
    expect(principal()[1].transicao_entrada).toEqual({ tipo: "escurecer", duracao_s: 1.2, audio: "cortar" });
    await clicar(botao(campo(tela.raiz, "Transição"), "Corte"));
    expect(principal()[1].transicao_entrada).toBeNull();
    expect(passos()).toBe(n + tipos + 4);
    st().desfazer();
    expect(principal()[1].transicao_entrada).toEqual({ tipo: "escurecer", duracao_s: 1.2, audio: "cortar" });
  });

  it("tipo de versão futura: nenhuma opção marcada, duração e som seguem editáveis", async () => {
    const id = principal()[2].id;
    await esperar(() => st().alterar("teste", (d) => void ((d.principal[2] as ClipeTrecho).transicao_entrada = { tipo: "transicao_do_futuro", duracao_s: 0.7, audio: "acompanhar" })));
    await esperar(() => selecionar(id));
    expect(opcoes(tela.raiz).filter((o) => o.marcado)).toEqual([]);
    expect((entradaDe(tela.raiz, "Duração (s)") as HTMLInputElement).value).toBe("0,70");
  });
});

describe("Saída · Fecho", () => {
  it("ligado: Transição, Duração (s) e Som gravam o fecho, um passo cada", async () => {
    expect(FECHO_PRONTO).toBe(true);
    await clicar(botao(tela.raiz, "Saída"));
    expect([...tela.raiz.querySelectorAll("h3")].map((h) => h.textContent)).toContain("Fecho");
    expect(doc().fecho).toBeNull();
    expect(opcoes(tela.raiz).map((o) => o.v)).toEqual(["corte", ...CATALOGO.map((c) => c.nome)]);
    const n = passos();
    await clicar(tela.raiz.querySelector<HTMLButtonElement>('[data-campo="Transição"] button[data-valor="fundir"]')!);
    expect(doc().fecho).toEqual({ tipo: "fundir", duracao_s: 0.5, audio: "acompanhar" });
    expect([passos(), ultimo()]).toEqual([n + 1, "Transição"]);
    await digitar(entradaDe(tela.raiz, "Duração (s)") as HTMLInputElement, "0,3");
    expect(doc().fecho?.duracao_s).toBe(0.3);
    await clicar(botao(campo(tela.raiz, "Som"), "Cortar"));
    expect(doc().fecho?.audio).toBe("cortar");
    await depois();
    await clicar(botao(campo(tela.raiz, "Transição"), "Escurecer"));
    expect(doc().fecho).toEqual({ tipo: "escurecer", duracao_s: 0.3, audio: "cortar" });
    await clicar(botao(campo(tela.raiz, "Transição"), "Corte"));
    expect(doc().fecho).toBeNull();
    expect(passos()).toBe(n + 5);
    expect(st().passado.slice(-5).map((p) => p.rotulo)).toEqual(["Transição", "Duração (s)", "Som", "Transição", "Transição"]);
    // nenhuma transição de clipe mudou
    expect(principal().map((c) => c.transicao_entrada?.tipo ?? null)).toEqual([null, null, "escurecer"]);
  });
});

describe("rótulos", () => {
  it("Entrada e Fecho: só rótulos do §8.6, nenhum texto de exemplo", async () => {
    const conferir = () => {
      expect(textosVisiveis(tela.raiz).filter((t) => !textoPermitido(t))).toEqual([]);
      expect(tela.raiz.querySelectorAll("[placeholder]").length).toBe(0);
    };
    await esperar(() => selecionar(principal()[2].id));
    conferir();
    for (const r of ["Entrada", "Transição", "Corte", "Escurecer", "Duração (s)", "Som", "Acompanhar", "Cortar"]) expect(textosVisiveis(tela.raiz)).toContain(r);
    await clicar(botao(tela.raiz, "Saída"));
    await clicar(tela.raiz.querySelector<HTMLButtonElement>('[data-campo="Transição"] button[data-valor="circulo"]')!);
    conferir();
    for (const r of ["Fecho", "Transição", "Duração (s)", "Som", "Acompanhar", "Cortar"]) expect(textosVisiveis(tela.raiz)).toContain(r);
  });
});
