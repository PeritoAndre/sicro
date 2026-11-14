/** Sequência: textos do cartão, selos, item inválido, arrasto, dividir/congelar/duplicar e as ações pela store. */

import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { Analise, ClipeTrecho, MidiaRef, Problema, ProjetoEdicao, TabelaQuadros } from "@domain/edicao";

const cmd = vi.hoisted(() => ({
  edicaoAbrir: vi.fn(),
  edicaoSalvar: vi.fn(),
  edicaoAnalisar: vi.fn(),
  edicaoMidiaInfo: vi.fn(),
  edicaoTabelaQuadros: vi.fn(),
}));
vi.mock("@core/commands", () => ({ commands: cmd }));
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn(async () => () => {}) }));

import { useEdicaoStore } from "../store/edicaoStore";
import { clipeConhecido, novoProjeto, novoTrecho } from "../modelo/projeto";
import { tempoDoQuadro } from "../modelo/tempo";
import { useRecursos } from "../palco/recursos";
import {
  clipeNaTela,
  comIdsNovos,
  congelarAqui,
  destinoDoArrasto,
  dividirTrecho,
  duplicarClipe,
  itemInvalido,
  presosAoClipe,
  problemasDoItem,
  selosDoClipe,
  textoClipes,
  textoDuracao,
  textoDuracaoMs,
  textoProblemas,
  textoVelocidade,
} from "./contas";
import { MODELOS, novoTitulo, paresDeEventos, prenderImagem, prenderNarracao, prenderTitulo } from "./novos";
import { congelar, dividir, duplicar, mover, moverPara, podeCongelar, remover, selecionarClipe, vizinho } from "./acoes";

const tab = (n: number): TabelaQuadros => ({ tb: { num: 1, den: 15360 }, pts0: 0, passos: Array(n - 1).fill(512), passo_mediano: 512, chaves: [0], vfr: false, origem: "decodificador" });
const prob = (codigo: string, clipe: string | null, nivel: Problema["nivel"] = "erro", efeito: string | null = null): Problema => ({ nivel, codigo, mensagem: codigo, clipe, efeito, trilha: null, midia: null, campo: null, n: null });

describe("textos do cartão e da barra de estado", () => {
  it("durações, velocidade, contagens", () => {
    expect(textoDuracao(12.4)).toBe("0:12");
    expect(textoDuracao(3723)).toBe("1:02:03");
    expect(textoDuracaoMs(47.3)).toBe("0:47,300");
    expect(textoDuracaoMs(3723.25)).toBe("1:02:03,250");
    expect(textoVelocidade(0.25)).toBe("0,25×");
    expect(textoVelocidade(2)).toBe("2×");
    expect(textoClipes(1)).toBe("1 clipe");
    expect(textoClipes(5)).toBe("5 clipes");
    expect(textoProblemas([prob("a", "x", "aviso"), prob("b", "y", "pericial")])).toEqual({ texto: "2 avisos", erro: false });
    expect(textoProblemas([prob("a", "x", "aviso"), prob("b", "y")])).toEqual({ texto: "1 erro", erro: true });
    expect(textoProblemas([])).toBeNull();
  });
});

