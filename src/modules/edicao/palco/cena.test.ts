/** Cena do palco (o que se desenha num instante) e o pintor, com um contexto 2D falso que registra as chamadas. */

import { describe, expect, it } from "vitest";
import type { ClipeCongelar, ClipeTrecho, EfDestaque, EfOcultar, MidiaRef, Primitiva, ProjetoEdicao, Rastreio } from "@domain/edicao";
import { novoProjeto, novoTrecho } from "../modelo/projeto";
import { ativoEm, bordasDoClipe, cenaDoClipe, duracaoLocal, instanteDaAncora, instanteDaChave, type FontesCena, type Instante } from "./cena";
import { comRegiao } from "./alcas";
import { porChave } from "../modelo/projeto";
import { pintar } from "./desenho/pintar";
import { CONTORNO } from "./desenho/primitivas";

const base = { ligado: true, nome: "", ativo: null };
const fontes: FontesCena = { sprite: () => null, atlas: () => null, midia: () => null };
const inst = (t: number, pts: number | null = null): Instante => ({ t, pts, indice: 0, q: 0, programa_s: 0 });

function rastreio(id: string, ks: [number, number | null, number][]): Rastreio {
  return {
    id,
    nome: "",
    cor: "#ff0000",
    midia: "m1",
    base: "fonte",
    tipo: "caixa",
    espaco: "original",
    ancoras: ks.map(([t, pts, x]) => ({ t, pts, v: { x, y: 0.2, w: 0.2, h: 0.2 } })),
    denso: null,
    silhuetas: null,
    ausencias: [],
    origem: { tipo: "manual" },
    confirmacao: null,
  };
}

function projeto(): { p: ProjetoEdicao; c: ClipeTrecho } {
  const p = novoProjeto();
  p.saida.largura = 1280;
  p.saida.altura = 720;
  const m = { id: "m1", ocultacoes: [] as EfOcultar[] } as unknown as MidiaRef;
  p.midias = [m];
  const c = novoTrecho("m1", 10, 20);
  p.principal = [c];
  p.rastreios = [rastreio("r1", [[11, 330, 0.1], [13, 390, 0.5]])];
  return { p, c };
}

const destaque = (id: string, ativo: EfDestaque["ativo"] = null): EfDestaque => ({
  ...base,
  id,
  ativo,
  tipo: "destaque",
  forma: "retangulo",
  rastreio: "r1",
  traco: { cor: "#ffcc00", espessura: 0.006, contorno: true },
  tamanho: 0.12,
  angulo_graus: 45,
  margem: 0,
  rotulo: null,
});

