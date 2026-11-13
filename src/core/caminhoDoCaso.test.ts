import { describe, expect, it } from "vitest";
import { caminhoDoCaso, relativoNovo } from "./caminhoDoCaso";

describe("relativoNovo", () => {
  it("mapeia prefixos antigos", () => {
    expect(relativoNovo("croquis/backgrounds/x.png")).toBe("Sistema/croquis/backgrounds/x.png");
    expect(relativoNovo("croquis/exports/a.png")).toBe("Exportações/Croquis/a.png");
    expect(relativoNovo("videos/originais/y.mp4")).toBe("Mídias/Vídeos/y.mp4");
    expect(relativoNovo("videos/frames/f.png")).toBe("Sistema/videos/frames/f.png");
    expect(relativoNovo("imagens/camadas/c.png")).toBe("Sistema/imagens/camadas/c.png");
    expect(relativoNovo("sicro.sqlite")).toBe("sicro.sqlite");
  });
  it("separa JSON e saída das montagens", () => {
    expect(relativoNovo("videos/montagens/m.json")).toBe("Sistema/videos/montagens/m.json");
    expect(relativoNovo("videos/montagens/m.mp4")).toBe("Exportações/Vídeos/Montagens/m.mp4");
  });
  it("deixa o que já é novo", () => {
    expect(relativoNovo("Sistema/croquis/x.png")).toBe("Sistema/croquis/x.png");
    expect(relativoNovo("Mídias/Vídeos/y.mp4")).toBe("Mídias/Vídeos/y.mp4");
    expect(relativoNovo("croquisx/y")).toBe("croquisx/y");
  });
});

describe("caminhoDoCaso", () => {
  it("junta com o separador do SO", () => {
    expect(caminhoDoCaso("/a/Caso.sicro/", "videos/originais/y.mp4")).toBe("/a/Caso.sicro/Mídias/Vídeos/y.mp4");
    expect(caminhoDoCaso("C:\\a\\Caso.sicro", "croquis/thumbs/1.png")).toBe("C:\\a\\Caso.sicro\\Sistema\\croquis\\thumbs\\1.png");
  });
});
