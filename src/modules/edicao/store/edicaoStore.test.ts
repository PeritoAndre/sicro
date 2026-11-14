import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { Analise, Caixa, Chave, ClipeTrecho, EfAjustes, ProjetoAberto, ProjetoEdicao, Selecao } from "@domain/edicao";

const cmd = vi.hoisted(() => ({
  edicaoAbrir: vi.fn(),
  edicaoSalvar: vi.fn(),
  edicaoAnalisar: vi.fn(),
  edicaoExcluir: vi.fn(),
  listVideoMedia: vi.fn(),
  edicaoMidiaInfo: vi.fn(),
  edicaoQuadroEm: vi.fn(),
  edicaoEnfileirar: vi.fn(),
}));
vi.mock("@core/commands", () => ({ commands: cmd }));
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn(async () => () => {}) }));

import { ANALISE_MS, correnteDe, definirCorrente, separarErro, useEdicaoStore } from "./edicaoStore";
import { ATRASO_MS, NOVA_TENTATIVA_MS, TETO_MS } from "./autosave";
import { LIMITE_PASSOS } from "./historico";
import { receber } from "./coleta";
import { useRenderStore } from "./renderStore";
import { aplicarMudanca, clipeConhecido, efeitoConhecido, moverClipe, novoTrecho, tirarChave } from "../modelo/projeto";
import { ler, lerTexto } from "../modelo/testeFixtures";

const WS = "/casos/c1";
const st = () => useEdicaoStore.getState();

const analiseVazia = (): Analise => ({
  problemas: [],
  linha: { fps: { num: 30, den: 1 }, total_q: 0, clipes: [], livres: [] },
  midias: [],
  prender: [],
  saida_auto: null,
});

/** Último documento mandado para `edicao_salvar`. */
const salvo = (): ProjetoEdicao => cmd.edicaoSalvar.mock.calls.at(-1)?.[1] as ProjetoEdicao;

async function abrirCom(projeto: ProjetoEdicao, so_leitura = false) {
  const r: ProjetoAberto = { projeto, importado_de: null, so_leitura, avisos: [], raiz_cache: "/cache/edicao/c1" };
  cmd.edicaoAbrir.mockResolvedValueOnce(r);
  await st().abrir(WS, projeto.id);
  await vi.advanceTimersByTimeAsync(0);
}

function congeladoFundo(v: unknown): boolean {
  if (v === null || typeof v !== "object") return true;
  if (!Object.isFrozen(v)) return false;
  return Object.values(v).every(congeladoFundo);
}

/** Gerador com semente (mulberry32). */
function sorteio(semente: number) {
  let a = semente >>> 0;
  return (n: number) => {
    a = (a + 0x6d2b79f5) >>> 0;
    let t = a;
    t = Math.imul(t ^ (t >>> 15), t | 1);
    t ^= t + Math.imul(t ^ (t >>> 7), t | 61);
    return Math.floor((((t ^ (t >>> 14)) >>> 0) / 4294967296) * n);
  };
}

beforeEach(() => {
  vi.useFakeTimers({ now: new Date("2026-11-14T04:00:00Z") });
  for (const f of Object.values(cmd)) f.mockReset();
  cmd.edicaoSalvar.mockImplementation(async () => ({ updated_at: new Date().toISOString() }));
  cmd.edicaoAnalisar.mockImplementation(async () => analiseVazia());
  cmd.edicaoExcluir.mockResolvedValue(undefined);
});

afterEach(async () => {
  await st().fechar().catch(() => {});
  vi.useRealTimers();
});