describe("cena", () => {
  it("ativo: campo nulo é a borda do clipe; intervalo fechado", () => {
    expect(ativoEm(null, 10, [10, 20])).toBe(true);
    expect(ativoEm({ de: 12, ate: null }, 11.9, [10, 20])).toBe(false);
    expect(ativoEm({ de: 12, ate: 15 }, 15, [10, 20])).toBe(true);
    expect(ativoEm({ de: null, ate: 15 }, 15.0001, [10, 20])).toBe(false);
  });

  it("bordas e duração na base do clipe", () => {
    const { c } = projeto();
    expect(bordasDoClipe(c)).toEqual([10, 20]);
    expect(duracaoLocal(c)).toBe(10);
    const cong: ClipeCongelar = { tipo: "congelar", id: "g", rotulo: "", desligado: false, transicao_entrada: null, efeitos: [], midia: "m1", instante_s: 12, instante_pts: 360, duracao_s: 3 };
    expect(bordasDoClipe(cong)).toEqual([0, 3]);
  });

  it("ocultação da mídia antes dos efeitos do clipe; só os ativos; erro de um não derruba os outros", () => {
    const { p, c } = projeto();
    const oc: EfOcultar = { ...base, id: "oc", tipo: "ocultar", modo: "solido", alvo: { rastreio: "r1", forma: "retangulo", vertices: null, margem: 0, borda_suave: 0, inverter: false }, intensidade: 1, cor: "#000000ff", conteudo: "placa", motivo: "", ativo: { de: 0, ate: 100 } };
    (p.midias[0] as MidiaRef).ocultacoes = [oc];
    // o efeito com erro: ocultar por silhueta (Fase 5), que nenhum pacote da Fase 1 converte
    const silhueta: EfOcultar = { ...oc, id: "lu", ativo: null, alvo: { ...oc.alvo, forma: "silhueta" } };
    c.efeitos = [destaque("d1"), silhueta, destaque("d2", { de: 15, ate: null })];
    const cena = cenaDoClipe(p, c, inst(12, 360), { w: 1280, h: 720 }, fontes);
    expect(cena.primitivas.map((x) => x.k)).toEqual(["regiao", "caminho"]);
    expect(cena.erros).toEqual([{ efeito: "lu", codigo: "nao_suportado" }]);
    // entre as âncoras: interpolada
    const r = cena.quadro.rastreio("r1");
    expect(r?.x).toBeCloseTo(0.3, 12);
    // antes da primeira âncora: nada
    expect(cenaDoClipe(p, c, inst(10.5, 315), { w: 1280, h: 720 }, fontes).primitivas.map((x) => x.k)).toEqual([]);
  });

  it("congelado: rastreio de base fonte amostrado no instante dele; efeitos no tempo local", () => {
    const { p } = projeto();
    const cong: ClipeCongelar = { tipo: "congelar", id: "g", rotulo: "", desligado: false, transicao_entrada: null, efeitos: [destaque("d", { de: 1, ate: null })], midia: "m1", instante_s: 13, instante_pts: 390, duracao_s: 3 };
    p.principal = [cong];
    // t local 0,5: o destaque (de 1 s) ainda não vale
    expect(cenaDoClipe(p, cong, inst(0.5), { w: 1280, h: 720 }, fontes).primitivas).toEqual([]);
    const cena = cenaDoClipe(p, cong, inst(2), { w: 1280, h: 720 }, fontes);
    expect(cena.primitivas).toHaveLength(1);
    expect(cena.quadro.rastreio("r1")?.x).toBe(0.5);
  });

  it("par (t, pts) gravado: trecho = quadro da fonte; congelado = instante dele (base fonte) ou tempo local sem pts", () => {
    const { p, c } = projeto();
    const cong: ClipeCongelar = { tipo: "congelar", id: "g", rotulo: "", desligado: false, transicao_entrada: null, efeitos: [], midia: "m1", instante_s: 13, instante_pts: 390, duracao_s: 3 };
    // o palco dá, no congelado, o tempo local com o pts do quadro parado
    const noCong = { t: 1.5, pts: 390 };
    expect(instanteDaAncora(c, "fonte", { t: 12, pts: 360 })).toEqual({ t: 12, pts: 360 });
    expect(instanteDaChave(c, { t: 12, pts: 360 })).toEqual({ t: 12, pts: 360 });
    expect(instanteDaAncora(cong, "fonte", noCong)).toEqual({ t: 13, pts: 390 });
    expect(instanteDaAncora(cong, "local", noCong)).toEqual({ t: 1.5, pts: null });
    expect(instanteDaChave(cong, noCong)).toEqual({ t: 1.5, pts: null });
    // âncora marcada no congelado entra na ordem do tempo de fonte (não some no meio da lista com t local)
    const r = p.rastreios[0]!;
    const v = { x: 0.9, y: 0.2, w: 0.2, h: 0.2 };
    const ks = porChave(r.ancoras, { ...instanteDaAncora(cong, "fonte", noCong), v });
    expect(ks.map((k) => k.t)).toEqual([11, 13]);
    expect(ks[1]!.v.x).toBe(0.9);
    // duas chaves de região em tempos locais diferentes do congelado: ficam as duas
    const e = { ...base, id: "e", tipo: "enquadramento", ajuste: "conter", interpolacao: "bicubica", regiao: [{ t: 0, pts: null, v: { x: 0, y: 0, w: 1, h: 1 } }] } as unknown as Parameters<typeof comRegiao>[0];
    const k1 = instanteDaChave(cong, { t: 1, pts: 390 });
    const k2 = instanteDaChave(cong, { t: 2, pts: 390 });
    const e2 = comRegiao(comRegiao(e, { x: 0.1, y: 0.1, w: 0.5, h: 0.5 }, k1.t, k1.pts), { x: 0.2, y: 0.2, w: 0.3, h: 0.3 }, k2.t, k2.pts);
    expect((e2.regiao as { t: number }[]).map((k) => k.t)).toEqual([0, 1, 2]);
  });
});

