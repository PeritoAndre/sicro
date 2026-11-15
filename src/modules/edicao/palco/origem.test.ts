/**
 * Fecho da Fase 1, contas puras do palco: intermediário de origem (§7.5: o que o palco toca, quando a anotação pode
 * ser marcada, relógio do arquivo por campo), busca medida e pedido de proxy (§7.6: 3 buscas pausadas, p50 acima de
 * 120 ms; clipe abaixo de 0,25×) com o comando falso.
 */

import { readdirSync, readFileSync } from "node:fs";
import { join } from "node:path";
import { beforeEach, describe, expect, it, vi } from "vitest";
import type { ClipeTrecho, Efeito, PreviaMidia, ProjetoEdicao, TabelaQuadros } from "@domain/edicao";

const cmd = vi.hoisted(() => ({ edicaoProxyPedir: vi.fn(), edicaoOrigemEstado: vi.fn(), edicaoMidiaInfo: vi.fn(), edicaoTabelaQuadros: vi.fn() }));
vi.mock("@core/commands", () => ({ commands: cmd }));
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn(async () => () => {}) }));
vi.mock("@tauri-apps/api/core", () => ({ convertFileSrc: (p: string) => `asset://${p}`, invoke: vi.fn() }));

import { camposPorQuadro, estadoDaOrigem, marcaForaDaOrigem, marcacaoParada, mudaAImagem, origemPendente, previaDoPalco, relogioDaOrigem, slotsDoQuadroDaFonte } from "./origem";
import { BUSCA_P50_MAX_MS, alvosDaMedida, clipePedeProxy, garantirMidia, p50, pedirProxy, precisaDeProxy, previaDaMidia, proxyPedido, recursoDe, registrarBusca, useRecursos, type EstadoRecurso } from "./recursos";
import { Buscador, MEDIDA_TETO_MS, type Agenda } from "./motor/busca";
import type { ElementoBusca } from "./motor/elemento";
import { relogioMidia } from "./motor/relogioMidia";
import { novoProjeto, novoTrecho } from "../modelo/projeto";
import { tabelaDoClipe } from "../modelo/realce";
import { exibido, mapaTrecho, tempoDoQuadro } from "../modelo/tempo";
import { useEdicaoStore } from "../store/edicaoStore";
import { useFundoStore } from "../store/fundoStore";
import type { OrigemDoClipe } from "../store/origemStore";

/** 30 qps em 1/15360. */
const tab = (n: number, passo = 512, chaves = [0]): TabelaQuadros => ({ tb: { num: 1, den: 15360 }, pts0: 0, passos: Array(n - 1).fill(passo), passo_mediano: passo, chaves, vfr: false, origem: "decodificador" });
/** Efeito com a forma mínima que a tela exige de cada tipo (`efeitoConhecido`). */
const FORMA: Record<string, object> = { estabilizar: { analise: {} }, destaque: { traco: {} }, ocultar: { alvo: {} } };
const ef = (tipo: string, ligado = true, mais: object = {}) => ({ id: `e-${tipo}`, tipo, ligado, nome: "", ativo: null, ...FORMA[tipo], ...mais }) as unknown as Efeito;
const previa = (tipo: PreviaMidia["tipo"], de_s = 0, ate_s = 10, pronta = true): PreviaMidia => ({ relativo: `${tipo}.mp4`, tipo, desloc_s: de_s, de_s, ate_s, largura: 640, altura: 360, pronta });
const pronto = (de_s: number, ate_s: number): OrigemDoClipe => ({ tipo: "pronto", previa: previa("origem", de_s, ate_s), trf: null });
const fazendo: OrigemDoClipe = { tipo: "fazendo", tarefa: "t1", feito: 0.4 };

