/**
 * Só para testes: lê as fixtures que o Rust grava em `src-tauri/tests/fixtures/edicao/` e compara bit a bit
 * (`Object.is`: −0 ≠ 0; chaves iguais nos dois lados).
 */

import { existsSync, readdirSync, readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";

/** `SICRO_FIXTURES_EDICAO` troca a pasta (conferir contra as fixtures de outra cópia). */
export const PASTA_FIXTURES = process.env.SICRO_FIXTURES_EDICAO?.replace(/\/?$/, "/") ?? fileURLToPath(new URL("../../../../src-tauri/tests/fixtures/edicao/", import.meta.url));

export interface Caso<E = unknown, S = unknown> {
  nome: string;
  entrada: E;
  saida: S;
}

export const existe = (nome: string) => existsSync(PASTA_FIXTURES + nome);
export const lerTexto = (nome: string) => readFileSync(PASTA_FIXTURES + nome, "utf8");
export const ler = <T = unknown>(nome: string): T => JSON.parse(lerTexto(nome)) as T;
export const casos = <E, S>(nome: string): Caso<E, S>[] => ler<Caso<E, S>[]>(nome);
/** Arquivos da pasta com esse prefixo. */
export const comPrefixo = (prefixo: string) => (existsSync(PASTA_FIXTURES) ? readdirSync(PASTA_FIXTURES).filter((n) => n.startsWith(prefixo) && n.endsWith(".json")) : []);

/** Primeira diferença entre `a` (calculado) e `b` (fixture), com o caminho; null = iguais. */
export function diferenca(a: unknown, b: unknown, caminho = "$"): string | null {
  if (typeof a === "bigint" && typeof b === "number") return a === BigInt(b) ? null : `${caminho}: ${a} ≠ ${b}`;
  if (typeof a === "number" || typeof b === "number") return Object.is(a, b) ? null : `${caminho}: ${String(a)} ≠ ${String(b)}`;
  if (a === null || b === null || typeof a !== "object" || typeof b !== "object") return a === b ? null : `${caminho}: ${JSON.stringify(a)} ≠ ${JSON.stringify(b)}`;
  if (Array.isArray(a) !== Array.isArray(b)) return `${caminho}: lista × objeto`;
  if (Array.isArray(a) && Array.isArray(b)) {
    if (a.length !== b.length) return `${caminho}: ${a.length} itens ≠ ${b.length}`;
    for (let i = 0; i < a.length; i++) {
      const d = diferenca(a[i], b[i], `${caminho}[${i}]`);
      if (d) return d;
    }
    return null;
  }
  const ka = Object.keys(a as object).filter((k) => (a as Record<string, unknown>)[k] !== undefined).sort();
  const kb = Object.keys(b as object).sort();
  if (ka.join(",") !== kb.join(",")) return `${caminho}: chaves {${ka.join(",")}} ≠ {${kb.join(",")}}`;
  for (const k of ka) {
    const d = diferenca((a as Record<string, unknown>)[k], (b as Record<string, unknown>)[k], `${caminho}.${k}`);
    if (d) return d;
  }
  return null;
}

/** Roda `f` em cada caso e junta as diferenças (uma linha por caso que falhou). */
export function conferirCasos<E, S>(lista: Caso<E, S>[], f: (e: E) => unknown): string[] {
  const erros: string[] = [];
  for (const c of lista) {
    let obtido: unknown;
    try {
      obtido = f(c.entrada);
    } catch (e) {
      erros.push(`${c.nome}: exceção ${String(e)}`);
      continue;
    }
    const d = diferenca(obtido, c.saida);
    if (d) erros.push(`${c.nome}: ${d}`);
  }
  return erros;
}
