// @vitest-environment happy-dom
/**
 * Quadros-chave no Inspetor de verdade (P4.2, D7): texto livre (Posição, Giro, Escala, Opacidade) e o volume do clipe
 * de áudio — "Chave no cursor" / "Tirar chave", o campo gravando a chave do cursor, a raia (mover, interpolação), um
 * passo por ação, só rótulos de (h) — e a alça do palco com chaves (mesma regra).
 */

import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { act } from "react";
import type { Chave, EfEnquadramento, ProjetoAberto, ProjetoEdicao, SobTitulo, TabelaQuadros } from "@domain/edicao";

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
  edicaoSpriteTexto: vi.fn(),
}));
vi.mock("@core/commands", () => ({ commands: cmd }));
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn(async () => () => {}) }));
vi.mock("@tauri-apps/plugin-dialog", () => ({ open: vi.fn(async () => null) }));
vi.mock("@components/Dialog/ask", () => ({ askConfirm: vi.fn(async () => true) }));

import { useEdicaoStore } from "../../store/edicaoStore";
import { Inspetor } from "../../inspetor/Inspetor";
import { esquecerLeituras } from "../../inspetor/contexto";
import { useInspetorUi } from "../../inspetor/ui";
import { arrastar, botao, clicar, desmontarTudo, digitar, entradaDe, esperar, lerFixture, montar, textosVisiveis, type Tela } from "../../inspetor/testeTela";
import { textoPermitido } from "../../inspetor/rotulos.teste";
import { adicionarTextoLivre } from "../../inspetor/secoes/textoLivre";
import { ptsDaTabela, resolver, tempoDoQuadro } from "../../modelo/tempo";
import { useRecursos } from "../../palco/recursos";
import { aplicarMudanca } from "../../modelo/projeto";
import { alcasDoLivre, livresNoQuadro } from "../../palco/livres";
import { useCurvaUi } from "./CurvasDoItem";
import { CurvasNaLinha, itemComCurva } from "./NaLinha";
import { LINHA_PRONTA, LinhaDoTempo } from "../LinhaDoTempo";

const st = () => useEdicaoStore.getState();
const doc = () => st().doc!;
const passos = () => st().passado.length;
const ultimo = () => st().passado.at(-1)?.rotulo;
const textoDe = (id: string) => doc().sobreposicoes.flatMap((t) => t.clipes).find((c) => (c as SobTitulo).id === id) as SobTitulo;
const depois = async () => vi.setSystemTime(Date.now() + 1500);

let tela: Tela;

