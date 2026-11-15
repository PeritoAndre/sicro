// @vitest-environment happy-dom
/**
 * Editor do título (P1.7) montado de verdade: "Modelo" arranja os blocos (o arranjo do Rust, pela fixture); a prévia
 * pede ao backend EXATAMENTE os pedidos de sprite que o render faz (fixture `primitivas-titulo.json`: mesmo pedido,
 * mesmo arquivo, mesmo SHA-256, conferido no Rust) e põe cada um na caixa do Rust; área segura com ícone; texto novo
 * no arranjo do modelo; "Até o fim do clipe"; só rótulos do §8.6.
 */

import { isDeepStrictEqual } from "node:util";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { ClipeTitulo, PedidoSprite, Primitiva, ProjetoAberto, ProjetoEdicao, SobTitulo, SpriteTexto, Titulo } from "@domain/edicao";

const cmd = vi.hoisted(() => ({
  edicaoAbrir: vi.fn(),
  edicaoSalvar: vi.fn(),
  edicaoAnalisar: vi.fn(),
  edicaoTabelaQuadros: vi.fn(),
  edicaoMidiaInfo: vi.fn(),
  edicaoSpriteTexto: vi.fn(),
}));
vi.mock("@core/commands", () => ({ commands: cmd }));
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn(async () => () => {}) }));
vi.mock("@tauri-apps/api/core", () => ({ convertFileSrc: (p: string) => `asset://${p}`, invoke: vi.fn() }));
vi.mock("@tauri-apps/plugin-dialog", () => ({ open: vi.fn(async () => null) }));

import { useEdicaoStore } from "../store/edicaoStore";
import { Inspetor } from "../inspetor/Inspetor";
import { esquecerLeituras } from "../inspetor/contexto";
import { useInspetorUi } from "../inspetor/ui";
import { EDITOR_TITULO_PRONTO, EditorTitulo } from "./EditorTitulo";
import { prenderTituloNoModelo } from "../sequencia/Presos";
import { aplicarModelo, blocoNoModelo, escalaDoCanvas, pedido, tituloNoModelo, type ModeloTitulo } from "../modelo/titulos";
import { textoPermitido } from "../inspetor/rotulos.teste";
import { campo, clicar, desmontarTudo, digitar, entradaDe, escolher, esperar, lerFixture, montar, sair, textosVisiveis } from "../inspetor/testeTela";

const st = () => useEdicaoStore.getState();
const doc = () => st().doc!;
const T = "00000000-0000-4000-8000-000000000004";
const C1 = "00000000-0000-4000-8000-000000000002";
const titulo = () => (doc().principal[0] as ClipeTitulo).titulo;
const CINCO: ModeloTitulo[] = ["cartela", "abertura", "evento", "terco_inferior", "encerramento"];

/** Sprite falso do backend: tamanho pelo texto, nome pelo pedido. */
const falso = (p: PedidoSprite): SpriteTexto => {
  const b = p.blocos[0]!;
  const tam = b.estilo.tamanho * p.altura_canvas;
  return { relativo: `sprites/${b.id}-${p.altura_canvas}.png`, w: Math.max(1, Math.round(b.conteudo.length * tam * 0.55)), h: Math.max(1, Math.round(tam * 1.25)) };
};

let n = 0;
async function abrir(mudar?: (p: ProjetoEdicao) => void) {
  const projeto = lerFixture<ProjetoEdicao>("projeto-do-store.json");
  mudar?.(projeto);
  cmd.edicaoAbrir.mockResolvedValueOnce({ projeto, importado_de: null, so_leitura: false, avisos: [], raiz_cache: "/cache/c1" } satisfies ProjetoAberto);
  // caso novo a cada teste: o cache de sprites da tela é por caso
  await st().abrir(`/casos/e${++n}`, projeto.id);
}

/** Igualdade estrita e profunda (−0 ≠ 0), como a da fixture. */
const igual = (a: unknown, b: unknown) => isDeepStrictEqual(a, b);
type Caso<E, S> = { nome: string; entrada: E; saida: S };

const pedidosFeitos = () => cmd.edicaoSpriteTexto.mock.calls.map((c) => c[1] as PedidoSprite);
const chegar = () => esperar(() => new Promise((r) => setTimeout(r, 0)));