describe("intermediário de origem: quando a anotação pode ser marcada", () => {
  it("estabilizar, lente e desentrelace ligados mudam a imagem marcada; ajustes, nitidez e média, não", () => {
    const trecho = (...efeitos: Efeito[]) => ({ tipo: "trecho" as const, efeitos });
    for (const t of ["estabilizar", "lente", "desentrelacar"]) {
      expect(mudaAImagem(trecho(ef(t))), t).toBe(true);
      expect(mudaAImagem(trecho(ef(t, false))), `${t} desligado`).toBe(false);
    }
    expect(mudaAImagem(trecho(ef("ajustes"), ef("nitidez"), ef("media_temporal"), ef("destaque"), { tipo: "do_futuro" } as never))).toBe(false);
    // quadro congelado (sem intermediário): só o que muda a geometria para a marcação; o desentrelace, não
    expect(mudaAImagem({ tipo: "congelar", efeitos: [ef("desentrelacar")] })).toBe(false);
    expect(mudaAImagem({ tipo: "congelar", efeitos: [ef("lente")] })).toBe(true);
    expect(origemPendente({ tipo: "congelar", efeitos: [ef("lente")] }, undefined)).toBe(true);
    expect(origemPendente({ tipo: "congelar", efeitos: [ef("desentrelacar")] }, undefined)).toBe(false);
  });

  it("pendente enquanto o intermediário não está pronto (em preparo, falhou, nunca pedido); clipe sem o efeito nunca", () => {
    const c = { tipo: "trecho" as const, efeitos: [ef("lente")] };
    expect(origemPendente(c, undefined)).toBe(true);
    expect(origemPendente(c, fazendo)).toBe(true);
    expect(origemPendente(c, { tipo: "falhou", erro: { codigo: null, frase: "x" } })).toBe(true);
    expect(origemPendente(c, { tipo: "pronto", previa: previa("origem", 0, 10, false), trf: null })).toBe(true);
    expect(origemPendente(c, pronto(0, 10))).toBe(false);
    expect(origemPendente({ tipo: "trecho", efeitos: [ef("ajustes")] }, undefined)).toBe(false);
    expect(origemPendente(null, undefined)).toBe(false);
  });

  it("ocultação (espaço original) e `lugar` (canvas) não esperam; destaque do clipe pendente espera", () => {
    const p: ProjetoEdicao = novoProjeto();
    const c = { ...novoTrecho("m", 0, 5), id: "c1", efeitos: [ef("lente"), ef("destaque"), ef("ocultar")] } as ClipeTrecho;
    const livre = { ...novoTrecho("m", 0, 5), id: "c2", efeitos: [ef("destaque", true, { id: "d2" })] } as ClipeTrecho;
    p.principal = [c, livre];
    expect(marcaForaDaOrigem(p, { tipo: "lugar", efeito: "e-destaque" })).toBe(true);
    expect(marcaForaDaOrigem(p, { tipo: "caixa", efeito: "e-ocultar" })).toBe(true);
    expect(marcaForaDaOrigem(p, { tipo: "caixa", efeito: "e-destaque" })).toBe(false);
    expect(marcacaoParada(p, { tipo: "caixa", efeito: "e-destaque" }, {})).toBe(true);
    expect(marcacaoParada(p, { tipo: "caixa", efeito: "e-destaque" }, { c1: fazendo })).toBe(true);
    expect(marcacaoParada(p, { tipo: "caixa", efeito: "e-destaque" }, { c1: pronto(0, 5) })).toBe(false);
    expect(marcacaoParada(p, { tipo: "caixa", efeito: "e-ocultar" }, {})).toBe(false);
    expect(marcacaoParada(p, { tipo: "lugar", efeito: "e-destaque" }, {})).toBe(false);
    // clipe sem efeito que muda a imagem: nunca para
    expect(marcacaoParada(p, { tipo: "caixa", efeito: "d2" }, {})).toBe(false);
  });

  it("o palco toca o intermediário pronto só com o quadro dentro dele e sem marcação no original", () => {
    const m = previa("original");
    const o = pronto(2, 6);
    expect(previaDoPalco({ origem: o, daMidia: m, t: 3, original: false })).toEqual({ previa: (o as { previa: PreviaMidia }).previa, daOrigem: true });
    expect(previaDoPalco({ origem: o, daMidia: m, t: 2, original: false }).daOrigem).toBe(true);
    // fim exclusivo, antes do começo, marcando ocultar, em preparo: a prévia da mídia
    for (const x of [
      { origem: o, t: 6, original: false },
      { origem: o, t: 1.99, original: false },
      { origem: o, t: 3, original: true },
      { origem: fazendo, t: 3, original: false },
      { origem: undefined, t: 3, original: false },
    ]) {
      expect(previaDoPalco({ ...x, daMidia: m })).toEqual({ previa: m, daOrigem: false });
    }
  });

  it("relógio do intermediário por campo: a busca do quadro j cai DENTRO do primeiro campo dele (nunca na divisa)", () => {
    const t = tab(100);
    const p = previa("origem", tempoDoQuadro(t, 10), tempoDoQuadro(t, 60));
    const campo = [ef("desentrelacar", true, { modo: "campo", ordem: "auto" })];
    expect(camposPorQuadro(t, campo)).toBe(2);
    expect(camposPorQuadro(t, [ef("desentrelacar", true, { modo: "quadro", ordem: "auto" })])).toBe(1);
    // base de um tique por quadro: o campo não cabe
    expect(camposPorQuadro(tab(100, 1), campo)).toBe(1);
    const dosCampos = tabelaDoClipe(t, campo);
    const r2 = relogioDaOrigem(p, t, 2);
    const r1 = relogioDaOrigem(p, t, 1);
    const ref = relogioMidia(p, t);
    for (const j of [10, 11, 37, 59]) {
      // tempo de fonte do alvo: dentro do campo 2j (o primeiro do quadro j), longe das duas divisas
      const tf = r2.alvo(j) + p.desloc_s;
      expect(exibido(dosCampos, tf)).toBe(2 * j);
      expect(tf - tempoDoQuadro(dosCampos, 2 * j)).toBeCloseTo(tempoDoQuadro(dosCampos, 2 * j + 1) - tf, 9);
      // o quadro da fonte lido de volta é o mesmo; sem campo, o relógio é o da mídia
      expect(r2.quadro(r2.alvo(j))).toBe(j);
      expect(r1.alvo(j)).toBe(ref.alvo(j));
      // pelo relógio da mídia o alvo cairia na divisa dos dois campos
      expect(exibido(dosCampos, ref.alvo(j) + p.desloc_s)).toBe(2 * j + 1);
    }
    // passo ímpar: o primeiro campo tem a metade arredondada para cima
    const impar = tab(50, 513);
    const ri = relogioDaOrigem(previa("origem", 0, 1), impar, 2);
    const di = tabelaDoClipe(impar, campo);
    expect(exibido(di, ri.alvo(7))).toBe(14);
  });

  it("slots do quadro da fonte num mapa feito na tabela de campos: os dois campos, seguidos", () => {
    const t = tab(120);
    const campo = [ef("desentrelacar", true, { modo: "campo", ordem: "auto" })];
    const dc = tabelaDoClipe(t, campo);
    // quadros 10..40 a 60 slots/s: cada campo tem o seu slot; a 30, o segundo campo é descartado
    const m60 = mapaTrecho(dc, 20, 80, { tipo: "constante", fator: 1 }, { num: 60, den: 1 });
    expect(slotsDoQuadroDaFonte(m60, 10, 2)).toEqual([0, 2]);
    expect(slotsDoQuadroDaFonte(m60, 25, 2)).toEqual([30, 2]);
    const m30 = mapaTrecho(dc, 20, 80, { tipo: "constante", fator: 1 }, { num: 30, den: 1 });
    expect(slotsDoQuadroDaFonte(m30, 25, 2)).toEqual([15, 1]);
    expect(slotsDoQuadroDaFonte(m30, 9, 2)).toBeNull();
    // sem campo: o mapa na tabela da fonte
    const m = mapaTrecho(t, 10, 40, { tipo: "constante", fator: 1 }, { num: 30, den: 1 });
    expect(slotsDoQuadroDaFonte(m, 25, 1)).toEqual([15, 1]);
  });

  it("edicao_origem_estado (comando falso): pronto, em preparo, nada (undefined) e a falha recente como `falhou`", async () => {
    const p = novoProjeto();
    cmd.edicaoOrigemEstado.mockResolvedValueOnce(fazendo);
    expect(await estadoDaOrigem("/c", p, "c1")).toEqual(fazendo);
    expect(cmd.edicaoOrigemEstado).toHaveBeenCalledWith("/c", p, "c1");
    cmd.edicaoOrigemEstado.mockResolvedValueOnce(null);
    expect(await estadoDaOrigem("/c", p, "c1")).toBeUndefined();
    cmd.edicaoOrigemEstado.mockRejectedValueOnce({ kind: "validation", message: "validation error: fonte_danificada: Vídeo danificado" });
    expect(await estadoDaOrigem("/c", p, "c1")).toEqual({ tipo: "falhou", erro: { codigo: "fonte_danificada", frase: "Vídeo danificado" } });
  });
});

