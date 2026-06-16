import { describe, expect, it } from "vitest";
import { buildTechnicalSummary } from "./relatorio";
import type {
  DetectedField,
  DocumentCaseFile,
  OcrRun,
} from "@domain/documentoscopia";

const doc = {
  id: "d1",
  title: "CNH frente",
  original_filename: "cnh.jpg",
  doc_type: "cnh",
  sha256: "abc123",
  size_bytes: 2048,
  created_at: "2026-01-15T12:00:00Z",
} as unknown as DocumentCaseFile;

const run = {
  engine: "rapidocr",
  engine_version: "1.3",
} as unknown as OcrRun;

const field = (name: string, value: string, corrected?: string) =>
  ({
    field_name: name,
    field_value: value,
    corrected_value: corrected ?? null,
  }) as unknown as DetectedField;

describe("buildTechnicalSummary", () => {
  it("inclui proveniência, hash, motor e tamanho", () => {
    const s = buildTechnicalSummary(doc, [], run);
    expect(s).toContain("Documento: CNH frente");
    expect(s).toContain("Arquivo: cnh.jpg");
    expect(s).toContain("Hash SHA-256: abc123");
    expect(s).toContain("rapidocr 1.3");
    expect(s).toContain("2.0 KB");
  });

  it("lista campos, preferindo o valor corrigido pelo perito", () => {
    const s = buildTechnicalSummary(
      doc,
      [field("CPF", "00000000000", "123.456.789-00")],
      run,
    );
    expect(s).toContain("- CPF: 123.456.789-00");
  });

  it("marca ausência de campos e de OCR", () => {
    const s = buildTechnicalSummary(doc, [], null);
    expect(s).toContain("(nenhum campo registrado)");
    expect(s).toContain("Motor de OCR: —");
  });

  it("sempre traz a nota §13 de apoio", () => {
    const s = buildTechnicalSummary(doc, [], run);
    expect(s).toContain("APOIO técnico-computacional");
    expect(s.toLowerCase()).toContain("conclusão documentoscópica");
  });
});
