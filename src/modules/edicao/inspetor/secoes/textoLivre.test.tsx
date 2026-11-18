// @vitest-environment happy-dom
/**
 * Texto livre (pacote do texto livre, contratos-fase4.md D4/D5): item novo no cursor (um passo, selecionado, foco no
 * "Texto", nenhum texto de exemplo) e a seção no Inspetor de verdade: cada campo grava o que o D4 diz, um passo
 * cada, e só os rótulos de (h) aparecem.
 */

import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { ClipeResolvido, LinhaResolvida, ProjetoAberto, ProjetoEdicao, SobTitulo } from "@domain/edicao";

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
import { Inspetor } from "../Inspetor";
import { esquecerLeituras } from "../contexto";
import { useInspetorUi } from "../ui";
import { tipoNoInspetor } from "../registro";
import { botao, campo, clicar, desmontarTudo, digitar, entradaDe, esperar, lerFixture, montar, sair, textosVisiveis, type Tela } from "../testeTela";
import { textoPermitido } from "../rotulos.teste";
import {
  CONTORNO,
  DURACAO_TEXTO,
  FAIXA,
  SOMBRA,
  TEXTO_LIVRE_PRONTO,
  adicionarTextoLivre,
  clipeNoSlot,
  comFundo,
  centroVisivel,
  comSurgir,
  ehTextoLivre,
  estiloNovo,
  fundoDe,
  novoTextoLivre,
  slotDoCursor,
} from "./textoLivre";
import { useAreaVisivel } from "../../palco/areaVisivel";
import { useRecursos } from "../../palco/recursos";
import { LINHA_PRONTA } from "../../linha/LinhaDoTempo";
import { resolver } from "../../modelo/tempo";
import { aplicarMudanca } from "../../modelo/projeto";
import { alcasDoLivre, livresNoQuadro } from "../../palco/livres";

const st = () => useEdicaoStore.getState();
const doc = () => st().doc!;
const passos = () => st().passado.length;
const ultimo = () => st().passado.at(-1)?.rotulo;
const textoDe = (id: string) => doc().sobreposicoes.flatMap((t) => t.clipes).find((c) => (c as SobTitulo).id === id) as SobTitulo;
const estiloDe = (id: string) => textoDe(id).titulo.blocos[0]!.estilo;
const depois = async () => vi.setSystemTime(Date.now() + 1500);

/** Linha falsa da fixture `projeto-do-store.json`: título (90 slots), dois trechos (150 e 60). */
function linhaDe(p: ProjetoEdicao): LinhaResolvida {
  const n = [90, 150, 60];
  let q = 0;
  const clipes: ClipeResolvido[] = p.principal.map((c, i) => {
    const r = { id: (c as { id: string }).id, inicio_q: q, n_q: n[i]!, sobrepoe_q: 0, fonte: null };
    q += n[i]!;
    return r;
  });
  return { fps: { num: 30, den: 1 }, total_q: q, clipes, livres: [] };
}

