/**
 * `src/types/edicao.ts` × `completo.json` (o Rust grava; §2.2, §8.3): o JSON inteiro, como literal, tem de
 * satisfazer `ProjetoEdicao` no compilador (campo a mais, a menos ou de tipo errado = erro), e os itens
 * conhecidos também sem a válvula do `Desconhecido`. E todo tipo de clipe e de efeito aparece nele.
 */

import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import ts from "typescript";
import { describe, expect, it } from "vitest";
import type { ProjetoEdicao } from "@domain/edicao";
import { completar, TIPOS_CLIPE, TIPOS_EFEITO, TIPOS_SOBREPOSTO } from "./projeto";
import { ler, lerTexto } from "./testeFixtures";

const TIPOS = fileURLToPath(new URL("../../../types/edicao.ts", import.meta.url));
// barras para a frente: é como o compilador nomeia os arquivos também no Windows
const VIRTUAL = join(dirname(TIPOS), "__conferencia_completo.ts").replace(/\\/g, "/");

/** Erros do compilador (strict) para um arquivo que importa de `src/types/edicao.ts`. */
function compilar(codigo: string): string[] {
  const opcoes: ts.CompilerOptions = {
    strict: true,
    noEmit: true,
    target: ts.ScriptTarget.ES2022,
    module: ts.ModuleKind.ESNext,
    moduleResolution: ts.ModuleResolutionKind.Bundler,
    skipLibCheck: true,
    types: [],
    lib: ["lib.es2022.d.ts"],
  };
  const host = ts.createCompilerHost(opcoes);
  const ler = host.getSourceFile.bind(host);
  host.getSourceFile = (nome, versao, ...resto) => (nome === VIRTUAL ? ts.createSourceFile(nome, codigo, versao) : ler(nome, versao, ...resto));
  const existe = host.fileExists.bind(host);
  host.fileExists = (nome) => nome === VIRTUAL || existe(nome);
  const programa = ts.createProgram([VIRTUAL], opcoes, host);
  return ts.getPreEmitDiagnostics(programa).map((d) => ts.flattenDiagnosticMessageText(d.messageText, "\n"));
}

type Obj = Record<string, unknown>;
function efeitosDe(p: Obj): unknown[] {
  const saida: unknown[] = [];
  const andar = (v: unknown) => {
    if (Array.isArray(v)) v.forEach(andar);
    else if (v && typeof v === "object") {
      const o = v as Obj;
      if (Array.isArray(o.efeitos)) saida.push(...o.efeitos);
      Object.values(o).forEach(andar);
    }
  };
  andar(p);
  return saida;
}

/** `semFuturo`: tira das listas tipadas os itens de versão futura (a fixture de todas as variantes traz um de cada). */
function fonte(texto: string, semFuturo = false): string {
  const p = JSON.parse(texto) as Obj;
  const sob = (p.sobreposicoes as Obj[]).flatMap((t) => t.clipes as unknown[]);
  const so = (lista: unknown[], tipos: readonly string[]) => (semFuturo ? lista.filter((x) => tipos.includes((x as Obj).tipo as string)) : lista);
  return [
    `import type { ProjetoEdicao, ClipePrincipal, ClipeSobreposto, Efeito } from "./edicao";`,
    `export const projeto = ${texto.trim()} satisfies ProjetoEdicao;`,
    `export const principal = ${JSON.stringify(so(p.principal as unknown[], TIPOS_CLIPE))} satisfies ClipePrincipal[];`,
    `export const sobrepostos = ${JSON.stringify(so(sob, TIPOS_SOBREPOSTO))} satisfies ClipeSobreposto[];`,
    `export const efeitos = ${JSON.stringify(so(efeitosDe(p), TIPOS_EFEITO))} satisfies Efeito[];`,
  ].join("\n");
}

describe("tipos × completo.json", () => {
  const texto = lerTexto("completo.json");
  const p = ler<ProjetoEdicao>("completo.json");

  it("o compilador aceita o completo.json como ProjetoEdicao, sem a válvula do Desconhecido", () => {
    expect(compilar(fonte(texto))).toEqual([]);
  }, 30_000);

  // a outra fixture do Rust traz o que o completo.json não tem: `Option` vazio e as variantes restantes
  it("o compilador aceita contrato-todas-variantes.json (fora os três itens de versão futura)", () => {
    const todas = lerTexto("contrato-todas-variantes.json");
    const futuros = [...todas.matchAll(/"tipo": "(\w+_do_futuro)"/g)].map((m) => m[1]);
    expect(new Set(futuros)).toEqual(new Set(["clipe_do_futuro", "sobreposto_do_futuro", "efeito_do_futuro"]));
    expect(compilar(fonte(todas, true))).toEqual([]);
  }, 30_000);

  it("projeto-minimo.json (campos que o Rust lê com padrão) fica completo com `completar`", () => {
    const minimo = completar(ler<ProjetoEdicao>("projeto-minimo.json"));
    expect(compilar(fonte(JSON.stringify(minimo)))).toEqual([]);
    expect([minimo.saida.auto, minimo.saida.exportacao.pacote, minimo.mixagem.normalizar, minimo.fecho]).toEqual([true, null, null, null]);
    // documento completo: o mesmo objeto
    expect(completar(p)).toBe(p);
  }, 30_000);

  it("a conferência acusa campo a mais, campo faltando, literal errado e efeito errado escondido no Desconhecido", () => {
    const erros = (f: (o: Obj) => void) => {
      const o = JSON.parse(texto) as Obj;
      f(o);
      return compilar(fonte(JSON.stringify(o))).join("\n");
    };
    expect(erros((o) => ((o.saida as Obj).extra = 1))).toMatch(/extra.*does not exist in type 'Saida'/);
    expect(erros((o) => delete (o.mixagem as Obj).limitador)).toMatch(/limitador.*is missing/);
    expect(erros((o) => (((o.principal as Obj[])[0] as Obj).velocidade = { tipo: "constante", fator: "1" }))).toMatch(/not assignable/);
    // efeito com campo de tipo errado: passaria como Desconhecido no projeto, não na lista de efeitos
    expect(
      erros((o) => {
        const e = (((o.principal as Obj[])[0] as Obj).efeitos as Obj[])[0] as Obj;
        e.ligado = "sim";
      }),
    ).toMatch(/not assignable/);
  }, 60_000);

  it("todo tipo de clipe, sobreposto e efeito aparece", () => {
    expect(new Set(p.principal.map((c) => c.tipo))).toEqual(new Set(TIPOS_CLIPE));
    expect(new Set(p.sobreposicoes.flatMap((t) => t.clipes.map((c) => c.tipo)))).toEqual(new Set(TIPOS_SOBREPOSTO));
    expect(new Set(efeitosDe(p as unknown as Obj).map((e) => (e as Obj).tipo))).toEqual(new Set(TIPOS_EFEITO));
  });
});
