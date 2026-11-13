import { describe, expect, it } from "vitest";
import { versaoMaisNova } from "./updaterStore";

describe("versaoMaisNova", () => {
  it("compara por número, não por texto", () => {
    expect(versaoMaisNova("5.4.0", "5.3.2")).toBe(true);
    expect(versaoMaisNova("5.10.0", "5.9.0")).toBe(true);
    expect(versaoMaisNova("5.3.0", "5.3.0")).toBe(false);
    expect(versaoMaisNova("5.3.0", "5.3.1")).toBe(false);
    expect(versaoMaisNova("6.0", "5.99.99")).toBe(true);
  });
});

describe("mostrarNovidades", () => {
  it("só abre o diálogo quando há versão nova", async () => {
    const { mostrarNovidades, useUpdaterStore } = await import("./updaterStore");
    useUpdaterStore.setState({ status: "none", aviso: false });
    mostrarNovidades();
    expect(useUpdaterStore.getState().aviso).toBe(false);
    useUpdaterStore.setState({ status: "available", aviso: false });
    mostrarNovidades();
    expect(useUpdaterStore.getState().aviso).toBe(true);
  });
});
