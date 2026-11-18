/** Zoom que segue (ZS): espelho de `edicao/segue.rs` pelas fixtures, as amostras pela tabela da mídia, o gancho do
 *  palco (`efeitosNoInstante`) e as contas das chaves do Inspetor. */

import { afterEach, describe, expect, it } from "vitest";
import type { Caixa, Chave, ClipeTrecho, Desconhecido, EfEnquadramento, Efeito, MidiaRef, ProjetoEdicao, Rastreio, TabelaQuadros } from "@domain/edicao";
import {
  amostrasDoSegue,
  candidatosDoSegue,
  centroNoGirado,
  comChaveNoCursor,
  comSegue,
  efeitosNoInstante,
  mostraTudo,
  nomeDoCandidato,
  recusa,
  regiaoDoSegue,
  regiaoNoInstante,
  regiaoSeguida,
  rotulosDasChaves,
  semChave,
  semSegue,
  SEGUE_PRONTO,
  SUAVIZAR_MAX_S,
  SUAVIZAR_PADRAO_S,
  type AmostraSegue,
} from "./segue";
import { IDENTIDADE, ponto } from "./geometria";
import { avaliar as avaliarRastreio } from "./rastreio";
import { novoProjeto, novoTrecho, porChave } from "./projeto";
import { ptsDaTabela } from "./tempo";
import { useRecursos } from "../palco/recursos";
import { alcasDaRegiao, comRegiao, regiaoNaSaida } from "../palco/alcas";
import { casos, conferirCasos } from "./testeFixtures";
import { novoDestaque, novoOcultar } from "../inspetor/novos";

const base = { ligado: true, nome: "", ativo: null };
const enq = (regiao: EfEnquadramento["regiao"], extra: Partial<EfEnquadramento> = {}): EfEnquadramento => ({
  ...base,
  id: "q1",
  tipo: "enquadramento",
  regiao,
  ajuste: "conter",
  interpolacao: "bilinear",
  ...extra,
});

describe("segue × fixtures (espelho de edicao/segue.rs)", () => {
  it("segue-regiao.json: média, mais próxima, borda, chaves de tamanho", () => {
    const lista = casos<{ efeito: EfEnquadramento; amostras: AmostraSegue[]; em: number[] }, Caixa[]>("segue-regiao.json");
    expect(lista.length).toBeGreaterThanOrEqual(15);
    expect(conferirCasos(lista, (e) => regiaoSeguida(e.efeito, e.amostras, e.em))).toEqual([]);
  });

  it("segue-centro.json: centro da caixa no espaço girado", () => {
    const lista = casos<{ caixa: Caixa; efeitos: (Efeito | Desconhecido)[] }, { x: number; y: number }>("segue-centro.json");
    expect(lista.length).toBe(6);
    expect(conferirCasos(lista, (e) => centroNoGirado(e.caixa, e.efeitos))).toEqual([]);
  });

  it("recusa convertida; constantes do Rust", () => {
    expect(recusa({ segue: { rastreio: "r1", suavizar_s: 1 } })).toBeNull();
    expect([SEGUE_PRONTO, SUAVIZAR_PADRAO_S, SUAVIZAR_MAX_S]).toEqual([true, 1, 10]);
  });
});

// ---- tabela e rastreio sintéticos: 300 quadros a 30 qps (base 1/15360) ----

const TB = { num: 1, den: 15360 };
const tabela = (n: number): TabelaQuadros => ({ tb: TB, pts0: 0, passos: Array(n - 1).fill(512), passo_mediano: 512, chaves: [0], vfr: false, origem: "decodificador" });
const TAB = tabela(300);
const PTS = ptsDaTabela(TAB);
const tempo = (j: number) => ((PTS[j] as number) * TB.num) / TB.den;