describe("cartão: problemas, selos e item inválido", () => {
  const c = { ...novoTrecho("m", 0, 10), id: "c1" };
  const ref = { id: "m", vfr: false } as MidiaRef;

  it("problema do item (abertura e análise); `item_invalido` no clipe vira desconhecido", () => {
    const a = { problemas: [prob("transicao", "c1"), prob("midia_ausente", "c2")] } as Analise;
    expect(problemasDoItem("c1", a, [prob("esquema", "c1", "aviso")]).map((p) => p.codigo)).toEqual(["esquema", "transicao"]);
    expect(itemInvalido("c1", a, [])).toBe(false);
    const inv = { problemas: [prob("item_invalido", "c1")] } as Analise;
    expect(itemInvalido("c1", inv, [])).toBe(true);
    expect(clipeNaTela(c, inv, [])).toBe(false);
    // inválido de um EFEITO do clipe não apaga o clipe
    expect(itemInvalido("c1", { problemas: [prob("item_invalido", "c1", "erro", "e9")] } as Analise, [])).toBe(false);
    expect(clipeNaTela({ tipo: "do_futuro", id: "f" }, null, [])).toBe(false);
  });

  it("selos: ausente, danificado, taxa variável, velocidade", () => {
    expect(selosDoClipe(c, ref, null, null, [])).toEqual([]);
    expect(selosDoClipe(c, ref, null, { id: "m", estado: "ausente", tabela: null, busca: null, erros: null }, [])).toEqual(["Vídeo ausente"]);
    expect(selosDoClipe(c, { ...ref, vfr: true }, null, null, [prob("fonte_danificada", "c1")])).toEqual(["Danificado", "Taxa variável"]);
    expect(selosDoClipe({ ...c, velocidade: { tipo: "constante", fator: 0.25 } }, ref, null, null, [])).toEqual([{ velocidade: "0,25×" }]);
  });

  it("arrasto: posição final pelos centros dos OUTROS cartões", () => {
    const centros = [10, 50, 90, 130];
    expect(destinoDoArrasto(5, centros, 2)).toBe(0);
    expect(destinoDoArrasto(60, centros, 0)).toBe(1);
    expect(destinoDoArrasto(200, centros, 0)).toBe(3);
    expect(destinoDoArrasto(70, centros, 2)).toBe(2);
  });
});

describe("mudanças da lista", () => {
  const t = tab(300);
  function doc(): ProjetoEdicao {
    const p = novoProjeto();
    const c = { ...novoTrecho("m", tempoDoQuadro(t, 30), tempoDoQuadro(t, 90)), id: "c1", inicio_pts: 30 * 512, fim_pts: 90 * 512 };
    c.efeitos = [{ id: "e1", ligado: true, nome: "", ativo: null, tipo: "ajustes", brilho: 0, contraste: 1, gama: 1, saturacao: 1 }];
    c.audio.silencios = [{ id: "s1", de: 1, ate: 2, modo: "tom", motivo: "" }];
    p.principal = [c, { ...novoTrecho("m", 0, 1), id: "c2" }];
    return p;
  }

  it("dividir no quadro: [a, j) e [j, b), ids novos na segunda parte; a transição de entrada fica só na primeira", () => {
    const d = doc();
    (d.principal[0] as ClipeTrecho).transicao_entrada = { tipo: "fundir", duracao_s: 0.5, audio: "acompanhar" };
    const id = dividirTrecho(d, "c1", 60, t)!;
    const [a, b] = d.principal as ClipeTrecho[];
    expect([a!.fim_pts, b!.inicio_pts, b!.fim_pts]).toEqual([60 * 512, 60 * 512, 90 * 512]);
    expect(a!.transicao_entrada?.tipo).toBe("fundir");
    expect(b!.transicao_entrada).toBeNull();
    expect(b!.id).toBe(id);
    expect(b!.efeitos[0]!).not.toHaveProperty("id", "e1");
    expect(b!.audio.silencios[0]!.id).not.toBe("s1");
    expect(dividirTrecho(d, "c1", 30, t)).toBeNull();
  });

  it("congelar aqui: no meio divide e põe o quadro entre as partes; no primeiro quadro vai antes", () => {
    const d = doc();
    const id = congelarAqui(d, "c1", 45, t)!;
    expect(d.principal.map((c) => (c as { tipo: string }).tipo)).toEqual(["trecho", "congelar", "trecho", "trecho"]);
    expect(d.principal[1]).toMatchObject({ id, instante_s: tempoDoQuadro(t, 45), instante_pts: 45 * 512, duracao_s: 3, midia: "m" });
    const e = doc();
    congelarAqui(e, "c1", 30, t);
    expect(e.principal.map((c) => (c as { tipo: string }).tipo)).toEqual(["congelar", "trecho", "trecho"]);
    expect(congelarAqui(doc(), "c1", 90, t)).toBeNull();
    expect(congelarAqui(doc(), "c1", 29, t)).toBeNull();
  });

  it("duplicar: logo depois, sem transição de entrada, nenhum id repetido", () => {
    const d = doc();
    (d.principal[0] as ClipeTrecho).transicao_entrada = { tipo: "fundir", duracao_s: 1, audio: "acompanhar" };
    const novo = duplicarClipe(d, "c1")!;
    const ids = JSON.stringify(d).match(/"id":"[^"]+"/g)!;
    expect(new Set(ids).size).toBe(ids.length);
    expect(d.principal[1]).toMatchObject({ id: novo, transicao_entrada: null });
    expect(comIdsNovos(d.principal[0] as ClipeTrecho).id).not.toBe("c1");
  });

  it("presos ao clipe: título, terço inferior (até o fim), imagem e narração", () => {
    const d = doc();
    prenderTitulo(d, "c1", "evento", "Chegada");
    prenderTitulo(d, "c1", "terco_inferior");
    prenderImagem(d, "c2", "img");
    prenderNarracao(d, "c1", "aud", "fala");
    expect(presosAoClipe(d, "c1").map((p) => [p.k, p.tipo])).toEqual([
      ["livre", "titulo"],
      ["livre", "titulo"],
      ["audio", "narracao"],
    ]);
    expect(d.sobreposicoes[0]!.clipes[1]).toMatchObject({ duracao_s: null, pos: { ref: "clipe", clipe: "c1", desloc: 0 } });
    expect(presosAoClipe(d, "c2").map((p) => p.tipo)).toEqual(["imagem"]);
    expect(d.audios[0]).toMatchObject({ papel: "narracao", abaixa_original: true });
  });

  it("títulos novos e pares de eventos", () => {
    expect(MODELOS.map((m) => m.rotulo)).toEqual(["Cartela", "Abertura", "Evento", "Terço inferior", "Encerramento"]);
    const t1 = novoTitulo("cartela", "Ocorrência 123", "");
    expect(t1.blocos).toHaveLength(1);
    expect(t1.fundo).toEqual({ tipo: "cor", cor: "#000000" });
    const t3 = novoTitulo("terco_inferior", "A", "B");
    expect(t3.fundo).toEqual({ tipo: "transparente" });
    expect(t3.blocos.map((b) => b.papel)).toEqual(["titulo", "subtitulo"]);
    expect(paresDeEventos([30, 10, 20, 40, 50])).toEqual([
      [10, 20],
      [30, 40],
    ]);
    expect(paresDeEventos([5, 5])).toEqual([]);
  });
});