describe("abrir, salvar e voltar igual", () => {
  it("completo.json passa pela store e o JSON volta igual", async () => {
    const original = lerTexto("completo.json");
    await abrirCom(JSON.parse(original) as ProjetoEdicao);
    expect(st().raizCache).toBe("/cache/edicao/c1");
    const titulo = st().doc!.titulo;
    st().alterar("Nome", (d) => {
      d.titulo = "outro";
    });
    st().alterar("Nome", (d) => {
      d.titulo = titulo;
    });
    await st().descarregar();
    expect(cmd.edicaoSalvar).toHaveBeenCalledTimes(1);
    expect(salvo()).toEqual(JSON.parse(original));
    expect(JSON.stringify(salvo())).toBe(JSON.stringify(JSON.parse(original)));
  });

  it("clipe e efeito desconhecidos: abre, aparecem, salvam e voltam iguais", async () => {
    const original = JSON.parse(lerTexto("projeto-desconhecido.json")) as ProjetoEdicao;
    const aberto = JSON.parse(lerTexto("projeto-desconhecido.json")) as ProjetoEdicao;
    await abrirCom(aberto);
    const doc = st().doc!;
    const clipesFuturos = doc.principal.filter((c) => !clipeConhecido(c));
    const efeitosFuturos = doc.principal.filter(clipeConhecido).flatMap((c) => c.efeitos.filter((e) => !efeitoConhecido(e)));
    expect(clipesFuturos.length).toBeGreaterThan(0);
    expect(efeitosFuturos.length).toBeGreaterThan(0);
    st().alterar("Nome", (d) => {
      d.titulo = "renomeado";
    });
    await st().descarregar();
    // os itens do futuro vão para o disco como chegaram (mesmo objeto)
    for (const c of clipesFuturos) expect(salvo().principal).toContain(c);
    expect(JSON.stringify(salvo())).toBe(JSON.stringify({ ...original, titulo: "renomeado" }));
  });

  it("documento congelado: atribuição fora do alterar é exceção", async () => {
    await abrirCom(ler<ProjetoEdicao>("completo.json"));
    expect(congeladoFundo(st().doc)).toBe(true);
    expect(() => {
      (st().doc as ProjetoEdicao).titulo = "x";
    }).toThrow(TypeError);
    st().alterar("Saída", (d) => {
      d.saida.largura = 1280;
    });
    expect(congeladoFundo(st().doc)).toBe(true);
  });

  it("lista de chaves vazia nunca entra no documento (tirar a última chave vira valor fixo)", async () => {
    await abrirCom(ler<ProjetoEdicao>("completo.json"));
    const antes = st().doc;
    expect(() =>
      st().alterar("Volume", (d) => {
        const c = d.principal[0] as ClipeTrecho;
        c.audio.volume = [];
      }),
    ).toThrow(/lista de chaves vazia: principal\[0\]\.audio\.volume/);
    expect(st().doc).toBe(antes);
    st().alterar("Volume", (d) => {
      const c = d.principal[0] as ClipeTrecho;
      c.audio.volume = [{ t: 1, pts: null, v: 0.5, interp: "linear" }];
    });
    st().alterar("Volume", (d) => {
      const c = d.principal[0] as ClipeTrecho;
      c.audio.volume = tirarChave(c.audio.volume as Chave<number>[], 0);
    });
    expect((st().doc!.principal[0] as ClipeTrecho).audio.volume).toBe(0.5);
  });

  it("só leitura: alterar não muda nada", async () => {
    await abrirCom(ler<ProjetoEdicao>("completo.json"), true);
    const d = st().doc;
    st().alterar("Nome", (x) => {
      x.titulo = "y";
    });
    expect(st().doc).toBe(d);
    await vi.advanceTimersByTimeAsync(TETO_MS);
    expect(cmd.edicaoSalvar).not.toHaveBeenCalled();
  });
});

