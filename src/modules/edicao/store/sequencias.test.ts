/**
 * Sequências adversas (revisão do P0.8a): corridas entre abrir, criar, coletar, fechar, excluir e gravar.
 * Nenhuma pode deixar mudança sem gravar nem gravar documento velho por cima do novo.
 */

import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { Analise, ClipeTrecho, Cursor, MidiaRef, ProjetoAberto, ProjetoEdicao, Tarefa } from "@domain/edicao";
import { EVENTO_FILA } from "@domain/edicao";

const cmd = vi.hoisted(() => ({
  edicaoAbrir: vi.fn(),
  edicaoSalvar: vi.fn(),
  edicaoAnalisar: vi.fn(),
  edicaoExcluir: vi.fn(),
  listVideoMedia: vi.fn(),
  videoProveniencia: vi.fn(),
  edicaoMidiaInfo: vi.fn(),
  edicaoQuadroEm: vi.fn(),
  edicaoEnfileirar: vi.fn(),
  edicaoFila: vi.fn(),
}));
const ev = vi.hoisted(() => ({ ouvintes: new Map<string, (e: { payload: unknown }) => void>() }));
vi.mock("@core/commands", () => ({ commands: cmd }));
vi.mock("@tauri-apps/api/event", () => ({
  listen: vi.fn(async (nome: string, f: (e: { payload: unknown }) => void) => {
    ev.ouvintes.set(nome, f);
    return () => ev.ouvintes.delete(nome);
  }),
}));

import { ANALISE_MS, correnteDe, definirCorrente, useEdicaoStore } from "./edicaoStore";
import { ATRASO_MS, NOVA_TENTATIVA_MS, TETO_MS } from "./autosave";
import { receber } from "./coleta";
import { useRenderStore } from "./renderStore";
import { clipeConhecido, efeitoConhecido, novoTrecho, removerClipe } from "../modelo/projeto";
import { aDe } from "../modelo/geometria";
import { resolver } from "../modelo/tempo";
import { ler } from "../modelo/testeFixtures";

const WS = "/casos/seq";
const st = () => useEdicaoStore.getState();
const sha = "b".repeat(64);

const analiseVazia = (): Analise => ({
  problemas: [],
  linha: { fps: { num: 30, den: 1 }, total_q: 0, clipes: [], livres: [] },
  midias: [],
  prender: [],
  saida_auto: null,
});
const aberto = (projeto: ProjetoEdicao): ProjetoAberto => ({ projeto, importado_de: null, so_leitura: false, avisos: [], raiz_cache: "/cache/seq" });
const gravados = (): ProjetoEdicao[] => cmd.edicaoSalvar.mock.calls.map((c) => c[1] as ProjetoEdicao);
/** Promessa que o teste resolve quando quiser. */
function adiada<T>() {
  let ok!: (v: T) => void;
  let erro!: (e: unknown) => void;
  const p = new Promise<T>((a, b) => {
    ok = a;
    erro = b;
  });
  return { p, ok, erro };
}

beforeEach(() => {
  vi.useFakeTimers({ now: new Date("2026-11-14T05:00:00Z") });
  for (const f of Object.values(cmd)) f.mockReset();
  cmd.edicaoSalvar.mockImplementation(async () => ({ updated_at: "" }));
  cmd.edicaoAnalisar.mockImplementation(async () => analiseVazia());
  cmd.edicaoExcluir.mockResolvedValue(undefined);
  cmd.edicaoMidiaInfo.mockRejectedValue({ kind: "validation", message: "validation error: tabela_pendente" });
  cmd.edicaoQuadroEm.mockImplementation(async (_w: string, _s: string, alvo: { t: number }) => ({ indice: 0, pts: Math.round(alvo.t * 1000), pts_s: alvo.t, dur_s: 0.04 }));
  // proveniência da análise: nada gravado (a coleta e o "Trecho…" leem sempre)
  cmd.videoProveniencia.mockResolvedValue({ recebido_nativo: null, recebido_obs: "", relogio_desvio_s: null, gravador: null });
  cmd.listVideoMedia.mockResolvedValue([
    { id: "vm", sha256: sha, filename: "cam.mkv", relative_path: "Mídias/Vídeos/cam.mkv", duration_s: 100, width: 1280, height: 720, raw_probe_json: "{}", derived_from_hash: null, derivation_json: null },
  ]);
  definirCorrente(WS, null);
});

