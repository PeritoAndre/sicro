/**
 * Lupa da prévia viva (P1.2): `pintarLupa` põe a janela de origem do vídeo no destino, recortada na forma; o espelho
 * dá a primitiva que o pintor recebe (a conferência bit a bit é a de `espelhos.test.ts`).
 */

import { describe, expect, it } from "vitest";
import type { EfLupa, Primitiva, QuadroPalco } from "@domain/edicao";
import { pintar, type Pintura } from "./pintar";
import { pintarLupa, primitivasLupa } from "./lupa";
import { primitivas } from "./primitivas";
import { ponto, type Afim } from "../../modelo/geometria";

type Chamada = [string, ...unknown[]];

/** Contexto 2D falso: guarda as chamadas e a transformação vigente em cada `drawImage`. */
function ctxFalso() {
  const chamadas: Chamada[] = [];
  let t = [1, 0, 0, 1, 0, 0];
  const pilha: number[][] = [];
  const ctx = new Proxy({} as Record<string, unknown>, {
    get(_, k: string) {
      if (k === "chamadas") return chamadas;
      return (...a: unknown[]) => {
        if (k === "setTransform") t = a as number[];
        if (k === "save") pilha.push(t);
        if (k === "restore") t = pilha.pop() ?? [1, 0, 0, 1, 0, 0];
        chamadas.push(k === "drawImage" ? [k, ...a, t] : [k, ...a]);
      };
    },
    set: () => true,
  });
  return { ctx: ctx as unknown as CanvasRenderingContext2D, chamadas };
}

const video = {} as CanvasImageSource;
const pintura = (ctx: CanvasRenderingContext2D, primitivas: Primitiva[] = [], comVideo = true): Pintura => ({
  ctx,
  w: 1280,
  h: 720,
  fundo: "#000000",
  video: comVideo ? video : null,
  A: [1, 0, 0, 1, 0, 0],
  primitivas,
  imagem: () => null,
});

const lupa = (forma: "circulo" | "retangulo"): Extract<Primitiva, { k: "lupa" }> => ({
  k: "lupa",
  origem: { x: 0.4, y: 0.3, w: 0.1, h: 0.2 },
  destino: { x: 0.6, y: 0.1, w: 0.3, h: 0.5 },
  forma,
});