describe("regra de código: a seção Realce só é importada pelo registro", () => {
  // `registro.ts` lê `REALCE_PRONTO` ao carregar e a seção importa o registro: quem importasse a seção antes do
  // registro o faria ler a bandeira ainda sem valor (a seção cairia na da Fase 0, ou o módulo nem carregaria)
  it("nenhum arquivo fora do registro e dos testes importa `secoes/RealceSecao`", () => {
    const raiz = join(process.cwd(), "src/modules/edicao");
    const arquivos = (readdirSync(raiz, { recursive: true }) as string[]).filter((f) => /\.tsx?$/.test(f) && !/\.test\.tsx?$/.test(f));
    expect(arquivos.length).toBeGreaterThan(80);
    const quem = arquivos.filter((f) => /from "[^"]*secoes\/RealceSecao"|from "\.\/RealceSecao"/.test(readFileSync(join(raiz, f), "utf8")));
    expect(quem.map((f) => f.replace(/\\/g, "/"))).toEqual(["inspetor/registro.ts"]);
  });
});

describe("busca medida (§7.6)", () => {
  function falso() {
    const ouvintes = new Set<() => void>();
    const pedidos: number[] = [];
    const estado = { buscando: false, agora: 0 };
    const el: ElementoBusca = {
      buscar: (j) => void pedidos.push(j),
      quadro: () => pedidos.at(-1) ?? 0,
      agora: () => 0,
      buscando: () => estado.buscando,
      aoBuscado: (f) => (ouvintes.add(f), () => ouvintes.delete(f)),
      tocar: async () => {},
      pausar: () => {},
      pausado: () => true,
    };
    const rafs: (() => void)[] = [];
    const timers = new Map<number, { f: () => void; ms: number }>();
    let n = 0;
    const agenda: Agenda = {
      raf: (f) => void rafs.push(f),
      setTimeout: (f, ms) => (timers.set(++n, { f, ms }), n),
      clearTimeout: (h) => void timers.delete(h as number),
      agora: () => estado.agora,
    };
    /** O `seeked` chega `ms` depois do pedido. */
    const buscado = (ms = 0) => {
      estado.agora += ms;
      ouvintes.forEach((f) => f());
    };
    const quadro = () => rafs.splice(0).forEach((f) => f());
    const estourar = () => [...timers.values()].forEach((t) => t.f());
    return { el, pedidos, agenda, buscado, quadro, timers, estado, estourar };
  }
  /** Buscador com o quadro `j` já pintado. */
  function pintadoEm(f: ReturnType<typeof falso>, j: number) {
    const pintados: number[] = [];
    const b = new Buscador(f.el, (q) => void pintados.push(q), f.agenda);
    b.ir(j);
    f.buscado();
    f.quadro();
    f.quadro();
    return { b, pintados };
  }

  it("mede o tempo de cada busca até o `seeked`, sem pintar, e volta ao quadro que estava na tela", async () => {
    const f = falso();
    const { b, pintados } = pintadoEm(f, 7);
    const n = pintados.length;
    const medida = b.medir([100, 200, 300]);
    expect(b.medindo).toBe(true);
    // medindo, a imagem do elemento não é a do quadro pintado: há alvo pendente (marcação parada)
    expect(b.alvo).not.toBeNull();
    expect(f.pedidos.at(-1)).toBe(100);
    f.buscado(90);
    expect(f.pedidos.at(-1)).toBe(200);
    f.buscado(150);
    f.buscado(130);
    expect(await medida).toEqual([90, 150, 130]);
    expect(b.medindo).toBe(false);
    // nada pintado durante a medida; depois, a busca de volta ao quadro 7
    expect(pintados).toHaveLength(n);
    expect(f.pedidos).toEqual([7, 100, 200, 300, 7]);
    f.buscado();
    f.quadro();
    f.quadro();
    expect(b.alvo).toBeNull();
    expect(b.ultimo).toBe(7);
    expect(pintados.slice(n)).toEqual([7, 7]);
  });

  it("um `ir` no meio cancela a medida (sem resultado) e vale o quadro pedido", async () => {
    const f = falso();
    const { b } = pintadoEm(f, 7);
    const medida = b.medir([100, 200, 300]);
    f.buscado(50);
    b.ir(8);
    expect(await medida).toBeNull();
    expect(b.medindo).toBe(false);
    expect(f.pedidos.at(-1)).toBe(8);
    f.buscado();
    f.quadro();
    f.quadro();
    expect(b.ultimo).toBe(8);
    // com busca em curso não mede
    b.ir(9);
    expect(await b.medir([1, 2, 3])).toBeNull();
  });

  it("busca que passa do teto conta como o teto e encerra; `seeked` que não veio com o elemento parado não mede", async () => {
    const f = falso();
    const { b } = pintadoEm(f, 7);
    let medida = b.medir([100, 200, 300]);
    f.buscado(40);
    expect([...f.timers.values()].map((t) => t.ms)).toEqual([MEDIDA_TETO_MS]);
    f.estado.buscando = true;
    f.estourar();
    expect(await medida).toEqual([40, MEDIDA_TETO_MS]);
    f.estado.buscando = false;
    f.buscado();
    f.quadro();
    f.quadro();
    medida = b.medir([100, 200, 300]);
    f.estourar();
    expect(await medida).toBeNull();
  });

  it("p50 = mediana; alvos = meio de três GOPs espalhados, nunca o quadro da tela", () => {
    expect(p50([])).toBeNull();
    expect(p50([150])).toBe(150);
    expect(p50([90, 300, 130])).toBe(130);
    expect(p50([90, 130])).toBe(110);
    // GOP de 100 quadros em 1000: o meio do GOP de 20, 50 e 80 % da mídia
    const t = tab(1000, 512, [0, 100, 200, 300, 400, 500, 600, 700, 800, 900]);
    expect(alvosDaMedida(t, 0)).toEqual([250, 550, 850]);
    expect(alvosDaMedida(t, 550)).toEqual([250, 551, 850]);
    // sem quadros-chave conhecidos (um só): as frações da mídia
    expect(alvosDaMedida(tab(300), null)).toEqual([60, 150, 240]);
    // mídia de 3 quadros: não há o que medir
    expect(alvosDaMedida(tab(3), 0)).toEqual([]);
    // último GOP aberto até o fim
    expect(alvosDaMedida(tab(1000, 512, [0, 700]), null)).toEqual([350, 351, 850]);
  });
});