afterEach(async () => {
  await st().fechar().catch(() => {});
  vi.useRealTimers();
});

describe("coleta × abrir", () => {
  it("trecho coletado enquanto a tela abre não é apagado pela gravação seguinte", async () => {
    const p = ler<ProjetoEdicao>("completo.json");
    const n = p.principal.length;
    definirCorrente(WS, p.id);
    // o disco: o que `edicao_salvar` gravou por último
    let disco = p;
    cmd.edicaoSalvar.mockImplementation(async (_w: string, d: ProjetoEdicao) => ((disco = structuredClone(d)), { updated_at: "" }));
    const lenta = adiada<ProjetoAberto>();
    // a tela lê o disco e demora a receber; a coleta lê na hora
    cmd.edicaoAbrir.mockImplementationOnce(() => {
      const lido = structuredClone(disco);
      return lenta.p.then(() => aberto(lido));
    });
    cmd.edicaoAbrir.mockImplementation(async () => aberto(structuredClone(disco)));
    const abrindo = st().abrir(WS, p.id);
    const coletando = receber(WS, [{ sha256: sha, inicio_s: 10, fim_s: 12 }]);
    await vi.advanceTimersByTimeAsync(50);
    lenta.ok(aberto(p));
    await abrindo;
    await coletando;
    st().alterar("Nome", (d) => void (d.titulo = "depois da coleta"));
    await vi.advanceTimersByTimeAsync(TETO_MS);
    expect(disco.titulo).toBe("depois da coleta");
    expect(disco.principal).toHaveLength(n + 1);
  });
});

describe("coleta vazia", () => {
  it("sem trecho nada é criado nem gravado", async () => {
    await expect(receber(WS, [])).rejects.toThrow("intervalo");
    expect(cmd.edicaoSalvar).not.toHaveBeenCalled();
    expect(correnteDe(WS)).toBeNull();
  });
});

describe("abrir × criar", () => {
  it("pedidos de abrir e de criar correm um de cada vez: vale o último e nada pendente se perde", async () => {
    const p = ler<ProjetoEdicao>("completo.json");
    const lenta = adiada<ProjetoAberto>();
    cmd.edicaoAbrir.mockImplementationOnce(() => lenta.p);
    const abrindo = st().abrir(WS, p.id);
    const criando = st()
      .novo(WS)
      .then((id) => {
        st().alterar("Nome", (d) => void (d.titulo = "nova com mudança"));
        return id;
      });
    await vi.advanceTimersByTimeAsync(10);
    lenta.ok(aberto(p));
    await abrindo;
    const idNovo = await criando;
    await vi.advanceTimersByTimeAsync(TETO_MS);
    expect(st().aberta?.projetoId).toBe(idNovo);
    expect(correnteDe(WS)).toBe(idNovo);
    expect(gravados().map((d) => d.titulo)).toContain("nova com mudança");
  });
});

describe("gesto", () => {
  it("clique sem mudança não vira passo, mesmo se a análise prender um tempo no meio", async () => {
    const p = ler<ProjetoEdicao>("completo.json");
    const c1 = p.principal[0] as ClipeTrecho;
    const resposta = adiada<Analise>();
    cmd.edicaoAnalisar.mockImplementationOnce(() => resposta.p);
    cmd.edicaoAbrir.mockResolvedValueOnce(aberto(p));
    await st().abrir(WS, p.id);
    await vi.advanceTimersByTimeAsync(0);
    const g = st().iniciarGesto("Mover");
    resposta.ok({ ...analiseVazia(), prender: [{ onde: { tipo: "inicio", clipe: c1.id }, pts: 4321, t: 4.321 }] });
    await vi.advanceTimersByTimeAsync(0);
    expect((st().doc!.principal[0] as ClipeTrecho).inicio_pts).toBe(4321);
    g.concluir();
    expect(st().passado).toHaveLength(0);
    // cancelar um gesto vazio também não desfaz o que a análise prendeu
    const g2 = st().iniciarGesto("Mover");
    g2.cancelar();
    expect((st().doc!.principal[0] as ClipeTrecho).inicio_pts).toBe(4321);
  });

  it("desfazer e refazer restauram o cursor", async () => {
    cmd.edicaoAbrir.mockResolvedValueOnce(aberto(ler<ProjetoEdicao>("completo.json")));
    await st().abrir(WS, ler<ProjetoEdicao>("completo.json").id);
    const antes: Cursor = { modo: "programa", q: 7 };
    const depois: Cursor = { modo: "programa", q: 99 };
    st().posicionar(antes);
    st().alterar("Nome", (d) => void (d.titulo = "x"));
    st().posicionar(depois);
    st().desfazer();
    expect(st().cursor).toBe(antes);
    st().refazer();
    expect(st().cursor).toBe(depois);
    expect(st().doc!.titulo).toBe("x");
  });
});

