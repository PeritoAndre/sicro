// @vitest-environment happy-dom
/**
 * Linha do tempo montada com a store e a Sequência (contratos-fase4.md D1, D3, (f)): lista e linha sempre iguais
 * depois de 200 operações sorteadas (documento congelado pela store), um arrasto = um passo, Esc desiste, lâmina,
 * seleção múltipla, ícones "Ver"/"Travar", J e L, e nenhum texto fora dos rótulos.
 */

import { act } from "react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { ClipePrincipal, MidiaInfo, ProjetoAberto, ProjetoEdicao, TabelaQuadros } from "@domain/edicao";

const cmd = vi.hoisted(() => ({
  edicaoAbrir: vi.fn(),
  edicaoSalvar: vi.fn(),
  edicaoAnalisar: vi.fn(),
  edicaoTabelaQuadros: vi.fn(),
  edicaoMidiaInfo: vi.fn(),
  edicaoFundo: vi.fn(),
  edicaoTira: vi.fn(),
  edicaoOnda: vi.fn(),
}));
// comando não listado: pendente para sempre (o que a tela pede de outros pacotes não interessa aqui)
vi.mock("@core/commands", () => ({ commands: new Proxy(cmd, { get: (t, k) => (t as Record<string | symbol, unknown>)[k] ?? (() => new Promise(() => {})) }) }));
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn(async () => () => {}) }));
vi.mock("@tauri-apps/api/core", () => ({ convertFileSrc: (p: string) => `asset://${p}`, invoke: vi.fn() }));

import { useEdicaoStore } from "../store/edicaoStore";
import { recursoDe } from "../palco/recursos";
import { resolver } from "../modelo/tempo";
import { acharMidia, clipeConhecido } from "../modelo/projeto";
import { duplicar, remover } from "../sequencia/acoes";
import { dividirTrecho, textoDuracao } from "../sequencia/contas";
import { Sequencia } from "../sequencia/Sequencia";
import { desmontarTudo, lerFixture, montar } from "../inspetor/testeTela";
import { textoPermitido, ROTULOS } from "../inspetor/rotulos.teste";
import { LinhaDoTempo } from "./LinhaDoTempo";
import { useLinhaUi } from "./estado";
import { aparar, apararLivre, destinoNaPrincipal, dividirSemFonte, moverLivre, moverNaPrincipal } from "./arrastar";
import { tabelaSintetica, textoLivre } from "./testeLinha";
import type { PalcoApi } from "../palco/Palco";

const WS = "/casos/c1";
const st = () => useEdicaoStore.getState();
const TAB: TabelaQuadros = tabelaSintetica(1800);

function info(sha: string): MidiaInfo {
  return {
    id: sha.slice(0, 4),
    sha256: sha,
    nome: "x.mp4",
    exibido: { w: 1280, h: 720 },
    taxa: { num: 30, den: 1 },
    vfr: false,
    passo: { min: 1 / 30, max: 1 / 30 },
    de_s: 0,
    ate_s: 60,
    n: 1800,
    tem_audio: true,
    previa: { relativo: "x.mp4", tipo: "original", desloc_s: 0, de_s: 0, ate_s: 60, largura: 1280, altura: 720, pronta: false },
    tabela: "decodificador",
    erros: null,
    marcavel: true,
    cor: { matriz: "bt709", faixa: "tv" },
  };
}

const palcoFalso = { tocar: vi.fn(), pausar: vi.fn(), tempo: () => Number.NaN } as unknown as PalcoApi;

