/**
 * Plano da prévia viva (P3.3): segmento × slot igual ao `resolver` (o espelho do Rust), quadro de cada slot igual ao
 * mapa do trecho, transições como na junção do motor (sobreposição k/d, `escurecer` por `fade`, entrada do primeiro e
 * `fecho` com o fundo), congelado, sobrepostos pela transformação do motor e sons por barra.
 */

import { describe, expect, it } from "vitest";
import type { ClipeAudio, ClipeCongelar, ClipeTitulo, ClipeTrecho, Id, MidiaRef, ProjetoEdicao, SobTitulo, TabelaQuadros, Transicao } from "@domain/edicao";
import { novoProjeto, novoTrecho } from "../../modelo/projeto";
import { exibido, faixaDoTrecho, mapaTrecho, resolver, tempoDoQuadro, type ContextoTempo } from "../../modelo/tempo";
import { transformacaoEm } from "../../modelo/sobreposto";
import { estadoEm, planoDe, segmentoDoSlot, segmentosEm, slotDoTempo, type PlanoPrevia } from "./plano";

/** 600 quadros a 30 qps em 1/15360, começando em 1,5 s (início ≠ 0). */
const TAB: TabelaQuadros = { tb: { num: 1, den: 15360 }, pts0: 23040, passos: Array(599).fill(512), passo_mediano: 512, chaves: [0, 60, 120], vfr: false, origem: "decodificador" };
/** 25 qps com oscilação de ±1 tique. */
const TAB25: TabelaQuadros = { tb: { num: 1, den: 1000 }, pts0: 0, passos: Array.from({ length: 299 }, (_, i) => 40 + (i % 3) - 1), passo_mediano: 40, chaves: [0], vfr: false, origem: "decodificador" };
const tabelas: Record<string, TabelaQuadros> = { m: TAB, n: TAB25 };
const CTX: ContextoTempo = { tabela: (id) => tabelas[id] ?? null };

const midia = (id: Id, sha: string, tem_audio = true): MidiaRef =>
  ({ id, sha256: sha, tipo: "video", nome: id, relativo: `${id}.mp4`, origem: "caso", media_id: null, duracao_s: 20, inicio_s: 0, largura: 640, altura: 360, taxa: null, vfr: false, tem_audio, cor: null, derivacao: null, recebido: { nativo: null, obs: "" }, relogio_desvio_s: null, gravador: null, decodificacao: null, ocultacoes: [] }) as MidiaRef;

function trecho(id: Id, m: Id, j0: number, j1: number, fator = 1, tr: Transicao | null = null): ClipeTrecho {
  const tab = tabelas[m]!;
  return { ...novoTrecho(m, tempoDoQuadro(tab, j0), tempoDoQuadro(tab, j1)), id, transicao_entrada: tr, velocidade: { tipo: "constante", fator } };
}
const tr = (tipo: string, duracao_s: number, audio: Transicao["audio"] = "acompanhar"): Transicao => ({ tipo, duracao_s, audio });

function projeto(principal: ProjetoEdicao["principal"], extra: Partial<ProjetoEdicao> = {}): ProjetoEdicao {
  const p = novoProjeto();
  p.saida.fps = { num: 30, den: 1 };
  p.saida.audio = true;
  p.midias = [midia("m", "a".repeat(64)), midia("n", "b".repeat(64))];
  p.principal = principal;
  return { ...p, ...extra };
}

function plano(p: ProjetoEdicao): PlanoPrevia {
  return planoDe(p, resolver(p, CTX), CTX);
}

