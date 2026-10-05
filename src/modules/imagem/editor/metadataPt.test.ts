import { describe, expect, it } from "vitest";
import { campoPt, grupoPt, valorPt } from "./metadataPt";

describe("metadados em português (tabela do yazi)", () => {
  it("traduz grupo, campo e valor como no yazi", () => {
    expect(grupoPt("IFD0")).toBe("EXIF: imagem principal");
    expect(grupoPt("DJI")).toBe("Notas do fabricante (DJI)");
    expect(grupoPt("XMP-drone-dji")).toMatch(/^XMP: /);
    expect(campoPt("GimbalReverseX")).toBe("Gimbal Reverse X");
    expect(valorPt("2026:05:26 18:04:30.123-03:00")).toBe("26/05/2026 18:04:30,123 (-03:00)");
    expect(valorPt("Yes")).toBe("Sim");
  });
});