beforeEach(async () => {
  for (const f of Object.values(cmd)) f.mockReset();
  cmd.edicaoAnalisar.mockImplementation(() => new Promise(() => {}));
  cmd.edicaoSalvar.mockImplementation(async () => ({ updated_at: new Date().toISOString(), raiz_cache: "/cache/c1" }));
  cmd.edicaoTabelaQuadros.mockImplementation(async () => TAB);
  cmd.edicaoMidiaInfo.mockImplementation(async (_ws: string, sha: string) => info(sha));
  cmd.edicaoFundo.mockResolvedValue([]);
  cmd.edicaoTira.mockImplementation(() => new Promise(() => {}));
  cmd.edicaoOnda.mockResolvedValue(null);
  const projeto = lerFixture<ProjetoEdicao>("projeto-do-store.json");
  cmd.edicaoAbrir.mockResolvedValue({ projeto, importado_de: null, so_leitura: false, avisos: [], raiz_cache: "/cache/c1" } satisfies ProjetoAberto);
  await st().abrir(WS, projeto.id);
  // o estado de tela já "carregado" para o projeto: nada do localStorage de um teste passa ao outro
  useLinhaUi.setState({ projeto: projeto.id, pxPorS: 14, ferramenta: "selecionar", ima: true, travadas: {}, alturas: {}, rolagem_s: 0, altura: 300 });
});

afterEach(async () => {
  desmontarTudo();
  await st()
    .fechar({ descartar: true })
    .catch(() => {});
});

const telaComLinha = () =>
  montar(
    <>
      <Sequencia />
      <LinhaDoTempo palco={() => palcoFalso} atalhos />
    </>,
  );

const ctxDaTela = () => ({ tabela: (id: string) => recursoDe(acharMidia(st().doc!, id)?.sha256).tabela });
const linhaAgora = () => resolver(st().doc!, ctxDaTela());

/** Lista × linha: os mesmos clipes, na mesma ordem, com a mesma duração (a da linha resolvida da store). */
function conferir(raiz: HTMLElement) {
  const doc = st().doc!;
  const linha = linhaAgora();
  const v1 = [...raiz.querySelectorAll<HTMLElement>('[data-trilha="principal"] [data-item]')];
  expect(v1.map((e) => e.dataset.item)).toEqual(doc.principal.map((c, i) => (c as { id?: string }).id ?? `?${i}`));
  const cartoes = [...raiz.querySelectorAll<HTMLElement>("[data-cartao]")];
  expect(cartoes).toHaveLength(doc.principal.length);
  doc.principal.forEach((c, i) => {
    const r = clipeConhecido(c) ? linha.clipes.find((x) => x.id === c.id) : undefined;
    const it = v1[i]!;
    expect([Number(it.dataset.deQ), Number(it.dataset.ateQ)]).toEqual(r ? [r.inicio_q, r.inicio_q + r.n_q] : [Number(it.dataset.deQ), Number(it.dataset.deQ)]);
    const dur = cartoes[i]!.querySelector("[class*=dur]")?.textContent ?? null;
    expect(dur).toBe(r ? textoDuracao((r.n_q * linha.fps.den) / linha.fps.num) : null);
  });
}

/** Sorteio determinístico. */
function sorteio(semente: number) {
  let s = semente >>> 0;
  return (n: number) => {
    s = (Math.imul(s, 1664525) + 1013904223) >>> 0;
    return s % n;
  };
}

const pointer = (tipo: string, x: number, op: { shift?: boolean; alt?: boolean } = {}) =>
  new PointerEvent(tipo, { bubbles: true, cancelable: true, clientX: x, clientY: 10, button: 0, pointerId: 1, shiftKey: !!op.shift, altKey: !!op.alt });

async function arrastarEl(el: Element, rolo: Element, de: number, ate: number, op: { esc?: boolean } = {}) {
  await act(async () => void el.dispatchEvent(pointer("pointerdown", de)));
  await act(async () => void rolo.dispatchEvent(pointer("pointermove", (de + ate) / 2)));
  await act(async () => void rolo.dispatchEvent(pointer("pointermove", ate)));
  if (op.esc) await act(async () => void window.dispatchEvent(new KeyboardEvent("keydown", { key: "Escape", bubbles: true })));
  await act(async () => void rolo.dispatchEvent(pointer("pointerup", ate)));
}

