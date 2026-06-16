/**
 * Agregação PURA das estatísticas GERAIS de trabalho (entre casos), a partir
 * do índice global `CaseIndexEntry[]` + um filtro de período. Descritivo:
 * volume de trabalho, distribuição por tipo/município/perito/status, casos ao
 * longo do tempo, sazonalidade e tempo de conclusão. Testável via vitest.
 */

import type { CaseIndexEntry } from "@domain/case_index";
import { countBy, numericSummary } from "./aggregate";
import { nf } from "./format";
import type { CategorySlice, Kpi, NumericSummary, TimePoint } from "./model";

export interface PeriodFilter {
  /** Ano selecionado, ou null para "todos". */
  year: number | null;
  /** Mês 1–12 selecionado (só quando há ano), ou null. */
  month: number | null;
}

const MONTHS = ["jan", "fev", "mar", "abr", "mai", "jun", "jul", "ago", "set", "out", "nov", "dez"];
const WEEKDAYS = ["dom", "seg", "ter", "qua", "qui", "sex", "sáb"];

const STATUS_LABEL: Record<string, string> = {
  aberta: "Aberta",
  em_andamento: "Em andamento",
  concluida: "Concluída",
  arquivada: "Arquivada",
};

export interface GeneralRecentCase {
  label: string;
  tipo: string | null;
  municipio: string | null;
  status: string;
  date: string | null;
}

export interface GeneralModel {
  generatedAt: string;
  totalAll: number;
  totalInPeriod: number;
  availableYears: number[];
  filterLabel: string;
  headline: Kpi[];
  byStatus: CategorySlice[];
  byType: CategorySlice[];
  byMunicipio: CategorySlice[];
  byPerito: CategorySlice[];
  byNatureza: CategorySlice[];
  overTime: TimePoint[];
  byMonthOfYear: CategorySlice[];
  byWeekday: CategorySlice[];
  cycleTime: NumericSummary | null;
  concluded: number;
  open: number;
  recent: GeneralRecentCase[];
}

function caseDate(e: CaseIndexEntry): Date | null {
  const s = e.data_fato ?? e.created_at ?? e.indexed_at ?? null;
  if (!s) return null;
  const d = new Date(s);
  return Number.isNaN(d.getTime()) ? null : d;
}

function caseLabel(e: CaseIndexEntry): string {
  const parts: string[] = [];
  if (e.numero_bo) parts.push(`BO ${e.numero_bo}`);
  if (e.tipo_pericia) parts.push(e.tipo_pericia);
  if (e.municipio) parts.push(e.municipio);
  return parts.length > 0 ? parts.join(" — ") : `Caso ${e.workspace_id.slice(0, 8)}`;
}

export function availableYears(entries: CaseIndexEntry[]): number[] {
  const set = new Set<number>();
  for (const e of entries) {
    const d = caseDate(e);
    if (d) set.add(d.getFullYear());
  }
  return [...set].sort((a, b) => b - a);
}

function applyFilter(entries: CaseIndexEntry[], f: PeriodFilter): CaseIndexEntry[] {
  if (f.year == null) return entries;
  return entries.filter((e) => {
    const d = caseDate(e);
    if (!d) return false;
    if (d.getFullYear() !== f.year) return false;
    if (f.month != null && d.getMonth() + 1 !== f.month) return false;
    return true;
  });
}

function byPeritoCounts(entries: CaseIndexEntry[]): CategorySlice[] {
  const map = new Map<string, number>();
  for (const e of entries) {
    const list = e.peritos.length > 0 ? e.peritos : ["—"];
    for (const p of list) {
      const k = p.trim() || "—";
      map.set(k, (map.get(k) ?? 0) + 1);
    }
  }
  return [...map.entries()]
    .map(([label, value]) => ({ label, value }))
    .sort((a, b) => b.value - a.value);
}

