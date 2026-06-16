/**
 * Testes do registry de templates de laudo.
 *
 * Estado atual: só há UM template ("Documento em branco"). Todos os
 * outros modelos foram removidos. Os testes garantem:
 *   - TEMPLATES tem exatamente 1 template
 *   - O template tem shape válido
 *   - `findTemplate` faz fallback para `documento_em_branco`
 *   - `findTemplateWithLegacyAlias` mapeia todos os IDs antigos
 *     (sinistro_transito, arrombamento, local_crime, etc) para o
 *     documento em branco — assim laudos antigos não quebram
 *   - O `build()` retorna `doc` com um único parágrafo vazio — folha
 *     em branco como no Word (sem título no corpo: ele vive em
 *     `doc.title`; sem campos automáticos; cabeçalho é camada separada
 *     e começa desligado)
 */

import { describe, expect, it } from "vitest";
import {
  findTemplate,
  findTemplateWithLegacyAlias,
  TEMPLATES,
} from "../index";

describe("Registry de templates", () => {
  it("tem exatamente 1 template publicado", () => {
    expect(TEMPLATES.length).toBe(1);
  });

  it("o template é o Documento em branco", () => {
    expect(TEMPLATES[0]?.id).toBe("documento_em_branco");
  });

  it("o template tem name, description, category e build", () => {
    const t = TEMPLATES[0]!;
    expect(t.name).toBe("Documento em branco");
    expect(t.description).toBeTruthy();
    expect(t.category).toBe("Genérico");
    expect(typeof t.build).toBe("function");
  });
});

describe("findTemplate", () => {
  it("encontra o template por ID exato", () => {
    expect(findTemplate("documento_em_branco").id).toBe("documento_em_branco");
  });

  it("retorna o template padrão como fallback em ID desconhecido", () => {
    expect(findTemplate("xpto_inexistente").id).toBe("documento_em_branco");
  });
});

describe("findTemplateWithLegacyAlias", () => {
  // Todos os IDs dos templates removidos devem cair no padrão sem
  // quebrar — laudos antigos guardam esses ids em `template_id`.
  const legacy = [
    "documento_livre",
    "em_branco",
    "generico",
    "sinistro_transito",
    "sinistro_transito_simples",
    "arrombamento",
    "local_crime",
    "avaliacao_merceologica",
    "constatacao",
    "exame_veicular",
  ];

  for (const id of legacy) {
    it(`legacy "${id}" → documento_em_branco`, () => {
      expect(findTemplateWithLegacyAlias(id).id).toBe("documento_em_branco");
    });
  }

  it("IDs desconhecidos também caem no padrão", () => {
    expect(findTemplateWithLegacyAlias("foo").id).toBe("documento_em_branco");
  });
});

describe("Documento em branco — build()", () => {
  it("retorna um doc vazio: um único parágrafo, sem título no corpo", () => {
    const json = findTemplate("documento_em_branco").build("Sinistro Av FAB");
    expect(json.type).toBe("doc");
    expect(json.content).toEqual([{ type: "paragraph" }]);
    // O título NÃO entra no corpo (vai em doc.title) — folha em branco.
    const hasHeading = (json.content ?? []).some(
      (n: { type?: string }) => n.type === "heading",
    );
    expect(hasHeading).toBe(false);
  });

  it("ignora o título passado (corpo é sempre a mesma folha vazia)", () => {
    const comTitulo = findTemplate("documento_em_branco").build("Qualquer");
    const semTitulo = findTemplate("documento_em_branco").build("");
    expect(comTitulo.content).toEqual(semTitulo.content);
  });

  it("NÃO usa fieldPlaceholder no corpo (cabeçalho é camada separada)", () => {
    const json = findTemplate("documento_em_branco").build("Teste");
    expect(JSON.stringify(json)).not.toContain("fieldPlaceholder");
  });
});