describe("salvamento em erro", () => {
  it("mudanças seguidas não adiam a nova tentativa", async () => {
    cmd.edicaoAbrir.mockResolvedValueOnce(aberto(ler<ProjetoEdicao>("completo.json")));
    await st().abrir(WS, ler<ProjetoEdicao>("completo.json").id);
    cmd.edicaoSalvar.mockRejectedValueOnce({ kind: "io", message: "i/o error: destino em uso" });
    st().alterar("Nome", (d) => void (d.titulo = "a"));
    await vi.advanceTimersByTimeAsync(ATRASO_MS);
    expect(st().salvando).toBe("erro");
    // o perito continua mexendo, uma mudança por segundo
    for (let i = 0; i < 6; i++) {
      st().alterar("Nome", (d) => void (d.titulo = `b${i}`));
      await vi.advanceTimersByTimeAsync(1000);
    }
    expect(cmd.edicaoSalvar.mock.calls.length).toBeGreaterThanOrEqual(2);
    expect(Math.ceil(6000 / NOVA_TENTATIVA_MS)).toBe(2);
  });

  it("Ctrl+S durante a exclusão não regrava o projeto", async () => {
    const p = ler<ProjetoEdicao>("completo.json");
    cmd.edicaoAbrir.mockResolvedValueOnce(aberto(p));
    await st().abrir(WS, p.id);
    const apagando = adiada<void>();
    cmd.edicaoExcluir.mockImplementationOnce(() => apagando.p);
    st().alterar("Nome", (d) => void (d.titulo = "pendente"));
    const excluindo = st().excluir(WS, p.id);
    await vi.advanceTimersByTimeAsync(0);
    await st().salvar();
    apagando.ok();
    await excluindo;
    await vi.advanceTimersByTimeAsync(TETO_MS);
    expect(cmd.edicaoSalvar).not.toHaveBeenCalled();
    expect(st().aberta).toBeNull();
  });
});

describe("fechar com a gravação falhando", () => {
  it("fechar rejeita e o projeto fica aberto, em erro; descartar fecha sem gravar; abrir outro só depois", async () => {
    const p = ler<ProjetoEdicao>("completo.json");
    cmd.edicaoAbrir.mockResolvedValueOnce(aberto(p));
    await st().abrir(WS, p.id);
    cmd.edicaoSalvar.mockRejectedValue({ kind: "io", message: "i/o error: sem espaço" });
    st().alterar("Nome", (d) => void (d.titulo = "não cabe"));
    await expect(st().fechar()).rejects.toBeTruthy();
    expect([st().aberta?.projetoId, st().salvando, st().doc?.titulo]).toEqual([p.id, "erro", "não cabe"]);
    // trocar de projeto com pendência que não grava: recusado, nada se perde calado
    await expect(st().novo(WS)).rejects.toBeTruthy();
    expect(st().aberta?.projetoId).toBe(p.id);
    const tentativas = cmd.edicaoSalvar.mock.calls.length;
    await st().fechar({ descartar: true });
    expect(st().aberta).toBeNull();
    await vi.advanceTimersByTimeAsync(TETO_MS);
    expect(cmd.edicaoSalvar.mock.calls.length).toBe(tentativas);
  });
});

