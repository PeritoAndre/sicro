/**
 * Testes da matemática pura do visualizador de confronto.
 */

import { describe, expect, it } from "vitest";
import {
  ZOOM_MAX,
  ZOOM_MIN,
  angleAtDeg,
  clampZoom,
  correspondingAnchor,
  distance,
  fitTransform,
  imageToScreen,
  mmPerPixel,
  pan,
  screenToImage,
  wheelFactor,
  zoomAt,
  zoomTo,
} from "./confrontoView";

describe("correspondingAnchor (zoom travado)", () => {
  it("painéis iguais → âncora == cursor (alinhamento perfeito)", () => {
    const a = correspondingAnchor({ x: 120, y: 80 }, { w: 600, h: 400 }, { w: 600, h: 400 });
    expect(a).toEqual({ x: 120, y: 80 });
  });

  it("painel travado maior/menor → preserva a fração relativa", () => {
    // cursor a 1/4 da largura e 1/2 da altura do painel ativo.
    const a = correspondingAnchor({ x: 150, y: 200 }, { w: 600, h: 400 }, { w: 300, h: 800 });
    expect(a.x).toBeCloseTo(75); // 0.25 * 300
    expect(a.y).toBeCloseTo(400); // 0.5 * 800
  });

  it("não é o centro do painel travado (o bug antigo)", () => {
    const locked = { w: 600, h: 400 };
    const a = correspondingAnchor({ x: 60, y: 40 }, { w: 600, h: 400 }, locked);
    expect(a).not.toEqual({ x: locked.w / 2, y: locked.h / 2 });
  });
});

describe("clampZoom", () => {
  it("mantém valores dentro de [MIN, MAX]", () => {
    expect(clampZoom(0.001)).toBe(ZOOM_MIN);
    expect(clampZoom(9999)).toBe(ZOOM_MAX);
    expect(clampZoom(2)).toBe(2);
  });

  it("trata valores não-finitos como 1 (reset seguro à identidade)", () => {
    expect(clampZoom(NaN)).toBe(1);
    expect(clampZoom(Infinity)).toBe(1);
    expect(clampZoom(-Infinity)).toBe(1);
  });
});

describe("zoomAt", () => {
  it("mantém FIXO o ponto sob o cursor ao dar zoom", () => {
    const view = { scale: 1, x: 0, y: 0 };
    const cursor = { x: 100, y: 50 };
    // ponto de imagem sob o cursor antes do zoom
    const before = screenToImage(view, cursor);
    const zoomed = zoomAt(view, 2, cursor);
    // o mesmo ponto de imagem deve permanecer sob o cursor (em tela)
    const after = imageToScreen(zoomed, before);
    expect(after.x).toBeCloseTo(cursor.x, 5);
    expect(after.y).toBeCloseTo(cursor.y, 5);
    expect(zoomed.scale).toBeCloseTo(2);
  });

  it("respeita os limites de escala (não passa do MAX)", () => {
    const view = { scale: ZOOM_MAX, x: 0, y: 0 };
    const zoomed = zoomAt(view, 4, { x: 10, y: 10 });
    expect(zoomed.scale).toBe(ZOOM_MAX);
  });

  it("não quebra se a escala atual for 0", () => {
    const zoomed = zoomAt({ scale: 0, x: 0, y: 0 }, 2, { x: 5, y: 5 });
    expect(Number.isFinite(zoomed.x)).toBe(true);
    expect(Number.isFinite(zoomed.y)).toBe(true);
  });
});

describe("zoomTo", () => {
  it("define a escala exata mantendo o ponto-âncora fixo", () => {
    const view = { scale: 0.37, x: 12, y: -5 };
    const anchor = { x: 300, y: 200 };
    const before = screenToImage(view, anchor);
    const out = zoomTo(view, 1, anchor);
    expect(out.scale).toBeCloseTo(1);
    const after = imageToScreen(out, before);
    expect(after.x).toBeCloseTo(anchor.x, 4);
    expect(after.y).toBeCloseTo(anchor.y, 4);
  });

  it("permite ampliar até o teto de pixel peeping", () => {
    const out = zoomTo({ scale: 1, x: 0, y: 0 }, 64, { x: 0, y: 0 });
    expect(out.scale).toBe(64);
  });
});

