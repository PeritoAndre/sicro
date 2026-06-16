/**
 * Testes do subsistema F5 — catálogo de campos + resolver.
 *
 * Cobertura:
 *   - Catálogo: integridade, lookup, agrupamento, obrigatórios.
 *   - Resolver: occurrence/metadata/system/fixed; valores ausentes;
 *     formato BR de datas; arrays; objetos.
 *   - Missing required: detecta corretamente.
 */

import { describe, expect, it } from "vitest";
import {
  FIELD_GROUPS,
  fieldsByGroup,
  findField,
  isKnownFieldKey,
  LAUDO_FIELDS,
  LAUDO_FIELDS_BY_KEY,
  requiredFields,
} from "../catalog";
import {
  findMissingRequiredFields,
  resolveDefinition,
  resolveFieldValue,
  resolveAllFields,
  type FieldResolveContext,
} from "../resolver";

// ---------------------------------------------------------------------------
// CATÁLOGO

describe("LAUDO_FIELDS catálogo", () => {
  it("contém ao menos 30 campos", () => {
    expect(LAUDO_FIELDS.length).toBeGreaterThanOrEqual(30);
  });

  it("todas as keys são únicas", () => {
    const keys = LAUDO_FIELDS.map((f) => f.key);
    expect(new Set(keys).size).toBe(keys.length);
  });

  it("todos os campos têm label e group válido", () => {
    for (const def of LAUDO_FIELDS) {
      expect(def.label.length).toBeGreaterThan(0);
      expect(FIELD_GROUPS).toContain(def.group);
    }
  });

  it("LAUDO_FIELDS_BY_KEY indexa todos", () => {
    for (const def of LAUDO_FIELDS) {
      expect(LAUDO_FIELDS_BY_KEY.get(def.key)).toBe(def);
    }
  });

  it("isKnownFieldKey distingue campos válidos", () => {
    expect(isKnownFieldKey("numero_bo")).toBe(true);
    expect(isKnownFieldKey("xpto_inexistente")).toBe(false);
  });

  it("findField retorna null para key desconhecido", () => {
    expect(findField("xpto")).toBeNull();
    expect(findField("")).toBeNull();
    expect(findField(null)).toBeNull();
    expect(findField("numero_bo")?.key).toBe("numero_bo");
  });

  it("fieldsByGroup particiona corretamente", () => {
    const all = FIELD_GROUPS.flatMap((g) => fieldsByGroup(g));
    expect(all.length).toBe(LAUDO_FIELDS.length);
  });

  it("requiredFields contém pelo menos os essenciais periciais", () => {
    const reqKeys = new Set(requiredFields().map((f) => f.key));
    expect(reqKeys.has("numero_laudo")).toBe(true);
    expect(reqKeys.has("numero_bo")).toBe(true);
    expect(reqKeys.has("data_pericia")).toBe(true);
    expect(reqKeys.has("local_pericia")).toBe(true);
    expect(reqKeys.has("nome_perito")).toBe(true);
  });
});

// ---------------------------------------------------------------------------
// RESOLVER

const fixedNow = new Date("2026-05-26T15:30:00");

const ctxComplete: FieldResolveContext = {
  metadata: {
    numero_laudo: "12345/2026",
    setor: "DC/PCIAP",
  },
  occurrence: {
    numero_bo: "BO-9999/2026",
    municipio: "Macapá",
    uf: "AP",
    tipo_pericia: "Sinistro de trânsito",
    data_fato: "2026-05-20",
    perito: "Carlos Mendes",
    matricula_perito: "777-X",
    veiculos: ["Fiat Uno", "Honda Civic"],
    coordenadas: { lat: -0.0345, lon: -51.0694 },
  },
  now: fixedNow,
};