describe("segmento × slot", () => {
  it("é o mesmo do resolver e cada slot mostra o quadro do mapa do trecho (1×, 0,5×, 2×, 25 qps oscilando)", () => {
    const p = projeto([trecho("a", "m", 10, 70), trecho("b", "m", 200, 230, 0.5), trecho("c", "n", 5, 125, 2), trecho("d", "m", 70, 100)]);
    const linha = resolver(p, CTX);
    const pl = planoDe(p, linha, CTX);
    expect(pl.total_q).toBe(linha.total_q);
    expect(pl.segmentos.map((s) => [s.clipe, s.de_q, s.ate_q])).toEqual(linha.clipes.map((c) => [c.id, c.inicio_q, c.inicio_q + c.n_q]));
    for (const c of p.principal as ClipeTrecho[]) {
      const tab = tabelas[c.midia]!;
      const [a, b] = faixaDoTrecho(tab, c.inicio_s, c.inicio_pts, c.fim_s, c.fim_pts);
      const m = mapaTrecho(tab, a, b, c.velocidade, linha.fps);
      const r = linha.clipes.find((x) => x.id === c.id)!;
      for (let k = 0; k < r.n_q; k++) {
        const e = estadoEm(pl, r.inicio_q + k);
        expect(e.camadas).toHaveLength(1);
        expect(e.camadas[0]!.segmento.clipe).toBe(c.id);
        expect(e.camadas[0]!.indice).toBe(m.quadro_do_slot[k]);
        expect(e.camadas[0]!.t).toBe(tempoDoQuadro(tab, m.quadro_do_slot[k]!));
      }
    }
    // 0,5×: cada quadro em 2 slots; 2×: um quadro sim, outro não
    const b = linha.clipes[1]!;
    expect([0, 1, 2, 3].map((k) => estadoEm(pl, b.inicio_q + k).camadas[0]!.indice)).toEqual([200, 200, 201, 201]);
    const c = linha.clipes[2]!;
    expect([0, 1, 2, 3].map((k) => estadoEm(pl, c.inicio_q + k).camadas[0]!.indice)).toEqual([5, 7, 8, 10]);
    // fora do programa: nada
    expect(estadoEm(pl, linha.total_q).camadas).toEqual([]);
    expect(estadoEm(pl, 40).programa_s).toBeCloseTo(40 / 30, 12);
  });

  it("clipes contíguos sem transição não se sobrepõem; o dono do slot é o que entra", () => {
    const pl = plano(projeto([trecho("a", "m", 0, 30), trecho("b", "m", 30, 60)]));
    expect(segmentosEm(pl, 29).map((s) => s.clipe)).toEqual(["a"]);
    expect(segmentosEm(pl, 30).map((s) => s.clipe)).toEqual(["b"]);
    expect(estadoEm(pl, 30).camadas[0]!.indice).toBe(30);
    expect(segmentoDoSlot(pl, 30)?.clipe).toBe("b");
  });

  it("sem tabela o tempo sai da velocidade, sem quadro", () => {
    const p = projeto([trecho("a", "m", 0, 60, 2)]);
    const linha = resolver(p, { tabela: () => null });
    const pl = planoDe(p, linha);
    const c = estadoEm(pl, 3).camadas[0]!;
    expect(c.indice).toBeNull();
    expect(c.t).toBeCloseTo(tempoDoQuadro(TAB, 0) + (3 / 30) * 2, 12);
  });

  it("desentrelaçar por campo: o mapa conta campos e o slot mostra o quadro da fonte (60 qps)", () => {
    const c = trecho("a", "m", 0, 20);
    c.efeitos = [{ id: "d", tipo: "desentrelacar", ligado: true, nome: "", ativo: null, modo: "campo", ordem: "auto" }];
    // a 60 qps de saída cada campo ganha um slot
    const p = projeto([c]);
    p.saida.fps = { num: 60, den: 1 };
    const pl = plano(p);
    expect(pl.segmentos[0]!.ate_q).toBe(40);
    expect([0, 1, 2, 3, 39].map((q) => estadoEm(pl, q).camadas[0]!.indice)).toEqual([0, 0, 1, 1, 19]);
  });

  it("rampa e velocidade fora de 0,25..8 tocam guiadas e sem som", () => {
    const a = trecho("a", "m", 0, 60);
    a.velocidade = { tipo: "rampa", pontos: [{ t: tempoDoQuadro(TAB, 0), v: 1 }, { t: tempoDoQuadro(TAB, 30), v: 0.5 }] };
    const pl = plano(projeto([a, trecho("b", "m", 60, 90, 0.2), trecho("c", "m", 90, 120, 8), trecho("d", "m", 120, 150, 10)]));
    expect(pl.segmentos.map((s) => s.guiado)).toEqual([true, true, false, true]);
    expect(pl.sons.map((s) => s.id)).toEqual(["c"]);
  });
});