describe("histórico", () => {
  it("200 operações sorteadas com o documento congelado: desfazer tudo = inicial, refazer tudo = final", async () => {
    await abrirCom(ler<ProjetoEdicao>("completo.json"));
    const r = sorteio(20261114);
    const midia = st().doc!.midias[0]!.id;
    const rastreio = st().doc!.rastreios[0]!.id;
    const ops: ((d: ProjetoEdicao, i: number) => boolean | void)[] = [
      (d, i) => void (d.titulo = `T${i}`),
      (d, i) => void d.principal.push(novoTrecho(midia, i, i + 1)),
      (d) => {
        if (d.principal.length < 2) return false;
        d.principal.splice(r(d.principal.length), 1);
      },
      (d) => {
        const n = d.principal.length;
        if (n < 2) return false;
        const de = r(n);
        moverClipe(d, de, (de + 1 + r(n - 1)) % n);
      },
      (d, i) => {
        const t = d.principal.find((c): c is ClipeTrecho => c.tipo === "trecho");
        if (!t) return false;
        t.velocidade = { tipo: "constante", fator: 0.05 + i / 1000 };
      },
      (d, i) => {
        const c = d.principal.find(clipeConhecido);
        if (!c) return false;
        const e: EfAjustes = { tipo: "ajustes", id: `aj${i}`, ligado: true, nome: "", ativo: null, brilho: i / 1000, contraste: 1, gama: 1, saturacao: 1 };
        c.efeitos.push(e);
      },
      (d, i) => void aplicarMudanca(d, { rastreio, ancora: { t: 1000 + i, pts: null, v: { x: 0.1, y: 0.2, w: 0.3, h: 0.4 }, interp: "linear" } }),
      (d) => {
        const a = d.rastreios.find((x) => x.id === rastreio)!;
        if (a.ancoras.length < 2) return false;
        a.ancoras = tirarChave(a.ancoras, r(a.ancoras.length)) as Chave<Caixa>[];
      },
      (d, i) => void (d.saida.cor_fundo = `#${i.toString(16).padStart(6, "0")}`),
      (d) => void (d.saida.audio = !d.saida.audio),
    ];
    const estados = [st().doc!];
    const selecoes: Selecao[] = [];
    for (let i = 0; i < LIMITE_PASSOS; i++) {
      const sel: Selecao = { itens: [], foco: { k: "clipe", id: `s${i}` }, efeito: null };
      selecoes.push(sel);
      st().selecionar(sel);
      const op = ops[r(ops.length)]!;
      st().alterar(`op${i}`, (d) => {
        if (op(d, i) === false) d.titulo = `F${i}`;
      });
      estados.push(st().doc!);
      st().selecionar({ itens: [], foco: { k: "clipe", id: `depois${i}` }, efeito: null });
      // passou o tempo: nenhuma operação se junta à anterior
      vi.advanceTimersByTime(10);
    }
    expect(new Set(estados).size).toBe(LIMITE_PASSOS + 1);
    expect(st().passado).toHaveLength(LIMITE_PASSOS);
    expect(congeladoFundo(st().doc)).toBe(true);
    const final = st().doc;
    for (let i = LIMITE_PASSOS - 1; i >= 0; i--) {
      st().desfazer();
      expect(st().doc).toBe(estados[i]);
      expect(st().selecao).toBe(selecoes[i]);
    }
    expect(st().doc).toEqual(ler<ProjetoEdicao>("completo.json"));
    st().desfazer();
    expect(st().doc).toBe(estados[0]);
    for (let i = 1; i <= LIMITE_PASSOS; i++) {
      st().refazer();
      expect(st().doc).toBe(estados[i]);
    }
    expect(st().doc).toBe(final);
    expect(st().futuro).toHaveLength(0);
  });

  it("um arrasto = um passo; cancelar volta ao começo", async () => {
    await abrirCom(ler<ProjetoEdicao>("completo.json"));
    const antes = st().doc;
    const n = st().passado.length;
    const g = st().iniciarGesto("Mover");
    for (let i = 0; i < 30; i++) {
      g.alterar((d) => {
        d.saida.cor_fundo = `#0000${i.toString(16).padStart(2, "0")}`;
      });
    }
    g.concluir();
    expect(st().passado).toHaveLength(n + 1);
    expect(st().doc!.saida.cor_fundo).toBe("#00001d");
    st().desfazer();
    expect(st().doc).toBe(antes);
    const g2 = st().iniciarGesto("Mover");
    g2.alterar((d) => {
      d.saida.largura = 640;
    });
    g2.cancelar();
    expect(st().doc).toBe(antes);
    expect(st().passado).toHaveLength(n);
  });

  it("campo numérico agrupa por tempo; outra chave não", async () => {
    await abrirCom(ler<ProjetoEdicao>("completo.json"));
    for (let i = 0; i < 5; i++) {
      st().alterar("Largura", (d) => void (d.saida.largura = 1000 + i * 2), { agrupar: "largura" });
      vi.advanceTimersByTime(300);
    }
    expect(st().passado).toHaveLength(1);
    vi.advanceTimersByTime(1500);
    st().alterar("Largura", (d) => void (d.saida.largura = 2000), { agrupar: "largura" });
    expect(st().passado).toHaveLength(2);
  });
});

