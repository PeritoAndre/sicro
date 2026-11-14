/** Tema claro: o âmbar de aviso tem valor próprio e se lê sobre as superfícies claras (contratos (h) 27). */

import { readFileSync } from "node:fs";
import { join } from "node:path";
import { fileURLToPath } from "node:url";
import { describe, expect, it } from "vitest";

const css = readFileSync(join(fileURLToPath(new URL(".", import.meta.url)), "tokens.css"), "utf8");
const bloco = (sel: string) => css.slice(css.indexOf(sel), css.indexOf("}", css.indexOf(sel)));
const cor = (b: string, nome: string) => new RegExp(`${nome}:\\s*(#[0-9a-f]{6})`, "i").exec(b)?.[1] ?? null;

function luminancia(hex: string): number {
  const c = [1, 3, 5].map((i) => parseInt(hex.slice(i, i + 2), 16) / 255).map((v) => (v <= 0.03928 ? v / 12.92 : ((v + 0.055) / 1.055) ** 2.4));
  return 0.2126 * c[0]! + 0.7152 * c[1]! + 0.0722 * c[2]!;
}
const contraste = (a: string, b: string) => {
  const [x, y] = [luminancia(a), luminancia(b)].sort((p, q) => q - p);
  return (x! + 0.05) / (y! + 0.05);
};

describe("tokens", () => {
  it("--sicro-warning no tema claro: valor próprio, contraste de texto (4,5:1) nas superfícies claras", () => {
    const claro = bloco(':root[data-theme="light"]');
    const aviso = cor(claro, "--sicro-warning");
    expect(aviso).not.toBeNull();
    for (const sup of ["--sicro-bg", "--sicro-surface-1", "--sicro-surface-2"]) {
      expect(contraste(aviso!, cor(claro, sup)!), sup).toBeGreaterThanOrEqual(4.5);
    }
  });
});