describe("item novo (D4, D5)", () => {
  const p = lerFixture<ProjetoEdicao>("projeto-do-store.json");
  const linha = linhaDe(p);
  const ids = p.principal.map((c) => (c as { id: string }).id);

  it("no slot do cursor, preso ao clipe sob ele, 5 s, Faixa branca 5,2 %, vazio, no centro dado (no pixel)", () => {
    const n = novoTextoLivre(p, linha, 100, { x: 0.31234, y: 0.7 })!;
    expect(n.trilha).toBeNull();
    const c = n.clipe;
    expect(ehTextoLivre(c)).toBe(true);
    expect(c.pos).toEqual({ ref: "clipe", clipe: ids[1], desloc: 10 / 30 });
    expect(c.duracao_s).toBe(DURACAO_TEXTO);
    expect([c.entrada_s, c.saida_s, c.animacao]).toEqual([0.3, 0.3, undefined]);
    expect(c.transformacao).toEqual({ x: Math.round(0.31234 * 1920) / 1920, y: Math.round(0.7 * 1080) / 1080, escala: 1, rotacao: 0, opacidade: 1 });
    const b = c.titulo.blocos[0]!;
    expect(b.conteudo).toBe("");
    expect(b.lugar).toEqual({ x: 0.5, y: 0.5, h: "centro", v: "meio" });
    expect(b.estilo).toEqual({ fonte: "sans", negrito: false, italico: false, tamanho: 0.052, cor: "#ffffff", alinhamento: "centro", caixa: { ...FAIXA }, contorno: null });
    expect(c.titulo.fundo).toEqual({ tipo: "transparente" });
    expect(c.efeitos).toEqual([]);
  });

  it("antes do começo: o primeiro; depois do fim: o último; programa vazio: preso ao programa", () => {
    expect(clipeNoSlot(linha, -5)!.id).toBe(ids[0]);
    expect(clipeNoSlot(linha, 9999)!.id).toBe(ids[2]);
    expect(novoTextoLivre(p, linha, 299, { x: 0.5, y: 0.5 })!.clipe.pos).toEqual({ ref: "clipe", clipe: ids[2], desloc: 59 / 30 });
    const vazio = { ...linha, clipes: [], total_q: 0 };
    expect(novoTextoLivre({ ...p, principal: [] }, vazio, 0, { x: 0.5, y: 0.5 })!.clipe.pos).toEqual({ ref: "programa", t: 0 });
    // na sobreposição de uma transição: o que entra
    const cruzado = { ...linha, clipes: [linha.clipes[0]!, { ...linha.clipes[1]!, inicio_q: 80 }] };
    expect(clipeNoSlot(cruzado, 85)!.id).toBe(ids[1]);
  });

  it("trilha: a de cima não oculta", () => {
    const q = structuredClone(p);
    q.sobreposicoes = [
      { id: "a", nome: "", oculta: false, clipes: [] },
      { id: "b", nome: "", oculta: false, clipes: [] },
      { id: "c", nome: "", oculta: true, clipes: [] },
    ];
    expect(novoTextoLivre(q, linha, 0, { x: 0.5, y: 0.5 })!.trilha).toBe("b");
  });

  it("cursor no programa e no clipe dão o slot", () => {
    expect(slotDoCursor(p, linha, { modo: "programa", q: 123 })).toBe(123);
    expect(slotDoCursor(p, linha, { modo: "programa", q: 5000 })).toBe(299);
    // título: tempo local
    expect(slotDoCursor(p, linha, { modo: "clipe", clipe: ids[0]!, t: 1 })).toBe(30);
    // trecho a 1×: (t − início) em slots, dentro do clipe
    const tr = p.principal[1] as { inicio_s: number };
    expect(slotDoCursor(p, linha, { modo: "clipe", clipe: ids[1]!, t: tr.inicio_s + 2 })).toBe(90 + 60);
    expect(slotDoCursor(p, linha, { modo: "clipe", clipe: ids[1]!, t: tr.inicio_s + 999 })).toBe(90 + 149);
  });

  it("centro da área visível: a do palco na origem do clipe, levada ao canvas pelo enquadramento", () => {
    const q = structuredClone(p);
    const tr = q.principal[1] as { id: string; midia: string; efeitos: unknown[] };
    tr.efeitos = [{ tipo: "enquadramento", id: "e1", ligado: true, nome: "", ativo: null, regiao: { x: 0.5, y: 0, w: 0.5, h: 0.5 }, ajuste: "cobrir", interpolacao: "bilinear" }];
    const sha = q.midias.find((m) => m.id === tr.midia)!.sha256;
    useRecursos.setState({ midias: { [sha]: { info: { exibido: { w: 1920, h: 1080 } } as never, tabela: null, erro: null, pedindo: false } } });
    try {
      // sem área publicada: o meio do canvas
      useAreaVisivel.setState({ clipe: null, caixa: null });
      expect(centroVisivel(q, linha, 100)).toEqual({ x: 0.5, y: 0.5 });
      // área (0,6; 0,1; 0,2; 0,2) da origem: centro (0,7; 0,2) → canvas ((0,7 − 0,5)/0,5; 0,2/0,5)
      useAreaVisivel.setState({ clipe: tr.id, caixa: { x: 0.6, y: 0.1, w: 0.2, h: 0.2 } });
      const c = centroVisivel(q, linha, 100);
      expect(c.x).toBeCloseTo(0.4, 12);
      expect(c.y).toBeCloseTo(0.4, 12);
      // área publicada de outro clipe não vale
      expect(centroVisivel(q, linha, 10)).toEqual({ x: 0.5, y: 0.5 });
      expect(novoTextoLivre(q, linha, 100, c)!.clipe.transformacao).toMatchObject({ x: 768 / 1920, y: 432 / 1080 });
    } finally {
      useAreaVisivel.setState({ clipe: null, caixa: null });
      useRecursos.setState({ midias: {} });
    }
  });

  it("fundo, contorno, sombra e entrada pelos padrões do D4", () => {
    const e = estiloNovo();
    expect(fundoDe(e)).toBe("faixa");
    expect(comFundo(e, "nenhum").caixa).toBeNull();
    expect(comFundo(e, "caixa").caixa).toEqual({ cor: "#000000", margem: 0.012, raio: 0.006 });
    expect(comFundo({ ...e, caixa: { cor: "#123456", margem: 0.012, raio: 0.006 } }, "caixa").caixa!.cor).toBe("#123456");
    expect(comFundo(comFundo(e, "caixa"), "faixa").caixa).toEqual({ cor: "#000000a0", margem: 0.012, raio: 0, faixa: true });
    expect(CONTORNO).toEqual({ cor: "#000000", largura: 0.003 });
    expect(SOMBRA).toEqual({ cor: "#000000b3", dx: 0, dy: 0.003, desfoque: 0.004 });
    expect(comSurgir(undefined, "entrada", "deslizar")).toEqual({ entrada: "deslizar", saida: "aparecer", lado: "baixo" });
    expect(comSurgir({ entrada: "deslizar", saida: "aparecer", lado: "esq" }, "entrada", "aparecer")).toBeUndefined();
    expect(comSurgir({ entrada: "deslizar", saida: "aparecer", lado: "esq" }, "saida", "corte")).toEqual({ entrada: "deslizar", saida: "corte", lado: "esq" });
  });
});

