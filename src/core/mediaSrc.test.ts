import { describe, expect, it } from "vitest";

import { toFileUrl } from "./mediaSrc";

describe("toFileUrl", () => {
  it("monta file:// absoluto escapando cada trecho do caminho", () => {
    expect(toFileUrl("/home/andre/SICRO/Casos/x.sicro/videos/originais/a b#1?.mp4")).toBe(
      "file:///home/andre/SICRO/Casos/x.sicro/videos/originais/a%20b%231%3F.mp4",
    );
  });

  it("preserva acentos como UTF-8 escapado", () => {
    expect(toFileUrl("/tmp/vídeo.mp4")).toBe("file:///tmp/v%C3%ADdeo.mp4");
  });
});