beforeEach(async () => {
  vi.useFakeTimers({ toFake: ["Date"], now: new Date("2026-11-14T10:00:00Z") });
  for (const f of Object.values(cmd)) f.mockReset();
  cmd.edicaoSalvar.mockImplementation(async () => ({ updated_at: new Date().toISOString() }));
  cmd.edicaoAnalisar.mockImplementation(async () => ({ problemas: [], linha: { fps: { num: 30, den: 1 }, total_q: 0, clipes: [], livres: [] }, midias: [], prender: [], saida_auto: null }));
  cmd.edicaoTabelaQuadros.mockRejectedValue({ kind: "validation", message: "validation error: tabela_pendente: ainda não" });
  cmd.edicaoMidiaInfo.mockRejectedValue({ kind: "validation", message: "validation error: tabela_pendente: ainda não" });
  cmd.edicaoSpriteTexto.mockImplementation(async (_ws: string, p: PedidoSprite) => falso(p));
  esquecerLeituras();
  useInspetorUi.setState({ aba: "clipe", ferramenta: null, aoMarcar: null, chave: null });
  erroDoReact = vi.spyOn(console, "error");
});

let erroDoReact: ReturnType<typeof vi.spyOn> | null = null;

afterEach(async () => {
  // o pedido de sprite nunca muda estado durante o render (React avisa "while rendering")
  const avisos = erroDoReact?.mock.calls.map((c) => String(c[0])).filter((m) => /while rendering/.test(m)) ?? [];
  erroDoReact?.mockRestore();
  erroDoReact = null;
  expect(avisos).toEqual([]);
  desmontarTudo();
  await st()
    .fechar({ descartar: true })
    .catch(() => {});
  vi.useRealTimers();
});

const selecionar = (k: "clipe" | "livre", id: string, trilha?: string) => {
  const it = { k, id, ...(trilha ? { trilha } : {}) };
  st().selecionar({ itens: [it], foco: it, efeito: null });
};