describe("projeto novo com mídia", () => {
  const midia = (): MidiaRef => ({
    id: "m-nova", tipo: "video", origem: "caso", media_id: "vm", sha256: sha, nome: "cam.mkv", relativo: "Mídias/Vídeos/cam.mkv",
    duracao_s: 100, inicio_s: 1.4, largura: 1280, altura: 720, taxa: null, vfr: false, tem_audio: true, cor: null, derivacao: null,
    recebido: { nativo: null, obs: "" }, relogio_desvio_s: null, gravador: null, decodificacao: null, ocultacoes: [],
  });

  it("o trecho do vídeo inteiro usa a faixa da tabela (de_s, ate_s), não a duração do contêiner", async () => {
    cmd.edicaoMidiaInfo.mockResolvedValueOnce({ de_s: 1.433, ate_s: 101.473 });
    await st().novo(WS, midia());
    const c = st().doc!.principal[0] as ClipeTrecho;
    expect([c.inicio_s, c.fim_s, c.fim_pts]).toEqual([1.433, 101.473, null]);
    expect(st().doc!.midias).toHaveLength(1);
    await vi.advanceTimersByTimeAsync(ATRASO_MS);
    expect(gravados()).toHaveLength(1);
  });

  it("sem a faixa da tabela: início do contêiner e duração", async () => {
    await st().novo(WS, midia());
    const c = st().doc!.principal[0] as ClipeTrecho;
    expect([c.inicio_s, c.fim_s]).toEqual([1.4, 101.4]);
  });
});

describe("arquivo antigo", () => {
  it("projeto-minimo.json (sem os campos que o Rust lê com padrão): abre completo, muda, resolve e grava como chegou", async () => {
    const minimo = ler<ProjetoEdicao>("projeto-minimo.json");
    cmd.edicaoAbrir.mockResolvedValueOnce(aberto(minimo));
    await st().abrir(WS, minimo.id);
    expect(st().doc!.saida.auto).toBe(true);
    st().alterar("Nome", (d) => void (d.titulo = "já não é mínimo"));
    st().alterar("Trecho", (d) => void d.principal.push(novoTrecho("m", 0, 2)));
    expect(resolver(st().doc!, { tabela: () => null }).total_q).toBe(60);
    expect([st().doc!.legendas.itens, st().doc!.mixagem.normalizar, st().doc!.saida.exportacao.pacote]).toEqual([[], null, null]);
    await st().descarregar();
    // no disco: só o que mudou entra; o que o `completar` pôs e ninguém mexeu não vai
    expect(JSON.stringify(gravados().at(-1))).toBe(JSON.stringify({ ...minimo, titulo: "já não é mínimo", principal: st().doc!.principal }));
    // mexer na saída grava a saída completa
    st().alterar("Saída", (d) => void (d.saida.largura = 1280));
    await st().descarregar();
    expect(gravados().at(-1)!.saida).toEqual({ ...minimo.saida, largura: 1280, auto: true, exportacao: { ...minimo.saida.exportacao, pacote: null, personalizada: null } });
  });

  it("coleta com a tela fechada num projeto mínimo", async () => {
    const minimo = ler<ProjetoEdicao>("projeto-minimo.json");
    definirCorrente(WS, minimo.id);
    cmd.edicaoAbrir.mockResolvedValueOnce(aberto(minimo));
    const r = await receber(WS, [{ sha256: sha, inicio_s: 1, fim_s: 2 }]);
    expect(r).toEqual({ projeto: minimo.id, n: 1 });
    const g = gravados().at(-1)!;
    expect([g.midias.length, g.principal.length]).toEqual([1, 1]);
    expect(Object.keys(g)).toEqual([...Object.keys(minimo), "midias", "principal"]);
    expect(g.saida).toEqual(minimo.saida);
  });
});

