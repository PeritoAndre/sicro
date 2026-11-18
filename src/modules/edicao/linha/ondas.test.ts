/** Ondas por caso: o aberto e o da medida do Diagnóstico convivem sem pedir de novo; um terceiro tira o mais antigo. */

import { describe, expect, it, vi } from "vitest";

vi.mock("@core/commands", () => ({ commands: {} }));

import type { Onda } from "@domain/edicao";
import { Ondas } from "./ondas";

const onda: Onda = { inicio_s: 0, por_s: 100, min: [0], max: [1] };

describe("ondas", () => {
  it("dois casos alternando: uma vez cada; o terceiro tira o mais antigo", async () => {
    const pedir = vi.fn(async () => onda);
    const o = new Ondas(pedir, () => {});
    expect(o.onda("a", "s")).toBeNull();
    expect(o.onda("b", "s")).toBeNull();
    await Promise.resolve();
    for (let i = 0; i < 3; i++) {
      expect(o.onda("a", "s")).toBe(onda);
      expect(o.onda("b", "s")).toBe(onda);
    }
    expect(pedir).toHaveBeenCalledTimes(2);
    o.onda("c", "s");
    await Promise.resolve();
    // "a" foi o menos recente: sai; "b" fica
    expect(o.onda("b", "s")).toBe(onda);
    expect(o.onda("a", "s")).toBeNull();
    expect(pedir).toHaveBeenCalledTimes(4);
  });
});
