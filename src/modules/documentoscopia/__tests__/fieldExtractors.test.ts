import { describe, it, expect } from "vitest";

import { extractFields, isValidCPF, isValidCNPJ } from "../fieldExtractors";

const SAMPLE = `
REPÚBLICA FEDERATIVA DO BRASIL
Eu, João da Silva, CPF 529.982.247-25, declaro para os devidos fins.
Empresa ACME COMERCIO LTDA, CNPJ 11.222.333/0001-81.
Veículo: placa ABC1D23, chassi 9BWZZZ377VT004251.
Lavrado em 01/06/2026 às 14:30. Valor da causa: R$ 1.234,56.
Processo nº 0001234-56.2026.8.03.0001.
Contato: perito@policia.ap.gov.br — CEP 68900-000.
`;

describe("validadores de dígito verificador", () => {
  it("valida CPF correto e rejeita inválido", () => {
    expect(isValidCPF("529.982.247-25")).toBe(true);
    expect(isValidCPF("111.111.111-11")).toBe(false);
    expect(isValidCPF("123.456.789-00")).toBe(false);
  });

  it("valida CNPJ correto e rejeita inválido", () => {
    expect(isValidCNPJ("11.222.333/0001-81")).toBe(true);
    expect(isValidCNPJ("11.111.111/1111-11")).toBe(false);
  });
});

describe("extractFields", () => {
  const fields = extractFields(SAMPLE);
  const byName = (n: string) => fields.filter((f) => f.field_name === n);

  it("detecta CPF válido com alta confiança", () => {
    const cpf = byName("CPF").find((f) => f.field_value === "529.982.247-25");
    expect(cpf).toBeDefined();
    expect(cpf!.confidence).toBeGreaterThan(0.9);
  });

  it("detecta CNPJ válido", () => {
    expect(byName("CNPJ").some((f) => f.field_value === "11.222.333/0001-81")).toBe(
      true,
    );
  });

  it("detecta placa Mercosul, chassi, data, valor e processo", () => {
    expect(byName("Placa").some((f) => f.field_value === "ABC1D23")).toBe(true);
    expect(byName("Chassi").length).toBeGreaterThan(0);
    expect(byName("Data").some((f) => f.field_value === "01/06/2026")).toBe(true);
    expect(byName("Valor").some((f) => f.field_value.includes("1.234,56"))).toBe(true);
    expect(byName("Processo").length).toBe(1);
    expect(byName("E-mail").some((f) => f.field_value.includes("@"))).toBe(true);
  });

  it("não duplica o mesmo valor", () => {
    const dupes = extractFields("ABC1D23 ABC1D23 ABC1D23");
    expect(dupes.filter((f) => f.field_value === "ABC1D23").length).toBe(1);
  });

  it("retorna vazio para texto em branco", () => {
    expect(extractFields("   ")).toEqual([]);
  });
});
