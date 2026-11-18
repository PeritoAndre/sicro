// @vitest-environment happy-dom
/**
 * Destaque novo onde se vê (contratos-fase4.md D12): a área do palco levada à origem desfaz a vista (zoom e
 * deslocamento), o enquadramento e o giro; a caixa nova nasce no centro dela e as quatro pontas ficam na tela. Conferido
 * por amostragem (ponto da origem → canvas por `A` → tela pela vista), não pela própria conta.
 */

import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { Caixa, EfEnquadramento, EfGirar, Efeito, ProjetoAberto } from "@domain/edicao";

const cmd = vi.hoisted(() => ({ edicaoAbrir: vi.fn(), edicaoSalvar: vi.fn(), edicaoAnalisar: vi.fn() }));
vi.mock("@core/commands", () => ({ commands: cmd }));

import { aDe, ponto, type Afim } from "../modelo/geometria";
import { CAIXA_CENTRO } from "../inspetor/novos";
import { contido, paraTela, VISTA_INICIAL, zoomEm, type Tamanho, type Vista } from "./vista";
import { areaVisivelNaOrigem, caixaNova, FRACAO_DA_AREA, useAreaVisivel } from "./areaVisivel";
import { useEdicaoStore } from "../store/edicaoStore";
import { useRecursos } from "./recursos";
import { adicionarEfeito } from "../inspetor/acoes";
import { adicionarLupa } from "../inspetor/secoes/LupaEditor";
import { adicionarHolofote } from "../inspetor/secoes/HolofoteEditor";
import { acharEfeito } from "../modelo/projeto";
import { rastreioDe } from "../inspetor/acoes";
import { info, projeto, q, SHA, tab, WS } from "../rastreio/cenario.teste";

const TELA: Tamanho = { w: 1000, h: 600 };
const SAIDA: Tamanho = { w: 1920, h: 1080 };
const ORIGEM: Tamanho = { w: 1920, h: 1080 };
const IMG = contido(TELA, SAIDA);
const EPS = 1e-9;

const enq = (regiao: Caixa, ajuste: "conter" | "cobrir"): EfEnquadramento =>
  ({ id: "q", ligado: true, nome: "", ativo: null, tipo: "enquadramento", regiao, ajuste }) as unknown as EfEnquadramento;
const giro = (graus: number): EfGirar => ({ id: "g", ligado: true, nome: "", ativo: null, tipo: "girar", graus, espelhar_h: false, espelhar_v: false }) as EfGirar;

/** Ponto da origem na tela (px da caixa do palco). */
const naTela = (v: Vista, A: Afim, p: { x: number; y: number }) => paraTela(v, IMG, ponto(A, p));
/** O ponto da origem aparece: cai dentro da caixa do palco e dentro do canvas da saída (fora dele não se desenha). */
const visivel = (v: Vista, A: Afim, p: { x: number; y: number }) => {
  const c = ponto(A, p);
  const s = paraTela(v, IMG, c);
  return c.x >= -EPS && c.x <= 1 + EPS && c.y >= -EPS && c.y <= 1 + EPS && s.x >= -1e-6 && s.x <= TELA.w + 1e-6 && s.y >= -1e-6 && s.y <= TELA.h + 1e-6;
};
const dentro = (c: Caixa, p: { x: number; y: number }, e = 1e-9) => p.x >= c.x - e && p.x <= c.x + c.w + e && p.y >= c.y - e && p.y <= c.y + c.h + e;
const pontas = (c: Caixa) => [
  { x: c.x, y: c.y },
  { x: c.x + c.w, y: c.y },
  { x: c.x, y: c.y + c.h },
  { x: c.x + c.w, y: c.y + c.h },
];

/** A área devolvida é exatamente a parte visível da origem: grade de 81×81 pontos. */
function conferirArea(v: Vista, A: Afim, area: Caixa) {
  const n = 80;
  let vistos = 0;
  for (let i = 0; i <= n; i++)
    for (let j = 0; j <= n; j++) {
      const p = { x: i / n, y: j / n };
      const ve = visivel(v, A, p);
      // margem de meia célula na borda da área (a grade não cai em cima dela)
      const meio = 0.5 / n;
      const bemDentro = p.x > area.x + meio && p.x < area.x + area.w - meio && p.y > area.y + meio && p.y < area.y + area.h - meio;
      if (ve) {
        vistos++;
        expect(dentro(area, p, 1e-6), `visível fora da área: ${p.x},${p.y}`).toBe(true);
      }
      if (bemDentro) expect(ve, `na área e fora da tela: ${p.x},${p.y}`).toBe(true);
    }
  expect(vistos).toBeGreaterThan(0);
}

afterEach(() => useAreaVisivel.setState({ clipe: null, caixa: null }));