/** Rastreio que anda, com âncoras de `de` a `ate` a cada 5 quadros; assistido não liga âncoras com mais de 4 s. */
function rastreio(id: string, de: number, ate: number, origem: Rastreio["origem"] = { tipo: "manual" }, buraco?: [number, number]): Rastreio {
  const ancoras: Chave<Caixa>[] = [];
  for (let j = de; j < ate; j += 5) {
    if (buraco && j >= buraco[0] && j < buraco[1]) continue;
    ancoras.push({ t: tempo(j), pts: PTS[j] as number, v: { x: 0.1 + 0.002 * j, y: 0.3 + 0.001 * j, w: 0.1, h: 0.2 } });
  }
  return { id, nome: id.toUpperCase(), cor: "#ff0000", midia: "m1", base: "fonte", tipo: "caixa", espaco: "original", ancoras, denso: null, silhuetas: null, ausencias: [], origem, confirmacao: null };
}

/** Todas as amostras da mídia (a definição do D9). */
function todas(r: Rastreio): AmostraSegue[] {
  return PTS.map((p, j) => {
    const a = avaliarRastreio(r, { t: tempo(j), pts: p }, null);
    return { t: tempo(j), centro: a ? ponto(IDENTIDADE, { x: a.caixa.x + a.caixa.w / 2, y: a.caixa.y + a.caixa.h / 2 }) : null };
  });
}

describe("amostras pela tabela da mídia", () => {
  it("a janela e a mais próxima de cada lado dão o mesmo que todos os quadros (o `segue::amostras` do Rust)", () => {
    const casosR: Rastreio[] = [
      rastreio("r1", 0, 300),
      rastreio("r2", 120, 200),
      rastreio("r3", 0, 300, { tipo: "assistido" }, [60, 250]),
    ];
    for (const r of casosR)
      for (const s of [0, 0.2, 1, 3]) {
        const e = enq({ x: 0.3, y: 0.3, w: 0.4, h: 0.4 }, { segue: { rastreio: r.id, suavizar_s: s } });
        const tudo = todas(r);
        for (let j = 0; j < 300; j += 7) {
          const t = tempo(j) + (j % 3 === 0 ? 0.004 : 0);
          const parcial = amostrasDoSegue(r, TAB, null, IDENTIDADE, [t], s);
          expect(parcial.length).toBeLessThan(tudo.length);
          expect(regiaoSeguida(e, parcial, [t]), `${r.id} s=${s} t=${t}`).toEqual(regiaoSeguida(e, tudo, [t]));
        }
      }
  });
});

// ---- o gancho do palco ----

function projetoCom(efeitos: (Efeito | Desconhecido)[], rastreios: Rastreio[]): { p: ProjetoEdicao; c: ClipeTrecho } {
  const p = novoProjeto("Teste", "2026-11-17T00:00:00.000Z");
  p.midias = [{ id: "m1", sha256: "a".repeat(64), ocultacoes: [] } as unknown as MidiaRef];
  const c = novoTrecho("m1", 1, 9);
  c.efeitos = efeitos;
  p.principal = [c];
  p.rastreios = rastreios;
  return { p, c };
}

afterEach(() => useRecursos.setState({ midias: {} }));

describe("efeitosNoInstante (palco)", () => {
  const comTabela = () => useRecursos.setState({ midias: { ["a".repeat(64)]: { info: null, tabela: TAB, erro: null, pedindo: false } } });

  it("sem enquadramento que segue: a mesma lista, sem cópia", () => {
    comTabela();
    const { p, c } = projetoCom([enq({ x: 0.3, y: 0.3, w: 0.4, h: 0.4 })], [rastreio("r1", 0, 300)]);
    expect(efeitosNoInstante(p, c.efeitos, 2)).toBe(c.efeitos);
    expect(regiaoNoInstante(p, c.efeitos, 2)).toBeNull();
  });

  it("com o segue, a região do instante acompanha o rastreio (centro suavizado, tamanho da gravada)", () => {
    comTabela();
    const r = rastreio("r1", 0, 300);
    const e = enq({ x: 0.3, y: 0.3, w: 0.4, h: 0.4 }, { ajuste: "cobrir", segue: { rastreio: "r1", suavizar_s: 0 } });
    const { p, c } = projetoCom([e], [r]);
    const em = (j: number) => {
      const l = efeitosNoInstante(p, c.efeitos, tempo(j));
      expect(l).not.toBe(c.efeitos);
      return (l[0] as EfEnquadramento).regiao as Caixa;
    };
    // sem média: o centro da caixa do quadro (0,1 + 0,002 j + 0,05; 0,3 + 0,001 j + 0,1), menos meio lado
    expect(em(100).x).toBeCloseTo(0.1 + 0.2 + 0.05 - 0.2, 12);
    expect(em(100).y).toBeCloseTo(0.3 + 0.1 + 0.1 - 0.2, 12);
    expect(em(200).x).toBeCloseTo(0.1 + 0.4 + 0.05 - 0.2, 12);
    expect([em(100).w, em(100).h]).toEqual([0.4, 0.4]);
    // nunca sai do quadro: no fim a caixa encosta na borda
    expect(em(299).x + 0.4).toBeLessThanOrEqual(1);
    // sem a tabela lida, fica a gravada
    useRecursos.setState({ midias: {} });
    expect(efeitosNoInstante(p, c.efeitos, tempo(100))).toBe(c.efeitos);
  });
});