describe("pan", () => {
  it("soma o delta ao offset e preserva a escala", () => {
    const view = { scale: 1.5, x: 10, y: 20 };
    const out = pan(view, { x: 5, y: -8 });
    expect(out).toEqual({ scale: 1.5, x: 15, y: 12 });
  });
});

describe("fitTransform", () => {
  it("enquadra e centraliza a imagem no container (limitado pelo lado maior)", () => {
    // imagem 200x100 num container 400x400 → escala limitada por largura
    const t = fitTransform({ w: 200, h: 100 }, { w: 400, h: 400 }, 1);
    expect(t.scale).toBeCloseTo(2); // 400/200
    // centralizado: x = (400 - 200*2)/2 = 0 ; y = (400 - 100*2)/2 = 100
    expect(t.x).toBeCloseTo(0);
    expect(t.y).toBeCloseTo(100);
  });

  it("usa a altura quando ela é o lado limitante", () => {
    const t = fitTransform({ w: 100, h: 200 }, { w: 400, h: 400 }, 1);
    expect(t.scale).toBeCloseTo(2); // 400/200
    expect(t.x).toBeCloseTo(100);
    expect(t.y).toBeCloseTo(0);
  });

  it("retorna identidade para tamanhos inválidos", () => {
    expect(fitTransform({ w: 0, h: 0 }, { w: 100, h: 100 })).toEqual({
      scale: 1,
      x: 0,
      y: 0,
    });
  });
});

describe("screenToImage / imageToScreen", () => {
  it("são inversos um do outro", () => {
    const view = { scale: 1.7, x: 33, y: -12 };
    const p = { x: 220, y: 140 };
    const round = imageToScreen(view, screenToImage(view, p));
    expect(round.x).toBeCloseTo(p.x, 5);
    expect(round.y).toBeCloseTo(p.y, 5);
  });
});

describe("wheelFactor", () => {
  it("scroll para cima (deltaY<0) aumenta; para baixo diminui", () => {
    expect(wheelFactor(-100)).toBeGreaterThan(1);
    expect(wheelFactor(100)).toBeLessThan(1);
    expect(wheelFactor(0)).toBeCloseTo(1);
  });
});

describe("distance", () => {
  it("calcula a hipotenusa 3-4-5", () => {
    expect(distance({ x: 0, y: 0 }, { x: 3, y: 4 })).toBeCloseTo(5);
  });
  it("é zero para pontos coincidentes", () => {
    expect(distance({ x: 7, y: 7 }, { x: 7, y: 7 })).toBe(0);
  });
});

describe("angleAtDeg", () => {
  it("ângulo reto = 90°", () => {
    const a = { x: 1, y: 0 };
    const v = { x: 0, y: 0 };
    const b = { x: 0, y: 1 };
    expect(angleAtDeg(a, v, b)).toBeCloseTo(90);
  });
  it("raios colineares opostos = 180°", () => {
    expect(angleAtDeg({ x: -1, y: 0 }, { x: 0, y: 0 }, { x: 1, y: 0 })).toBeCloseTo(180);
  });
  it("mesma direção = 0°", () => {
    expect(angleAtDeg({ x: 2, y: 0 }, { x: 0, y: 0 }, { x: 5, y: 0 })).toBeCloseTo(0);
  });
  it("é robusto a vértice coincidente (retorna 0)", () => {
    expect(angleAtDeg({ x: 0, y: 0 }, { x: 0, y: 0 }, { x: 1, y: 1 })).toBe(0);
  });
});

describe("mmPerPixel", () => {
  it("deriva a escala de um comprimento conhecido", () => {
    // 50 mm medidos em 200 px → 0,25 mm/px
    expect(mmPerPixel(50, 200)).toBeCloseTo(0.25);
  });
  it("retorna 0 para comprimento de pixel inválido", () => {
    expect(mmPerPixel(50, 0)).toBe(0);
  });
});