describe("lista e linha sempre iguais", () => {
  it("200 operações sorteadas (mover, aparar, dividir, duplicar, remover, desligar, livres)", async () => {
    const t = await telaComLinha();
    await act(async () => {});
    conferir(t.raiz);
    const r = sorteio(17);
    const qual = () => {
      const ids = st().doc!.principal.filter(clipeConhecido).map((c) => c.id);
      return ids[r(ids.length)] ?? null;
    };
    let mudancas = 0;
    for (let k = 0; k < 200; k++) {
      const doc0 = st().doc;
      const linha = linhaAgora();
      const ctx = ctxDaTela();
      const id = qual();
      const op = r(8);
      await act(async () => {
        if (!id) return;
        const q = r(Math.max(linha.total_q + 60, 1));
        if (op === 0) {
          const para = destinoNaPrincipal(st().doc!, linha, id, q);
          st().alterar("Mover", (d) => moverNaPrincipal(d, id, para));
        } else if (op === 1 || op === 2) st().alterar("Aparar", (d) => aparar(d, linha, id, op === 1 ? "inicio" : "fim", q, ctx));
        else if (op === 3)
          st().alterar("Dividir", (d) => {
            const c = d.principal.find((x) => (x as { id?: string }).id === id) as ClipePrincipal;
            if (c.tipo === "trecho") dividirTrecho(d, id, r(TAB.passos.length), TAB);
            else dividirSemFonte(d, linha, id, q);
          });
        else if (op === 4 && st().doc!.principal.length < 14) duplicar(id);
        else if (op === 5 && st().doc!.principal.length > 2) remover(id);
        else if (op === 6)
          st().alterar("Desligar", (d) => {
            const c = d.principal.find((x) => (x as { id?: string }).id === id) as ClipePrincipal;
            c.desligado = !c.desligado;
          });
        else {
          const livres = linha.livres;
          const l = livres[r(Math.max(livres.length, 1))];
          if (l) st().alterar("Mover", (d) => (r(2) ? moverLivre(d, linha, l.id, q, { manterAncora: r(2) === 1 }) : apararLivre(d, linha, l.id, r(2) ? "inicio" : "fim", q)));
        }
      });
      await act(async () => {});
      if (st().doc !== doc0) mudancas++;
      conferir(t.raiz);
    }
    expect(mudancas).toBeGreaterThan(60);
    // a store congela o documento: nada da linha escreveu nele fora do `alterar`
    expect(Object.isFrozen(st().doc!.principal)).toBe(true);
  }, 120_000);
});