describe("salvamento", () => {
  it("mudança a cada 1 s por 40 s: pelo menos 2 gravações", async () => {
    await abrirCom(ler<ProjetoEdicao>("completo.json"));
    const t0 = Date.now();
    const quando: number[] = [];
    cmd.edicaoSalvar.mockImplementation(async () => {
      quando.push(Date.now() - t0);
      return { updated_at: "" };
    });
    for (let i = 0; i < 40; i++) {
      st().alterar("Nome", (d) => void (d.titulo = `T${i}`));
      await vi.advanceTimersByTimeAsync(1000);
    }
    expect(quando.length).toBeGreaterThanOrEqual(2);
    expect(quando[0]).toBeLessThanOrEqual(TETO_MS);
    expect(st().salvando).toBe("alterado");
    await vi.advanceTimersByTimeAsync(ATRASO_MS);
    expect(salvo().titulo).toBe("T39");
    expect(st().salvando).toBe("salvo");
    expect(st().revisaoSalva).toBe(st().revisao);
  });

  it("mudança durante a gravação: regrava", async () => {
    await abrirCom(ler<ProjetoEdicao>("completo.json"));
    cmd.edicaoSalvar.mockImplementation(() => new Promise((ok) => setTimeout(() => ok({ updated_at: "" }), 2000)));
    st().alterar("Nome", (d) => void (d.titulo = "A"));
    await vi.advanceTimersByTimeAsync(ATRASO_MS);
    expect(st().salvando).toBe("salvando");
    st().alterar("Nome", (d) => void (d.titulo = "B"));
    await vi.advanceTimersByTimeAsync(2000);
    expect(cmd.edicaoSalvar).toHaveBeenCalledTimes(1);
    expect(st().salvando).toBe("alterado");
    await vi.advanceTimersByTimeAsync(ATRASO_MS + 2000);
    expect(cmd.edicaoSalvar).toHaveBeenCalledTimes(2);
    expect(salvo().titulo).toBe("B");
    expect(st().salvando).toBe("salvo");
  });

  it("fechar com pendência: grava antes de fechar", async () => {
    await abrirCom(ler<ProjetoEdicao>("completo.json"));
    st().alterar("Nome", (d) => void (d.titulo = "último"));
    await st().fechar();
    expect(cmd.edicaoSalvar).toHaveBeenCalledTimes(1);
    expect(salvo().titulo).toBe("último");
    expect(st().aberta).toBeNull();
    expect(st().doc).toBeNull();
  });

  it("erro de gravação: estado \"erro\" com o código separado, e tenta de novo", async () => {
    await abrirCom(ler<ProjetoEdicao>("completo.json"));
    cmd.edicaoSalvar.mockRejectedValueOnce({ kind: "validation", message: "validation error: esquema_menor: Versão mais nova" });
    st().alterar("Nome", (d) => void (d.titulo = "x"));
    await vi.advanceTimersByTimeAsync(ATRASO_MS);
    expect(st().salvando).toBe("erro");
    expect(st().erro).toEqual({ codigo: "esquema_menor", frase: "Versão mais nova" });
    await vi.advanceTimersByTimeAsync(NOVA_TENTATIVA_MS);
    expect(cmd.edicaoSalvar).toHaveBeenCalledTimes(2);
    expect(st().salvando).toBe("salvo");
    expect(st().erro).toBeNull();
  });

  it("edição nova só é gravada depois da primeira mudança", async () => {
    const id = await st().novo(WS);
    expect(st().doc!.saida.auto).toBe(true);
    expect(st().doc!.mixagem.normalizar).toBeNull();
    expect(correnteDe(WS)).toBe(id);
    await vi.advanceTimersByTimeAsync(TETO_MS * 2);
    expect(cmd.edicaoSalvar).not.toHaveBeenCalled();
    st().alterar("Nome", (d) => void (d.titulo = "Primeira"));
    await vi.advanceTimersByTimeAsync(ATRASO_MS);
    expect(cmd.edicaoSalvar).toHaveBeenCalledTimes(1);
    expect(salvo().id).toBe(id);
  });

  it("excluir cancela a gravação pendente (e espera a que está em curso)", async () => {
    await abrirCom(ler<ProjetoEdicao>("completo.json"));
    const id = st().doc!.id;
    st().alterar("Nome", (d) => void (d.titulo = "pendente"));
    await st().excluir(WS, id);
    await vi.advanceTimersByTimeAsync(TETO_MS * 2);
    expect(cmd.edicaoSalvar).not.toHaveBeenCalled();
    expect(cmd.edicaoExcluir).toHaveBeenCalledWith(WS, id);
    expect(st().aberta).toBeNull();

    await abrirCom(ler<ProjetoEdicao>("completo.json"));
    const ordem: string[] = [];
    cmd.edicaoSalvar.mockImplementation(() => new Promise((ok) => setTimeout(() => (ordem.push("salvou"), ok({ updated_at: "" })), 1000)));
    cmd.edicaoExcluir.mockImplementation(async () => void ordem.push("excluiu"));
    st().alterar("Nome", (d) => void (d.titulo = "em curso"));
    await vi.advanceTimersByTimeAsync(ATRASO_MS);
    st().alterar("Nome", (d) => void (d.titulo = "depois"));
    const p = st().excluir(WS, id);
    await vi.advanceTimersByTimeAsync(1000);
    await p;
    await vi.advanceTimersByTimeAsync(TETO_MS * 2);
    expect(ordem).toEqual(["salvou", "excluiu"]);
  });

  it("abrir a corrente: a guardada, ou uma nova se ela sumiu", async () => {
    const p = ler<ProjetoEdicao>("completo.json");
    definirCorrente(WS, p.id);
    cmd.edicaoAbrir.mockResolvedValueOnce({ projeto: p, importado_de: null, so_leitura: false, avisos: [], raiz_cache: "" });
    expect(await st().abrirCorrente(WS)).toBe(p.id);
    await st().fechar();
    cmd.edicaoAbrir.mockRejectedValueOnce({ kind: "validation", message: "validation error: projeto_nao_encontrado: x" });
    const novo = await st().abrirCorrente(WS);
    expect(novo).not.toBe(p.id);
    expect(st().aberta?.projetoId).toBe(novo);
    expect(correnteDe(WS)).toBe(novo);
  });

  it("excluir edição nova nunca gravada não chama o backend", async () => {
    const id = await st().novo(WS);
    await st().excluir(WS, id);
    expect(cmd.edicaoExcluir).not.toHaveBeenCalled();
    expect(correnteDe(WS)).toBeNull();
  });

  it("excluir recusado: volta a salvar", async () => {
    await abrirCom(ler<ProjetoEdicao>("completo.json"));
    cmd.edicaoExcluir.mockRejectedValueOnce({ kind: "validation", message: "validation error: em_uso: Exportação na fila" });
    st().alterar("Nome", (d) => void (d.titulo = "fica"));
    await expect(st().excluir(WS, st().doc!.id)).rejects.toBeTruthy();
    expect(st().erro?.codigo).toBe("em_uso");
    expect(st().salvando).toBe("alterado");
    await vi.advanceTimersByTimeAsync(ATRASO_MS);
    expect(salvo().titulo).toBe("fica");
  });
});

