/**
 * EstatisticasModule — dashboard analítico (estilo PowerBI) da OCORRÊNCIA ATIVA.
 *
 * Não há agregador no backend: o módulo compõe comandos de leitura já
 * existentes (dossiê, registro de evidências, laudos, vídeo, velocidade,
 * distância), agrega em memória com `aggregate()` (puro/testado) e desenha com
 * um kit de gráficos em SVG. Exporta o painel em HTML/CSV/JSON.
 *
 * Tudo é DESCRITIVO (KNOWN_LIMITATIONS §13): contagens e distribuições do que o
 * caso armazena — nunca interpretação pericial.
 */

import { useCallback, useEffect, useMemo, useState } from "react";
import {
  CalendarClock,
  ClipboardList,
  Download,
  FileText,
  Film,
  LayoutDashboard,
  PieChart,
  RefreshCw,
  ShieldCheck,
} from "lucide-react";
import { Button } from "@components/Button/Button";
import { commands } from "@core/commands";
import { toSicroError } from "@core/errors";
import {
  selectActiveOccurrence,
  selectActiveWorkspacePath,
  useWorkspaceStore,
} from "@stores/workspaceStore";
import type { ChecklistItem, Entity, FieldNote, Measurement, TimelineEvent, Trace } from "@domain/dossie";
import type { DossieSummary } from "@domain/dossie";
import type { WorkspaceIntegrityReport } from "@domain/evidence_registry";
import type { Laudo } from "@domain/laudo";
import type { VideoMedia } from "@domain/video";
import type { VideoSpeedCalculation } from "@domain/video_speed";
import type { VideoDistanceMeasurement } from "@domain/video_distance";
import type { ImageAnalysis } from "@domain/image_analysis";
import type { Croqui } from "@domain/croqui";

import { aggregate } from "./stats/aggregate";
import { fmtBytes, fmtDurationS, nf } from "./stats/format";
import { buildCsv, buildHtml, buildJson } from "./stats/export";
import type { StatisticsModel, StatsRawData } from "./stats/model";
import {
  BarChart,
  ChartCard,
  DonutChart,
  HBarChart,
  Histogram,
  KpiCard,
  LineChart,
  ProgressBar,
  StatTable,
} from "./charts/Charts";
import { GeralView } from "./GeralView";
import styles from "./EstatisticasModule.module.css";

type ViewMode = "caso" | "geral";

type PageKey =
  | "overview"
  | "evidence"
  | "operational"
  | "production"
  | "video"
  | "timeline";

const PAGES: { key: PageKey; label: string; icon: typeof PieChart }[] = [
  { key: "overview", label: "Visão geral", icon: LayoutDashboard },
  { key: "evidence", label: "Evidências", icon: ShieldCheck },
  { key: "operational", label: "Dossiê operacional", icon: ClipboardList },
  { key: "production", label: "Laudos & produção", icon: FileText },
  { key: "video", label: "Vídeo & medições", icon: Film },
  { key: "timeline", label: "Linha do tempo", icon: CalendarClock },
];

function safe<T>(p: Promise<T>, fallback: T): Promise<T> {
  return p.then((v) => v).catch(() => fallback);
}