describe("areaVisivelNaOrigem", () => {
  it("vista inicial sem efeitos: a imagem inteira", () => {
    const A = aDe([], 0, ORIGEM, SAIDA);
    expect(areaVisivelNaOrigem(VISTA_INICIAL, TELA, SAIDA, A)).toEqual({ x: 0, y: 0, w: 1, h: 1 });
  });

  it("zoom 3× no canto e enquadramento (Preencher): só o pedaço que se vê", () => {
    const v = zoomEm(VISTA_INICIAL, 3, { x: TELA.w, y: 0 }, TELA, IMG);
    expect(v.s).toBe(3);
    const A = aDe([enq({ x: 0.2, y: 0.1, w: 0.5, h: 0.5 }, "cobrir")], 0, ORIGEM, SAIDA);
    const a = areaVisivelNaOrigem(v, TELA, SAIDA, A);
    // na largura, um terço da região (0,5); na altura a tela é mais alta que a imagem (600 px contra 3 × 562,5)
    expect(a.w).toBeCloseTo(0.5 / 3, 9);
    expect(a.h).toBeCloseTo((0.5 * TELA.h) / (3 * IMG.h), 9);
    expect(a.x + a.w).toBeCloseTo(0.7, 9);
    expect(a.y).toBeCloseTo(0.1, 9);
    conferirArea(v, A, a);
  });

  it("Inteiro com faixas, giro de 90° e zoom 2,5×: a faixa preta não conta", () => {
    const efeitos: Efeito[] = [giro(90), enq({ x: 0, y: 0, w: 1, h: 1 }, "conter")];
    const A = aDe(efeitos, 0, ORIGEM, SAIDA);
    const v = zoomEm(VISTA_INICIAL, 2.5, { x: 120, y: 300 }, TELA, IMG);
    const a = areaVisivelNaOrigem(v, TELA, SAIDA, A);
    conferirArea(v, A, a);
    for (const k of [a.x, a.y, a.x + a.w, a.y + a.h]) expect(k >= 0 && k <= 1).toBe(true);
  });

  it("sem nada da imagem na tela (zoom na faixa preta): área vazia", () => {
    // girada em 90°, a imagem fica numa coluna no meio do canvas; o canto do canvas é só faixa
    const A = aDe([giro(90)], 0, ORIGEM, SAIDA);
    const v = zoomEm(VISTA_INICIAL, 8, { x: 2, y: 25 }, TELA, IMG);
    const a = areaVisivelNaOrigem(v, TELA, SAIDA, A);
    expect(a.w === 0 || a.h === 0).toBe(true);
    useAreaVisivel.getState().publicar("c", a);
    expect(caixaNova("c")).toEqual(CAIXA_CENTRO);
  });

  it("palco sem tamanho (escondido) ou A degenerada: a imagem inteira", () => {
    const A = aDe([], 0, ORIGEM, SAIDA);
    expect(areaVisivelNaOrigem(VISTA_INICIAL, { w: 0, h: 0 }, SAIDA, A)).toEqual({ x: 0, y: 0, w: 1, h: 1 });
    expect(areaVisivelNaOrigem(VISTA_INICIAL, TELA, SAIDA, [0, 0, 0, 0, 0, 0])).toEqual({ x: 0, y: 0, w: 1, h: 1 });
  });
});