let tela: Tela;

beforeEach(async () => {
  vi.useFakeTimers({ toFake: ["Date"], now: new Date("2026-11-17T10:00:00Z") });
  for (const f of Object.values(cmd)) f.mockReset();
  cmd.edicaoSalvar.mockImplementation(async () => ({ updated_at: new Date().toISOString() }));
  cmd.edicaoAnalisar.mockImplementation(async () => ({ problemas: [], linha: { fps: { num: 30, den: 1 }, total_q: 0, clipes: [], livres: [] }, midias: [], prender: [], saida_auto: null }));
  cmd.edicaoTabelaQuadros.mockRejectedValue({ kind: "validation", message: "validation error: tabela_pendente: ainda não" });
  cmd.edicaoMidiaInfo.mockRejectedValue({ kind: "validation", message: "validation error: tabela_pendente: ainda não" });
  cmd.edicaoSpriteTexto.mockImplementation(async () => ({ relativo: "sprites/x.png", w: 100, h: 40 }));
  esquecerLeituras();
  useInspetorUi.setState({ aba: "clipe", ferramenta: null, aoMarcar: null, chave: null });
  useAreaVisivel.setState({ clipe: null, caixa: null });
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

/** Cria pelo botão (o que a Sequência chama) e espera a seção montar. */
async function criar(): Promise<string> {
  let id: string | null = null;
  await esperar(() => {
    id = adicionarTextoLivre();
  });
  expect(id).not.toBeNull();
  return id!;
}

describe("Texto livre no Inspetor", () => {
  it("a bandeira está ligada e o registro dá a seção ao título livre de um bloco", () => {
    expect(TEXTO_LIVRE_PRONTO).toBe(true);
    expect(tipoNoInspetor("livre", { tipo: "titulo", titulo: { modelo: "livre", blocos: [{}] } })).toBe("texto");
    expect(tipoNoInspetor("livre", { tipo: "titulo", titulo: { modelo: "livre", blocos: [{}, {}] } })).toBe("titulo");
    expect(tipoNoInspetor("livre", { tipo: "titulo", titulo: { modelo: "evento", blocos: [{}] } })).toBe("titulo");
  });

  it("criar: um passo, trilha nova, selecionado, foco no Texto vazio; só rótulos de (h)", async () => {
    await esperar(() => st().posicionar({ modo: "programa", q: 100 }));
    const n = passos();
    const id = await criar();
    expect([passos(), ultimo()]).toEqual([n + 1, "Texto livre"]);
    expect(doc().sobreposicoes).toHaveLength(1);
    expect(doc().sobreposicoes[0]!.clipes.map((c) => (c as SobTitulo).id)).toEqual([id]);
    expect(st().selecao.foco).toEqual({ k: "livre", id, trilha: doc().sobreposicoes[0]!.id });
    const raiz = tela.raiz.querySelector(`[data-texto-livre="${id}"]`) as HTMLElement;
    expect(raiz).not.toBeNull();
    const area = entradaDe(raiz, "Texto") as HTMLTextAreaElement;
    expect(document.activeElement).toBe(area);
    expect(area.value).toBe("");
    const fora = textosVisiveis(tela.raiz).filter((t) => !textoPermitido(t));
    expect(fora).toEqual([]);
    for (const r of ["Texto livre", "Texto", "Fonte", "Tamanho", "Cor", "Fundo", "Contorno", "Sombra", "Posição", "Giro", "Largura", "Entrada", "Saída"])
      expect(textosVisiveis(tela.raiz)).toContain(r);
    // desfazer tira o item (a trilha nova também)
    st().desfazer();
    expect(doc().sobreposicoes).toEqual([]);
  });

  it("nasce no centro da área visível publicada pelo palco (sem enquadramento: a mesma no canvas)", async () => {
    const titulo = (doc().principal[0] as { id: string }).id;
    useAreaVisivel.setState({ clipe: titulo, caixa: { x: 0.6, y: 0.1, w: 0.3, h: 0.3 } });
    // o título não tem mídia: sem a origem, o meio do canvas
    await esperar(() => st().posicionar({ modo: "programa", q: 10 }));
    const id = await criar();
    expect(textoDe(id).transformacao).toMatchObject({ x: 0.5, y: 0.5 });
  });

  it("Texto, Fonte, Tamanho e Cor: um passo cada", async () => {
    const id = await criar();
    const raiz = tela.raiz.querySelector(`[data-texto-livre="${id}"]`) as HTMLElement;
    const n = passos();
    const area = entradaDe(raiz, "Texto") as HTMLTextAreaElement;
    await digitar(area, "Policiais chegam");
    await sair(area);
    expect(textoDe(id).titulo.blocos[0]!.conteudo).toBe("Policiais chegam");
    expect([passos(), ultimo()]).toEqual([n + 1, "Texto"]);
    await clicar(campo(raiz, "Fonte").querySelector<HTMLButtonElement>('button[data-valor="serifa"]')!);
    expect(estiloDe(id).fonte).toBe("serifa");
    await digitar(entradaDe(raiz, "Tamanho") as HTMLInputElement, "8");
    expect(estiloDe(id).tamanho).toBeCloseTo(0.08, 12);
    await depois();
    await clicar(raiz.querySelector<HTMLButtonElement>('button[data-cor="#ffd84a"]')!);
    expect(estiloDe(id).cor).toBe("#ffd84a");
    expect(raiz.querySelector('button[data-cor="#ffd84a"]')!.getAttribute("aria-pressed")).toBe("true");
    expect(passos()).toBe(n + 4);
  });

  it("Fundo, Contorno e Sombra gravam os padrões do D4; sem sombra a chave sai do arquivo", async () => {
    const id = await criar();
    const raiz = tela.raiz.querySelector(`[data-texto-livre="${id}"]`) as HTMLElement;
    expect(raiz.querySelector('[data-campo="Fundo"] button[aria-pressed="true"]')!.textContent).toBe("Faixa");
    await clicar(botao(campo(raiz, "Fundo"), "Caixa"));
    expect(estiloDe(id).caixa).toEqual({ cor: "#000000", margem: 0.012, raio: 0.006 });
    // a cor da caixa aparece só com "Caixa"
    expect(textosVisiveis(raiz)).toContain("Caixa");
    await clicar(botao(campo(raiz, "Fundo"), "Nenhum"));
    expect(estiloDe(id).caixa).toBeNull();
    await clicar(botao(campo(raiz, "Fundo"), "Faixa"));
    expect(estiloDe(id).caixa).toEqual({ cor: "#000000a0", margem: 0.012, raio: 0, faixa: true });
    const marca = (r: string) => [...raiz.querySelectorAll<HTMLLabelElement>("label")].find((l) => l.textContent?.trim() === r)!.querySelector("input")!;
    await clicar(marca("Contorno"));
    expect(estiloDe(id).contorno).toEqual({ cor: "#000000", largura: 0.003 });
    await clicar(marca("Sombra"));
    expect(estiloDe(id).sombra).toEqual(SOMBRA);
    await clicar(marca("Sombra"));
    expect("sombra" in estiloDe(id)).toBe(false);
    await clicar(marca("Contorno"));
    expect(estiloDe(id).contorno).toBeNull();
  });

  it("Entrada e Saída: Deslizar mostra Lado (ícones com title); Corte esconde a duração; aparecer nos dois tira a animação", async () => {
    const id = await criar();
    const raiz = tela.raiz.querySelector(`[data-texto-livre="${id}"]`) as HTMLElement;
    const entrada = campo(raiz, "Entrada");
    const saida = campo(raiz, "Saída");
    expect(raiz.querySelectorAll('[data-campo="Duração (s)"]')).toHaveLength(2);
    expect(raiz.querySelector('[data-campo="Lado"]')).toBeNull();
    await clicar(botao(entrada, "Deslizar"));
    expect(textoDe(id).animacao).toEqual({ entrada: "deslizar", saida: "aparecer", lado: "baixo" });
    const lado = raiz.querySelector('[data-campo="Lado"]') as HTMLElement;
    expect([...lado.querySelectorAll("button")].map((b) => [b.getAttribute("title"), (b.textContent ?? "").trim()])).toEqual([
      ["Cima", ""], ["Baixo", ""], ["Esquerda", ""], ["Direita", ""],
    ]);
    await clicar(lado.querySelector<HTMLButtonElement>('button[data-valor="esq"]')!);
    expect(textoDe(id).animacao!.lado).toBe("esq");
    await clicar(botao(saida, "Corte"));
    expect(textoDe(id).animacao).toEqual({ entrada: "deslizar", saida: "corte", lado: "esq" });
    expect(raiz.querySelectorAll('[data-campo="Duração (s)"]')).toHaveLength(1);
    await digitar(entradaDe(raiz, "Duração (s)") as HTMLInputElement, "0,5");
    expect(textoDe(id).entrada_s).toBe(0.5);
    await clicar(botao(campo(raiz, "Entrada"), "Aparecer"));
    await clicar(botao(campo(raiz, "Saída"), "Aparecer"));
    expect("animacao" in textoDe(id)).toBe(false);
    expect(textosVisiveis(tela.raiz).filter((t) => !textoPermitido(t))).toEqual([]);
  });

  // a conta de aparar é do P4.1 (`linha/arrastar.ts::apararLivre`, D5); a seção só a chama com o slot do cursor
  it.skipIf(!LINHA_PRONTA)("fecho do bloco 6.4: P4.1 — De, Até, Começar e Terminar no cursor movem as bordas do texto", async () => {
    await esperar(() => st().posicionar({ modo: "programa", q: 100 }));
    const id = await criar();
    const raiz = tela.raiz.querySelector(`[data-texto-livre="${id}"]`) as HTMLElement;
    expect(textosVisiveis(raiz)).toEqual(expect.arrayContaining(["De", "Até", "Começar no cursor", "Terminar no cursor"]));
    const inicio = resolver(doc(), { tabela: () => null }).livres.find((l) => l.id === id)!.inicio_q;
    await esperar(() => st().posicionar({ modo: "programa", q: inicio + 30 }));
    await clicar(botao(raiz, "Terminar no cursor"));
    expect(textoDe(id).duracao_s).toBeCloseTo(1, 9);
    await esperar(() => st().posicionar({ modo: "programa", q: inicio + 10 }));
    await clicar(botao(raiz, "Começar no cursor"));
    expect(textoDe(id).duracao_s).toBeCloseTo(20 / 30, 9);
  });

  it("Posição em % da saída (gravada no pixel inteiro), Giro em graus, Largura em %", async () => {
    const id = await criar();
    const raiz = tela.raiz.querySelector(`[data-texto-livre="${id}"]`) as HTMLElement;
    const pos = campo(raiz, "Posição");
    const [x, y] = [...pos.querySelectorAll<HTMLInputElement>("input")];
    expect([x!.value, y!.value]).toEqual(["50,0", "50,0"]);
    await digitar(x!, "15,6");
    // 15,6 % de 1920 = 299,52 px → 300 px
    expect((textoDe(id).transformacao.x as number) * 1920).toBeCloseTo(300, 9);
    await digitar(y!, "83,3");
    expect((textoDe(id).transformacao.y as number) * 1080).toBeCloseTo(900, 9);
    await digitar(entradaDe(raiz, "Giro") as HTMLInputElement, "-12,5");
    expect(textoDe(id).transformacao.rotacao).toBe(-12.5);
    await digitar(entradaDe(raiz, "Largura") as HTMLInputElement, "50");
    expect(textoDe(id).titulo.blocos[0]!.largura_max).toBe(0.5);
  });

  it("alças no palco: cada arrasto (mover, canto, lado, giro) é um passo; desfazer e refazer andam de um em um", async () => {
    const id = await criar();
    await esperar(() => st().alterar("Texto", (d) => void ((d.sobreposicoes[0]!.clipes[0] as SobTitulo).titulo.blocos[0]!.conteudo = "Policiais")));
    const trilha = doc().sobreposicoes[0]!.id;
    const linha: LinhaResolvida = { fps: { num: 30, den: 1 }, total_q: 300, clipes: [], livres: [{ id, trilha, inicio_q: 0, n_q: 150 }] };
    const fontes = { sprite: () => ({ relativo: "sprites/x.png", w: 400, h: 70 }), atlas: () => null, midia: () => null };
    const canvas = { w: 1920, h: 1080 };
    const sem = { shift: false, alt: false };
    const estados = [structuredClone(doc())];
    for (const [nome, dx, dy] of [["mover", 120, -40], ["se", 60, 20], ["l", 90, 0], ["giro", 200, 150]] as const) {
      // como o palco: a alça e o ponto de partida ficam os do começo do gesto; vários movimentos, um passo
      const item = livresNoQuadro(doc(), linha, 30, fontes)[0]!;
      const a = alcasDoLivre(doc(), item, canvas).find((x) => x.id.endsWith(`:${nome}`))!;
      const n = passos();
      const g = st().iniciarGesto("Marcar");
      for (const k of [0.25, 0.5, 1]) {
        const m = a.arrastar(a.em, { x: a.em.x + (dx * k) / canvas.w, y: a.em.y + (dy * k) / canvas.h }, sem);
        g.alterar((d) => void aplicarMudanca(d, m));
      }
      g.concluir();
      expect(passos(), nome).toBe(n + 1);
      expect(doc(), nome).not.toEqual(estados.at(-1));
      estados.push(structuredClone(doc()));
    }
    for (let i = estados.length - 2; i >= 0; i--) {
      st().desfazer();
      expect(doc()).toEqual(estados[i]);
    }
    for (let i = 1; i < estados.length; i++) {
      st().refazer();
      expect(doc()).toEqual(estados[i]);
    }
  });
});