export function EstatisticasModule() {
  const occurrence = useWorkspaceStore(selectActiveOccurrence);
  const workspacePath = useWorkspaceStore(selectActiveWorkspacePath);

  const [raw, setRaw] = useState<StatsRawData | null>(null);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [feedback, setFeedback] = useState<string | null>(null);
  const [exporting, setExporting] = useState<string | null>(null);
  const [page, setPage] = useState<PageKey>("overview");
  const [mode, setMode] = useState<ViewMode>(occurrence ? "caso" : "geral");

  const load = useCallback(
    async (ws: string) => {
      setLoading(true);
      setError(null);
      try {
        const [
          dossie,
          checklist,
          entities,
          traces,
          measurements,
          notes,
          timeline,
          integrity,
          laudos,
          videos,
          speeds,
          distances,
          imageAnalyses,
          croquis,
        ] = await Promise.all([
          safe<DossieSummary | null>(commands.getDossieSummary(ws), null),
          safe<ChecklistItem[]>(commands.listDossieChecklist(ws), []),
          safe<Entity[]>(commands.listDossieEntities(ws), []),
          safe<Trace[]>(commands.listDossieTraces(ws), []),
          safe<Measurement[]>(commands.listDossieMeasurements(ws), []),
          safe<FieldNote[]>(commands.listDossieNotes(ws), []),
          safe<TimelineEvent[]>(commands.listDossieTimeline(ws), []),
          safe<WorkspaceIntegrityReport | null>(
            commands.verifyWorkspaceIntegrity(ws, { deep: false }),
            null,
          ),
          safe<Laudo[]>(commands.listLaudos(ws), []),
          safe<VideoMedia[]>(commands.listVideoMedia(ws), []),
          safe<VideoSpeedCalculation[]>(
            commands.listSpeedCalculationsForOccurrence(ws),
            [],
          ),
          safe<VideoDistanceMeasurement[]>(
            commands.listDistanceMeasurementsForOccurrence(ws),
            [],
          ),
          safe<ImageAnalysis[]>(commands.listImageAnalyses(ws), []),
          safe<Croqui[]>(commands.listCroquis(ws), []),
        ]);

        setRaw({
          occurrence: occurrence ?? null,
          dossie,
          checklist,
          entities,
          traces,
          measurements,
          notes,
          timeline,
          integrity,
          laudos,
          videos,
          speeds,
          distances,
          imageAnalysesCount: imageAnalyses.length,
          croquisCount: croquis.length,
        });
      } catch (e) {
        setError(toSicroError(e).message);
      } finally {
        setLoading(false);
      }
    },
    [occurrence],
  );

  useEffect(() => {
    if (!workspacePath) {
      setRaw(null);
      return;
    }
    void load(workspacePath);
  }, [workspacePath, load]);

  const model = useMemo<StatisticsModel | null>(
    () => (raw ? aggregate(raw) : null),
    [raw],
  );

  const handleExport = async (format: "html" | "csv" | "json") => {
    if (!workspacePath || !model) return;
    setExporting(format);
    setError(null);
    try {
      const content =
        format === "html"
          ? buildHtml(model)
          : format === "csv"
            ? buildCsv(model)
            : buildJson(model);
      const rel = await commands.saveStatisticsExport(workspacePath, format, content);
      setFeedback(`Exportado: ${rel}`);
      const sep = workspacePath.includes("\\") ? "\\" : "/";
      const abs = `${workspacePath}${sep}${rel.replace(/\//g, sep)}`;
      try {
        await commands.revealPathInExplorer(abs);
      } catch {
        /* reveal é best-effort */
      }
      setTimeout(() => setFeedback(null), 4000);
    } catch (e) {
      setError(toSicroError(e).message);
    } finally {
      setExporting(null);
    }
  };

  return (
    <div className={styles.wrap}>
      <header className={styles.topBar}>
        <div className={styles.title}>
          <h1>
            <PieChart size={16} aria-hidden /> Estatísticas
          </h1>
          <p className={styles.subtitle}>
            {mode === "geral"
              ? "Painel gerencial — todos os casos"
              : `${model?.occurrence?.label ?? "—"} · painel descritivo do caso`}
          </p>
        </div>
        <div className={styles.headActions}>
          {mode === "caso" && (
            <>
              {feedback && <span className={styles.feedback}>{feedback}</span>}
              {error && <span className={styles.errorMsg}>{error}</span>}
              <div className={styles.exportGroup}>
                <Download size={13} aria-hidden />
                {(["html", "csv", "json"] as const).map((f) => (
                  <button
                    key={f}
                    type="button"
                    className={styles.exportBtn}
                    disabled={!model || exporting != null}
                    onClick={() => void handleExport(f)}
                  >
                    {exporting === f ? "…" : f.toUpperCase()}
                  </button>
                ))}
              </div>
              <Button
                variant="secondary"
                size="sm"
                leftIcon={<RefreshCw size={13} />}
                onClick={() => workspacePath && void load(workspacePath)}
                disabled={loading}
              >
                {loading ? "Atualizando…" : "Atualizar"}
              </Button>
            </>
          )}
          <div
            className={styles.modeToggle}
            role="tablist"
            aria-label="Modo das estatísticas"
          >
            <button
              type="button"
              role="tab"
              aria-selected={mode === "caso"}
              className={`${styles.modeBtn} ${mode === "caso" ? styles.modeBtnActive : ""}`}
              onClick={() => setMode("caso")}
            >
              Por caso
            </button>
            <button
              type="button"
              role="tab"
              aria-selected={mode === "geral"}
              className={`${styles.modeBtn} ${mode === "geral" ? styles.modeBtnActive : ""}`}
              onClick={() => setMode("geral")}
            >
              Geral
            </button>
          </div>
        </div>
      </header>

      {mode === "geral" ? (
        <main className={styles.content}>
          <GeralView />
        </main>
      ) : !workspacePath || !occurrence ? (
        <main className={styles.content}>
          <p className={styles.loading}>
            Abra uma ocorrência para ver o painel do caso — ou use o modo{" "}
            <strong>Geral</strong> (todos os casos).
          </p>
        </main>
      ) : (
        <>
          <nav className={styles.tabStrip} role="tablist">
            {PAGES.map((p) => (
              <button
                key={p.key}
                type="button"
                role="tab"
                aria-selected={page === p.key}
                className={`${styles.tab} ${page === p.key ? styles.tabActive : ""}`}
                onClick={() => setPage(p.key)}
              >
                <p.icon size={14} aria-hidden />
                <span>{p.label}</span>
              </button>
            ))}
          </nav>
          <main className={styles.content}>
            {loading && !model ? (
              <p className={styles.loading}>Calculando estatísticas…</p>
            ) : !model ? (
              <p className={styles.loading}>Sem dados.</p>
            ) : page === "overview" ? (
              <OverviewPage model={model} />
            ) : page === "evidence" ? (
              <EvidencePage model={model} />
            ) : page === "operational" ? (
              <OperationalPage model={model} />
            ) : page === "production" ? (
              <ProductionPage model={model} />
            ) : page === "video" ? (
              <VideoPage model={model} />
            ) : (
              <TimelinePage model={model} />
            )}
          </main>
        </>
      )}
    </div>
  );
}

