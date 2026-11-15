// @vitest-environment happy-dom
/**
 * Presos ao clipe (P1.7) montados de verdade: o que está preso aparece no cartão do clipe e continua nele quando a
 * principal é reordenada e o clipe aparado (e a linha resolvida dá ao terço inferior "até o fim do clipe" os slots do
 * clipe); "Adicionar" cria título e terço inferior no modelo; imagem e narração pela importação; só rótulos do §8.6.
 */

import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { ClipeTrecho, MidiaRef, ProjetoAberto, ProjetoEdicao, SobImagem, SobTitulo, TabelaQuadros } from "@domain/edicao";

const cmd = vi.hoisted(() => ({ edicaoAbrir: vi.fn(), edicaoSalvar: vi.fn(), edicaoAnalisar: vi.fn(), edicaoMidiaInfo: vi.fn(), edicaoTabelaQuadros: vi.fn(), edicaoMidiaImportar: vi.fn() }));
vi.mock("@core/commands", () => ({ commands: cmd }));
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn(async () => () => {}) }));
const dialogo = vi.hoisted(() => ({ open: vi.fn(async (_o: { filters: { extensions: string[] }[] }): Promise<string | null> => null) }));
vi.mock("@tauri-apps/plugin-dialog", () => dialogo);

import { useEdicaoStore } from "../store/edicaoStore";
import { useMenusAbertos } from "../menus";
import { Presos, PRESOS_PRONTOS, prenderTituloNoModelo } from "./Presos";
import { moverPara } from "./acoes";
import { DURACAO_PADRAO, presosAoClipe } from "./contas";
import { resolver } from "../modelo/tempo";
import { tituloNoModelo } from "../modelo/titulos";
import { EXTENSOES } from "../inspetor/secoes/importar";
import { ROTULOS, textoPermitido } from "../inspetor/rotulos.teste";
import { botao, clicar, desmontarTudo, esperar, lerFixture, montar, tecla, textosVisiveis } from "../inspetor/testeTela";

const st = () => useEdicaoStore.getState();
const doc = () => st().doc!;
const [T, C1, C2] = ["00000000-0000-4000-8000-000000000004", "00000000-0000-4000-8000-000000000002", "00000000-0000-4000-8000-000000000003"];
const CANVAS = { w: 1920, h: 1080 };

/** Tabela 30 qps em base 1/15360 (a das mídias da fixture). */
const tabela = (n: number): TabelaQuadros => ({ tb: { num: 1, den: 15360 }, pts0: 0, passos: Array(n - 1).fill(512), passo_mediano: 512, chaves: [0], vfr: false, origem: "decodificador" });
const TABELAS: Record<string, TabelaQuadros> = { m1: tabela(1800), m2: tabela(1200) };
const linha = () => resolver(doc(), { tabela: (id) => TABELAS[id] ?? null });

/** Slots do clipe da principal e do livre na linha resolvida. */
function slots(id: string, livre = false): [number, number] {
  const l = linha();
  const c = livre ? l.livres.find((x) => x.id === id) : l.clipes.find((x) => x.id === id);
  if (!c) throw new Error(`${id} fora da linha`);
  return [c.inicio_q, c.n_q];
}

beforeEach(async () => {
  for (const f of Object.values(cmd)) f.mockReset();
  dialogo.open.mockReset();
  dialogo.open.mockResolvedValue(null);
  cmd.edicaoSalvar.mockImplementation(async () => ({ updated_at: "2026-11-14T12:00:00Z" }));
  cmd.edicaoAnalisar.mockImplementation(() => new Promise(() => {}));
  for (const f of [cmd.edicaoMidiaInfo, cmd.edicaoTabelaQuadros]) f.mockImplementation(() => new Promise(() => {}));
  const projeto = lerFixture<ProjetoEdicao>("projeto-do-store.json");
  cmd.edicaoAbrir.mockResolvedValue({ projeto, importado_de: null, so_leitura: false, avisos: [], raiz_cache: "/cache/c1" } satisfies ProjetoAberto);
  await st().abrir("/casos/c1", projeto.id);
  st().selecionar({ itens: [], foco: null, efeito: null });
  st().posicionar({ modo: "programa", q: 0 });
});

afterEach(async () => {
  desmontarTudo();
  await st()
    .fechar({ descartar: true })
    .catch(() => {});
});

const focar = (id: string) => st().selecionar({ itens: [{ k: "clipe", id }], foco: { k: "clipe", id }, efeito: null });

