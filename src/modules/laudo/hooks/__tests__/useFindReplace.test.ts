/**
 * useFindReplace — testes da lógica pura de busca.
 *
 * TipTap requer DOM real para instanciar `new Editor(...)`, o que não
 * funciona bem no ambiente Vitest+jsdom para componentes complexos.
 * Aqui testamos apenas as primitivas puras: escape de regex e a
 * construção de pattern com/sem whole-word + case-sensitivity.
 *
 * Nota sobre regex global: como usamos flag `g`, `.test()` avança
 * `lastIndex` e chamadas subsequentes podem retornar `false` por isso.
 * Cada teste cria um regex NOVO (via `buildPattern`) para cada assert.
 */

import { describe, expect, it } from "vitest";

function escapeRegex(str: string): string {
  return str.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
}

function buildPattern(
  query: string,
  opts: { caseSensitive?: boolean; wholeWord?: boolean } = {},
): RegExp | null {
  if (!query) return null;
  const flags = opts.caseSensitive ? "g" : "gi";
  const escaped = escapeRegex(query);
  return opts.wholeWord
    ? new RegExp(`\\b${escaped}\\b`, flags)
    : new RegExp(escaped, flags);
}

describe("useFindReplace — escapeRegex", () => {
  it("escapa todos os metacaracteres", () => {
    expect(escapeRegex(".")).toBe("\\.");
    expect(escapeRegex("*")).toBe("\\*");
    expect(escapeRegex("+")).toBe("\\+");
    expect(escapeRegex("?")).toBe("\\?");
    expect(escapeRegex("^")).toBe("\\^");
    expect(escapeRegex("$")).toBe("\\$");
    expect(escapeRegex("(")).toBe("\\(");
    expect(escapeRegex(")")).toBe("\\)");
    expect(escapeRegex("[")).toBe("\\[");
    expect(escapeRegex("]")).toBe("\\]");
    expect(escapeRegex("{")).toBe("\\{");
    expect(escapeRegex("}")).toBe("\\}");
    expect(escapeRegex("|")).toBe("\\|");
    expect(escapeRegex("\\")).toBe("\\\\");
  });

  it("não toca em caracteres alfanuméricos", () => {
    expect(escapeRegex("abc123")).toBe("abc123");
    expect(escapeRegex("perícia")).toBe("perícia");
  });

  it("combina múltiplos metas + texto", () => {
    expect(escapeRegex("12.50 (a+b)")).toBe("12\\.50 \\(a\\+b\\)");
  });
});

describe("useFindReplace — buildPattern", () => {
  it("query vazia → null", () => {
    expect(buildPattern("")).toBeNull();
  });

  it("default é case-insensitive", () => {
    expect(buildPattern("perícia")?.flags.includes("i")).toBe(true);
    expect(buildPattern("perícia")?.test("Perícia")).toBe(true);
    expect(buildPattern("perícia")?.test("PERÍCIA")).toBe(true);
  });

  it("caseSensitive=true não casa maiúsculas diferentes", () => {
    expect(
      buildPattern("perícia", { caseSensitive: true })?.flags.includes("i"),
    ).toBe(false);
    expect(buildPattern("perícia", { caseSensitive: true })?.test("perícia")).toBe(
      true,
    );
    expect(buildPattern("perícia", { caseSensitive: true })?.test("PERÍCIA")).toBe(
      false,
    );
  });

  it("wholeWord distingue 'perícia' de 'perícias'", () => {
    expect(buildPattern("perícia", { wholeWord: true })?.source).toContain(
      "\\b",
    );
    expect(buildPattern("perícia", { wholeWord: true })?.test("perícia.")).toBe(
      true,
    );
    expect(buildPattern("perícia", { wholeWord: true })?.test("perícias")).toBe(
      false,
    );
  });

  it("metas regex na query são tratados como literais", () => {
    expect(buildPattern("12.50")?.test("12.50")).toBe(true);
    // Se não escapasse, "." casaria com "X" — verificamos que não casa.
    expect(buildPattern("12.50")?.test("12X50")).toBe(false);
  });

  it("contagem global de matches em texto longo", () => {
    const p = buildPattern("perícia");
    const text = "A perícia foi feita. A perícia confirmou. A perícia.";
    if (!p) throw new Error("pattern null");
    const matches: RegExpExecArray[] = [];
    let m: RegExpExecArray | null;
    while ((m = p.exec(text)) !== null) {
      matches.push(m);
      if (p.lastIndex === m.index) p.lastIndex += 1;
    }
    expect(matches.length).toBe(3);
  });

  it("acentos unicode são preservados", () => {
    expect(buildPattern("análise")?.test("a análise técnica")).toBe(true);
    // "analise" sem acento NÃO deve casar.
    expect(buildPattern("análise")?.test("a analise técnica")).toBe(false);
  });

  it("combinação wholeWord + caseSensitive", () => {
    const o = { caseSensitive: true, wholeWord: true };
    expect(buildPattern("Perito", o)?.test("O Perito chegou.")).toBe(true);
    expect(buildPattern("Perito", o)?.test("perito")).toBe(false);
    expect(buildPattern("Perito", o)?.test("Peritos")).toBe(false);
  });
});