describe("caixaNova", () => {
  it("sem área publicada, ou publicada para outro clipe: a do centro de sempre", () => {
    expect(caixaNova("c")).toEqual(CAIXA_CENTRO);
    useAreaVisivel.getState().publicar("outro", { x: 0.6, y: 0.6, w: 0.3, h: 0.3 });
    expect(caixaNova("c")).toEqual(CAIXA_CENTRO);
    expect(caixaNova(null)).toEqual(CAIXA_CENTRO);
  });

  it("zoom 3× e enquadramento: nasce no centro do que se vê, inteira na tela (a do centro de sempre ficava fora)", () => {
    const v = zoomEm(VISTA_INICIAL, 3, { x: TELA.w, y: 0 }, TELA, IMG);
    const A = aDe([enq({ x: 0.2, y: 0.1, w: 0.5, h: 0.5 }, "cobrir")], 0, ORIGEM, SAIDA);
    const area = areaVisivelNaOrigem(v, TELA, SAIDA, A);
    useAreaVisivel.getState().publicar("c", area);
    const c = caixaNova("c");
    for (const p of pontas(c)) expect(visivel(v, A, p), `${p.x},${p.y}`).toBe(true);
    // centro da caixa no centro da tela (a imagem cobre a tela toda nessa vista)
    const meio = naTela(v, A, { x: c.x + c.w / 2, y: c.y + c.h / 2 });
    expect(meio.x).toBeCloseTo(TELA.w / 2, 6);
    expect(meio.y).toBeCloseTo(TELA.h / 2, 6);
    // a caixa de sempre nem aparece
    expect(pontas(CAIXA_CENTRO).some((p) => visivel(v, A, p))).toBe(false);
    // tamanho: o de sempre, reduzido por igual até a fração da área
    expect(c.w / c.h).toBeCloseTo(CAIXA_CENTRO.w / CAIXA_CENTRO.h, 9);
    expect(c.w).toBeLessThanOrEqual(FRACAO_DA_AREA * area.w + 1e-12);
    expect(c.h).toBeLessThanOrEqual(FRACAO_DA_AREA * area.h + 1e-12);
  });

  it("área grande: o tamanho de sempre, no centro dela", () => {
    useAreaVisivel.getState().publicar("c", { x: 0.1, y: 0.2, w: 0.8, h: 0.6 });
    const c = caixaNova("c");
    expect(c.w).toBeCloseTo(CAIXA_CENTRO.w, 12);
    expect(c.h).toBeCloseTo(CAIXA_CENTRO.h, 12);
    expect(c.x + c.w / 2).toBeCloseTo(0.5, 12);
    expect(c.y + c.h / 2).toBeCloseTo(0.5, 12);
  });

  it("zoom máximo: a caixa encolhe e fica dentro do que se vê", () => {
    const v = zoomEm(VISTA_INICIAL, 16, { x: 400, y: 300 }, TELA, IMG);
    const A = aDe([], 0, ORIGEM, SAIDA);
    const area = areaVisivelNaOrigem(v, TELA, SAIDA, A);
    useAreaVisivel.getState().publicar("c", area);
    const c = caixaNova("c");
    expect(c.w).toBeLessThan(CAIXA_CENTRO.w);
    for (const p of pontas(c)) expect(dentro(area, p)).toBe(true);
    for (const p of pontas(c)) expect(visivel(v, A, p)).toBe(true);
  });

  it("publicar igual não troca o estado (o palco publica a cada pintura)", () => {
    const a = { x: 0.1, y: 0.1, w: 0.5, h: 0.5 };
    useAreaVisivel.getState().publicar("c", a);
    const antes = useAreaVisivel.getState();
    useAreaVisivel.getState().publicar("c", { ...a });
    expect(useAreaVisivel.getState()).toBe(antes);
    useAreaVisivel.getState().publicar("c", { ...a, w: 0.4 });
    expect(useAreaVisivel.getState()).not.toBe(antes);
  });
});

describe("Adicionar → destaque, ocultar, lupa, holofote: o rastreio novo nasce em caixaNova", () => {
  const st = () => useEdicaoStore.getState();
  const AREA: Caixa = { x: 0.55, y: 0.05, w: 0.3, h: 0.25 };

  beforeEach(async () => {
    for (const f of Object.values(cmd)) f.mockReset();
    cmd.edicaoSalvar.mockImplementation(async () => ({ updated_at: new Date().toISOString() }));
    cmd.edicaoAnalisar.mockImplementation(() => new Promise(() => {}));
    const p = projeto();
    cmd.edicaoAbrir.mockResolvedValue({ projeto: p, importado_de: null, so_leitura: false, avisos: [], raiz_cache: "/cache/r" } satisfies ProjetoAberto);
    await st().abrir(WS, p.id);
    useRecursos.setState((s) => ({ midias: { [SHA]: { info, tabela: tab, erro: null, pedindo: false } }, versao: s.versao + 1 }));
    st().posicionar({ modo: "clipe", clipe: "c", t: q(40).t });
    useAreaVisivel.getState().publicar("c", AREA);
  });

  afterEach(async () => {
    await st()
      .fechar({ descartar: true })
      .catch(() => {});
  });

  const caixaDoEfeito = (id: string | null) => {
    const d = st().doc!;
    const e = acharEfeito(d, id!)!.efeito;
    const r = d.rastreios.find((x) => x.id === rastreioDe(e))!;
    return r.ancoras[0]!.v as Caixa;
  };

  it("as quatro ações gravam a âncora na caixa da área publicada, no quadro do cursor", async () => {
    const esperada = caixaNova("c");
    expect(esperada).not.toEqual(CAIXA_CENTRO);
    for (const p of pontas(esperada)) expect(dentro(AREA, p)).toBe(true);
    const ids = [
      adicionarEfeito("c", "destaque", "Destaque"),
      adicionarEfeito("c", "ocultar", "Ocultar"),
      await adicionarLupa("c", "Lupa"),
      await adicionarHolofote("c", "Holofote"),
    ];
    for (const id of ids) {
      expect(caixaDoEfeito(id)).toEqual(esperada);
      const d = st().doc!;
      const r = d.rastreios.find((x) => x.id === rastreioDe(acharEfeito(d, id!)!.efeito))!;
      expect(r.ancoras[0]!.pts).toBe(q(40).pts);
    }
  });
});