describe("Editor do título", () => {
  it("ligado no Inspetor", () => {
    expect(EDITOR_TITULO_PRONTO).toBe(true);
  });

  it("Modelo: cada um dos cinco arranja blocos e fundo como o Rust (um passo 'Modelo' cada)", async () => {
    await abrir();
    const tela = await montar(<Inspetor />);
    await esperar(() => selecionar("clipe", T));
    // a fixture já está em cartela: ela por último (escolher o mesmo não é mudança)
    for (const m of [...CINCO.slice(1), CINCO[0]!]) {
      const antes = titulo();
      const k = st().passado.length;
      await escolher(entradaDe(tela.raiz, "Modelo") as HTMLSelectElement, m);
      expect(titulo()).toEqual(aplicarModelo(antes, m, false, { w: 1920, h: 1080 }));
      expect([st().passado.length, st().passado.at(-1)?.rotulo]).toEqual([k + 1, "Modelo"]);
    }
  });

  it("prévia: os pedidos de sprite do render (fixture do Rust), um por bloco, e cada sprite na caixa do Rust", async () => {
    type E = { titulo: Titulo; quadro: { canvas: { w: number; h: number } }; sprites: { pedido: PedidoSprite; sprite: SpriteTexto }[] };
    type S = { primitivas?: Primitiva[]; pedidos: PedidoSprite[] };
    const lista = lerFixture<Caso<E, S>[]>("primitivas-titulo.json").filter((c) => c.nome.startsWith("arranjo "));
    expect(lista.map((c) => c.nome)).toEqual([
      "arranjo cartela",
      "arranjo abertura",
      "arranjo evento",
      "arranjo evento preso 720p",
      "arranjo terco_inferior",
      "arranjo terco_inferior preso 720p",
      "arranjo encerramento",
    ]);
    for (const c of lista) {
      desmontarTudo();
      await st()
        .fechar({ descartar: true })
        .catch(() => {});
      cmd.edicaoSpriteTexto.mockClear();
      // o backend falso responde o que o falso do Rust respondeu
      cmd.edicaoSpriteTexto.mockImplementation(async (_ws: string, p: PedidoSprite) => c.entrada.sprites.find((s) => igual(p, s.pedido))?.sprite ?? falso(p));
      const { w, h } = c.entrada.quadro.canvas;
      await abrir((p) => {
        p.saida.largura = w;
        p.saida.altura = h;
      });
      const clipe = { ...(doc().principal[0] as ClipeTitulo), titulo: c.entrada.titulo };
      const tela = await montar(<EditorTitulo clipe={clipe} />);
      await chegar();
      // um pedido por bloco com texto, iguais aos do Rust (o último da fixture é o índice fora da lista)
      const doRust = c.saida.pedidos.slice(0, -1).filter((p) => p.blocos[0]!.conteudo !== "");
      expect(pedidosFeitos().length, c.nome).toBe(doRust.length);
      for (const [i, p] of pedidosFeitos().entries()) expect(igual(p, doRust[i]), `${c.nome} pedido ${i}`).toBe(true);
      // as imagens: o arquivo do cache e a caixa das primitivas do Rust, em % do canvas
      const imgs = [...tela.raiz.querySelectorAll<HTMLImageElement>("img[data-bloco]")];
      const sprites = (c.saida.primitivas ?? []).filter((p): p is Extract<Primitiva, { k: "sprite" }> => p.k === "sprite" && !p.src.startsWith("midia:"));
      expect(imgs.length, c.nome).toBe(sprites.length);
      imgs.forEach((img, i) => {
        const s = sprites[i]!;
        expect(img.getAttribute("src")).toBe(`asset:///cache/c1/${s.src}`);
        expect([img.style.left, img.style.top, img.style.width, img.style.height]).toEqual([s.caixa.x, s.caixa.y, s.caixa.w, s.caixa.h].map((v) => `${v * 100}%`));
      });
      // cada modelo do §8.6 cabe na área segura
      expect(tela.raiz.querySelector('[data-estado="fora"]'), c.nome).toBeNull();
      const previa = tela.raiz.querySelector<HTMLElement>("[data-previa-titulo]")!;
      expect(previa.getAttribute("data-canvas")).toBe(`${w}x${h}`);
    }
  });

  it("área segura: guia em 2 %; bloco que sai dela fica marcado, com ícone de estado e nenhum texto", async () => {
    await abrir();
    const base = doc().principal[0] as ClipeTitulo;
    const com = (x: number): ClipeTitulo => ({ ...base, titulo: { ...base.titulo, blocos: base.titulo.blocos.map((b, i) => (i === 0 ? { ...b, lugar: { ...b.lugar, x, h: "esq" } } : b)) } });
    let tela = await montar(<EditorTitulo clipe={com(0.3)} />);
    await chegar();
    expect(tela.raiz.querySelectorAll("img[data-bloco]").length).toBe(2);
    expect(tela.raiz.querySelector<HTMLElement>("[data-area-segura]")!.style.inset).toBe("2%");
    expect(tela.raiz.querySelector('[data-estado="fora"]')).toBeNull();
    tela.desmontar();
    tela = await montar(<EditorTitulo clipe={com(0.005)} />);
    await chegar();
    const fora = tela.raiz.querySelectorAll("img[data-fora]");
    expect([...fora].map((i) => i.getAttribute("data-bloco"))).toEqual([base.titulo.blocos[0]!.id]);
    expect(tela.raiz.querySelector('[data-estado="fora"]')).not.toBeNull();
    expect(textosVisiveis(tela.raiz)).toEqual([]);
    // fundo: a cor do título; sem fundo, o xadrez
    expect(tela.raiz.querySelector<HTMLElement>("[data-previa-titulo]")!.style.background).toBe("#000000");
  });

  it("sem resposta do backend (comando ausente ou falha): a prévia fica sem os blocos e nada cai", async () => {
    await abrir();
    cmd.edicaoSpriteTexto.mockImplementation(() => {
      throw new Error("sem comando");
    });
    const tela = await montar(<EditorTitulo clipe={doc().principal[0] as ClipeTitulo} />);
    await chegar();
    expect(tela.raiz.querySelector("[data-previa-titulo]")).not.toBeNull();
    expect(tela.raiz.querySelectorAll("img").length).toBe(0);
  });

  it("terço inferior preso: texto novo no arranjo do modelo, 'Até o fim do clipe', só rótulos e sem texto de exemplo", async () => {
    await abrir();
    let id = "";
    let trilha = "";
    st().alterar("Terço inferior", (d) => {
      const s = prenderTituloNoModelo(d, C1, "terco_inferior");
      [id, trilha] = [s.id, s.trilha!];
    });
    const tela = await montar(<Inspetor />);
    await esperar(() => selecionar("livre", id, trilha));
    const sob = () => doc().sobreposicoes[0]!.clipes.find((c) => (c as SobTitulo).id === id) as SobTitulo;
    const t0 = sob().titulo;
    const tit = entradaDe(tela.raiz, "Título") as HTMLInputElement;
    await digitar(tit, "Câmera 2");
    await sair(tit);
    const sub = entradaDe(tela.raiz, "Subtítulo") as HTMLInputElement;
    await digitar(sub, "Rua das Flores");
    await sair(sub);
    const t1 = sob().titulo;
    expect(t1.blocos.map((b) => [b.papel, b.conteudo])).toEqual([["titulo", "Câmera 2"], ["subtitulo", "Rua das Flores"]]);
    expect(t1.blocos[0]).toEqual({ ...t0.blocos[0], conteudo: "Câmera 2" });
    expect(t1.blocos[1]).toEqual(blocoNoModelo(t0, "subtitulo", t1.blocos[1]!.id, "Rua das Flores", { w: 1920, h: 1080 }));
    // a prévia pediu os dois blocos como o render
    await chegar();
    for (const i of [0, 1]) expect(pedidosFeitos().some((p) => igual(p, pedido(t1, i, { w: 1920, h: 1080 })))).toBe(true);
    // até o fim do clipe: ligado (nulo); desligar dá duração; religar volta a nulo
    const marca = campo(tela.raiz, "Até o fim do clipe").querySelector("input") as HTMLInputElement;
    expect(marca.checked).toBe(true);
    await clicar(marca);
    expect(sob().duracao_s).toBe(3);
    await clicar(marca);
    expect(sob().duracao_s).toBeNull();
    const fora = textosVisiveis(tela.raiz).filter((x) => !textoPermitido(x));
    expect(fora).toEqual([]);
    expect(tela.raiz.querySelectorAll("[placeholder]").length).toBe(0);
  });

  it("preso em saída vertical: nasce, ganha texto e troca de modelo no canvas da saída; faixa sem fundo, cartão com fundo", async () => {
    const V = { w: 1080, h: 1920 };
    await abrir((p) => {
      p.saida.largura = V.w;
      p.saida.altura = V.h;
    });
    let id = "";
    let trilha = "";
    st().alterar("Terço inferior", (d) => {
      const s = prenderTituloNoModelo(d, C1, "terco_inferior");
      [id, trilha] = [s.id, s.trilha!];
    });
    const sob = () => doc().sobreposicoes[0]!.clipes.find((c) => (c as SobTitulo).id === id) as SobTitulo;
    // nasce na escala da largura (mais estreito que 4:3)
    const k = escalaDoCanvas(V);
    expect(k).toBe((1080 * 3) / (1920 * 4));
    const t0 = sob().titulo;
    expect(t0).toEqual(tituloNoModelo("terco_inferior", true, t0.blocos[0]!.id, V));
    expect(t0.blocos[0]!.estilo.tamanho).toBe(0.052 * k);
    const tela = await montar(<Inspetor />);
    await esperar(() => selecionar("livre", id, trilha));
    const tit = entradaDe(tela.raiz, "Título") as HTMLInputElement;
    await digitar(tit, "Câmera 2");
    await sair(tit);
    const rod = entradaDe(tela.raiz, "Rodapé") as HTMLInputElement;
    await digitar(rod, "Instituto");
    await sair(rod);
    const t1 = sob().titulo;
    expect(t1.blocos[1]).toEqual(blocoNoModelo(t0, "rodape", t1.blocos[1]!.id, "Instituto", V));
    expect(t1.blocos[1]!.estilo.tamanho).toBe(0.024 * k);
    // a prévia pede os sprites na altura da saída
    await chegar();
    for (const i of [0, 1]) expect(pedidosFeitos().some((p) => igual(p, pedido(t1, i, V)))).toBe(true);
    expect(tela.raiz.querySelector("[data-previa-titulo]")!.getAttribute("data-canvas")).toBe("1080x1920");
    // "Modelo" num preso: faixa (evento, terço inferior) sem fundo; cartão ganha o fundo cheio
    for (const [m, fundo] of [["evento", "transparente"], ["cartela", "cor"], ["terco_inferior", "transparente"]] as const) {
      const antes = sob().titulo;
      await escolher(entradaDe(tela.raiz, "Modelo") as HTMLSelectElement, m);
      expect(sob().titulo).toEqual(aplicarModelo(antes, m, true, V));
      expect(sob().titulo.fundo.tipo, m).toBe(fundo);
    }
  });
});