describe("pintarLupa", () => {
  it("a janela de origem cai no destino: canto e lado pela transformação do drawImage", () => {
    const { ctx, chamadas } = ctxFalso();
    pintarLupa(pintura(ctx), lupa("retangulo"));
    const d = chamadas.find((c) => c[0] === "drawImage")!;
    expect(d.slice(1, 6)).toEqual([video, 0, 0, 1, 1]);
    const [a, , , dd, e, f] = d[6] as [number, number, number, number, number, number];
    // ponto (u, v) da fonte → (a·u + e, d·v + f) px
    const px = (u: number, v: number): [number, number] => [a * u + e, dd * v + f];
    const [x0, y0] = px(0.4, 0.3);
    const [x1, y1] = px(0.5, 0.5);
    expect([x0, y0, x1, y1].map((v) => Math.round(v * 1e6) / 1e6)).toEqual([0.6 * 1280, 0.1 * 720, 0.9 * 1280, 0.6 * 720]);
    // recorte no destino, antes de desenhar; estado devolvido
    const nomes = chamadas.map((c) => c[0]);
    expect(nomes).toEqual(["save", "beginPath", "rect", "clip", "setTransform", "drawImage", "setTransform", "restore"]);
    expect(chamadas[2]!.slice(1)).toEqual([0.6 * 1280, 0.1 * 720, 0.3 * 1280, 0.5 * 720]);
  });

  it("vídeo girado ou espelhado: a janela cai no destino na orientação do vídeo", () => {
    const o = { x: 0.4, y: 0.3, w: 0.1, h: 0.2 };
    // A do vídeo: giro horário de 90° reduzido a 0,5 e centrado; espelho horizontal; 180°
    const casos: Afim[] = [
      [0, 0.5, -0.5, 0, 0.75, 0.25],
      [-1, 0, 0, 1, 1, 0],
      [-1, 0, 0, -1, 1, 1],
      [0, -1, 1, 0, 0, 1],
    ];
    for (const A of casos) {
      const { ctx, chamadas } = ctxFalso();
      pintarLupa({ ...pintura(ctx), A }, { ...lupa("retangulo"), origem: o });
      const t = chamadas.find((c) => c[0] === "drawImage")![6] as Afim;
      // ponto (u, v) da fonte → px do canvas pela transformação do drawImage
      const px = (u: number, v: number) => [t[0] * u + t[2] * v + t[4], t[1] * u + t[3] * v + t[5]] as const;
      // a janela ocupa o destino inteiro
      const cantos = [px(0.4, 0.3), px(0.5, 0.3), px(0.5, 0.5), px(0.4, 0.5)];
      const xs = cantos.map((q) => q[0]);
      const ys = cantos.map((q) => q[1]);
      [Math.min(...xs), Math.min(...ys), Math.max(...xs), Math.max(...ys)].forEach((v, i) => expect(v).toBeCloseTo([0.6 * 1280, 0.1 * 720, 0.9 * 1280, 0.6 * 720][i]!, 6));
      // e cada canto dela vai para o canto do destino que A manda: o mesmo lado (esquerda/direita, cima/baixo) do vídeo
      const noVideo = (u: number, v: number) => ponto(A, { x: u, y: v });
      const centroV = noVideo(0.45, 0.4);
      const centroL = px(0.45, 0.4);
      expect(centroL[0]).toBeCloseTo(0.75 * 1280, 6);
      expect(centroL[1]).toBeCloseTo(0.35 * 720, 6);
      for (const [u, v] of [[0.4, 0.3], [0.5, 0.3], [0.4, 0.5]] as const) {
        const pv = noVideo(u, v);
        const pl = px(u, v);
        expect(Math.sign(pl[0] - centroL[0])).toBe(Math.sign(pv.x - centroV.x));
        expect(Math.sign(pl[1] - centroL[1])).toBe(Math.sign(pv.y - centroV.y));
      }
    }
  });

  it("círculo recorta pela elipse inscrita no destino", () => {
    const { ctx, chamadas } = ctxFalso();
    pintarLupa(pintura(ctx), lupa("circulo"));
    const el = chamadas.find((c) => c[0] === "ellipse")!;
    [0.75 * 1280, 0.35 * 720, 0.15 * 1280, 0.25 * 720].forEach((v, i) => expect(el[i + 1] as number).toBeCloseTo(v, 9));
    expect(chamadas.some((c) => c[0] === "rect")).toBe(false);
  });

  it("o que o quadro oculta sai tapado também na ampliação (como no render)", () => {
    // sólido sobre (0,42..0,46 × 0,35..0,45) da fonte, dentro da janela (0,4..0,5 × 0,3..0,5); holofote não entra
    const solido: Primitiva = {
      k: "regiao", modo: "solido", cor: "#102030ff", intensidade: 1, borda_suave: 0, inverter: false, espaco: "fonte",
      forma: { k: "caminho", d: [["M", 0.42, 0.35], ["L", 0.46, 0.35], ["L", 0.46, 0.45], ["L", 0.42, 0.45], ["Z"]] },
    };
    const holofote: Primitiva = { ...solido, modo: "escurecer", espaco: "canvas" };
    const mosaico: Primitiva = { ...solido, modo: "mosaico", inverter: true, forma: { k: "elipse", cx: 0.45, cy: 0.4, rx: 0.02, ry: 0.05 } };
    const { ctx, chamadas } = ctxFalso();
    const estilos: unknown[] = [];
    const comEstilo = new Proxy(ctx as unknown as Record<string, unknown>, {
      get: (o, k: string) => o[k],
      set: (_, k: string, v) => (k === "fillStyle" ? (estilos.push(v), true) : true),
    }) as unknown as CanvasRenderingContext2D;
    pintarLupa(pintura(comEstilo, [solido, holofote, mosaico]), lupa("circulo"));
    const nomes = chamadas.map((c) => c[0]);
    // depois do vídeo, ainda dentro do recorte: duas regiões da fonte tapadas, a do canvas não
    expect(nomes.filter((n) => n === "fill")).toHaveLength(2);
    expect(nomes.indexOf("fill")).toBeGreaterThan(nomes.indexOf("drawImage"));
    expect(nomes.lastIndexOf("fill")).toBeLessThan(nomes.lastIndexOf("restore"));
    expect(estilos).toEqual(["#102030ff", "#808080"]);
    // o sólido cai onde a janela põe a região: (0,42; 0,35) da fonte → destino (0,6 + 0,2 × 0,3; 0,1 + 0,25 × 0,5)
    const m = chamadas.find((c) => c[0] === "moveTo")!;
    expect(m[1] as number).toBeCloseTo((0.6 + 0.2 * 0.3) * 1280, 6);
    expect(m[2] as number).toBeCloseTo((0.1 + 0.25 * 0.5) * 720, 6);
    const fim = chamadas.filter((c) => c[0] === "lineTo")[1]!;
    expect(fim[1] as number).toBeCloseTo((0.6 + 0.6 * 0.3) * 1280, 6);
    expect(fim[2] as number).toBeCloseTo((0.1 + 0.75 * 0.5) * 720, 6);
    // invertido: tudo menos a forma, dentro do destino
    const fills = chamadas.filter((c) => c[0] === "fill");
    expect(fills.map((c) => c[1])).toEqual(["nonzero", "evenodd"]);
    // (o recorte da lupa é a elipse: o único retângulo é o do invertido)
    expect(chamadas.filter((c) => c[0] === "rect").map((c) => c.slice(1))).toEqual([[0.6 * 1280, 0.1 * 720, 0.3 * 1280, 0.5 * 720]]);
    expect(chamadas.filter((c) => c[0] === "ellipse")).toHaveLength(2);
  });

  it("sem vídeo (título, imagem) ou com caixa vazia, nada", () => {
    const { ctx, chamadas } = ctxFalso();
    pintarLupa(pintura(ctx, [], false), lupa("circulo"));
    pintarLupa(pintura(ctx), { ...lupa("circulo"), origem: { x: 0, y: 0, w: 0, h: 0.1 } });
    expect(chamadas).toEqual([]);
  });

  it("o pintor chama a lupa no lugar dela na lista (depois do vídeo, antes da moldura)", () => {
    const e: EfLupa = { id: "l1", ligado: true, nome: "", ativo: null, tipo: "lupa", rastreio: "r1", janela: 0.2, fator: 2, forma: "circulo", interpolacao: "bilinear",
      onde: { modo: "fixa", x: 0.8, y: 0.3 }, moldura: { cor: "#00ff00", espessura: 0.004, contorno: false }, ligacao: true };
    const q: QuadroPalco = { q: 0, t: 0, pts: null, indice: null, programa_s: 0, A: [1, 0, 0, 1, 0, 0], canvas: { w: 1280, h: 720 },
      rastreio: () => ({ x: 0.4, y: 0.4, w: 0.05, h: 0.1 }), sprite: () => null, atlas: () => null, midia: () => null };
    const p = primitivas(e, q);
    expect(p.map((x) => x.k)).toEqual(["lupa", "elipse", "elipse", "caminho"]);
    expect(primitivasLupa(e, q)).toEqual(p);
    const { ctx, chamadas } = ctxFalso();
    pintar(pintura(ctx, p));
    const imagens = chamadas.flatMap((c, i) => (c[0] === "drawImage" ? [i] : []));
    const traco = chamadas.findIndex((c) => c[0] === "stroke");
    // vídeo, lupa e só depois a moldura
    expect(imagens).toHaveLength(2);
    expect(traco).toBeGreaterThan(imagens[1]!);
  });

  it("parâmetro fora da faixa é item_invalido (como o Rust)", () => {
    const e = { tipo: "lupa", janela: 0, fator: 2, onde: { modo: "fixa", x: 0, y: 0 } } as unknown as EfLupa;
    const q = { rastreio: () => null } as unknown as QuadroPalco;
    expect(() => primitivasLupa(e, q)).toThrow(/^item_invalido: janela/);
    expect(() => primitivasLupa({ ...e, janela: 0.2, fator: 8.5 }, q)).toThrow(/^item_invalido: fator/);
    expect(primitivasLupa({ ...e, janela: 0.2 }, q)).toEqual([]);
  });
});