describe("exportar", () => {
  it("grava o pendente antes de enfileirar, e enfileira o documento gravado", async () => {
    await abrirCom(ler<ProjetoEdicao>("completo.json"));
    const ordem: string[] = [];
    cmd.edicaoSalvar.mockImplementation(async () => (ordem.push("salvou"), { updated_at: "" }));
    cmd.edicaoEnfileirar.mockImplementation(async () => (ordem.push("enfileirou"), "tarefa-1"));
    st().alterar("Nome", (d) => void (d.titulo = "para exportar"));
    const id = await useRenderStore.getState().exportar({ tipo: "final", exportacao: st().doc!.saida.exportacao }, ["fonte_danificada"]);
    expect(id).toBe("tarefa-1");
    expect(ordem).toEqual(["salvou", "enfileirou"]);
    const [ws, projeto, , ciencias] = cmd.edicaoEnfileirar.mock.calls[0]!;
    expect([ws, (projeto as ProjetoEdicao).titulo, ciencias]).toEqual([WS, "para exportar", ["fonte_danificada"]]);
  });
});

describe("análise", () => {
  it("pedida 250 ms depois da última mudança", async () => {
    await abrirCom(ler<ProjetoEdicao>("completo.json"));
    expect(cmd.edicaoAnalisar).toHaveBeenCalledTimes(1);
    st().alterar("Nome", (d) => void (d.titulo = "a"));
    await vi.advanceTimersByTimeAsync(200);
    st().alterar("Nome", (d) => void (d.titulo = "b"));
    await vi.advanceTimersByTimeAsync(200);
    expect(cmd.edicaoAnalisar).toHaveBeenCalledTimes(1);
    await vi.advanceTimersByTimeAsync(ANALISE_MS - 200);
    expect(cmd.edicaoAnalisar).toHaveBeenCalledTimes(2);
    expect((cmd.edicaoAnalisar.mock.calls[1]?.[1] as ProjetoEdicao).titulo).toBe("b");
  });

  it("a resposta do último pedido vence, mesmo chegando antes", async () => {
    await abrirCom(ler<ProjetoEdicao>("completo.json"));
    const pendentes: ((a: Analise) => void)[] = [];
    cmd.edicaoAnalisar.mockImplementation(() => new Promise<Analise>((ok) => pendentes.push(ok)));
    st().alterar("Nome", (d) => void (d.titulo = "a"));
    await vi.advanceTimersByTimeAsync(ANALISE_MS);
    st().alterar("Nome", (d) => void (d.titulo = "b"));
    await vi.advanceTimersByTimeAsync(ANALISE_MS);
    const velha = { ...analiseVazia(), problemas: [{ nivel: "erro", codigo: "velha", mensagem: "", clipe: null, efeito: null, trilha: null, midia: null, campo: null, n: null }] } as Analise;
    const nova = analiseVazia();
    pendentes[1]!(nova);
    await vi.advanceTimersByTimeAsync(0);
    pendentes[0]!(velha);
    await vi.advanceTimersByTimeAsync(0);
    expect(st().analise).toBe(nova);
    expect(st().analiseRevisao).toBe(st().revisao);
  });

  it("prender e saída automática entram no documento sem passo de histórico", async () => {
    const p = ler<ProjetoEdicao>("completo.json");
    p.saida.auto = true;
    await abrirCom(p);
    const c1 = st().doc!.principal[0] as ClipeTrecho;
    cmd.edicaoAnalisar.mockResolvedValueOnce({
      ...analiseVazia(),
      prender: [{ onde: { tipo: "inicio", clipe: c1.id }, pts: 777, t: 1.25 }],
      saida_auto: [1280, 720, { num: 25, den: 1 }],
    });
    const n = st().passado.length;
    const rev = st().revisao;
    st().alterar("Nome", (d) => void (d.titulo = "a"));
    await vi.advanceTimersByTimeAsync(ANALISE_MS);
    const c = st().doc!.principal[0] as ClipeTrecho;
    expect([c.inicio_s, c.inicio_pts]).toEqual([1.25, 777]);
    expect([st().doc!.saida.largura, st().doc!.saida.altura, st().doc!.saida.fps]).toEqual([1280, 720, { num: 25, den: 1 }]);
    expect(st().passado).toHaveLength(n + 1);
    expect(st().revisao).toBe(rev + 2);
    // a análise seguinte já não muda nada: para aí
    await vi.advanceTimersByTimeAsync(ANALISE_MS * 4);
    expect(st().revisao).toBe(rev + 2);
  });
});

