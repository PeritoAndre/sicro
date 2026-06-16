import { describe, it, expect } from "vitest";
import { aggregateGeneral, availableYears } from "../general";
import type { CaseIndexEntry } from "@domain/case_index";

function entry(p: Partial<CaseIndexEntry>): CaseIndexEntry {
  return {
    workspace_id: Math.random().toString(36).slice(2),
    workspace_path: "",
    numero_bo: null,
    tipo_pericia: null,
    natureza: null,
    municipio: null,
    bairro: null,
    status: "aberta",
    data_fato: null,
    data_acionamento: null,
    data_chegada: null,
    data_encerramento: null,
    peritos: [],
    created_at: null,
    indexed_at: "",
    counts: null,
    ...p,
  };
}

const sample: CaseIndexEntry[] = [
  entry({
    tipo_pericia: "Sinistro de trânsito",
    municipio: "Macapá",
    status: "concluida",
    data_fato: "2026-01-10T10:00:00Z",
    data_acionamento: "2026-01-10T10:00:00Z",
    data_encerramento: "2026-01-13T10:00:00Z",
    peritos: ["André", "Bruna"],
  }),
  entry({
    tipo_pericia: "Sinistro de trânsito",
    municipio: "Macapá",
    status: "em_andamento",
    data_fato: "2026-02-15T10:00:00Z",
    peritos: ["André"],
  }),
  entry({
    tipo_pericia: "Local de morte",
    municipio: "Santana",
    status: "concluida",
    data_fato: "2025-12-01T10:00:00Z",
    data_acionamento: "2025-12-01T10:00:00Z",
    data_encerramento: "2025-12-02T10:00:00Z",
    peritos: ["André"],
  }),
];

describe("availableYears", () => {
  it("extrai anos distintos, desc", () => {
    expect(availableYears(sample)).toEqual([2026, 2025]);
  });
  it("vazio → []", () => {
    expect(availableYears([])).toEqual([]);
  });
});

describe("aggregateGeneral — todos os anos", () => {
  const m = aggregateGeneral(sample, { year: null, month: null });

  it("conta totais", () => {
    expect(m.totalAll).toBe(3);
    expect(m.totalInPeriod).toBe(3);
    expect(m.concluded).toBe(2);
    expect(m.open).toBe(1);
  });

  it("agrupa por tipo (desc)", () => {
    expect(m.byType[0]).toEqual({ label: "Sinistro de trânsito", value: 2 });
  });

  it("achata peritos (André em 3 casos)", () => {
    const andre = m.byPerito.find((s) => s.label === "André");
    expect(andre?.value).toBe(3);
    const bruna = m.byPerito.find((s) => s.label === "Bruna");
    expect(bruna?.value).toBe(1);
  });

  it("série temporal por ANO quando todos", () => {
    // 2025: 1, 2026: 2
    expect(m.overTime.map((t) => [t.label, t.value])).toEqual([
      ["2025", 1],
      ["2026", 2],
    ]);
  });

  it("tempo de conclusão (dias) dos 2 concluídos", () => {
    expect(m.cycleTime?.count).toBe(2);
    // 3 dias e 1 dia → média 2
    expect(m.cycleTime?.mean).toBeCloseTo(2, 6);
  });

  it("status traduzido", () => {
    const conc = m.byStatus.find((s) => s.label === "Concluída");
    expect(conc?.value).toBe(2);
  });
});

describe("aggregateGeneral — filtrado por ano", () => {
  const m = aggregateGeneral(sample, { year: 2026, month: null });

  it("filtra para 2026 (2 casos)", () => {
    expect(m.totalInPeriod).toBe(2);
    expect(m.filterLabel).toBe("2026");
  });

  it("série temporal por MÊS quando ano escolhido", () => {
    // jan e fev de 2026
    expect(m.overTime.map((t) => t.label)).toEqual(["jan", "fev"]);
  });

  it("filtra por mês quando informado", () => {
    const jan = aggregateGeneral(sample, { year: 2026, month: 1 });
    expect(jan.totalInPeriod).toBe(1);
    expect(jan.filterLabel).toBe("jan/2026");
  });
});
