/**
 * "Atualizar" (P2.5): o aviso `retrato_desatualizado` de um rastreio vindo de Pessoas chega com `campo` = id do
 * rastreio (`pessoas_commands::desatualizados`; teste do Rust `desatualizado_quando_a_trilha_muda`).
 */

import { describe, expect, it } from "vitest";
import type { Problema } from "@domain/edicao";
import { rastreioDesatualizado } from "@domain/video_pessoas";

const problema = (codigo: string, campo: string | null): Problema => ({
  nivel: "aviso", codigo, mensagem: "", clipe: null, efeito: null, trilha: null, midia: "m1", campo, n: null,
});

describe("rastreio desatualizado", () => {
  it("só o retrato_desatualizado do próprio rastreio", () => {
    const lista = [problema("retrato_desatualizado", "r1"), problema("retrato_desatualizado", "calibracao"), problema("sem_cobertura", "r2")];
    expect(rastreioDesatualizado(lista, "r1")).toBe(true);
    expect(rastreioDesatualizado(lista, "r2")).toBe(false);
    expect(rastreioDesatualizado(lista, "r")).toBe(false);
    expect(rastreioDesatualizado([], "r1")).toBe(false);
  });
});