// ==========================================================================
// Páginas

function OverviewPage({ model }: { model: StatisticsModel }) {
  return (
    <>
      <div className={styles.kpiStrip}>
        {model.headline.map((k) => (
          <KpiCard key={k.key} label={k.label} value={k.value} sub={k.sub} tone={k.tone} />
        ))}
      </div>
      <div className={styles.grid}>
        <ChartCard title="Evidências por tipo" subtitle="Tudo que o caso reúne">
          <HBarChart data={model.evidenceByKind} />
        </ChartCard>
        <ChartCard title="Integridade" subtitle="Verificação em disco">
          <DonutChart data={model.integrityByStatus} centerLabel="itens" />
        </ChartCard>
        <ChartCard
          title="Produção ao longo do tempo"
          subtitle="Itens criados por período"
          span="wide"
        >
          <LineChart data={model.productionOverTime} />
        </ChartCard>
        <ChartCard
          title="Checklist de campo"
          subtitle={`${Math.round(model.checklistCompletionPct)}% respondido`}
        >
          <ProgressBar
            value={model.checklistCompletionPct}
            label="Respondido"
            tone={model.checklistCompletionPct >= 80 ? "ok" : "warn"}
          />
          <div style={{ height: 12 }} />
          <DonutChart data={model.checklistBreakdown} centerLabel="itens" />
        </ChartCard>
        <ChartCard title="Dossiê operacional" subtitle="Coletado em campo">
          <HBarChart
            data={[
              { label: "Vestígios", value: model.counts.vestigios ?? 0 },
              { label: "Medições", value: model.counts.medicoes_campo ?? 0 },
              { label: "Veículos", value: model.counts.veiculos ?? 0 },
              { label: "Vítimas", value: model.counts.vitimas ?? 0 },
              { label: "Observações", value: model.counts.observacoes ?? 0 },
            ].filter((s) => s.value > 0)}
          />
        </ChartCard>
      </div>
    </>
  );
}