describe("Presos no cartão", () => {
  // desligada: o teste do cartão em `telas.test.tsx` monta `CartaoClipe` sem id e sem store; liga com a queda do
  // cartão para o bloco da Fase 0 sem id (relato ao integrador)
  it("bandeira", () => {
    expect(typeof PRESOS_PRONTOS).toBe("boolean");
  });

  it("mostra o que está preso ao clipe; sem preso, só no cartão em foco, com 'Adicionar'", async () => {
    const t = await montar(
      <>
        <Presos clipe={T} />
        <Presos clipe={C1} />
      </>,
    );
    // a narração da fixture está presa ao primeiro trecho: botão com o nome dela
    expect(t.raiz.querySelectorAll("[data-presos]").length).toBe(1);
    expect(t.raiz.querySelector(`[data-presos="${C1}"] [aria-label="narracao.wav"]`)).not.toBeNull();
    await esperar(() => focar(T));
    expect(t.raiz.querySelector(`[data-presos="${T}"]`)).not.toBeNull();
    expect(botao(t.raiz.querySelector(`[data-presos="${T}"]`)!, "Adicionar")).toBeTruthy();
    // clicar no preso seleciona ele (narração: aba Áudio)
    await clicar(t.raiz.querySelector<HTMLButtonElement>(`[data-presos="${C1}"] [data-preso]`)!);
    expect(st().selecao.foco).toEqual({ k: "audio", id: doc().audios[0]!.clipes[0]!.id, trilha: doc().audios[0]!.id });
  });

  it("Adicionar → Terço inferior e Título: no modelo, sem fundo, presos ao clipe; um passo cada; o novo fica selecionado", async () => {
    await esperar(() => focar(C1));
    const t = await montar(<Presos clipe={C1} />);
    const n = st().passado.length;
    await clicar(botao(t.raiz, "Adicionar"));
    expect(t.raiz.querySelector('[role="menu"]')).not.toBeNull();
    expect([...t.raiz.querySelectorAll('[role="menuitem"]')].map((b) => b.textContent)).toEqual(["Título", "Terço inferior", "Imagem", "Narração"]);
    await clicar(botao(t.raiz, "Terço inferior"));
    expect(t.raiz.querySelector('[role="menu"]')).toBeNull();
    const terco = doc().sobreposicoes[0]!.clipes.at(-1) as SobTitulo;
    expect(terco.pos).toEqual({ ref: "clipe", clipe: C1, desloc: 0 });
    expect(terco.duracao_s).toBeNull();
    expect(terco.titulo).toEqual(tituloNoModelo("terco_inferior", true, terco.titulo.blocos[0]!.id, CANVAS));
    expect(terco.titulo.fundo).toEqual({ tipo: "transparente" });
    expect(st().selecao.foco).toEqual({ k: "livre", id: terco.id, trilha: doc().sobreposicoes[0]!.id });
    await clicar(botao(t.raiz, "Adicionar"));
    // no menu: o preso novo também se chama "Título" (sem rótulo próprio)
    await clicar(botao(t.raiz.querySelector('[role="menu"]')!, "Título"));
    const ev = doc().sobreposicoes[0]!.clipes.at(-1) as SobTitulo;
    expect([ev.titulo.modelo, ev.duracao_s, ev.titulo.fundo.tipo]).toEqual(["evento", DURACAO_PADRAO, "transparente"]);
    expect(ev.titulo).toEqual(tituloNoModelo("evento", true, ev.titulo.blocos[0]!.id, CANVAS));
    expect(st().passado.slice(-2).map((p) => p.rotulo)).toEqual(["Terço inferior", "Título"]);
    expect(st().passado.length).toBe(n + 2);
    // os dois aparecem no cartão, com o ícone do título
    expect(t.raiz.querySelectorAll("[data-preso]").length).toBe(3);
  });

  it("Imagem e Narração: diálogo com as extensões do Rust, importação e o preso no clipe", async () => {
    await esperar(() => focar(C2));
    const t = await montar(<Presos clipe={C2} />);
    const img: MidiaRef = { ...doc().midias[0]!, id: "img1", tipo: "imagem", origem: "projeto", sha256: "9".repeat(64), nome: "brasao.png", relativo: "x/brasao.png" };
    dialogo.open.mockResolvedValueOnce("/fora/brasao.png");
    cmd.edicaoMidiaImportar.mockResolvedValueOnce(img);
    await clicar(botao(t.raiz, "Adicionar"));
    await clicar(botao(t.raiz, "Imagem"));
    await esperar(() => new Promise((r) => setTimeout(r, 0)));
    expect(dialogo.open.mock.calls.at(-1)?.[0].filters[0]?.extensions).toEqual(EXTENSOES.imagem);
    expect(cmd.edicaoMidiaImportar).toHaveBeenLastCalledWith("/casos/c1", doc().id, "/fora/brasao.png", "imagem");
    const sob = doc().sobreposicoes[0]!.clipes.at(-1) as SobImagem;
    expect([sob.tipo, sob.midia, sob.pos]).toEqual(["imagem", "img1", { ref: "clipe", clipe: C2, desloc: 0 }]);
    // narração: o diálogo cancelado não muda nada
    const antes = JSON.stringify(doc());
    await clicar(botao(t.raiz, "Adicionar"));
    await clicar(botao(t.raiz, "Narração"));
    await esperar(() => new Promise((r) => setTimeout(r, 0)));
    expect(dialogo.open.mock.calls.at(-1)?.[0].filters[0]?.extensions).toEqual(EXTENSOES.audio);
    expect(JSON.stringify(doc())).toBe(antes);
  });

  it("terço inferior até o fim do clipe: segue o clipe reordenado e aparado, nos slots da linha e no cartão", async () => {
    let item: { id: string } | null = null;
    st().alterar("Terço inferior", (d) => void (item = prenderTituloNoModelo(d, C1, "terco_inferior")));
    const id = item!.id;
    await esperar(() => focar(C1));
    const t = await montar(
      <>
        <Presos clipe={T} />
        <Presos clipe={C1} />
        <Presos clipe={C2} />
      </>,
    );
    const noCartao = (c: string) => [...t.raiz.querySelectorAll(`[data-presos="${c}"] [data-preso]`)].map((b) => b.getAttribute("data-preso"));
    const conferir = (nome: string) => {
      expect(slots(id, true), nome).toEqual(slots(C1));
      expect(noCartao(C1), nome).toContain(id);
    };
    conferir("no meio");
    const [a0] = slots(C1);
    expect(a0).toBe(90);
    // reordenar: o trecho vai para o começo
    await esperar(() => moverPara(1, 0));
    expect(slots(C1)[0]).toBe(0);
    conferir("reordenado");
    // aparar: fim e começo novos (pts da tabela)
    await esperar(() =>
      st().alterar("Fim", (d) => {
        const c = d.principal.find((x) => (x as { id?: string }).id === C1) as ClipeTrecho;
        c.inicio_s = 13;
        c.inicio_pts = 13 * 15360;
        c.fim_s = 14;
        c.fim_pts = 14 * 15360;
      }),
    );
    expect(slots(C1)[1]).toBe(30);
    conferir("aparado");
    // para o fim, depois do outro trecho
    await esperar(() => moverPara(0, 2));
    expect(slots(C1)[0]).toBe(90 + 150);
    conferir("aparado e reordenado");
    expect(presosAoClipe(doc(), C1).map((p) => p.id)).toContain(id);
    // marca fixa (presa ao programa, sem duração): o programa inteiro
    await esperar(() =>
      st().alterar("Título", (d) => {
        const s = prenderTituloNoModelo(d, C1, "evento");
        const c = d.sobreposicoes[0]!.clipes.find((x) => (x as SobTitulo).id === s.id) as SobTitulo;
        c.pos = { ref: "programa", t: 0 };
        c.duracao_s = null;
        item = s;
      }),
    );
    expect(slots(item!.id, true)).toEqual([0, linha().total_q]);
  });

  it("menu aberto desliga os atalhos; Esc fecha; só rótulos do §8.6, sem texto de exemplo", async () => {
    await esperar(() => focar(C1));
    const t = await montar(<Presos clipe={C1} />);
    await clicar(botao(t.raiz, "Adicionar"));
    // o editor desliga os atalhos com menu aberto (Del não remove o clipe por trás)
    expect(useMenusAbertos.getState().n).toBe(1);
    const textos = textosVisiveis(t.raiz);
    expect(textos.filter((x) => !textoPermitido(x) && x !== "narracao.wav")).toEqual([]);
    for (const r of ["Adicionar", "Título", "Terço inferior", "Imagem", "Narração"]) expect(ROTULOS.has(r), r).toBe(true);
    expect(t.raiz.querySelectorAll("[placeholder]").length).toBe(0);
    await tecla(document.body, "Escape");
    expect(t.raiz.querySelector('[role="menu"]')).toBeNull();
    expect(useMenusAbertos.getState().n).toBe(0);
  });

  it("só leitura: sem 'Adicionar'", async () => {
    desmontarTudo();
    await st().fechar({ descartar: true });
    const projeto = lerFixture<ProjetoEdicao>("projeto-do-store.json");
    cmd.edicaoAbrir.mockResolvedValueOnce({ projeto, importado_de: null, so_leitura: true, avisos: [], raiz_cache: "/cache/c1" } satisfies ProjetoAberto);
    await st().abrir("/casos/c1", projeto.id);
    await esperar(() => focar(C1));
    const t = await montar(<Presos clipe={C1} />);
    expect(t.raiz.querySelector("[data-preso]")).not.toBeNull();
    expect(() => botao(t.raiz, "Adicionar")).toThrow();
  });
});