/** Contexto 2D falso: registra chamadas e o estado na hora de cada pintura. */
function ctxFalso(w: number, h: number) {
  const log: string[] = [];
  const st = { fillStyle: "", strokeStyle: "", lineWidth: 0, globalAlpha: 1, imageSmoothingEnabled: true, lineJoin: "", lineCap: "" };
  const ctx = new Proxy(st as unknown as Record<string, unknown>, {
    get(alvo, k: string) {
      if (k in alvo) return alvo[k];
      if (k === "canvas") return { width: w, height: h };
      return (...a: unknown[]) => {
        log.push(`${k}(${a.map((x) => (typeof x === "number" ? +x.toFixed(4) : typeof x === "object" ? "obj" : String(x))).join(",")})${k === "fill" || k === "stroke" || k === "fillRect" ? ` ${k === "stroke" ? st.strokeStyle + "/" + st.lineWidth : st.fillStyle}` : ""}`);
      };
    },
    set(alvo, k: string, v) {
      alvo[k] = v;
      return true;
    },
  }) as unknown as CanvasRenderingContext2D;
  return { ctx, log };
}

describe("pintor", () => {
  it("fundo, vídeo com A em px, traço com contorno por baixo", () => {
    const { ctx, log } = ctxFalso(1280, 720);
    const video = {} as CanvasImageSource;
    const prim: Primitiva = { k: "caminho", d: [["M", 0.1, 0.1], ["L", 0.2, 0.1], ["Z"]], traco: { cor: "#ffcc00", largura: 0.006, contorno: "#000000bf" } };
    pintar({ ctx, w: 1280, h: 720, fundo: "#101010", video, A: [0.5, 0, 0, 0.5, 0.25, 0.25], primitivas: [prim], imagem: () => null });
    expect(log[0]).toBe("setTransform(1,0,0,1,0,0)");
    expect(log[1]).toBe("fillRect(0,0,1280,720) #101010");
    expect(log).toContain("setTransform(640,0,0,360,320,180)");
    expect(log).toContain("drawImage(obj,0,0,1,1)");
    const tracos = log.filter((l) => l.startsWith("stroke("));
    expect(tracos).toEqual([`stroke() #000000bf/${0.006 * 720 * CONTORNO}`, `stroke() #ffcc00/${0.006 * 720}`]);
  });

  it("região sólida invertida enche tudo menos a forma (evenodd); sprite na caixa; sprite que não chegou não pinta", () => {
    const { ctx, log } = ctxFalso(100, 50);
    const forma: Primitiva = { k: "caminho", d: [["M", 0.2, 0.2], ["L", 0.4, 0.2], ["L", 0.4, 0.4], ["Z"]] };
    pintar({
      ctx,
      w: 100,
      h: 50,
      fundo: "#000",
      video: null,
      A: [1, 0, 0, 1, 0, 0],
      primitivas: [
        { k: "regiao", modo: "solido", forma, intensidade: 1, cor: "#123456ff", borda_suave: 0, inverter: true, espaco: "fonte" },
        { k: "sprite", src: "s.png", caixa: { x: 0.5, y: 0.5, w: 0.1, h: 0.2 }, opacidade: 0.5 },
        { k: "sprite", src: "falta.png", caixa: { x: 0, y: 0, w: 1, h: 1 }, opacidade: 1 },
      ],
      imagem: (s) => (s === "s.png" ? ({} as CanvasImageSource) : null),
    });
    expect(log).toContain("rect(0,0,100,50)");
    expect(log).toContain("fill(evenodd) #123456ff");
    expect(log).toContain("drawImage(obj,50,25,10,10)");
    expect(log.filter((l) => l.startsWith("drawImage")).length).toBe(1);
  });
});