function EvidencePage({ model }: { model: StatisticsModel }) {
  const i = model.integrity;
  return (
    <>
      <div className={styles.kpiStrip}>
        <KpiCard label="Evidências" value={nf(i.total)} tone="accent" />
        <KpiCard label="Em disco" value={fmtBytes(i.bytesTotal)} />
        <KpiCard label="Arquivos íntegros" value={nf(i.filesOk)} tone="ok" />
        <KpiCard
          label="Com problema"
          value={nf(i.problems)}
          tone={i.problems > 0 ? "crit" : "ok"}
        />
        <KpiCard label="Vinculadas a laudos" value={nf(i.linkedInLaudos)} />
      </div>
      <div className={styles.grid}>
        <ChartCard title="Evidências por tipo">
          <HBarChart data={model.evidenceByKind} />
        </ChartCard>
        <ChartCard title="Integridade por status">
          <DonutChart data={model.integrityByStatus} centerLabel="itens" />
        </ChartCard>
        <ChartCard title="Tamanho em disco por tipo" subtitle="megabytes">
          <HBarChart data={model.sizeByKindMB} unit="MB" />
        </ChartCard>
        <ChartCard
          title="Produção ao longo do tempo"
          subtitle="Itens criados por período"
          span="wide"
        >
          <LineChart data={model.productionOverTime} />
        </ChartCard>
        <ChartCard
          title="Links quebrados em laudos"
          subtitle="Evidências referenciadas que falharam"
          span="full"
          footnote="Vazio = todos os vínculos de evidência nos laudos resolvem corretamente."
        >
          <StatTable
            columns={["Laudo", "Nó", "Detalhe"]}
            rows={model.brokenLinks.map((b) => [b.laudo, b.node, b.detail])}
            emptyMessage="Nenhum link quebrado."
          />
        </ChartCard>
      </div>
    </>
  );
}

function OperationalPage({ model }: { model: StatisticsModel }) {
  return (
    <>
      <div className={styles.kpiStrip}>
        <KpiCard label="Veículos" value={nf(model.counts.veiculos ?? 0)} />
        <KpiCard label="Vítimas" value={nf(model.counts.vitimas ?? 0)} />
        <KpiCard label="Vestígios" value={nf(model.counts.vestigios ?? 0)} />
        <KpiCard label="Medições" value={nf(model.counts.medicoes_campo ?? 0)} />
        <KpiCard label="Observações" value={nf(model.counts.observacoes ?? 0)} />
        <KpiCard label="Eventos" value={nf(model.counts.eventos_timeline ?? 0)} />
      </div>
      <div className={styles.grid}>
        <ChartCard
          title="Checklist — respostas"
          subtitle={`${Math.round(model.checklistCompletionPct)}% respondido · ${Math.round(
            model.checklistRequiredPendingPct,
          )}% dos obrigatórios pendentes`}
        >
          <DonutChart data={model.checklistBreakdown} centerLabel="itens" />
        </ChartCard>
        <ChartCard title="Checklist por categoria">
          <HBarChart data={model.checklistByCategory} />
        </ChartCard>
        <ChartCard title="Entidades">
          <DonutChart data={model.entitiesSplit} centerLabel="entidades" />
        </ChartCard>
        <ChartCard title="Vestígios por tipo">
          <HBarChart data={model.tracesByType} />
        </ChartCard>
        <ChartCard
          title="Medições de campo — distribuição"
          subtitle={
            model.measurementsSummary
              ? `n=${model.measurementsSummary.count} · média ${nf(
                  model.measurementsSummary.mean,
                  2,
                )}`
              : undefined
          }
        >
          <Histogram bins={model.measurementsHistogram} />
        </ChartCard>
        <ChartCard title="Medições por unidade">
          <HBarChart data={model.measurementsByUnit} />
        </ChartCard>
        <ChartCard title="Observações por prioridade">
          <DonutChart data={model.notesByPriority} centerLabel="notas" />
        </ChartCard>
        <ChartCard title="Observações por categoria">
          <HBarChart data={model.notesByCategory} />
        </ChartCard>
        <ChartCard
          title="Eventos operacionais no tempo"
          subtitle="Timeline do dossiê"
          span="wide"
        >
          <LineChart data={model.operationalTimeline} />
        </ChartCard>
      </div>
    </>
  );
}

