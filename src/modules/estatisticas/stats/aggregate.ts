/**
 * Agregação PURA das estatísticas. Sem React, sem I/O — recebe `StatsRawData`
 * e devolve um `StatisticsModel` pronto para os gráficos. Testável via vitest.
 *
 * Princípio (KNOWN_LIMITATIONS §13): tudo aqui é DESCRITIVO — contagens e
 * distribuições do que o caso armazena. Nada de interpretação pericial.
 */

import { fmtBytes, fmtDurationS, nf, pct } from "./format";
import type {
  CategorySlice,
  HistogramBin,
  Kpi,
  NumericSummary,
  StatisticsModel,
  StatsRawData,
  TimePoint,
} from "./model";

// --------------------------------------------------------------------------
// Helpers genéricos (exportados para teste)

export function countBy<T>(
  items: T[],
  keyFn: (t: T) => string | null | undefined,
): CategorySlice[] {
  const map = new Map<string, number>();
  for (const it of items) {
    const raw = keyFn(it);
    const label = raw == null || raw === "" ? "—" : raw;
    map.set(label, (map.get(label) ?? 0) + 1);
  }
  return [...map.entries()]
    .map(([label, value]) => ({ label, value }))
    .sort((a, b) => b.value - a.value);
}

export function numericSummary(raw: number[]): NumericSummary | null {
  const values = raw.filter((v) => Number.isFinite(v)).sort((a, b) => a - b);
  const n = values.length;
  if (n === 0) return null;
  const sum = values.reduce((s, v) => s + v, 0);
  const mean = sum / n;
  const variance =
    n > 1 ? values.reduce((s, v) => s + (v - mean) ** 2, 0) / (n - 1) : 0;
  const stdev = Math.sqrt(variance);
  const q = (p: number): number => {
    if (n === 1) return values[0]!;
    const idx = p * (n - 1);
    const lo = Math.floor(idx);
    const hi = Math.ceil(idx);
    const w = idx - lo;
    return values[lo]! * (1 - w) + values[hi]! * w;
  };
  return {
    count: n,
    min: values[0]!,
    max: values[n - 1]!,
    mean,
    median: q(0.5),
    p2_5: q(0.025),
    p97_5: q(0.975),
    sum,
    stdev,
  };
}

export function histogramBins(raw: number[], maxBins = 10): HistogramBin[] {
  const values = raw.filter((v) => Number.isFinite(v));
  if (values.length === 0) return [];
  const min = Math.min(...values);
  const max = Math.max(...values);
  if (min === max) {
    return [{ label: nf(min, 2), from: min, to: max, count: values.length }];
  }
  const binCount = Math.min(
    maxBins,
    Math.max(3, Math.ceil(Math.sqrt(values.length))),
  );
  const width = (max - min) / binCount;
  const bins: HistogramBin[] = [];
  for (let i = 0; i < binCount; i++) {
    const from = min + i * width;
    const to = i === binCount - 1 ? max : from + width;
    bins.push({ label: `${nf(from, 1)}–${nf(to, 1)}`, from, to, count: 0 });
  }
  for (const v of values) {
    let idx = Math.floor((v - min) / width);
    if (idx >= binCount) idx = binCount - 1;
    if (idx < 0) idx = 0;
    const bin = bins[idx];
    if (bin) bin.count++;
  }
  return bins;
}

export function bucketTime(
  isoDates: (string | null | undefined)[],
): TimePoint[] {
  const dates = isoDates
    .map((s) => (s ? new Date(s) : null))
    .filter((d): d is Date => d != null && !Number.isNaN(d.getTime()));
  if (dates.length === 0) return [];
  const times = dates.map((d) => d.getTime()).sort((a, b) => a - b);
  const spanDays = (times[times.length - 1]! - times[0]!) / 86_400_000;
  const mode: "day" | "month" | "year" =
    spanDays <= 62 ? "day" : spanDays <= 760 ? "month" : "year";
  const keyOf = (d: Date): { key: string; label: string } => {
    const y = d.getFullYear();
    const m = String(d.getMonth() + 1).padStart(2, "0");
    const day = String(d.getDate()).padStart(2, "0");
    if (mode === "day") return { key: `${y}-${m}-${day}`, label: `${day}/${m}` };
    if (mode === "month") return { key: `${y}-${m}`, label: `${m}/${y}` };
    return { key: `${y}`, label: `${y}` };
  };
  const map = new Map<string, { label: string; value: number }>();
  for (const d of dates) {
    const { key, label } = keyOf(d);
    const cur = map.get(key);
    if (cur) cur.value++;
    else map.set(key, { label, value: 1 });
  }
  return [...map.entries()]
    .sort((a, b) => (a[0] < b[0] ? -1 : a[0] > b[0] ? 1 : 0))
    .map(([key, v]) => ({ key, label: v.label, value: v.value }));
}

