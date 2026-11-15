/** Pintor do palco: traço de largura zero não desenha (como o rasterizador do Rust), com contexto 2D falso. */

import { describe, expect, it } from "vitest";
import type { Primitiva } from "@domain/edicao";
import { pintar, type Pintura } from "./pintar";

/** Contexto 2D falso: guarda as chamadas e as propriedades postas. */
function ctxFalso() {
  const chamadas: [string, ...unknown[]][] = [];
  const postas: Record<string, unknown> = {};
  const ctx = new Proxy({} as Record<string, unknown>, {
    get: (_, k: string) => (k in postas ? postas[k] : (...a: unknown[]) => void chamadas.push([k, ...a])),
    set: (_, k: string, v) => ((postas[k] = v), chamadas.push([`=${k}`, v]), true),
  });
  return { ctx: ctx as unknown as CanvasRenderingContext2D, chamadas };
}

const pintura = (ctx: CanvasRenderingContext2D, primitivas: Primitiva[]): Pintura => ({ ctx, w: 1280, h: 720, fundo: "#000000", video: null, A: [1, 0, 0, 1, 0, 0], primitivas, imagem: () => null });
const caminho = (largura: number, contorno?: string): Primitiva => ({ k: "caminho", d: [["M", 0.1, 0.1], ["L", 0.9, 0.9]], traco: { cor: "#ff0000", largura, ...(contorno ? { contorno } : {}) } });
const tracos = (c: [string, ...unknown[]][]) => c.filter((x) => x[0] === "stroke").length;
const larguras = (c: [string, ...unknown[]][]) => c.filter((x) => x[0] === "=lineWidth").map((x) => x[1]);

describe("pintor: traço", () => {
  it("largura positiva traça (contorno e cor); largura zero ou negativa não traça nem põe largura", () => {
    const a = ctxFalso();
    pintar(pintura(a.ctx, [caminho(0.004, "#000000")]));
    expect(tracos(a.chamadas)).toBe(2);
    expect(larguras(a.chamadas)).toHaveLength(2);
    expect(larguras(a.chamadas)[1]).toBeCloseTo(0.004 * 720, 9);
    for (const largura of [0, -0.01]) {
      const b = ctxFalso();
      // traçado antes com 3 px: o seguinte, de largura zero, não pode herdar essa largura
      pintar(pintura(b.ctx, [caminho(3 / 720), caminho(largura, "#000000"), { k: "elipse", cx: 0.5, cy: 0.5, rx: 0.1, ry: 0.1, traco: { cor: "#00ff00", largura } }]));
      expect(tracos(b.chamadas), String(largura)).toBe(1);
      expect(larguras(b.chamadas)).toEqual([3]);
    }
  });

  it("o preenchimento não depende do traço", () => {
    const c = ctxFalso();
    pintar(pintura(c.ctx, [{ k: "caminho", d: [["M", 0, 0], ["L", 1, 0], ["L", 1, 1], ["Z"]], preenche: "#112233", traco: { cor: "#ff0000", largura: 0 } }]));
    expect(c.chamadas.filter((x) => x[0] === "fill")).toHaveLength(1);
    expect(tracos(c.chamadas)).toBe(0);
  });
});