describe("proxy pedido pelo palco (§7.6)", () => {
  const WS = "/casos/proxy";
  const SHA = "e".repeat(64);
  const rec = (x: Partial<EstadoRecurso> = {}): EstadoRecurso => ({ info: null, tabela: null, erro: null, pedindo: false, ...x });
  const trecho = (fator: number) => ({ ...novoTrecho("m", 0, 5), velocidade: { tipo: "constante" as const, fator } });

  beforeEach(() => {
    for (const f of Object.values(cmd)) f.mockReset();
    for (const f of [cmd.edicaoMidiaInfo, cmd.edicaoTabelaQuadros]) f.mockImplementation(() => new Promise(() => {}));
    useFundoStore.setState({ tarefas: [] });
    // dois trechos da mesma mídia
    const doc = novoProjeto();
    doc.midias = [{ id: "m", sha256: SHA, tipo: "video", origem: "caso" } as never];
    doc.principal = [{ ...novoTrecho("m", 0, 5), id: "c1" }, { ...novoTrecho("m", 5, 8), id: "c2" }];
    useEdicaoStore.setState({ doc, ws: WS });
  });

  it("a regra do palco: p50 acima de 120 ms, ou clipe com rampa ou abaixo de 0,25×", () => {
    expect(BUSCA_P50_MAX_MS).toBe(120);
    for (const [ms, esperado] of [[60, false], [120, false], [120.5, true], [238, true], [null, false], [undefined, false]] as const) {
      expect(precisaDeProxy(trecho(1), ms), String(ms)).toBe(esperado);
    }
    for (const [fator, esperado] of [[1, false], [0.25, false], [0.249, true], [0.05, true], [-1, true], [NaN, true]] as const) {
      expect(clipePedeProxy(trecho(fator)), String(fator)).toBe(esperado);
      expect(precisaDeProxy(trecho(fator), 10)).toBe(esperado);
    }
    expect(clipePedeProxy({ tipo: "trecho", velocidade: { tipo: "rampa" } })).toBe(true);
    // quadro congelado, imagem e título não pedem por si
    expect(clipePedeProxy({ tipo: "congelar" })).toBe(false);
  });

  it("o palco toca o proxy pedido só quando ele está pronto; antes, a prévia da mídia", () => {
    const info = { previa: previa("original") } as EstadoRecurso["info"];
    expect(previaDaMidia(rec({ info }))).toBe(info!.previa);
    expect(previaDaMidia(rec({ info, pedida: previa("original") }))).toBe(info!.previa);
    expect(previaDaMidia(rec({ info, pedida: previa("proxy", 0, 10, false) }))).toBe(info!.previa);
    const proxy = previa("proxy");
    expect(previaDaMidia(rec({ info, pedida: proxy }))).toBe(proxy);
    expect(previaDaMidia(rec())).toBeNull();
  });

  it("edicao_proxy_pedir (comando falso): pede com a busca medida, guarda a resposta e pede de novo quando a fila de fundo termina o proxy", async () => {
    garantirMidia(WS, SHA);
    useRecursos.setState((s) => ({ midias: { [SHA]: rec({ info: { previa: previa("original") } as EstadoRecurso["info"] }) }, versao: s.versao + 1 }));
    registrarBusca(WS, SHA, 238);
    expect(recursoDe(SHA).busca_p50_ms).toBe(238);
    // enquanto o proxy é feito, a resposta é o que já toca
    cmd.edicaoProxyPedir.mockResolvedValueOnce(previa("original"));
    expect(proxyPedido(SHA)).toBe(false);
    await pedirProxy(WS, "c1", SHA, 238);
    expect(proxyPedido(SHA)).toBe(true);
    expect(cmd.edicaoProxyPedir).toHaveBeenCalledWith(WS, useEdicaoStore.getState().doc, "c1", 238);
    expect(previaDaMidia(recursoDe(SHA))?.tipo).toBe("original");
    // a fila de fundo termina o proxy dessa mídia: pede de novo e passa a tocar o proxy
    const proxy = previa("proxy");
    cmd.edicaoProxyPedir.mockResolvedValueOnce(proxy);
    useFundoStore.setState({ tarefas: [{ id: "f9", tipo: "proxy", chave: `proxy:${SHA}`, feito: 1, estado: { tipo: "pronta" } }] });
    await vi.waitFor(() => expect(previaDaMidia(recursoDe(SHA))).toEqual(proxy));
    expect(cmd.edicaoProxyPedir).toHaveBeenCalledTimes(2);
    expect(cmd.edicaoProxyPedir.mock.calls[1]).toEqual([WS, useEdicaoStore.getState().doc, "c1", 238]);
    // a mesma tarefa pronta não pede outra vez
    useFundoStore.setState({ tarefas: [{ id: "f9", tipo: "proxy", chave: `proxy:${SHA}`, feito: 1, estado: { tipo: "pronta" } }] });
    await Promise.resolve();
    expect(cmd.edicaoProxyPedir).toHaveBeenCalledTimes(2);
    // falha do pedido: o erro fica guardado (ícone no palco), o que tocava continua
    cmd.edicaoProxyPedir.mockRejectedValueOnce({ kind: "validation", message: "validation error: disco_cheio: Disco cheio" });
    await pedirProxy(WS, "c1", SHA, 238);
    expect(recursoDe(SHA).erroDoProxy).toEqual({ codigo: "disco_cheio", frase: "Disco cheio" });
    expect(previaDaMidia(recursoDe(SHA))).toEqual(proxy);
  });

  it("o clipe que pediu saiu do projeto enquanto o proxy era feito: o pedido seguinte vai por outro trecho da mídia; sem nenhum, nada é pedido e a mídia volta a não pedida", async () => {
    garantirMidia(WS, SHA);
    useRecursos.setState((s) => ({ midias: { [SHA]: rec({ info: { previa: previa("original") } as EstadoRecurso["info"] }) }, versao: s.versao + 1 }));
    cmd.edicaoProxyPedir.mockResolvedValue(previa("original"));
    await pedirProxy(WS, "c1", SHA, 238);
    expect(cmd.edicaoProxyPedir.mock.calls.map((c) => c[2])).toEqual(["c1"]);
    const semC1 = { ...useEdicaoStore.getState().doc!, principal: useEdicaoStore.getState().doc!.principal.slice(1) };
    useEdicaoStore.setState({ doc: semC1 });
    const proxy = previa("proxy");
    cmd.edicaoProxyPedir.mockResolvedValue(proxy);
    useFundoStore.setState({ tarefas: [{ id: "f10", tipo: "proxy", chave: `proxy:${SHA}`, feito: 1, estado: { tipo: "pronta" } }] });
    await vi.waitFor(() => expect(previaDaMidia(recursoDe(SHA))).toEqual(proxy));
    expect(cmd.edicaoProxyPedir.mock.calls.map((c) => c[2])).toEqual(["c1", "c2"]);
    expect(recursoDe(SHA).erroDoProxy ?? null).toBeNull();
    // nenhum trecho da mídia: nada é pedido (nem erro), e um clipe novo dela pode pedir de novo
    useEdicaoStore.setState({ doc: { ...semC1, principal: [] } });
    useFundoStore.setState({ tarefas: [{ id: "f11", tipo: "proxy", chave: `proxy:${SHA}`, feito: 1, estado: { tipo: "pronta" } }] });
    await Promise.resolve();
    await Promise.resolve();
    expect(cmd.edicaoProxyPedir).toHaveBeenCalledTimes(2);
    expect(proxyPedido(SHA)).toBe(false);
  });
});
