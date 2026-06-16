/**
 * Modelo de dados das Estatísticas.
 *
 * `StatsRawData` é o que o módulo carrega (compondo comandos de leitura já
 * existentes). `aggregate()` transforma isso em `StatisticsModel` — uma coleção
 * de séries prontas para os gráficos (KPIs, fatias categóricas, séries temporais,
 * histogramas e resumos numéricos). Tudo é DESCRITIVO: contagens e distribuições
 * do que o caso armazena — nunca interpretação pericial.
 */

import type { Occurrence } from "@domain/occurrence";
import type {
  ChecklistItem,
  DossieSummary,
  Entity,
  FieldNote,
  Measurement,
  TimelineEvent,
  Trace,
} from "@domain/dossie";
import type { WorkspaceIntegrityReport } from "@domain/evidence_registry";
import type { Laudo } from "@domain/laudo";
import type { VideoMedia } from "@domain/video";
import type { VideoSpeedCalculation } from "@domain/video_speed";
import type { VideoDistanceMeasurement } from "@domain/video_distance";

/** Dados crus coletados dos comandos, antes da agregação. */
export interface StatsRawData {
  occurrence: Occurrence | null;
  dossie: DossieSummary | null;
  checklist: ChecklistItem[];
  entities: Entity[];
  traces: Trace[];
  measurements: Measurement[];
  notes: FieldNote[];
  timeline: TimelineEvent[];
  integrity: WorkspaceIntegrityReport | null;
  laudos: Laudo[];
  videos: VideoMedia[];
  speeds: VideoSpeedCalculation[];
  distances: VideoDistanceMeasurement[];
  imageAnalysesCount: number;
  croquisCount: number;
}

// --------------------------------------------------------------------------
// Primitivas de série

/** Uma fatia categórica (barra, fatia de rosca). */
export interface CategorySlice {
  label: string;
  value: number;
}

/** Um ponto de série temporal. `key` é ordenável; `label` é exibível. */
export interface TimePoint {
  key: string;
  label: string;
  value: number;
}

/** Uma faixa de histograma. */
export interface HistogramBin {
  label: string;
  from: number;
  to: number;
  count: number;
}

/** Resumo numérico de uma distribuição. */
export interface NumericSummary {
  count: number;
  min: number;
  max: number;
  mean: number;
  median: number;
  p2_5: number;
  p97_5: number;
  sum: number;
  stdev: number;
}

export type KpiTone = "default" | "ok" | "warn" | "crit" | "accent";

export interface Kpi {
  key: string;
  label: string;
  value: string;
  sub?: string;
  tone?: KpiTone;
}

// --------------------------------------------------------------------------
// Modelo agregado

export interface StatsOccurrenceInfo {
  label: string;
  numero_bo: string | null;
  tipo_pericia: string | null;
  natureza: string | null;
  municipio: string | null;
  status: string;
  peritos: string[];
  dataFato: string | null;
  resultado: string | null;
  fieldDurationMin: number | null;
  gpsAccuracyM: number | null;
}

export interface IntegrityRollup {
  total: number;
  filesOk: number;
  problems: number;
  overall: string;
  bytesTotal: number;
  linkedInLaudos: number;
  hashMismatches: number;
  filesMissing: number;
  brokenLinks: number;
  unsafePaths: number;
}

export interface BrokenLinkRow {
  laudo: string;
  node: string;
  detail: string;
}

export interface VideoRollup {
  count: number;
  totalDurationS: number;
  totalBytes: number;
  resolutions: CategorySlice[];
}

export interface SpeedRow {
  id: string;
  kmh: number;
  ciLow: number | null;
  ciHigh: number | null;
  createdAt: string;
}

export interface StatisticsModel {
  generatedAt: string;
  hasData: boolean;
  occurrence: StatsOccurrenceInfo | null;

  headline: Kpi[];

  // Evidências & integridade
  evidenceByKind: CategorySlice[];
  integrityByStatus: CategorySlice[];
  sizeByKindMB: CategorySlice[];
  integrity: IntegrityRollup;
  productionOverTime: TimePoint[];
  brokenLinks: BrokenLinkRow[];

  // Dossiê operacional
  checklistBreakdown: CategorySlice[];
  checklistByCategory: CategorySlice[];
  checklistCompletionPct: number;
  checklistRequiredPendingPct: number;
  entitiesSplit: CategorySlice[];
  tracesByType: CategorySlice[];
  measurementsSummary: NumericSummary | null;
  measurementsHistogram: HistogramBin[];
  measurementsByUnit: CategorySlice[];
  notesByPriority: CategorySlice[];
  notesByCategory: CategorySlice[];
  operationalTimeline: TimePoint[];

  // Laudos & produção
  laudosByStatus: CategorySlice[];
  laudosBySignature: CategorySlice[];
  laudoExports: { pdf: number; docx: number };
  laudosTotal: number;

  // Vídeo & medições técnicas
  video: VideoRollup;
  speedsSummary: NumericSummary | null;
  speedsHistogram: HistogramBin[];
  speeds: SpeedRow[];
  distancesSummary: NumericSummary | null;
  distancesHistogram: HistogramBin[];
  framesCollected: number;

  // Contagens cruas (KPIs + exportação)
  counts: Record<string, number>;
}