describe("itens que o Rust lê e o front não esperava", () => {
  // o Rust abre para gravação e avisa `item_invalido`: a tela tem de continuar editável
  it("item de tipo conhecido com a forma errada (e o que nem é objeto) fica como desconhecido: edita, resolve, remove e grava igual", async () => {
    const p = ler<ProjetoEdicao>("completo.json");
    const n = p.principal.length;
    const ruins: unknown[] = [{ tipo: "trecho", id: "sem-campos" }, null, "texto", { tipo: "tela", id: "tela-ruim", efeitos: [], velocidade: {}, audio: {}, celulas: [null] }];
    (p.principal as unknown[]).push(...ruins);
    const efeitosRuins: unknown[] = [{ tipo: "enquadramento", id: "sem-regiao" }, null, { tipo: "ocultar", id: "sem-alvo" }];
    (p.principal[0] as unknown as { efeitos: unknown[] }).efeitos.push(...efeitosRuins);
    (p.sobreposicoes[0]!.clipes as unknown[]).push({ tipo: "imagem", id: "sob-ruim" }, null);
    const original = JSON.stringify(p);
    cmd.edicaoAbrir.mockResolvedValueOnce(aberto(p));
    await st().abrir(WS, p.id);
    const d = st().doc!;
    expect(d.principal.slice(n).map(clipeConhecido)).toEqual([false, false, false, false]);
    expect((d.principal[0] as ClipeTrecho).efeitos.slice(-3).map(efeitoConhecido)).toEqual([false, false, false]);
    const linha = resolver(d, { tabela: () => null });
    expect(linha.clipes.map((c) => c.id)).not.toContain("sem-campos");
    expect(() => aDe((d.principal[0] as ClipeTrecho).efeitos, 0, { w: 1920, h: 1080 }, { w: 1280, h: 720 })).not.toThrow();
    const titulo = d.titulo;
    st().alterar("Nome", (x) => void (x.titulo = "mexido"));
    st().alterar("Nome", (x) => void (x.titulo = titulo));
    await st().descarregar();
    expect(JSON.stringify(gravados().at(-1))).toBe(original);
    // o item ruim sai pela mesma porta dos outros
    st().alterar("Remover", (x) => void removerClipe(x, "sem-campos"));
    expect(st().doc!.principal).toHaveLength(n + 3);
  });

  it("item sem os campos que o Rust lê com padrão é conhecido e completo; efeito sem `ligado` vale ligado", async () => {
    const p = ler<ProjetoEdicao>("completo.json");
    const magro = { tipo: "trecho", id: "magro", midia: p.midias[0]!.id, inicio_s: 1, fim_s: 3, velocidade: { tipo: "constante", fator: 1 }, efeitos: [{ tipo: "girar", id: "g", graus: 90, espelhar_h: false, espelhar_v: false }] };
    (p.principal as unknown[]).push(magro, { tipo: "titulo", id: "cartela", duracao_s: 2, titulo: { modelo: "cartela", fundo: { tipo: "transparente" } } });
    const original = structuredClone(p);
    cmd.edicaoAbrir.mockResolvedValueOnce(aberto(p));
    await st().abrir(WS, p.id);
    const c = st().doc!.principal.find((x) => x.id === "magro") as ClipeTrecho;
    expect(clipeConhecido(c)).toBe(true);
    expect([c.rotulo, c.desligado, c.transicao_entrada, c.inicio_pts, c.fim_pts, c.audio]).toEqual(["", false, null, null, null, { volume: 1, mudo: false, manter_tom: true, silencios: [] }]);
    expect(c.efeitos[0]).toMatchObject({ ligado: true, nome: "", ativo: null });
    // A(τ) com o giro: como no Rust, que lê `ligado` ausente como verdadeiro
    expect(aDe(c.efeitos, 0, { w: 1920, h: 1080 }, { w: 1920, h: 1080 })).not.toEqual([1, 0, 0, 1, 0, 0]);
    expect(st().doc!.principal.find((x) => x.id === "cartela")).toMatchObject({ efeitos: [], titulo: { blocos: [] } });
    // sem mexer nos clipes, a lista vai para o disco como chegou
    st().alterar("Nome", (x) => void (x.titulo = "só o nome"));
    await st().descarregar();
    expect(JSON.stringify(gravados().at(-1))).toBe(JSON.stringify({ ...original, titulo: "só o nome" }));
    // mexendo num clipe, a lista vai completa
    st().alterar("Nome", (x) => void ((x.principal.find((y) => y.id === "magro") as ClipeTrecho).rotulo = "Entrada"));
    await st().descarregar();
    expect((gravados().at(-1)!.principal.find((x) => x.id === "magro") as ClipeTrecho).audio.silencios).toEqual([]);
  });
});

describe("arquivo com outro id dentro (o Rust abre só para leitura)", () => {
  it("o projeto aberto é o da pasta pedida: excluir fecha a tela", async () => {
    const p = ler<ProjetoEdicao>("completo.json");
    cmd.edicaoAbrir.mockResolvedValueOnce({ ...aberto(p), so_leitura: true });
    await st().abrir(WS, "pasta-com-outro-id");
    expect(st().aberta?.projetoId).toBe("pasta-com-outro-id");
    await st().excluir(WS, "pasta-com-outro-id");
    expect(cmd.edicaoExcluir).toHaveBeenCalledWith(WS, "pasta-com-outro-id");
    expect(st().aberta).toBeNull();
  });
});