function ProductionPage({ model }: { model: StatisticsModel }) {
  return (
    <>
      <div className={styles.kpiStrip}>
        <KpiCard label="Laudos" value={nf(model.laudosTotal)} tone="accent" />
        <KpiCard label="Exportados PDF" value={nf(model.laudoExports.pdf)} />
        <KpiCard label="Exportados DOCX" value={nf(model.laudoExports.docx)} />
        <KpiCard label="Croquis" value={nf(model.counts.croquis ?? 0)} />
        <KpiCard label="Análises de imagem" value={nf(model.counts.analises_imagem ?? 0)} />
      </div>
      <div className={styles.grid}>
        <ChartCard title="Laudos por status">
          <DonutChart data={model.laudosByStatus} centerLabel="laudos" />
        </ChartCard>
        <ChartCard title="Laudos por assinatura">
          <HBarChart data={model.laudosBySignature} />
        </ChartCard>
        <ChartCard title="Exportações de laudo">
          <BarChart
            data={[
              { label: "PDF", value: model.laudoExports.pdf },
              { label: "DOCX", value: model.laudoExports.docx },
            ]}
          />
        </ChartCard>
        <ChartCard
          title="Produção ao longo do tempo"
          subtitle="Todos os artefatos do workspace"
          span="wide"
        >
          <LineChart data={model.productionOverTime} />
        </ChartCard>
      </div>
    </>
  );
}

function VideoPage({ model }: { model: StatisticsModel }) {
  const note =
    "Velocidades e distâncias são MEDIÇÕES com incerteza própria, exibidas aqui de forma descritiva — não constituem conclusão pericial.";
  return (
    <>
      <div className={styles.kpiStrip}>
        <KpiCard label="Vídeos" value={nf(model.video.count)} tone="accent" />
        <KpiCard
          label="Duração total"
          value={model.video.totalDurationS > 0 ? fmtDurationS(model.video.totalDurationS) : "—"}
        />
        <KpiCard label="Volume" value={fmtBytes(model.video.totalBytes)} />
        <KpiCard label="Frames coletados" value={nf(model.framesCollected)} />
        <KpiCard label="Velocidades" value={nf(model.counts.velocidades ?? 0)} />
        <KpiCard label="Distâncias" value={nf(model.counts.distancias ?? 0)} />
      </div>
      <div className={styles.grid}>
        <ChartCard title="Resoluções de vídeo">
          <HBarChart data={model.video.resolutions} />
        </ChartCard>
        <ChartCard
          title="Velocidades medidas — distribuição"
          subtitle={
            model.speedsSummary
              ? `n=${model.speedsSummary.count} · média ${nf(model.speedsSummary.mean, 1)} km/h`
              : undefined
          }
          footnote={note}
        >
          <Histogram bins={model.speedsHistogram} unit="km/h" />
        </ChartCard>
        <ChartCard
          title="Distâncias medidas — distribuição"
          subtitle={
            model.distancesSummary
              ? `n=${model.distancesSummary.count} · média ${nf(model.distancesSummary.mean, 2)} m`
              : undefined
          }
          footnote={note}
        >
          <Histogram bins={model.distancesHistogram} unit="m" />
        </ChartCard>
        <ChartCard
          title="Velocidades calculadas"
          subtitle="Com intervalo de confiança, quando disponível"
          span="full"
        >
          <StatTable
            columns={["Velocidade (km/h)", "IC 95%", "Data"]}
            rows={model.speeds.map((s) => [
              nf(s.kmh, 1),
              s.ciLow != null && s.ciHigh != null
                ? `${nf(s.ciLow, 1)} – ${nf(s.ciHigh, 1)}`
                : "—",
              new Date(s.createdAt).toLocaleString("pt-BR"),
            ])}
            emptyMessage="Nenhuma velocidade calculada nesta ocorrência."
          />
        </ChartCard>
      </div>
    </>
  );
}

function TimelinePage({ model }: { model: StatisticsModel }) {
  const busiest = [...model.productionOverTime]
    .sort((a, b) => b.value - a.value)
    .slice(0, 8);
  return (
    <>
      <div className={styles.grid}>
        <ChartCard
          title="Produção de artefatos"
          subtitle="Fotos, croquis, vídeos, frames e laudos por período"
          span="full"
        >
          <LineChart data={model.productionOverTime} />
        </ChartCard>
        <ChartCard
          title="Eventos operacionais"
          subtitle="Timeline declarada no dossiê de campo"
          span="full"
        >
          <LineChart data={model.operationalTimeline} />
        </ChartCard>
        <ChartCard title="Períodos mais ativos" subtitle="Top por nº de artefatos" span="wide">
          <StatTable
            columns={["Período", "Artefatos"]}
            rows={busiest.map((t) => [t.label, t.value])}
            emptyMessage="Sem produção registrada."
          />
        </ChartCard>
      </div>
    </>
  );
}