describe("gestos na tela", () => {
  const v1Item = (raiz: HTMLElement, i: number) => raiz.querySelectorAll<HTMLElement>('[data-trilha="principal"] [data-item]')[i]!;
  const rolo = (raiz: HTMLElement) => raiz.querySelector<HTMLElement>("[data-linha-rolo]")!;

  it("aparar pela borda = um passo; Esc no meio desiste sem passo", async () => {
    const t = await telaComLinha();
    await act(async () => {});
    const id = (st().doc!.principal[1] as { id: string }).id;
    const antes = linhaAgora().clipes.find((c) => c.id === id)!.n_q;
    const passos = st().passado.length;
    const borda = v1Item(t.raiz, 1).querySelector("[data-borda=fim]")!;
    // 14 px/s a 30 qps: 28 px = 60 slots
    await arrastarEl(borda, rolo(t.raiz), 500, 472, { esc: true });
    expect(st().passado.length).toBe(passos);
    expect(linhaAgora().clipes.find((c) => c.id === id)!.n_q).toBe(antes);
    await arrastarEl(borda, rolo(t.raiz), 500, 472);
    expect(st().passado.length).toBe(passos + 1);
    expect(linhaAgora().clipes.find((c) => c.id === id)!.n_q).toBe(antes - 60);
    conferir(t.raiz);
    st().desfazer();
    expect(linhaAgora().clipes.find((c) => c.id === id)!.n_q).toBe(antes);
  });

  it("o clipe que andou (aparado o de antes) apara pela posição nova", async () => {
    const t = await telaComLinha();
    await act(async () => useLinhaUi.setState({ ima: false }));
    const ids = st().doc!.principal.map((c) => (c as { id: string }).id);
    const k = ids.findIndex((_, i) => i > 1 && !!v1Item(t.raiz, i).querySelector("[data-borda=fim]"));
    const n = (id: string) => linhaAgora().clipes.find((c) => c.id === id)!.n_q;
    const [n1, nk] = [n(ids[1]!), n(ids[k]!)];
    await arrastarEl(v1Item(t.raiz, 1).querySelector("[data-borda=fim]")!, rolo(t.raiz), 500, 472);
    expect(n(ids[1]!)).toBe(n1 - 60);
    // o seguinte andou 60 slots para trás: a borda dele vale onde está agora
    await arrastarEl(v1Item(t.raiz, k).querySelector("[data-borda=fim]")!, rolo(t.raiz), 700, 686);
    expect(n(ids[k]!)).toBe(nk - 30);
    conferir(t.raiz);
  });

  it("arrastar o clipe na V1 reordena por inserção (um passo)", async () => {
    const t = await telaComLinha();
    await act(async () => {});
    const ids0 = st().doc!.principal.map((c) => (c as { id: string }).id);
    const passos = st().passado.length;
    // o primeiro (título, 3 s = 42 px) para depois do último
    await arrastarEl(v1Item(t.raiz, 0), rolo(t.raiz), 160, 160 + 14 * 20);
    expect(st().doc!.principal.map((c) => (c as { id: string }).id)).toEqual([ids0[1], ids0[2], ids0[0]]);
    expect(st().passado.length).toBe(passos + 1);
    conferir(t.raiz);
  });

  it("lâmina no clipe corta no slot sob o ponteiro (trecho pelo Dividir no cursor; título pela duração)", async () => {
    const t = await telaComLinha();
    await act(async () => {});
    await act(async () => useLinhaUi.setState({ ferramenta: "lamina" }));
    const n0 = st().doc!.principal.length;
    // trecho 2 (m1, 12 a 15,5 s) começa no slot 90 (x = 150 + 42); 1 s depois, na metade direita do slot (o corte é o
    // do slot sob o ponteiro, não o mais perto)
    await act(async () => void v1Item(t.raiz, 1).dispatchEvent(pointer("pointerdown", 150 + 42 + 14.4)));
    expect(st().doc!.principal.length).toBe(n0 + 1);
    const [a, b] = [st().doc!.principal[1] as { fim_s: number }, st().doc!.principal[2] as { inicio_s: number }];
    expect(a.fim_s).toBe(13);
    expect(b.inicio_s).toBe(13);
    await act(async () => void v1Item(t.raiz, 0).dispatchEvent(pointer("pointerdown", 150 + 14.2)));
    expect(st().doc!.principal.length).toBe(n0 + 2);
    expect((st().doc!.principal[0] as { duracao_s: number }).duracao_s).toBe(1);
    conferir(t.raiz);
  });

  it("seleção: clique = foco; Shift acrescenta; o foco do clipe abre na lista também", async () => {
    const t = await telaComLinha();
    await act(async () => {});
    const [a, b] = [v1Item(t.raiz, 0), v1Item(t.raiz, 2)];
    await act(async () => void a.dispatchEvent(pointer("pointerdown", 160)));
    await act(async () => void rolo(t.raiz).dispatchEvent(pointer("pointerup", 160)));
    expect(st().selecao.foco?.id).toBe(a.dataset.item);
    await act(async () => void b.dispatchEvent(pointer("pointerdown", 260, { shift: true })));
    await act(async () => void rolo(t.raiz).dispatchEvent(pointer("pointerup", 260)));
    expect(st().selecao.itens.map((x) => x.id)).toEqual([a.dataset.item, b.dataset.item]);
    expect(st().selecao.foco?.id).toBe(b.dataset.item);
    expect(t.raiz.querySelector('[data-cartao="2"]')?.getAttribute("aria-selected")).toBe("true");
  });

  it("clique num clipe com a cabeça dentro dele: seleciona e a cabeça fica (o Ctrl+B corta onde ela está); fora: vai ao ponto", async () => {
    const t = await telaComLinha();
    await act(async () => {});
    const [titulo, trecho] = st().doc!.principal.map((c) => (c as { id: string }).id);
    // cabeça parada no trecho (slots 90..195), um segundo depois do começo dele
    const parado = { modo: "clipe" as const, clipe: trecho!, t: 13 + 1 / 60 };
    await act(async () => st().posicionar(parado));
    await act(async () => void (await new Promise((r) => setTimeout(r, 50))));
    // clique no mesmo trecho, longe da cabeça (slot 160): seleciona, o cursor não anda
    const x = 150 + (160 * 14) / 30;
    await act(async () => void v1Item(t.raiz, 1).dispatchEvent(pointer("pointerdown", x)));
    await act(async () => void rolo(t.raiz).dispatchEvent(pointer("pointerup", x)));
    expect(st().selecao.foco?.id).toBe(trecho);
    expect(st().cursor).toEqual(parado);
    // aparar mostra a borda no palco; Esc desiste e a cabeça volta ao quadro parado
    const passos = st().passado.length;
    await arrastarEl(v1Item(t.raiz, 1).querySelector("[data-borda=fim]")!, rolo(t.raiz), 500, 472, { esc: true });
    expect(st().passado.length).toBe(passos);
    expect(st().cursor).toEqual(parado);
    // clique noutro clipe: a cabeça vai ao ponto do clique
    await act(async () => void v1Item(t.raiz, 0).dispatchEvent(pointer("pointerdown", 150 + 14)));
    await act(async () => void rolo(t.raiz).dispatchEvent(pointer("pointerup", 150 + 14)));
    expect(st().cursor).toMatchObject({ modo: "clipe", clipe: titulo });
  });

  it("textos selecionados com Shift andam juntos: o mesmo deslocamento, um passo", async () => {
    st().alterar("x", (d) => {
      d.sobreposicoes.push({ id: "s1", nome: "", oculta: false, clipes: [{ ...textoLivre({ ref: "programa", t: 1 }, 1, "a"), id: "ta" }, { ...textoLivre({ ref: "programa", t: 5 }, 1, "b"), id: "tb" }] });
    });
    const t = await telaComLinha();
    await act(async () => {});
    const item = (id: string) => t.raiz.querySelector<HTMLElement>(`[data-k="livre"][data-item="${id}"]`)!;
    await act(async () => void item("ta").dispatchEvent(pointer("pointerdown", 200)));
    await act(async () => void rolo(t.raiz).dispatchEvent(pointer("pointerup", 200)));
    await act(async () => void item("tb").dispatchEvent(pointer("pointerdown", 300, { shift: true })));
    await act(async () => void rolo(t.raiz).dispatchEvent(pointer("pointerup", 300)));
    const passos = st().passado.length;
    // 14 px = 30 slots = 1 s (sem ímã: o fim do trecho 2 está a 15 slots)
    await act(async () => useLinhaUi.setState({ ima: false }));
    await arrastarEl(item("tb"), rolo(t.raiz), 300, 314);
    const pos = (id: string) => (st().doc!.sobreposicoes[0]!.clipes.find((c) => (c as { id: string }).id === id) as { pos: { t: number } }).pos.t;
    expect([pos("ta"), pos("tb")]).toEqual([2, 6]);
    expect(st().passado.length).toBe(passos + 1);
  });

  it("atalhos desligados (diálogo ou menu aberto): B, S, V, J, L, =, - e 0 não fazem nada", async () => {
    await montar(<LinhaDoTempo palco={() => palcoFalso} atalhos={false} />);
    await act(async () => {});
    vi.mocked(palcoFalso.tocar).mockClear();
    const antes = { ...useLinhaUi.getState() };
    for (const [code, key] of [["KeyB", "b"], ["KeyS", "s"], ["KeyV", "v"], ["KeyJ", "j"], ["KeyL", "l"], ["Equal", "="], ["Minus", "-"], ["Digit0", "0"]] as const)
      await act(async () => void window.dispatchEvent(new KeyboardEvent("keydown", { key, code, bubbles: true })));
    await act(async () => void (await new Promise((r) => setTimeout(r, 200))));
    const depois = useLinhaUi.getState();
    expect([depois.ferramenta, depois.ima, depois.pxPorS]).toEqual([antes.ferramenta, antes.ima, antes.pxPorS]);
    expect(palcoFalso.tocar).not.toHaveBeenCalled();
  });

  it("Travar: a trilha não arrasta; Ver da V1 e do áudio só esconde o conteúdo; Ver dos textos é edição (um passo)", async () => {
    st().alterar("t", (d) => {
      d.sobreposicoes.push({ id: "s1", nome: "", oculta: false, clipes: [] });
    });
    const t = await telaComLinha();
    await act(async () => {});
    await act(async () => (t.raiz.querySelector('[data-travar="principal"]') as HTMLElement).click());
    expect(useLinhaUi.getState().travadas.principal).toBe(true);
    const passos = st().passado.length;
    const borda = v1Item(t.raiz, 1).querySelector("[data-borda=fim]");
    expect(borda).toBeNull();
    // o olho da maquete na V1 e nas trilhas de som: tela, sem passo
    const a1 = st().doc!.audios[0]!.id;
    await act(async () => (t.raiz.querySelector('[data-ver="principal"]') as HTMLElement).click());
    await act(async () => (t.raiz.querySelector(`[data-ver="${a1}"]`) as HTMLElement).click());
    expect(useLinhaUi.getState().semConteudo).toMatchObject({ principal: true, [a1]: true });
    expect(t.raiz.querySelector('[data-ver="principal"]')!.getAttribute("aria-pressed")).toBe("false");
    expect(st().doc!.audios[0]!.mudo).toBe(false);
    expect(st().passado.length).toBe(passos);
    await act(async () => (t.raiz.querySelector('[data-ver="s1"]') as HTMLElement).click());
    expect(st().doc!.sobreposicoes[0]!.oculta).toBe(true);
    expect(st().passado.length).toBe(passos + 1);
  });

  it("Ctrl+roda: o zoom vale já na tela (pistas e régua esticadas, sem leiaute nem React); assenta quando a roda para", async () => {
    const t = await telaComLinha();
    await act(async () => {});
    const conteudo = rolo(t.raiz).firstElementChild as HTMLElement;
    const px0 = Number(conteudo.style.getPropertyValue("--px"));
    expect(px0).toBeCloseTo(14 / 30, 9);
    // (as posições `calc(var(--px) * …)` o happy-dom não guarda: conferidas na bancada do navegador)
    const roda = new WheelEvent("wheel", { bubbles: true, cancelable: true, ctrlKey: true, deltaY: -100 });
    // o happy-dom não guarda `ctrlKey` na roda
    Object.defineProperty(roda, "ctrlKey", { value: true });
    await act(async () => void rolo(t.raiz).dispatchEvent(roda));
    const k = Math.pow(1.0018, 100);
    const pista = t.raiz.querySelector<HTMLElement>('[data-pista="principal"]')!;
    const esticada = /^matrix\(([-\d.e]+), 0, 0, 1, ([-\d.e]+), 0\)$/.exec(pista.style.transform);
    expect(Number(esticada![1])).toBeCloseTo(k, 9);
    expect(t.raiz.querySelector<HTMLElement>("[data-regua]")!.style.transform).toBe(pista.style.transform);
    expect(Number(conteudo.style.getPropertyValue("--px"))).toBe(px0);
    expect(useLinhaUi.getState().pxPorS).toBe(14);
    await act(async () => void (await new Promise((r) => setTimeout(r, 250))));
    expect(useLinhaUi.getState().pxPorS).toBeCloseTo(14 * k, 6);
    expect(Number(conteudo.style.getPropertyValue("--px"))).toBeCloseTo(px0 * k, 9);
    expect(pista.style.transform).toBe("");
  });

  it("projeto aberto pela primeira vez: a linha abre ajustada ao programa", async () => {
    await act(async () => useLinhaUi.setState({ projeto: null, ajustarAoAbrir: false }));
    // sem nada guardado para o projeto
    const guardado = (globalThis as { localStorage?: Storage }).localStorage;
    guardado?.removeItem?.("sicro.edicao.linha.v1");
    await telaComLinha();
    await act(async () => void (await new Promise((r) => setTimeout(r, 200))));
    // o programa e tudo o que passa dele (a narração presa ao último clipe)
    const l = linhaAgora();
    const fimS = Math.max(l.total_q, ...l.livres.map((x) => x.inicio_q + x.n_q)) / 30;
    // sem largura medida (happy-dom), 800 px para o programa inteiro
    expect(useLinhaUi.getState().pxPorS).toBeCloseTo(800 / Math.max(fimS, 1), 6);
    expect(useLinhaUi.getState().ajustarAoAbrir).toBe(false);
  });

  it("só a parte visível (mais uma tela de cada lado) é desenhada; a rolagem traz o resto", async () => {
    const id = (st().doc!.principal[1] as { id: string }).id;
    for (let i = 0; i < 40; i++) duplicar(id);
    const total = st().doc!.principal.length;
    useLinhaUi.setState({ pxPorS: 200 });
    const t = await telaComLinha();
    const r = rolo(t.raiz);
    const v1 = () => t.raiz.querySelectorAll('[data-trilha="principal"] [data-item]');
    expect(v1()).toHaveLength(total);
    // 600 px de pista a 200 px/s: 3 s visíveis, 3 s de cada lado
    Object.defineProperty(r, "clientWidth", { value: 750, configurable: true });
    await act(async () => void r.dispatchEvent(new Event("scroll")));
    await act(async () => {});
    const n = v1().length;
    expect(n).toBeGreaterThan(0);
    expect(n).toBeLessThan(6);
    const ultimo = st().doc!.principal.at(-1) as { id: string };
    expect(t.raiz.querySelector(`[data-item="${ultimo.id}"]`)).toBeNull();
    // rolar até o fim traz o último
    Object.defineProperty(r, "scrollLeft", { value: (linhaAgora().total_q / 30) * 200 - 600, configurable: true, writable: true });
    await act(async () => void r.dispatchEvent(new Event("scroll")));
    await act(async () => {});
    expect(t.raiz.querySelector(`[data-trilha="principal"] [data-item="${ultimo.id}"]`)).not.toBeNull();
  });

  it("J e L tocam pelo palco; S liga e desliga o ímã; B e V trocam a ferramenta", async () => {
    await telaComLinha();
    const tecla = (key: string) => act(async () => void window.dispatchEvent(new KeyboardEvent("keydown", { key, code: `Key${key.toUpperCase()}`, bubbles: true })));
    await tecla("j");
    expect(palcoFalso.tocar).toHaveBeenLastCalledWith(-1);
    await tecla("l");
    expect(palcoFalso.tocar).toHaveBeenLastCalledWith(1);
    await tecla("s");
    expect(useLinhaUi.getState().ima).toBe(false);
    await tecla("b");
    expect(useLinhaUi.getState().ferramenta).toBe("lamina");
    await tecla("v");
    expect(useLinhaUi.getState().ferramenta).toBe("selecionar");
  });

  it("textos e dicas só dos rótulos (nomes de clipe são dado)", async () => {
    const t = await telaComLinha();
    await act(async () => {});
    const painel = t.raiz.querySelector<HTMLElement>("[data-linha-painel]")!;
    const w = document.createTreeWalker(painel, NodeFilter.SHOW_TEXT);
    const textos: string[] = [];
    for (let n = w.nextNode(); n; n = w.nextNode()) {
      if ((n.parentElement as HTMLElement).closest("[data-item]")) continue;
      const s = (n.textContent ?? "").trim();
      if (s) textos.push(s);
    }
    expect(textos.filter((s) => !textoPermitido(s))).toEqual([]);
    // cabeças da maquete: "V1 Vídeo", "A1 Som", "A2 Narração"; o zoom "− ▬ +"
    expect(textos).toEqual(expect.arrayContaining(["Selecionar", "Lâmina", "Ímã", "V1", "Vídeo", "A1", "Som", "A2", "Narração", "−", "+", "0:05"]));
    const dicas = [...painel.querySelectorAll<HTMLElement>("[title]")].filter((e) => !e.closest("[data-item]") && !e.closest("[data-marcador]")).map((e) => e.title);
    expect(dicas.filter((d) => !ROTULOS.has(d) && d !== "Ajustar à tela" && !textoPermitido(d))).toEqual([]);
  });
});