describe("erros de comando", () => {
  it("separa o código da frase", () => {
    expect(separarErro({ kind: "validation", message: "validation error: tabela_pendente: Falta a tabela: cam 1" })).toEqual({
      codigo: "tabela_pendente",
      frase: "Falta a tabela: cam 1",
    });
    expect(separarErro({ kind: "validation", message: "validation error: cancelado" })).toEqual({ codigo: "cancelado", frase: "" });
    expect(separarErro({ kind: "validation", message: "validation error: Texto livre." })).toEqual({ codigo: null, frase: "Texto livre." });
    expect(separarErro({ kind: "io", message: "i/o error: disco cheio" })).toEqual({ codigo: null, frase: "disco cheio" });
  });
});

describe("coleta de trechos", () => {
  const WS2 = "/casos/c2";
  const sha = "a".repeat(64);
  beforeEach(() => {
    definirCorrente(WS2, null);
    cmd.listVideoMedia.mockResolvedValue([
      {
        id: "vm1",
        occurrence_id: "c2",
        original_path: null,
        relative_path: "Mídias/Vídeos/cam1.mp4",
        filename: "cam1.mp4",
        sha256: sha,
        size_bytes: 1,
        duration_s: 600,
        codec: "h264",
        width: 1920,
        height: 1080,
        pixel_format: "yuv420p",
        fps_declared: 30,
        avg_frame_rate: "30/1",
        r_frame_rate: "30/1",
        time_base: "1/15360",
        frame_count: 18000,
        bitrate: null,
        raw_probe_json: JSON.stringify({ format: { start_time: "-0.021" }, streams: [{ codec_type: "video" }, { codec_type: "audio" }] }),
        warnings_json: "[]",
        created_at: "",
        updated_at: "",
      },
    ]);
    cmd.edicaoMidiaInfo.mockResolvedValue({
      id: "vm1",
      sha256: sha,
      nome: "cam1.mp4",
      exibido: { w: 1920, h: 1080 },
      taxa: { num: 30, den: 1 },
      vfr: false,
      passo: { min: 1 / 30, max: 1 / 30 },
      de_s: 0,
      ate_s: 600,
      n: 18000,
      tem_audio: true,
      previa: { relativo: "", tipo: "original", desloc_s: 0, de_s: 0, ate_s: 600, largura: 1920, altura: 1080, pronta: true },
      tabela: "decodificador",
      erros: 0,
    });
    cmd.edicaoQuadroEm.mockImplementation(async (_ws: string, _s: string, alvo: { t: number }) => ({ indice: 0, pts: Math.round(alvo.t * 15360), pts_s: alvo.t, dur_s: 1 / 30 }));
  });

  it("com a tela fechada: cria a edição corrente, grava e depois acrescenta nela", async () => {
    const r1 = await receber(WS2, [{ sha256: sha, inicio_s: 12, fim_s: 15.5 }]);
    expect(r1.n).toBe(1);
    expect(correnteDe(WS2)).toBe(r1.projeto);
    const p1 = salvo();
    expect(p1.id).toBe(r1.projeto);
    expect(p1.midias).toHaveLength(1);
    expect(p1.midias[0]).toMatchObject({ media_id: "vm1", relativo: "Mídias/Vídeos/cam1.mp4", inicio_s: -0.021, taxa: { num: 30, den: 1 }, tem_audio: true });
    expect(p1.principal[0]).toMatchObject({ tipo: "trecho", midia: p1.midias[0]!.id, inicio_s: 12, fim_s: 15.5, inicio_pts: 184320, fim_pts: 238080 });

    cmd.edicaoAbrir.mockResolvedValueOnce({ projeto: p1, importado_de: null, so_leitura: false, avisos: [], raiz_cache: "" });
    const r2 = await receber(WS2, [{ sha256: sha, inicio_s: 600 - 2, fim_s: 600, rotulo: "fim" }]);
    expect(r2).toEqual({ projeto: r1.projeto, n: 2 });
    const p2 = salvo();
    expect(p2.midias).toHaveLength(1);
    // até o fim da mídia: fim_pts nulo
    expect(p2.principal[1]).toMatchObject({ rotulo: "fim", fim_pts: null });
  });

  it("corrente sumida: cria outra", async () => {
    definirCorrente(WS2, "sumiu");
    cmd.edicaoAbrir.mockRejectedValueOnce({ kind: "validation", message: "validation error: projeto_nao_encontrado: x" });
    const r = await receber(WS2, [{ sha256: sha, inicio_s: 1, fim_s: 2 }]);
    expect(r.projeto).not.toBe("sumiu");
    expect(correnteDe(WS2)).toBe(r.projeto);
  });

  it("com a tela aberta na corrente: entra pela store (um passo, salvamento automático)", async () => {
    const p = ler<ProjetoEdicao>("completo.json");
    cmd.edicaoAbrir.mockResolvedValueOnce({ projeto: p, importado_de: null, so_leitura: false, avisos: [], raiz_cache: "" });
    await st().abrir(WS2, p.id);
    const n = st().doc!.principal.length;
    const passos = st().passado.length;
    const r = await receber(WS2, [{ sha256: sha, inicio_s: 1, fim_s: 2 }]);
    expect(r).toEqual({ projeto: p.id, n: n + 1 });
    expect(st().doc!.principal).toHaveLength(n + 1);
    expect(st().passado).toHaveLength(passos + 1);
    expect(cmd.edicaoSalvar).not.toHaveBeenCalled();
    await vi.advanceTimersByTimeAsync(ATRASO_MS);
    expect(cmd.edicaoSalvar).toHaveBeenCalledTimes(1);
  });
});