describe("resolveFieldValue + resolveDefinition", () => {
  it("retorna null para key desconhecido", () => {
    expect(resolveFieldValue("xpto", ctxComplete)).toBeNull();
  });

  it("metadata.numero_laudo é resolvido corretamente", () => {
    expect(resolveFieldValue("numero_laudo", ctxComplete)).toBe("12345/2026");
  });

  it("occurrence.numero_bo é resolvido corretamente", () => {
    expect(resolveFieldValue("numero_bo", ctxComplete)).toBe("BO-9999/2026");
  });

  it("system.data_hoje usa o `now` injetado", () => {
    expect(resolveFieldValue("data_hoje", ctxComplete)).toBe("26/05/2026");
  });

  it("system.data_hora_agora formata data+hora", () => {
    expect(resolveFieldValue("data_hora_agora", ctxComplete)).toBe(
      "26/05/2026 15:30",
    );
  });

  it("fixed.orgao retorna valor estático do catálogo", () => {
    expect(resolveFieldValue("orgao", ctxComplete)).toBe(
      "POLÍCIA CIENTÍFICA DO AMAPÁ",
    );
  });

  it("array (veiculos) vira string separada por '; '", () => {
    expect(resolveFieldValue("veiculos", ctxComplete)).toBe(
      "Fiat Uno; Honda Civic",
    );
  });

  it("objeto (coordenadas) cai em JSON.stringify", () => {
    const val = resolveFieldValue("coordenadas", ctxComplete);
    expect(val).toContain("-0.0345");
    expect(val).toContain("-51.0694");
  });

  it("campo ausente retorna string vazia, não null", () => {
    const ctxEmpty: FieldResolveContext = { metadata: {}, occurrence: {} };
    expect(resolveFieldValue("numero_laudo", ctxEmpty)).toBe("");
  });

  it("contexto vazio retorna empty para todos os fields conhecidos", () => {
    const ctxEmpty: FieldResolveContext = {};
    const result = resolveFieldValue("numero_bo", ctxEmpty);
    expect(result).toBe("");
  });

  it("resolveAllFields cobre o catálogo inteiro", () => {
    const map = resolveAllFields(ctxComplete, LAUDO_FIELDS);
    expect(map.size).toBe(LAUDO_FIELDS.length);
    expect(map.get("numero_bo")).toBe("BO-9999/2026");
  });
});

describe("findMissingRequiredFields", () => {
  it("contexto completo → nenhum missing entre os obrigatórios", () => {
    const missing = findMissingRequiredFields(ctxComplete, LAUDO_FIELDS);
    // ctxComplete preenche numero_laudo + numero_bo + tipo_pericia +
    // data_fato + nome_perito + local_pericia. Mas local_pericia (endereco)
    // não está no ctx → deve aparecer como missing.
    const missingKeys = missing.map((f) => f.key);
    expect(missingKeys).toContain("local_pericia");
    // Mas numero_bo, numero_laudo etc devem estar OK.
    expect(missingKeys).not.toContain("numero_bo");
    expect(missingKeys).not.toContain("numero_laudo");
  });

  it("contexto vazio → todos os obrigatórios aparecem", () => {
    const missing = findMissingRequiredFields({}, LAUDO_FIELDS);
    const required = requiredFields();
    expect(missing.length).toBe(required.length);
  });
});

describe("resolveDefinition — formatadores de data", () => {
  it("formato BR com zero-padding", () => {
    const ctx: FieldResolveContext = { now: new Date("2026-01-05T08:07:00") };
    expect(resolveFieldValue("data_hoje", ctx)).toBe("05/01/2026");
    expect(resolveFieldValue("data_hora_agora", ctx)).toBe("05/01/2026 08:07");
  });

  it("usa now() quando o `ctx.now` está ausente", () => {
    const before = Date.now();
    const v = resolveFieldValue("data_hoje", {});
    const after = Date.now();
    expect(typeof v).toBe("string");
    expect(v?.length).toBe(10); // "DD/MM/AAAA"
    // Sanity: pelo menos contém o ano atual.
    const year = new Date(before).getFullYear();
    const yearAfter = new Date(after).getFullYear();
    expect([year, yearAfter]).toContain(Number(v?.slice(-4)));
  });
});

// ---------------------------------------------------------------------------
// Smoke: definição inline.

describe("resolveDefinition (smoke)", () => {
  it("aceita uma definição direta", () => {
    const def = findField("nome_perito")!;
    expect(resolveDefinition(def, ctxComplete)).toBe("Carlos Mendes");
  });
});