describe("transições", () => {
  it("fundir: d slots com as duas camadas, progresso k/d; antes e depois, uma só", () => {
    const p = projeto([trecho("a", "m", 0, 60), trecho("b", "m", 300, 360, 1, tr("fundir", 0.5))]);
    const linha = resolver(p, CTX);
    const pl = planoDe(p, linha, CTX);
    const b = linha.clipes[1]!;
    expect(b.sobrepoe_q).toBe(15);
    expect(pl.segmentos[1]!.entrada).toEqual({ tipo: "fundir", n_q: 15 });
    expect(pl.segmentos[0]!.saida).toEqual({ tipo: "fundir", n_q: 15 });
    for (let k = 0; k < 15; k++) {
      const e = estadoEm(pl, b.inicio_q + k);
      expect(e.transicao).toEqual({ tipo: "fundir", progresso: k / 15, fundo: null });
      expect(e.camadas.map((c) => c.segmento.clipe)).toEqual(["a", "b"]);
      expect(e.camadas[1]!.peso).toBeCloseTo(k / 15, 12);
      // a cauda de a segue o mapa dele; a cabeça de b começa no quadro de entrada
      expect(e.camadas[0]!.indice).toBe(45 + k);
      expect(e.camadas[1]!.indice).toBe(300 + k);
    }
    expect(estadoEm(pl, b.inicio_q - 1).transicao).toBeNull();
    expect(estadoEm(pl, b.inicio_q + 15).camadas.map((c) => c.segmento.clipe)).toEqual(["b"]);
  });

  it("escurecer: fade por tempo dos dois lados, sem sobrepor (o `fade` do ffmpeg: k/d e 1 − k'/d)", () => {
    const p = projeto([trecho("a", "m", 0, 60), trecho("b", "m", 300, 360, 1, tr("escurecer", 0.2))]);
    const pl = plano(p);
    const [a, b] = pl.segmentos as [PlanoPrevia["segmentos"][0], PlanoPrevia["segmentos"][0]];
    expect(a.ate_q).toBe(b.de_q);
    expect(a.saida).toEqual({ tipo: "escurecer", n_q: 6 });
    expect(b.entrada).toEqual({ tipo: "escurecer", n_q: 6 });
    expect([54, 55, 59].map((q) => estadoEm(pl, q).camadas[0]!.luz)).toEqual([1, 1 - 1 / 6, 1 - 5 / 6]);
    expect([53].map((q) => estadoEm(pl, q).camadas[0]!.luz)).toEqual([1]);
    expect([60, 61, 65, 66].map((q) => estadoEm(pl, q).camadas[0]!.luz)).toEqual([0, 1 / 6, 5 / 6, 1]);
    expect(estadoEm(pl, 60).transicao).toBeNull();
  });

  it("a entrada do primeiro e o fecho misturam com o fundo dentro do próprio clipe", () => {
    const p = projeto([trecho("a", "m", 0, 60, 1, tr("circulo", 0.3))], { fecho: tr("cortina_esq", 0.4) });
    const pl = plano(p);
    expect(pl.total_q).toBe(60);
    expect(estadoEm(pl, 0).transicao).toEqual({ tipo: "circulo", progresso: 0, fundo: "sai" });
    expect(estadoEm(pl, 8).transicao).toEqual({ tipo: "circulo", progresso: 8 / 9, fundo: "sai" });
    expect(estadoEm(pl, 9).transicao).toBeNull();
    expect(estadoEm(pl, 47).transicao).toBeNull();
    expect(estadoEm(pl, 48).transicao).toEqual({ tipo: "cortina_esq", progresso: 0, fundo: "entra" });
    expect(estadoEm(pl, 59).transicao).toEqual({ tipo: "cortina_esq", progresso: 11 / 12, fundo: "entra" });
    expect(estadoEm(pl, 59).camadas[0]!.peso).toBeCloseTo(1 / 12, 12);
  });
});