// --------------------------------------------------------------------------
// Rótulos amigáveis

const ANSWER_LABEL: Record<string, string> = {
  sim: "Sim",
  nao: "Não",
  nao_se_aplica: "Não se aplica",
  nao_verificado: "Não verificado",
};

const KIND_LABEL: Record<string, string> = {
  photo: "Fotos",
  croqui: "Croquis",
  croqui_export: "Croquis (PNG)",
  video: "Vídeos",
  video_frame: "Frames",
  storyboard_frame: "Frames",
  laudo: "Laudos",
  laudo_export: "Laudos (export)",
  imported_package: "Pacotes",
  other: "Outros",
};

const STATUS_LABEL: Record<string, string> = {
  ok: "Íntegro",
  missing_file: "Arquivo ausente",
  hash_mismatch: "Hash divergente",
  missing_sidecar: "Sidecar ausente",
  broken_link: "Link quebrado",
  unsafe_path: "Caminho inseguro",
  unknown: "Não verificado",
};

const LAUDO_STATUS_LABEL: Record<string, string> = {
  rascunho: "Rascunho",
  revisado: "Revisado",
  exportado: "Exportado",
  assinado: "Assinado",
  arquivado: "Arquivado",
};

function friendly(map: Record<string, string>, key: string): string {
  return map[key] ?? key;
}

// --------------------------------------------------------------------------
// Agregação principal