describe("ações pela store (alterar e desfazer)", () => {
  const WS = "/casos/sq";
  const sha = "c".repeat(64);
  const t = tab(300);
  const st = () => useEdicaoStore.getState();

  beforeEach(async () => {
    vi.useFakeTimers({ now: new Date("2026-11-14T05:00:00Z") });
    for (const f of Object.values(cmd)) f.mockReset();
    cmd.edicaoSalvar.mockResolvedValue({ updated_at: "" });
    cmd.edicaoAnalisar.mockResolvedValue({ problemas: [], linha: { fps: { num: 30, den: 1 }, total_q: 0, clipes: [], livres: [] }, midias: [], prender: [], saida_auto: null });
    const p = novoProjeto();
    p.midias = [{ id: "m", sha256: sha, tipo: "video", nome: "cam1.mp4", relativo: "cam1.mp4", vfr: false, ocultacoes: [] } as unknown as MidiaRef];
    p.principal = ["a", "b", "c"].map((id, i) => ({ ...novoTrecho("m", tempoDoQuadro(t, i * 60), tempoDoQuadro(t, i * 60 + 50)), id, inicio_pts: i * 60 * 512, fim_pts: (i * 60 + 50) * 512 }));
    cmd.edicaoAbrir.mockResolvedValue({ projeto: p, importado_de: null, so_leitura: false, avisos: [], raiz_cache: "/cache" });
    await st().abrir(WS, p.id);
    // a tela já tem a tabela dessa mídia
    useRecursos.setState({ midias: { [sha]: { info: null, tabela: t, erro: null, pedindo: false } } });
  });
  afterEach(async () => {
    await st().fechar({ descartar: true }).catch(() => {});
    vi.useRealTimers();
  });
  const ids = () => st().doc!.principal.map((c) => (c as { id: string }).id);

  it("selecionar põe o cursor no meio do primeiro quadro do trecho", () => {
    selecionarClipe("b");
    expect(st().selecao.foco).toEqual({ k: "clipe", id: "b" });
    expect(st().cursor).toEqual({ modo: "clipe", clipe: "b", t: (60 * 512 + 256) / 15360 });
  });

  it("PgUp/PgDn, Alt+PgUp/PgDn e arrastar: um passo de histórico cada; desfazer volta", () => {
    selecionarClipe("a");
    vizinho(1);
    expect(st().selecao.foco?.id).toBe("b");
    mover(-1);
    expect(ids()).toEqual(["b", "a", "c"]);
    moverPara(0, 2);
    expect(ids()).toEqual(["a", "c", "b"]);
    st().desfazer();
    st().desfazer();
    expect(ids()).toEqual(["a", "b", "c"]);
    remover("c");
    expect(ids()).toEqual(["a", "b"]);
    expect(st().selecao.foco?.id).toBe("b");
  });

  it("remover leva o que está preso ao clipe; desfazer traz tudo de volta", () => {
    selecionarClipe("b");
    st().alterar("Título", (d) => {
      prenderTitulo(d, "b", "terco_inferior", "Agente");
      prenderNarracao(d, "b", "aud", "fala");
      prenderTitulo(d, "a", "evento", "fica");
      d.marcadores.push({ id: "mk", rotulo: "", cor: "#ff0000", pos: { ref: "clipe", clipe: "b", desloc: 0 } });
    });
    expect(presosAoClipe(st().doc!, "b")).toHaveLength(2);
    remover("b");
    expect(ids()).toEqual(["a", "c"]);
    const d = st().doc!;
    expect(d.sobreposicoes.flatMap((t) => t.clipes)).toHaveLength(1);
    expect(presosAoClipe(d, "a")).toHaveLength(1);
    expect(d.audios.flatMap((t) => t.clipes)).toEqual([]);
    expect(d.marcadores).toEqual([]);
    st().desfazer();
    expect(ids()).toEqual(["a", "b", "c"]);
    expect(presosAoClipe(st().doc!, "b")).toHaveLength(2);
    expect(st().doc!.marcadores).toHaveLength(1);
  });

  it("com um item preso em foco, Del tira o preso e Duplicar/Mover não caem no clipe por trás", () => {
    selecionarClipe("b");
    let preso = "";
    st().alterar("Título", (d) => {
      preso = prenderTitulo(d, "b", "evento", "x");
    });
    const p = presosAoClipe(st().doc!, "b")[0]!;
    st().selecionar({ itens: [{ k: p.k, id: p.id, trilha: p.trilha }], foco: { k: p.k, id: p.id, trilha: p.trilha }, efeito: null });
    duplicar();
    mover(1);
    expect(ids()).toEqual(["a", "b", "c"]);
    remover();
    expect(ids()).toEqual(["a", "b", "c"]);
    expect(presosAoClipe(st().doc!, "b")).toEqual([]);
    expect(preso).not.toBe("");
    expect(st().selecao.foco).toEqual({ k: "clipe", id: "b" });
    // botão do cartão de item sem id: nada
    remover(null);
    expect(ids()).toEqual(["a", "b", "c"]);
    // agora o foco é o clipe: Del tira o clipe
    remover();
    expect(ids()).toEqual(["a", "c"]);
  });

  it("congelar aqui e dividir pelo cursor; fora do trecho, nada", () => {
    selecionarClipe("b");
    st().posicionar({ modo: "clipe", clipe: "b", t: (75 * 512 + 256) / 15360 });
    expect(podeCongelar()).toBe(true);
    congelar();
    expect(st().doc!.principal.map((c) => (c as { tipo: string }).tipo)).toEqual(["trecho", "trecho", "congelar", "trecho", "trecho"]);
    expect(st().selecao.foco?.id).toBe(ids()[2]);
    st().desfazer();
    expect(ids()).toEqual(["a", "b", "c"]);
    st().posicionar({ modo: "clipe", clipe: "b", t: (75 * 512 + 256) / 15360 });
    dividir();
    expect(ids()).toHaveLength(4);
    // cursor fora do trecho (o fim é exclusivo)
    st().posicionar({ modo: "clipe", clipe: "a", t: (55 * 512 + 256) / 15360 });
    expect(podeCongelar()).toBe(false);
    expect(st().doc!.principal.filter(clipeConhecido)).toHaveLength(4);
  });
});