describe("clipes sem fonte, sobrepostos e sons", () => {
  it("congelado mostra sempre o quadro do instante, com tempo local; título não tem fonte", () => {
    const cg: ClipeCongelar = { tipo: "congelar", id: "g", rotulo: "", desligado: false, transicao_entrada: null, efeitos: [], midia: "m", instante_s: tempoDoQuadro(TAB, 77) + 0.004, instante_pts: null, duracao_s: 1 };
    const ti: ClipeTitulo = { tipo: "titulo", id: "t", rotulo: "", desligado: false, transicao_entrada: null, efeitos: [], duracao_s: 2, titulo: { modelo: "cartela", fundo: { tipo: "cor", cor: "#000000" }, blocos: [] } };
    const pl = plano(projeto([cg, ti]));
    expect([0, 10, 29].map((q) => estadoEm(pl, q).camadas[0]!.indice)).toEqual([77, 77, 77]);
    expect(estadoEm(pl, 15).camadas[0]!.t).toBeCloseTo(0.5, 12);
    expect(pl.segmentos[0]!.fonte).toMatchObject({ congelado: true, inicio_s: tempoDoQuadro(TAB, 77) });
    expect(pl.segmentos[1]!.fonte).toBeNull();
    expect(estadoEm(pl, 45).camadas[0]).toMatchObject({ indice: null, t: 0.5 });
  });

  it("sobreposto: slots da linha, tempo local e a transformação do motor; trilha oculta não entra", () => {
    const sob: SobTitulo = {
      tipo: "titulo",
      id: "s",
      rotulo: "",
      pos: { ref: "clipe", clipe: "b", desloc: 0.5 },
      duracao_s: 2,
      transformacao: { x: 0.3, y: [{ t: 0, v: 0.2 }, { t: 2, v: 0.8 }], escala: 1, rotacao: 10, opacidade: 1 },
      entrada_s: 0.3,
      saida_s: 0.3,
      efeitos: [],
      titulo: { modelo: "livre", fundo: { tipo: "transparente" }, blocos: [] },
    };
    const p = projeto([trecho("a", "m", 0, 30), trecho("b", "m", 30, 90)]);
    p.sobreposicoes = [
      { id: "T1", nome: "", oculta: false, clipes: [sob] },
      { id: "T2", nome: "", oculta: true, clipes: [{ ...sob, id: "x" }] },
    ];
    const pl = plano(p);
    expect(pl.livres.map((l) => [l.id, l.de_q, l.ate_q])).toEqual([["s", 45, 105]]);
    for (const q of [45, 50, 70, 104]) {
      const e = estadoEm(pl, q);
      const tau = (q - 45) / 30;
      expect(e.livres).toHaveLength(1);
      expect(e.livres[0]!.tau).toBeCloseTo(tau, 12);
      expect(e.livres[0]!.transformacao).toEqual(transformacaoEm(sob, tau, 2));
    }
    expect(estadoEm(pl, 105).livres).toEqual([]);
  });

  it("sons: o do trecho na barra do original (mudo e sem som de fora), as trilhas na da narração", () => {
    const a = trecho("a", "m", 0, 60);
    const b = trecho("b", "m", 60, 90);
    b.audio.mudo = true;
    const c = trecho("c", "n", 0, 30);
    const ca: ClipeAudio = { id: "na", nome: "", midia: "m", pos: { ref: "programa", t: 1 }, de_s: 2, ate_s: 4, volume: 0.5, entrada_s: 0.1, saida_s: 0.2 };
    const p = projeto([a, b, c]);
    p.midias[1] = midia("n", "b".repeat(64), false);
    p.mixagem.volume_original = 0.8;
    p.audios = [{ id: "A1", nome: "", papel: "narracao", volume: 1.5, mudo: false, abaixa_original: true, clipes: [ca] }];
    const pl = plano(p);
    expect(pl.sons.map((s) => [s.id, s.barra, s.de_q, s.ate_q, s.ganho])).toEqual([
      ["a", "original", 0, 60, 0.8],
      ["na", "narracao", 30, 90, 1.5],
    ]);
    expect(pl.sons[1]).toMatchObject({ inicio_s: 2, volume: 0.5, entrada_s: 0.1, saida_s: 0.2, abaixa: true });
    // saída sem som: nada toca
    expect(plano({ ...p, saida: { ...p.saida, audio: false } }).sons).toEqual([]);
  });
});

describe("seletor Programa | Clipe", () => {
  it("slotDoTempo leva o tempo do quadro ao primeiro slot dele (ida e volta com estadoEm)", () => {
    const p = projeto([trecho("a", "m", 10, 70), trecho("b", "m", 200, 230, 0.5), trecho("c", "n", 5, 125, 2)]);
    const pl = plano(p);
    for (const s of pl.segmentos) {
      for (let q = s.de_q; q < s.ate_q; q++) {
        const c = estadoEm(pl, q).camadas.find((x) => x.segmento === s)!;
        const volta = slotDoTempo(pl, s.clipe, c.t)!;
        // o primeiro slot que mostra o mesmo quadro
        expect(estadoEm(pl, volta).camadas.find((x) => x.segmento === s)!.indice).toBe(c.indice);
        expect(volta).toBeLessThanOrEqual(q);
      }
    }
    // fora do clipe: as pontas
    expect(slotDoTempo(pl, "a", 0)).toBe(0);
    expect(slotDoTempo(pl, "a", 100)).toBe(59);
    // quadro que a aceleração descarta: o slot do quadro que o cobre
    const c = pl.segmentos[2]!;
    expect(slotDoTempo(pl, "c", tempoDoQuadro(TAB25, 6))).toBe(c.de_q);
    expect(slotDoTempo(pl, "x", 1)).toBeNull();
    // o meio do quadro (cursor do modo clipe) também
    const meio = tempoDoQuadro(TAB, 40) + 1 / 60;
    expect(estadoEm(pl, slotDoTempo(pl, "a", meio)!).camadas[0]!.indice).toBe(exibido(TAB, meio));
  });
});
