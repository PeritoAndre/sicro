import { describe, expect, it } from "vitest";
import { folhaEmPx, nomePadrao, pxPorMetroParaCena } from "./novoCroqui";
import { quadkey, urlDoTile, mapaBasePorId } from "./mapasBase";
import { caixaDoRaio, lonLatParaPixel, metrosPorPixel, zoomPara } from "./mercator";

describe("novo croqui: folha em metros", () => {
  it("30 × 40 m cabe no A4 a 300 dpi", () => {
    expect(pxPorMetroParaCena(30, 40)).toBe(87);
    expect(folhaEmPx(30, 40)).toEqual({ width_px: 2610, height_px: 3480, px_per_m: 87 });
  });
  it("cena grande baixa a resolução, nunca abaixo de 1 px/m", () => {
    expect(pxPorMetroParaCena(60, 150)).toBe(23);
    expect(pxPorMetroParaCena(5000, 5000)).toBe(1);
  });
  it("nome padrão pula os que já existem", () => {
    expect(nomePadrao([])).toBe("Croqui viário 1");
    expect(nomePadrao(["Croqui viário 1", "Croqui viário 2"])).toBe("Croqui viário 3");
  });
});

describe("mapas base", () => {
  it("quadkey do Bing e endereço do tile", () => {
    expect(quadkey(3, 3, 5)).toBe("213");
    expect(urlDoTile(mapaBasePorId("bing")!, 3, 3, 5)).toContain("/tiles/a213.jpeg");
    expect(urlDoTile(mapaBasePorId("google-satelite")!, 15, 10, 20)).toBe("https://mt2.google.com/vt/lyrs=s&x=10&y=20&z=15");
  });
});

describe("mercator", () => {
  it("pixel do (0,0) é o centro do mundo", () => {
    const p = lonLatParaPixel(0, 0, 1);
    expect(p).toEqual({ x: 256, y: 256 });
  });
  it("zoom sobe até a resolução pedida ou o máximo", () => {
    expect(metrosPorPixel(0, 20)).toBeCloseTo(0.149, 3);
    expect(zoomPara(0, 3, 20)).toBe(19);
    expect(zoomPara(0, 5, 20)).toBe(20);
    expect(zoomPara(0, 50, 17)).toBe(17);
  });
  it("caixa do raio é simétrica", () => {
    const c = caixaDoRaio(0, -51, 150);
    expect(c.max_lat - c.min_lat).toBeCloseTo(2 * (150 / 6_371_000) * (180 / Math.PI), 9);
    expect((c.max_lon + c.min_lon) / 2).toBeCloseTo(-51, 9);
  });
});