describe("exportar", () => {
  it("projeto fechado no meio da gravação: não enfileira", async () => {
    const p = ler<ProjetoEdicao>("completo.json");
    cmd.edicaoAbrir.mockResolvedValueOnce(aberto(p));
    await st().abrir(WS, p.id);
    const gravando = adiada<{ updated_at: string }>();
    cmd.edicaoSalvar.mockImplementationOnce(() => gravando.p);
    st().alterar("Nome", (d) => void (d.titulo = "x"));
    const exportando = useRenderStore.getState().exportar({ tipo: "quadro", q: 0, largura: 640 }, []);
    const pega = exportando.catch((e: unknown) => e);
    const fechando = st().fechar();
    gravando.ok({ updated_at: "" });
    await fechando;
    expect(await pega).toBeInstanceOf(Error);
    expect(cmd.edicaoEnfileirar).not.toHaveBeenCalled();
  });
});

describe("fila: leitura inicial × evento", () => {
  it("a leitura que chega depois de um evento não troca a lista nova pela velha", async () => {
    const t = (id: string): Tarefa => ({ id, tipo: "exportar", caso: "c", projeto_id: "p", titulo: "", estado: { tipo: "na_fila" }, criado_em: "" });
    const leitura = adiada<Tarefa[]>();
    cmd.edicaoFila.mockImplementationOnce(() => leitura.p);
    const ouvindo = useRenderStore.getState().ouvir();
    await vi.advanceTimersByTimeAsync(0);
    ev.ouvintes.get(EVENTO_FILA)!({ payload: [t("nova")] });
    leitura.ok([t("velha")]);
    await ouvindo;
    expect(useRenderStore.getState().tarefas.map((x) => x.id)).toEqual(["nova"]);
  });
});

describe("análise", () => {
  it("resposta de uma revisão que já mudou não prende tempo velho por cima do novo", async () => {
    const p = ler<ProjetoEdicao>("completo.json");
    const c1 = p.principal[0] as ClipeTrecho;
    cmd.edicaoAbrir.mockResolvedValueOnce(aberto(p));
    await st().abrir(WS, p.id);
    await vi.advanceTimersByTimeAsync(0);
    const resposta = adiada<Analise>();
    cmd.edicaoAnalisar.mockImplementationOnce(() => resposta.p);
    const mexer = (t: number) => st().alterar("Início", (d) => void Object.assign(d.principal[0] as ClipeTrecho, { inicio_s: t, inicio_pts: null }));
    mexer(5);
    await vi.advanceTimersByTimeAsync(ANALISE_MS);
    // o perito mexe de novo; a resposta do pedido anterior chega antes do pedido seguinte sair
    mexer(7);
    await vi.advanceTimersByTimeAsync(ANALISE_MS - 100);
    resposta.ok({ ...analiseVazia(), prender: [{ onde: { tipo: "inicio", clipe: c1.id }, pts: 5010, t: 5.01 }] });
    await vi.advanceTimersByTimeAsync(0);
    const c = st().doc!.principal[0] as ClipeTrecho;
    expect([c.inicio_s, c.inicio_pts]).toEqual([7, null]);
    expect(st().analiseRevisao).toBeLessThan(st().revisao);
  });

  it("resposta que chega depois de fechar não mexe em nada", async () => {
    const p = ler<ProjetoEdicao>("completo.json");
    const resposta = adiada<Analise>();
    cmd.edicaoAbrir.mockResolvedValueOnce(aberto(p));
    await st().abrir(WS, p.id);
    await vi.advanceTimersByTimeAsync(0);
    cmd.edicaoAnalisar.mockImplementationOnce(() => resposta.p);
    st().alterar("Nome", (d) => void (d.titulo = "y"));
    await vi.advanceTimersByTimeAsync(ANALISE_MS);
    await st().fechar();
    resposta.ok({ ...analiseVazia(), saida_auto: [640, 480, { num: 25, den: 1 }] });
    await vi.advanceTimersByTimeAsync(TETO_MS);
    expect(st().doc).toBeNull();
    expect(st().analise).toBeNull();
    expect(gravados()).toHaveLength(1);
  });
});