export function aggregate(raw: StatsRawData): StatisticsModel {
  const generatedAt = new Date().toISOString();
  const occ = raw.occurrence;

  // ---- Ocorrência ----
  const occurrence = occ
    ? {
        label: occLabel(occ),
        numero_bo: occ.numero_bo,
        tipo_pericia: occ.tipo_pericia,
        natureza: occ.natureza,
        municipio: occ.municipio,
        status: occ.status,
        peritos: occ.peritos ?? [],
        dataFato: occ.data_fato,
        resultado: occ.resultado ?? null,
        fieldDurationMin:
          raw.dossie?.stats?.duration_seconds != null
            ? raw.dossie.stats.duration_seconds / 60
            : null,
        gpsAccuracyM:
          raw.dossie?.stats?.best_gps_accuracy_m ??
          occ.primary_accuracy_m ??
          null,
      }
    : null;

  // ---- Evidências & integridade ----
  const summary = raw.integrity?.summary ?? null;
  const items = raw.integrity?.items ?? [];

  const evidenceByKind: CategorySlice[] = summary
    ? [
        { label: "Fotos", value: summary.photos },
        { label: "Croquis", value: summary.croquis },
        { label: "Croquis (PNG)", value: summary.croqui_exports },
        { label: "Vídeos", value: summary.videos },
        { label: "Frames", value: summary.storyboard_frames },
        { label: "Laudos", value: summary.laudos },
        { label: "Laudos (export)", value: summary.laudo_exports },
        { label: "Pacotes", value: summary.imported_packages },
      ].filter((s) => s.value > 0)
    : [];

  const integrityByStatus = countBy(items, (i) =>
    friendly(STATUS_LABEL, i.integrity_status),
  );

  const sizeByKindMap = new Map<string, number>();
  for (const it of items) {
    if (it.size_bytes && it.size_bytes > 0) {
      const label = friendly(KIND_LABEL, it.kind);
      sizeByKindMap.set(label, (sizeByKindMap.get(label) ?? 0) + it.size_bytes);
    }
  }
  const sizeByKindMB: CategorySlice[] = [...sizeByKindMap.entries()]
    .map(([label, bytes]) => ({ label, value: bytes / (1024 * 1024) }))
    .sort((a, b) => b.value - a.value);

  const bytesTotal = items.reduce((s, i) => s + (i.size_bytes ?? 0), 0);

  const integrity = {
    total: summary?.total_items ?? 0,
    filesOk: summary?.files_ok ?? 0,
    problems:
      (summary?.files_missing ?? 0) +
      (summary?.broken_links ?? 0) +
      (summary?.hash_mismatches ?? 0) +
      (summary?.unsafe_paths ?? 0),
    overall: summary?.overall_status ?? "ok",
    bytesTotal,
    linkedInLaudos: summary?.linked_in_laudos ?? 0,
    hashMismatches: summary?.hash_mismatches ?? 0,
    filesMissing: summary?.files_missing ?? 0,
    brokenLinks: summary?.broken_links ?? 0,
    unsafePaths: summary?.unsafe_paths ?? 0,
  };

  const productionOverTime = bucketTime(items.map((i) => i.created_at));

  const brokenLinks = (raw.integrity?.broken_laudo_links ?? []).map((b) => ({
    laudo: b.laudo_title,
    node: b.node_type,
    detail: b.detail ?? friendly(STATUS_LABEL, b.status),
  }));

  // ---- Dossiê operacional ----
  const checklistBreakdown = countBy(raw.checklist, (c) =>
    friendly(ANSWER_LABEL, c.answer),
  );
  const checklistByCategory = countBy(raw.checklist, (c) => c.category);

  const cl = raw.dossie?.counts.checklist ?? null;
  const checklistTotal = cl?.total ?? raw.checklist.length;
  const checklistAnswered =
    cl?.answered ??
    raw.checklist.filter((c) => c.answer === "sim" || c.answer === "nao").length;
  const checklistCompletionPct = pct(checklistAnswered, checklistTotal);
  const checklistRequiredPendingPct = cl
    ? pct(cl.required_pending, cl.required_total)
    : 0;

  const entitiesSplit: CategorySlice[] = [
    {
      label: "Veículos",
      value:
        raw.dossie?.counts.vehicles ??
        raw.entities.filter((e) => e.type === "vehicle").length,
    },
    {
      label: "Vítimas",
      value:
        raw.dossie?.counts.victims ??
        raw.entities.filter((e) => e.type === "victim").length,
    },
  ].filter((s) => s.value > 0);

  const tracesByType = countBy(raw.traces, (t) => t.type);

  const measurementValues = raw.measurements
    .map((m) => m.value)
    .filter((v): v is number => v != null && Number.isFinite(v));
  const measurementsSummary = numericSummary(measurementValues);
  const measurementsHistogram = histogramBins(measurementValues);
  const measurementsByUnit = countBy(raw.measurements, (m) => m.unit);

  const notesByPriority = countBy(raw.notes, (n) => n.priority);
  const notesByCategory = countBy(raw.notes, (n) => n.category);

  const operationalTimeline = bucketTime(
    raw.timeline.map((e) => e.occurred_at ?? e.created_at),
  );

  // ---- Laudos & produção ----
  const laudosByStatus = countBy(raw.laudos, (l) =>
    friendly(LAUDO_STATUS_LABEL, l.status),
  );
  const laudosBySignature = countBy(raw.laudos, (l) =>
    l.signature_type ? l.signature_type : "Sem assinatura",
  );
  const laudoExports = {
    pdf: raw.laudos.filter((l) => l.last_export_pdf).length,
    docx: raw.laudos.filter((l) => l.last_export_docx).length,
  };

  // ---- Vídeo & medições técnicas ----
  const video = {
    count: raw.videos.length,
    totalDurationS: raw.videos.reduce((s, v) => s + (v.duration_s ?? 0), 0),
    totalBytes: raw.videos.reduce((s, v) => s + (v.size_bytes ?? 0), 0),
    resolutions: countBy(raw.videos, (v) =>
      v.width && v.height ? `${v.width}×${v.height}` : null,
    ),
  };

  const speedValues = raw.speeds
    .map((s) => s.velocity_kmh)
    .filter((v): v is number => Number.isFinite(v));
  const speedsSummary = numericSummary(speedValues);
  const speedsHistogram = histogramBins(speedValues);
  const speeds = raw.speeds.map((s) => ({
    id: s.id,
    kmh: s.velocity_kmh,
    ciLow: s.ci_low,
    ciHigh: s.ci_high,
    createdAt: s.created_at,
  }));

  const distanceValues = raw.distances
    .map((d) => d.distance_m)
    .filter((v): v is number => Number.isFinite(v));
  const distancesSummary = numericSummary(distanceValues);
  const distancesHistogram = histogramBins(distanceValues);

  const framesCollected = summary?.storyboard_frames ?? 0;

  // ---- Contagens ----
  const counts: Record<string, number> = {
    evidencias: integrity.total,
    fotos: summary?.photos ?? raw.dossie?.counts.photos ?? 0,
    croquis: summary?.croquis ?? raw.croquisCount,
    videos: video.count,
    frames: framesCollected,
    laudos: raw.laudos.length,
    laudos_pdf: laudoExports.pdf,
    laudos_docx: laudoExports.docx,
    analises_imagem: raw.imageAnalysesCount,
    veiculos: raw.dossie?.counts.vehicles ?? 0,
    vitimas: raw.dossie?.counts.victims ?? 0,
    vestigios: raw.dossie?.counts.traces ?? raw.traces.length,
    medicoes_campo: raw.dossie?.counts.measurements ?? raw.measurements.length,
    observacoes: raw.dossie?.counts.notes ?? raw.notes.length,
    eventos_timeline: raw.dossie?.counts.timeline ?? raw.timeline.length,
    velocidades: raw.speeds.length,
    distancias: raw.distances.length,
    itens_com_problema: integrity.problems,
    bytes_total: integrity.bytesTotal,
  };

  // ---- KPIs de destaque ----
  const headline: Kpi[] = [
    {
      key: "evidencias",
      label: "Evidências",
      value: nf(integrity.total),
      sub: `${fmtBytes(integrity.bytesTotal)} em disco`,
      tone: "accent",
    },
    { key: "fotos", label: "Fotos", value: nf(counts.fotos ?? 0) },
    { key: "laudos", label: "Laudos", value: nf(raw.laudos.length) },
    {
      key: "videos",
      label: "Vídeos",
      value: nf(video.count),
      sub: video.totalDurationS > 0 ? fmtDurationS(video.totalDurationS) : undefined,
    },
    {
      key: "medicoes",
      label: "Medições (campo)",
      value: nf(counts.medicoes_campo ?? 0),
    },
    {
      key: "velocidades",
      label: "Velocidades",
      value: nf(raw.speeds.length),
    },
    {
      key: "integridade",
      label: "Integridade",
      value:
        integrity.overall === "ok"
          ? "Íntegro"
          : integrity.overall === "warning"
            ? "Atenção"
            : integrity.overall === "critical"
              ? "Crítico"
              : "—",
      sub:
        integrity.problems > 0
          ? `${nf(integrity.problems)} com problema`
          : `${nf(integrity.filesOk)} ok`,
      tone:
        integrity.overall === "critical"
          ? "crit"
          : integrity.overall === "warning"
            ? "warn"
            : "ok",
    },
    {
      key: "checklist",
      label: "Checklist",
      value: checklistTotal > 0 ? `${Math.round(checklistCompletionPct)}%` : "—",
      sub: checklistTotal > 0 ? `${checklistAnswered}/${checklistTotal}` : undefined,
    },
  ];

  return {
    generatedAt,
    hasData: occ != null,
    occurrence,
    headline,
    evidenceByKind,
    integrityByStatus,
    sizeByKindMB,
    integrity,
    productionOverTime,
    brokenLinks,
    checklistBreakdown,
    checklistByCategory,
    checklistCompletionPct,
    checklistRequiredPendingPct,
    entitiesSplit,
    tracesByType,
    measurementsSummary,
    measurementsHistogram,
    measurementsByUnit,
    notesByPriority,
    notesByCategory,
    operationalTimeline,
    laudosByStatus,
    laudosBySignature,
    laudoExports,
    laudosTotal: raw.laudos.length,
    video,
    speedsSummary,
    speedsHistogram,
    speeds,
    distancesSummary,
    distancesHistogram,
    framesCollected,
    counts,
  };
}

function occLabel(o: {
  numero_bo: string | null;
  tipo_pericia: string | null;
  municipio: string | null;
  id: string;
}): string {
  const parts: string[] = [];
  if (o.numero_bo) parts.push(`BO ${o.numero_bo}`);
  if (o.tipo_pericia) parts.push(o.tipo_pericia);
  if (o.municipio) parts.push(o.municipio);
  return parts.length > 0 ? parts.join(" — ") : `Ocorrência ${o.id.slice(0, 8)}`;
}