export function aggregateGeneral(
  entries: CaseIndexEntry[],
  filter: PeriodFilter,
): GeneralModel {
  const generatedAt = new Date().toISOString();
  const years = availableYears(entries);
  const filtered = applyFilter(entries, filter);

  const byStatus = countBy(filtered, (e) => STATUS_LABEL[e.status] ?? e.status);
  const byType = countBy(filtered, (e) => e.tipo_pericia);
  const byMunicipio = countBy(filtered, (e) => e.municipio);
  const byNatureza = countBy(filtered, (e) => e.natureza);
  const byPerito = byPeritoCounts(filtered);

  // Série temporal: por ANO quando "todos"; por MÊS quando um ano é escolhido.
  const overTimeMap = new Map<string, { label: string; value: number }>();
  for (const e of filtered) {
    const d = caseDate(e);
    if (!d) continue;
    let key: string;
    let label: string;
    if (filter.year == null) {
      key = `${d.getFullYear()}`;
      label = key;
    } else {
      key = `${String(d.getMonth() + 1).padStart(2, "0")}`;
      label = MONTHS[d.getMonth()] ?? key;
    }
    const cur = overTimeMap.get(key);
    if (cur) cur.value++;
    else overTimeMap.set(key, { label, value: 1 });
  }
  const overTime: TimePoint[] = [...overTimeMap.entries()]
    .sort((a, b) => (a[0] < b[0] ? -1 : a[0] > b[0] ? 1 : 0))
    .map(([key, v]) => ({ key, label: v.label, value: v.value }));

  // Sazonalidade (mês do ano) e dia da semana.
  const monthCounts = new Array(12).fill(0) as number[];
  const weekdayCounts = new Array(7).fill(0) as number[];
  for (const e of filtered) {
    const d = caseDate(e);
    if (!d) continue;
    monthCounts[d.getMonth()] = (monthCounts[d.getMonth()] ?? 0) + 1;
    weekdayCounts[d.getDay()] = (weekdayCounts[d.getDay()] ?? 0) + 1;
  }
  const byMonthOfYear: CategorySlice[] = MONTHS.map((m, i) => ({
    label: m,
    value: monthCounts[i] ?? 0,
  }));
  const byWeekday: CategorySlice[] = WEEKDAYS.map((w, i) => ({
    label: w,
    value: weekdayCounts[i] ?? 0,
  }));

  // Tempo de conclusão (dias entre acionamento e encerramento).
  const cycleDays: number[] = [];
  for (const e of filtered) {
    if (!e.data_acionamento || !e.data_encerramento) continue;
    const a = new Date(e.data_acionamento).getTime();
    const b = new Date(e.data_encerramento).getTime();
    if (Number.isNaN(a) || Number.isNaN(b) || b < a) continue;
    cycleDays.push((b - a) / 86_400_000);
  }
  const cycleTime = numericSummary(cycleDays);

  const concluded = filtered.filter(
    (e) => e.status === "concluida" || e.status === "arquivada",
  ).length;
  const open = filtered.length - concluded;

  const recent: GeneralRecentCase[] = [...filtered]
    .sort((x, y) => {
      const dx = caseDate(x)?.getTime() ?? 0;
      const dy = caseDate(y)?.getTime() ?? 0;
      return dy - dx;
    })
    .slice(0, 12)
    .map((e) => ({
      label: caseLabel(e),
      tipo: e.tipo_pericia,
      municipio: e.municipio,
      status: STATUS_LABEL[e.status] ?? e.status,
      date: e.data_fato ?? e.created_at ?? null,
    }));

  const filterLabel =
    filter.year == null
      ? "Todos os anos"
      : filter.month != null
        ? `${MONTHS[filter.month - 1] ?? filter.month}/${filter.year}`
        : `${filter.year}`;

  const headline: Kpi[] = [
    { key: "total", label: "Casos (total)", value: nf(entries.length), tone: "accent" },
    { key: "periodo", label: "No período", value: nf(filtered.length) },
    { key: "concluidos", label: "Concluídos", value: nf(concluded), tone: "ok" },
    { key: "andamento", label: "Em aberto", value: nf(open), tone: open > 0 ? "warn" : "ok" },
    { key: "tipos", label: "Tipos de perícia", value: nf(byType.length) },
    { key: "municipios", label: "Municípios", value: nf(byMunicipio.length) },
    { key: "peritos", label: "Peritos", value: nf(byPerito.length) },
    {
      key: "tempo",
      label: "Tempo médio",
      value: cycleTime ? `${nf(cycleTime.mean, 1)} d` : "—",
      sub: cycleTime ? `${cycleTime.count} concluído(s)` : "sem datas",
    },
  ];

  return {
    generatedAt,
    totalAll: entries.length,
    totalInPeriod: filtered.length,
    availableYears: years,
    filterLabel,
    headline,
    byStatus,
    byType,
    byMunicipio,
    byPerito,
    byNatureza,
    overTime,
    byMonthOfYear,
    byWeekday,
    cycleTime,
    concluded,
    open,
    recent,
  };
}