// ---- chaves do Inspetor ----

describe("chaves do zoom", () => {
  const k = (t: number, w: number): Chave<Caixa> => ({ t, v: { x: (1 - w) / 2, y: (1 - w) / 2, w, h: w } });

  it("rótulo pelo vizinho: Cena inteira, Aproximar, Segurar, Afastar", () => {
    const r = rotulosDasChaves([k(0, 1), k(1, 0.5), k(2, 0.5), k(3, 0.8), k(4, 1)]);
    expect(r.map((x) => x.rotulo)).toEqual(["Cena inteira", "Aproximar", "Segurar", "Afastar", "Afastar"]);
    // parado na cena inteira
    expect(rotulosDasChaves([k(0, 1), k(1, 1)]).map((x) => x.rotulo)).toEqual(["Cena inteira", "Cena inteira"]);
    expect(r.map((x) => x.fator)).toEqual([1, 2, 2, 1.25, 1]);
    // a primeira, já ampliada, aproxima da cena inteira
    expect(rotulosDasChaves([k(0, 0.25)])[0]).toEqual({ rotulo: "Aproximar", fator: 4 });
  });

  it("porChave: a chave trocada deixa a `interp` dela; a que traz a sua troca", () => {
    const ks: Chave<number>[] = [{ t: 1, v: 1, interp: "degrau" }, { t: 2, v: 2 }];
    expect(porChave(ks, { t: 1, v: 5 })).toEqual([{ t: 1, v: 5, interp: "degrau" }, { t: 2, v: 2 }]);
    expect(porChave(ks, { t: 1, v: 5, interp: "suave" })[0]).toEqual({ t: 1, v: 5, interp: "suave" });
    expect(porChave(ks, { t: 2, v: 7 })[1]).toEqual({ t: 2, v: 7 });
    expect(porChave(ks, { t: 3, v: 7 })).toHaveLength(3);
  });

  it("Cena inteira só quando a região mostra a fonte inteira: com fonte de outra proporção, Preencher recorta", () => {
    // fonte 4:3 na saída 16:9: a proporção da saída na origem é 4/3
    const p43 = 4 / 3;
    expect(rotulosDasChaves([k(0, 1), k(1, 0.5), k(2, 1)], "cobrir", p43).map((x) => x.rotulo)).toEqual(["Segurar", "Aproximar", "Afastar"]);
    expect(rotulosDasChaves([k(0, 1)], "conter", p43)[0]!.rotulo).toBe("Cena inteira");
    // a mesma proporção: Preencher mostra tudo
    expect(rotulosDasChaves([k(0, 1), k(1, 0.5), k(2, 1), k(3, 1)], "cobrir", 1).map((x) => x.rotulo)).toEqual(["Cena inteira", "Aproximar", "Afastar", "Cena inteira"]);
    expect(rotulosDasChaves([k(0, 1), k(1, 1)], "cobrir", p43).map((x) => x.rotulo)).toEqual(["Segurar", "Segurar"]);
    // a maior região na proporção da saída corta as bordas da fonte 4:3: não é a cena inteira
    const corte: Chave<Caixa> = { t: 0, v: { x: 0, y: 0.125, w: 1, h: 0.75 } };
    expect(rotulosDasChaves([corte], "cobrir", p43)[0]!.rotulo).toBe("Segurar");
    expect([mostraTudo({ x: 0, y: 0, w: 1, h: 1 }, "cobrir", 1), mostraTudo({ x: 0, y: 0, w: 1, h: 1 }, "cobrir", p43), mostraTudo({ x: 0.1, y: 0, w: 0.9, h: 1 }, "conter", 1)]).toEqual([true, false, false]);
  });

  it("chave no cursor: fixa vira a primeira chave; outra chave no mesmo trecho; a segunda diferente grava Preencher", () => {
    const fixa = enq({ x: 0, y: 0, w: 1, h: 1 }, { interpolacao: "vizinho" });
    const um = comChaveNoCursor(fixa, 2, 60, { x: 0, y: 0, w: 1, h: 1 });
    // chave nova do zoom nasce suave (maquete: entra e sai devagar)
    expect(um.regiao).toEqual([{ t: 2, pts: 60, v: { x: 0, y: 0, w: 1, h: 1 }, interp: "suave" }]);
    expect([um.ajuste, um.interpolacao]).toEqual(["conter", "vizinho"]);
    const dois = comChaveNoCursor(um, 4, null, { x: 0, y: 0, w: 1, h: 1 });
    expect([dois.ajuste, (dois.regiao as Chave<Caixa>[]).length]).toEqual(["conter", 2]);
    const tres = comChaveNoCursor(dois, 3, 90, { x: 0.25, y: 0.25, w: 0.5, h: 0.5 });
    expect((tres.regiao as Chave<Caixa>[]).map((x) => x.t)).toEqual([2, 3, 4]);
    expect([tres.ajuste, tres.interpolacao]).toEqual(["cobrir", "bilinear"]);
    // mesma chave (mesmo quadro) trocada
    expect((comChaveNoCursor(tres, 3, 90, { x: 0.3, y: 0.3, w: 0.4, h: 0.4 }).regiao as Chave<Caixa>[]).length).toBe(3);
    // a última que sai volta a valor fixo
    expect(semChave(semChave(semChave(tres, 0), 0), 0).regiao).toEqual({ x: 0, y: 0, w: 1, h: 1 });
  });

  it("arrastar a alça no palco com chaves grava a chave do cursor e Preencher (o mesmo gesto)", () => {
    const e = enq([k(1, 1), k(5, 1)]);
    const novo = comRegiao(e, { x: 0.2, y: 0.2, w: 0.5, h: 0.5 }, 3, null);
    expect((novo.regiao as Chave<Caixa>[]).map((x) => x.t)).toEqual([1, 3, 5]);
    expect((novo.regiao as Chave<Caixa>[])[1]).toEqual({ t: 3, v: { x: 0.2, y: 0.2, w: 0.5, h: 0.5 }, interp: "suave" });
    // arrastar de novo no mesmo instante troca o valor e deixa a curva da chave (a antiga, sem `interp`, fica linear)
    const de_novo = comRegiao(novo, { x: 0.1, y: 0.1, w: 0.6, h: 0.6 }, 3, null);
    expect((de_novo.regiao as Chave<Caixa>[])[1]).toEqual({ t: 3, v: { x: 0.1, y: 0.1, w: 0.6, h: 0.6 }, interp: "suave" });
    expect((comRegiao(novo, { x: 0.1, y: 0.1, w: 0.6, h: 0.6 }, 1, null).regiao as Chave<Caixa>[])[0]).toEqual({ t: 1, v: { x: 0.1, y: 0.1, w: 0.6, h: 0.6 } });
    expect(novo.ajuste).toBe("cobrir");
    // fixa troca o valor e fica como estava
    expect(comRegiao(enq({ x: 0, y: 0, w: 1, h: 1 }), { x: 0.2, y: 0.2, w: 0.5, h: 0.5 }, 3, 90).ajuste).toBe("conter");
  });

  it("região marcada na proporção da saída, inteira dentro da imagem", () => {
    const r = regiaoNaSaida({ x: 0.1, y: 0.1, w: 0.4, h: 0.2 }, 1.5);
    expect(r.w / r.h).toBeCloseTo(1.5, 12);
    expect(r.x + r.w / 2).toBeCloseTo(0.3, 12);
    const fora = regiaoNaSaida({ x: 0.7, y: -0.2, w: 0.6, h: 0.5 }, 1);
    expect(fora.x >= 0 && fora.x + fora.w <= 1 + 1e-12 && fora.y >= 0 && fora.y + fora.h <= 1 + 1e-12).toBe(true);
    expect(fora.w).toBeCloseTo(fora.h, 12);
    const grande = regiaoNaSaida({ x: -0.2, y: 0, w: 1.4, h: 0.9 }, 1);
    expect([grande.w, grande.x]).toEqual([1, 0]);
  });

  it("no zoom que acompanha, a alça só muda o tamanho, a partir do centro", () => {
    const e = enq({ x: 0.2, y: 0.2, w: 0.4, h: 0.4 }, { ajuste: "cobrir", segue: { rastreio: "r1", suavizar_s: 1 } });
    const alcas = alcasDaRegiao(e, e.regiao as Caixa, 2, null, IDENTIDADE, 1);
    expect(alcas.some((a) => a.tipo === "mover")).toBe(false);
    const se = alcas.find((a) => a.id.endsWith(":se"))!;
    const m = se.arrastar(se.em, { x: se.em.x + 0.1, y: se.em.y + 0.1 }, { shift: false, alt: false });
    if (!("efeito" in m) || m.efeito.tipo !== "enquadramento" || Array.isArray(m.efeito.regiao)) throw new Error("mudança errada");
    const r = m.efeito.regiao;
    expect(r.x + r.w / 2).toBeCloseTo(0.4, 12);
    expect(r.w).toBeCloseTo(0.6, 12);
    // sem o segue, "mover" continua
    expect(alcasDaRegiao(semSegue(e), e.regiao as Caixa, 2, null, IDENTIDADE, 1).some((a) => a.tipo === "mover")).toBe(true);
  });

  it("Acompanhar rastreio: candidatos do trecho (o do destaque primeiro); liga com Preencher e a região na proporção", () => {
    const outraMidia = { ...rastreio("r3", 0, 30), midia: "m9" };
    const { p, c } = projetoCom([novoOcultar("r1"), novoDestaque("r2"), novoDestaque("r3")], [rastreio("r1", 0, 30), rastreio("r2", 0, 30), outraMidia]);
    expect(candidatosDoSegue(p, c).map((r) => r.id)).toEqual(["r2", "r1"]);
    expect(candidatosDoSegue(p, c).map((r, i) => nomeDoCandidato(c, { ...r, nome: r.id === "r1" ? "" : r.nome }, i))).toEqual(["R2", "Ocultar 2"]);
    expect(candidatosDoSegue(p, { ...c, tipo: "congelar" } as unknown as ClipeTrecho)).toEqual([]);
    const ligado = comSegue(enq({ x: 0, y: 0, w: 1, h: 1 }, { interpolacao: "vizinho" }), "r2", 1);
    expect(ligado.segue).toEqual({ rastreio: "r2", suavizar_s: SUAVIZAR_PADRAO_S });
    expect([ligado.ajuste, ligado.interpolacao, ligado.regiao]).toEqual(["cobrir", "bilinear", { x: 0.25, y: 0.25, w: 0.5, h: 0.5 }]);
    // região já ampliada mantém o tamanho
    expect(comSegue(enq({ x: 0.1, y: 0.1, w: 0.3, h: 0.3 }), "r1", 1).regiao).toEqual({ x: 0.1, y: 0.1, w: 0.3, h: 0.3 });
    expect(regiaoDoSegue(16 / 9 * (9 / 16))).toEqual({ x: 0.25, y: 0.25, w: 0.5, h: 0.5 });
    expect(regiaoDoSegue(4).w).toBe(1);
    expect(semSegue(ligado).segue).toBeUndefined();
    expect("segue" in semSegue(ligado)).toBe(false);
  });
});