beforeEach(async () => {
  vi.useFakeTimers({ toFake: ["Date"], now: new Date("2026-11-18T10:00:00Z") });
  for (const f of Object.values(cmd)) f.mockReset();
  cmd.edicaoSalvar.mockImplementation(async () => ({ updated_at: new Date().toISOString() }));
  cmd.edicaoAnalisar.mockImplementation(async () => ({ problemas: [], linha: { fps: { num: 30, den: 1 }, total_q: 0, clipes: [], livres: [] }, midias: [], prender: [], saida_auto: null }));
  cmd.edicaoTabelaQuadros.mockRejectedValue({ kind: "validation", message: "validation error: tabela_pendente: ainda não" });
  cmd.edicaoMidiaInfo.mockRejectedValue({ kind: "validation", message: "validation error: tabela_pendente: ainda não" });
  cmd.edicaoSpriteTexto.mockImplementation(async () => ({ relativo: "sprites/x.png", w: 100, h: 40 }));
  esquecerLeituras();
  useInspetorUi.setState({ aba: "clipe", ferramenta: null, aoMarcar: null, chave: null });
  useCurvaUi.setState({ foco: {}, escolhida: null });
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

/** Texto livre novo no slot 100 (preso ao trecho 1, que começa no 90): o item vai de 100 a 250. */
async function criar(): Promise<{ id: string; raiz: HTMLElement; inicio: number }> {
  await esperar(() => st().posicionar({ modo: "programa", q: 100 }));
  let id: string | null = null;
  await esperar(() => {
    id = adicionarTextoLivre();
  });
  const raiz = tela.raiz.querySelector(`[data-texto-livre="${id}"]`) as HTMLElement;
  const inicio = resolver(doc(), { tabela: () => null }).livres.find((l) => l.id === id)!.inicio_q;
  return { id: id!, raiz, inicio };
}

const raia = (raiz: HTMLElement, prop: string) => raiz.querySelector<HTMLElement>(`[data-raia="${prop}"]`)!;
const chave = (raiz: HTMLElement, prop: string) => raia(raiz, prop).querySelector<HTMLButtonElement>("button[aria-pressed]")!;
const ir = (q: number) => esperar(() => st().posicionar({ modo: "programa", q }));

describe("texto livre: chaves no Inspetor", () => {
  it("as quatro raias com os rótulos de (h); Escala e Opacidade novos; nada fora de (h)", async () => {
    const { raiz } = await criar();
    expect([...raiz.querySelectorAll("[data-raia]")].map((r) => r.getAttribute("data-raia"))).toEqual(["posicao", "rotacao", "escala", "opacidade"]);
    for (const r of ["Posição", "Giro", "Escala", "Opacidade"]) expect(textosVisiveis(raiz)).toContain(r);
    expect([...raiz.querySelectorAll("[data-raia] button[aria-pressed]")].map((b) => b.getAttribute("title"))).toEqual(Array(4).fill("Chave no cursor"));
    expect(textosVisiveis(tela.raiz).filter((t) => !textoPermitido(t))).toEqual([]);
    // títulos e nomes dos botões das raias (os do campo numérico são do componente de sempre)
    expect([...raiz.querySelectorAll("[data-curvas] [title], [data-curvas] [aria-label]")].map((e) => e.getAttribute("title") ?? e.getAttribute("aria-label")!).filter((t) => !textoPermitido(t))).toEqual([]);
  });

  it("Chave no cursor na posição: x e y ganham a chave do instante (suave), um passo; o campo grava a chave do cursor", async () => {
    const { id, raiz, inicio } = await criar();
    const x0 = textoDe(id).transformacao.x as number;
    const y0 = textoDe(id).transformacao.y as number;
    await ir(inicio + 15);
    const n = passos();
    await clicar(chave(raiz, "posicao"));
    expect(textoDe(id).transformacao.x).toEqual([{ t: 0.5, v: x0, interp: "suave" }]);
    expect(textoDe(id).transformacao.y).toEqual([{ t: 0.5, v: y0, interp: "suave" }]);
    expect([passos(), ultimo()]).toEqual([n + 1, "Chave no cursor"]);
    expect(chave(raiz, "posicao").getAttribute("title")).toBe("Tirar chave");
    // outro instante: o campo x grava a chave ali, e y ganha a dele (a raia é uma só)
    await ir(inicio + 45);
    expect(chave(raiz, "posicao").getAttribute("title")).toBe("Chave no cursor");
    const [cx] = [...raiz.querySelectorAll<HTMLInputElement>('[data-campo="Posição"] input')];
    await digitar(cx!, "25");
    expect(textoDe(id).transformacao.x).toEqual([
      { t: 0.5, v: x0, interp: "suave" },
      { t: 1.5, v: 0.25, interp: "suave" },
    ]);
    expect(textoDe(id).transformacao.y).toEqual([
      { t: 0.5, v: y0, interp: "suave" },
      { t: 1.5, v: y0, interp: "suave" },
    ]);
    // no meio, o campo mostra a curva (suave: metade do caminho no meio)
    await ir(inicio + 30);
    expect(Number(cx!.value.replace(",", "."))).toBeCloseTo(((x0 + 0.25) / 2) * 100, 0);
    expect(raia(raiz, "posicao").querySelectorAll("[data-chave]")).toHaveLength(2);
  });

  it("Giro, Escala e Opacidade: fixo troca o valor; com chaves, chave no cursor; Tirar chave volta a fixo", async () => {
    const { id, raiz, inicio } = await criar();
    await digitar(entradaDe(raiz, "Escala") as HTMLInputElement, "150");
    expect(textoDe(id).transformacao.escala).toBe(1.5);
    await depois();
    await digitar(entradaDe(raiz, "Opacidade") as HTMLInputElement, "40");
    expect(textoDe(id).transformacao.opacidade).toBeCloseTo(0.4, 12);
    await ir(inicio);
    await clicar(chave(raiz, "opacidade"));
    await ir(inicio + 30);
    await depois();
    await digitar(entradaDe(raiz, "Opacidade") as HTMLInputElement, "100");
    expect(textoDe(id).transformacao.opacidade).toEqual([
      { t: 0, v: 0.4, interp: "suave" },
      { t: 1, v: 1, interp: "suave" },
    ]);
    await ir(inicio + 10);
    await depois();
    await clicar(chave(raiz, "rotacao"));
    await digitar(entradaDe(raiz, "Giro") as HTMLInputElement, "-15");
    expect(textoDe(id).transformacao.rotacao).toEqual([{ t: 1 / 3, v: -15, interp: "suave" }]);
    // a última chave que sai deixa o valor dela
    const n = passos();
    await clicar(chave(raiz, "rotacao"));
    expect(textoDe(id).transformacao.rotacao).toBe(-15);
    expect([passos(), ultimo()]).toEqual([n + 1, "Tirar chave"]);
    // cursor fora do texto: sem chave nova
    await ir(inicio - 5);
    expect(chave(raiz, "escala").disabled).toBe(true);
    expect(textosVisiveis(tela.raiz).filter((t) => !textoPermitido(t))).toEqual([]);
  });

  it("raia: escolher a chave mostra Linear, Suave, Degrau; trocar e arrastar levam x e y juntos, um passo cada", async () => {
    const { id, raiz, inicio } = await criar();
    await ir(inicio);
    await clicar(chave(raiz, "posicao"));
    await ir(inicio + 60);
    await clicar(chave(raiz, "posicao"));
    await ir(inicio + 120);
    await clicar(chave(raiz, "posicao"));
    const faixa = raia(raiz, "posicao").querySelector<HTMLElement>("[data-curva]")!;
    // 150 slots em 300 px: 2 px por slot
    faixa.getBoundingClientRect = () => ({ left: 0, top: 0, width: 300, height: 22, right: 300, bottom: 22, x: 0, y: 0, toJSON: () => ({}) });
    const losango = (i: number) => faixa.querySelector<HTMLButtonElement>(`[data-chave="${i}"]`)!;
    const p = (el: HTMLElement, tipo: string, x: number) => act(async () => void el.dispatchEvent(new PointerEvent(tipo, { bubbles: true, clientX: x, button: 0, pointerId: 1 })));
    // clique na primeira: escolhe e leva o cursor ao instante dela
    await p(losango(0), "pointerdown", 1);
    await p(losango(0), "pointerup", 1);
    expect(st().cursor).toEqual({ modo: "programa", q: inicio });
    const grupo = raiz.querySelector<HTMLElement>('[data-interp-de="posicao"]')!;
    expect([...grupo.querySelectorAll("button")].map((b) => [b.textContent, b.getAttribute("aria-pressed")])).toEqual([
      ["Linear", "false"],
      ["Suave", "true"],
      ["Degrau", "false"],
    ]);
    let n = passos();
    await clicar(botao(grupo, "Degrau"));
    expect((textoDe(id).transformacao.x as Chave<number>[]).map((k) => k.interp)).toEqual(["degrau", "suave", "suave"]);
    expect((textoDe(id).transformacao.y as Chave<number>[]).map((k) => k.interp)).toEqual(["degrau", "suave", "suave"]);
    expect([passos(), ultimo()]).toEqual([n + 1, "Degrau"]);
    // a do meio vai do slot 60 ao 90 do item
    n = passos();
    await p(losango(1), "pointerdown", 121);
    await p(losango(1), "pointermove", 151);
    await p(losango(1), "pointermove", 181);
    await p(losango(1), "pointerup", 181);
    expect((textoDe(id).transformacao.x as Chave<number>[]).map((k) => k.t)).toEqual([0, 3, 4]);
    expect((textoDe(id).transformacao.y as Chave<number>[]).map((k) => k.t)).toEqual([0, 3, 4]);
    expect([passos(), ultimo()]).toEqual([n + 1, "Posição"]);
    // a última não tem trecho depois dela: a interpolação fica desligada
    await p(losango(2), "pointerdown", 241);
    await p(losango(2), "pointerup", 241);
    expect([...raiz.querySelectorAll<HTMLButtonElement>('[data-interp-de="posicao"] button')].every((b) => b.disabled)).toBe(true);
    // desfazer volta passo a passo
    st().desfazer();
    expect((textoDe(id).transformacao.x as Chave<number>[]).map((k) => k.t)).toEqual([0, 2, 4]);
  });

  it("alça do palco com chaves: grava a chave do cursor (nova nasce suave; a que existe fica com a curva dela)", async () => {
    const { id, inicio } = await criar();
    await esperar(() => st().alterar("Texto", (d) => void ((d.sobreposicoes[0]!.clipes[0] as SobTitulo).titulo.blocos[0]!.conteudo = "Policiais")));
    await esperar(() =>
      st().alterar("Chave no cursor", (d) => {
        const t = (d.sobreposicoes[0]!.clipes[0] as SobTitulo).transformacao;
        t.x = [{ t: 0, v: 0.5, interp: "degrau" }];
        t.y = [{ t: 0, v: 0.5, interp: "degrau" }];
      }),
    );
    const linha = resolver(doc(), { tabela: () => null });
    const fontes = { sprite: () => ({ relativo: "sprites/x.png", w: 400, h: 70 }), atlas: () => null, midia: () => null };
    const mover = (q: number, dx: number) => {
      const item = livresNoQuadro(doc(), linha, q, fontes)[0]!;
      const a = alcasDoLivre(doc(), item, { w: 1920, h: 1080 }).find((x) => x.id.endsWith(":mover"))!;
      st().alterar("Posição", (d) => void aplicarMudanca(d, a.arrastar(a.em, { x: a.em.x + dx / 1920, y: a.em.y }, { shift: false, alt: false })));
    };
    await esperar(() => mover(inicio, 192));
    expect(textoDe(id).transformacao.x).toEqual([{ t: 0, v: 0.6, interp: "degrau" }]);
    await esperar(() => mover(inicio + 30, -384));
    expect(textoDe(id).transformacao.x).toEqual([
      { t: 0, v: 0.6, interp: "degrau" },
      { t: 1, v: 0.4, interp: "suave" },
    ]);
  });
});

describe("volume do clipe de áudio", () => {
  it("Volume no cursor e a raia: Chave no cursor nasce linear (sem o campo); Tirar chave volta a fixo", async () => {
    await esperar(() =>
      st().alterar("Importar…", (d) => {
        d.audios[0]!.clipes = [{ id: "n1", nome: "narração", midia: "m3", pos: { ref: "programa", t: 1 }, de_s: 0, ate_s: 4, volume: 0.8, entrada_s: 0, saida_s: 0 }];
      }),
    );
    const trilha = doc().audios[0]!.id;
    await esperar(() => st().selecionar({ itens: [{ k: "audio", id: "n1", trilha }], foco: { k: "audio", id: "n1", trilha }, efeito: null }));
    const raiz = tela.raiz.querySelector<HTMLElement>('[data-audio="n1"]')!;
    expect(raiz).not.toBeNull();
    const volume = () => doc().audios[0]!.clipes[0]!.volume;
    const ch = () => raia(raiz, "volume").querySelector<HTMLButtonElement>("button[aria-pressed]")!;
    await ir(30 + 15);
    await clicar(ch());
    expect(volume()).toEqual([{ t: 0.5, v: 0.8 }]);
    await ir(30 + 45);
    const som = raiz.querySelector<HTMLInputElement>('[data-campo="Volume"] input[type="range"]')!;
    const n = passos();
    await arrastar(som, [0.6, 0.3]);
    expect(volume()).toEqual([{ t: 0.5, v: 0.8 }, { t: 1.5, v: 0.3 }]);
    expect(passos()).toBe(n + 1);
    await ir(30 + 30);
    expect(Number(som.value)).toBeCloseTo(0.55, 9);
    await ir(30 + 15);
    await clicar(ch());
    await ir(30 + 45);
    await clicar(ch());
    expect(volume()).toBe(0.3);
    expect(textosVisiveis(tela.raiz).filter((t) => !textoPermitido(t))).toEqual([]);
  });
});

describe("na linha do tempo (ponto de montagem do P4.1)", () => {
  async function animado() {
    const { id, inicio } = await criar();
    await esperar(() =>
      st().alterar("Chave no cursor", (d) => {
        const t = (d.sobreposicoes[0]!.clipes[0] as SobTitulo).transformacao;
        t.x = [{ t: 0, v: 0.3, interp: "suave" }, { t: 2, v: 0.6 }];
        t.y = [{ t: 0, v: 0.5, interp: "suave" }, { t: 2, v: 0.5 }];
        t.opacidade = [{ t: 1, v: 0.2 }, { t: 3, v: 1 }];
      }),
    );
    return { id, inicio, trilha: doc().sobreposicoes[0]!.id };
  }

  it("item da linha → item com curvas: sobreposto, clipe de áudio, som do trecho; o resto não", () => {
    expect(itemComCurva({ k: "livre", id: "a", trilha: "s1" })).toEqual({ k: "livre", id: "a" });
    expect(itemComCurva({ k: "audio", id: "b", trilha: "a1" })).toEqual({ k: "audio", id: "b" });
    expect(itemComCurva({ k: "clipe", id: "c", trilha: "som" })).toEqual({ k: "som", id: "c" });
    expect(itemComCurva({ k: "clipe", id: "c", trilha: "principal" })).toBeNull();
    expect(itemComCurva({ k: "efeito", id: "e", trilha: "x" })).toBeNull();
  });

  it("por cima do item: as chaves da raia em foco (sem foco, a primeira com chaves), arrastáveis; a curva da opacidade", async () => {
    const { id, trilha } = await animado();
    const t = await montar(<div style={{ position: "relative", width: 300, height: 34 }}><CurvasNaLinha item={{ k: "livre", id, trilha }} /></div>);
    const faixa = () => t.raiz.querySelector<HTMLElement>("[data-curva]")!;
    expect(faixa().getAttribute("data-curva")).toBe("Posição");
    expect(faixa().querySelectorAll("[data-chave]")).toHaveLength(2);
    expect(faixa().querySelector("polyline")).toBeNull();
    // a raia escolhida no Inspetor é a que a linha mostra
    await esperar(() => useCurvaUi.getState().focar(id, "opacidade"));
    expect(faixa().getAttribute("data-curva")).toBe("Opacidade");
    expect(faixa().querySelector("polyline")).not.toBeNull();
    faixa().getBoundingClientRect = () => ({ left: 0, top: 0, width: 300, height: 34, right: 300, bottom: 34, x: 0, y: 0, toJSON: () => ({}) });
    const l = faixa().querySelector<HTMLButtonElement>('[data-chave="1"]')!;
    const p = (tipo: string, x: number) => act(async () => void l.dispatchEvent(new PointerEvent(tipo, { bubbles: true, clientX: x, button: 0, pointerId: 1 })));
    const n = passos();
    await p("pointerdown", 181);
    await p("pointermove", 121);
    await p("pointerup", 121);
    expect(textoDe(id).transformacao.opacidade).toEqual([{ t: 1, v: 0.2 }, { t: 2, v: 1 }]);
    expect([passos(), ultimo()]).toEqual([n + 1, "Opacidade"]);
  });

  it("barra do enquadramento: as chaves da região na faixa da barra; arrastar leva o pts do quadro novo", async () => {
    const tab: TabelaQuadros = { tb: { num: 1, den: 15360 }, pts0: 0, passos: Array(1799).fill(512), passo_mediano: 512, chaves: [0], vfr: false, origem: "decodificador" };
    const sha = doc().midias.find((m) => m.id === "m1")!.sha256;
    useRecursos.setState({ midias: { [sha]: { info: null, tabela: tab, erro: null, pedindo: false } as never } });
    try {
      const pts = ptsDaTabela(tab);
      const ch = (j: number) => ({ t: tempoDoQuadro(tab, j), pts: pts[j]!, v: { x: 0.2, y: 0.2, w: 0.5, h: 0.5 }, interp: "suave" as const });
      const trecho = (doc().principal[1] as { id: string }).id;
      await esperar(() =>
        st().alterar("Chave no cursor", (d) => {
          (d.principal[1] as { efeitos: unknown[] }).efeitos.push({ tipo: "enquadramento", id: "z1", ligado: true, nome: "", ativo: { de: 12.5, ate: 15 }, regiao: [ch(380), ch(400)], ajuste: "cobrir", interpolacao: "bilinear" });
        }),
      );
      // a barra do P4.1 é só o `ativo`: slots 105 a 180 (12,5 s a 15 s de fonte no trecho que começa no 90 em 12 s)
      const t = await montar(<CurvasNaLinha item={{ k: "efeito", id: "z1", trilha: "enquadramento", tipo: "enquadramento", de_q: 105, ate_q: 180 }} />);
      const faixa = t.raiz.querySelector<HTMLElement>("[data-curva]")!;
      expect(faixa.getAttribute("data-curva")).toBe("Enquadramento");
      expect([...faixa.querySelectorAll("[data-chave]")].map((b) => b.getAttribute("data-q"))).toEqual(["110", "130"]);
      faixa.getBoundingClientRect = () => ({ left: 0, top: 0, width: 750, height: 20, right: 750, bottom: 20, x: 0, y: 0, toJSON: () => ({}) });
      const l = faixa.querySelector<HTMLButtonElement>('[data-chave="1"]')!;
      const p = (tipo: string, x: number) => act(async () => void l.dispatchEvent(new PointerEvent(tipo, { bubbles: true, clientX: x, button: 0, pointerId: 1 })));
      // 10 px por slot: do slot 130 (quadro 400) ao 140 (quadro 410)
      await p("pointerdown", 255);
      await p("pointermove", 355);
      await p("pointerup", 355);
      const z = (doc().principal[1] as { efeitos: EfEnquadramento[] }).efeitos.find((e) => e.id === "z1")!;
      expect(z.regiao).toEqual([ch(380), { ...ch(410), interp: "suave" }]);
      expect(trecho).toBeTruthy();
    } finally {
      useRecursos.setState({ midias: {} });
    }
  });

  it("sem nenhuma chave, nada por cima do item", async () => {
    const { id, raiz } = await criar();
    void raiz;
    const t = await montar(<CurvasNaLinha item={{ k: "livre", id, trilha: doc().sobreposicoes[0]!.id }} />);
    expect(t.raiz.querySelector("[data-curva]")).toBeNull();
  });

  // a linha (P4.1) não está na base desta onda: o fecho liga `CurvasNaLinha` no `ClipeNaLinha` e tira o skip
  it.skipIf(!LINHA_PRONTA)("fecho do bloco 6.4: P4.1 — losangos da posição no item do texto, na linha do tempo", async () => {
    const { id, trilha } = await animado();
    await esperar(() => st().selecionar({ itens: [{ k: "livre", id, trilha }], foco: { k: "livre", id, trilha }, efeito: null }));
    const t = await montar(<LinhaDoTempo palco={() => null} atalhos={false} />);
    const item = t.raiz.querySelector<HTMLElement>(`[data-item="${id}"]`)!;
    expect(item.querySelector('[data-curva="Posição"]')!.querySelectorAll("[data-chave]")).toHaveLength(2);
  });
});
