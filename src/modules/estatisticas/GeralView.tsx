/**
 * GeralView — estatísticas GERAIS de trabalho (entre casos), a partir do índice
 * global de casos. Funciona mesmo sem nenhuma ocorrência aberta.
 *
 * O índice é alimentado quando um caso vira ativo (criar/abrir/importar). O
 * botão "Reindexar" faz backfill a partir dos casos recentes. Tudo descritivo.
 */

import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { Download, RefreshCw } from "lucide-react";
import { Button } from "@components/Button/Button";
import { commands } from "@core/commands";
import { toSicroError } from "@core/errors";
import type { CaseIndexEntry } from "@domain/case_index";
import { reindexCaseIndexFromRecents } from "@core/caseIndex";
import { aggregateGeneral, type PeriodFilter } from "./stats/general";
import { buildGeneralCsv, buildGeneralHtml, buildGeneralJson } from "./stats/export";
import { nf } from "./stats/format";
import {
  BarChart,
  ChartCard,
  DonutChart,
  HBarChart,
  KpiCard,
  LineChart,
  StatTable,
} from "./charts/Charts";
import styles from "./EstatisticasModule.module.css";

const MONTHS = [
  "Janeiro",
  "Fevereiro",
  "Março",
  "Abril",
  "Maio",
  "Junho",
  "Julho",
  "Agosto",
  "Setembro",
  "Outubro",
  "Novembro",
  "Dezembro",
];

export function GeralView() {
  const [entries, setEntries] = useState<CaseIndexEntry[]>([]);
  const [loading, setLoading] = useState(false);
  const [reindexing, setReindexing] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [feedback, setFeedback] = useState<string | null>(null);
  const [exporting, setExporting] = useState<string | null>(null);
  const [filter, setFilter] = useState<PeriodFilter>({ year: null, month: null });
  const autoReindexed = useRef(false);

  const reindexFromRecents = useCallback(async () => {
    setReindexing(true);
    setError(null);
    try {
      const fresh = await reindexCaseIndexFromRecents();
      setEntries(fresh);
      setFeedback(`Índice atualizado: ${fresh.length} caso(s).`);
      setTimeout(() => setFeedback(null), 3500);
    } catch (e) {
      setError(toSicroError(e).message);
    } finally {
      setReindexing(false);
    }
  }, []);

  const load = useCallback(async () => {
    setLoading(true);
    setError(null);
    try {
      const idx = await commands.getCaseIndex();
      setEntries(idx);
      // Primeira visita com índice vazio: tenta backfill dos recentes uma vez.
      if (idx.length === 0 && !autoReindexed.current) {
        autoReindexed.current = true;
        void reindexFromRecents();
      }
    } catch (e) {
      setError(toSicroError(e).message);
    } finally {
      setLoading(false);
    }
  }, [reindexFromRecents]);

  useEffect(() => {
    void load();
  }, [load]);

  const model = useMemo(() => aggregateGeneral(entries, filter), [entries, filter]);

  const handleExport = async (format: "html" | "csv" | "json") => {
    setExporting(format);
    setError(null);
    try {
      const content =
        format === "html"
          ? buildGeneralHtml(model)
          : format === "csv"
            ? buildGeneralCsv(model)
            : buildGeneralJson(model);
      const abs = await commands.saveGeneralStatisticsExport(format, content);
      setFeedback(`Exportado: ${abs}`);
      try {
        await commands.revealPathInExplorer(abs);
      } catch {
        /* reveal best-effort */
      }
      setTimeout(() => setFeedback(null), 4000);
    } catch (e) {
      setError(toSicroError(e).message);
    } finally {
      setExporting(null);
    }
  };

  const onYear = (v: string) => {
    if (v === "") setFilter({ year: null, month: null });
    else setFilter((f) => ({ year: Number(v), month: f.month }));
  };
  const onMonth = (v: string) => {
    setFilter((f) => ({ year: f.year, month: v === "" ? null : Number(v) }));
  };

  return (
    <>
      <div className={styles.geralToolbar}>
        <div className={styles.filters}>
          <label className={styles.filterLabel}>Ano</label>
          <select
            className={styles.select}
            value={filter.year ?? ""}
            onChange={(e) => onYear(e.target.value)}
          >
            <option value="">Todos</option>
            {model.availableYears.map((y) => (
              <option key={y} value={y}>
                {y}
              </option>
            ))}
          </select>
          <label className={styles.filterLabel}>Mês</label>
          <select
            className={styles.select}
            value={filter.month ?? ""}
            onChange={(e) => onMonth(e.target.value)}
            disabled={filter.year == null}
          >
            <option value="">Todos</option>
            {MONTHS.map((m, i) => (
              <option key={m} value={i + 1}>
                {m}
              </option>
            ))}
          </select>
          <span className={styles.filterInfo}>
            {model.totalInPeriod} de {model.totalAll} caso(s)
          </span>
        </div>
        <div className={styles.headActions}>
          {feedback && <span className={styles.feedback}>{feedback}</span>}
          {error && <span className={styles.errorMsg}>{error}</span>}
          <div className={styles.exportGroup}>
            <Download size={13} aria-hidden />
            {(["html", "csv", "json"] as const).map((f) => (
              <button
                key={f}
                type="button"
                className={styles.exportBtn}
                disabled={exporting != null || model.totalAll === 0}
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
            onClick={() => void reindexFromRecents()}
            disabled={reindexing}
          >
            {reindexing ? "Reindexando…" : "Reindexar"}
          </Button>
        </div>
      </div>

      {loading && entries.length === 0 ? (
        <p className={styles.loading}>Carregando índice de casos…</p>
      ) : model.totalAll === 0 ? (
        <div className={styles.geralEmpty}>
          <p>Nenhum caso no índice ainda.</p>
          <p className={styles.geralEmptyHint}>
            O índice é preenchido conforme você abre, cria ou importa casos.
            Clique em <strong>Reindexar</strong> para incluir os casos recentes
            de uma vez.
          </p>
        </div>
      ) : (
        <>
          <div className={styles.kpiStrip}>
            {model.headline.map((k) => (
              <KpiCard
                key={k.key}
                label={k.label}
                value={k.value}
                sub={k.sub}
                tone={k.tone}
              />
            ))}
          </div>
          <div className={styles.grid}>
            <ChartCard
              title="Casos ao longo do tempo"
              subtitle={
                filter.year == null ? "por ano" : `meses de ${filter.year}`
              }
              span="full"
            >
              <LineChart data={model.overTime} />
            </ChartCard>
            <ChartCard title="Por tipo de perícia">
              <HBarChart data={model.byType} />
            </ChartCard>
            <ChartCard title="Por status">
              <DonutChart data={model.byStatus} centerLabel="casos" />
            </ChartCard>
            <ChartCard title="Por município">
              <HBarChart data={model.byMunicipio} />
            </ChartCard>
            <ChartCard title="Por perito" subtitle="um caso conta para cada perito">
              <HBarChart data={model.byPerito} />
            </ChartCard>
            <ChartCard title="Por natureza">
              <HBarChart data={model.byNatureza} />
            </ChartCard>
            <ChartCard title="Sazonalidade (mês do ano)">
              <BarChart data={model.byMonthOfYear} color="var(--sicro-info)" />
            </ChartCard>
            <ChartCard title="Por dia da semana">
              <BarChart data={model.byWeekday} color="var(--sicro-success)" />
            </ChartCard>
            <ChartCard
              title="Tempo de conclusão"
              subtitle="dias entre acionamento e encerramento"
              footnote="Considera apenas casos com ambas as datas preenchidas."
            >
              {model.cycleTime ? (
                <StatTable
                  columns={["Métrica", "Dias"]}
                  rows={[
                    ["Casos", model.cycleTime.count],
                    ["Média", nf(model.cycleTime.mean, 1)],
                    ["Mediana", nf(model.cycleTime.median, 1)],
                    ["Mínimo", nf(model.cycleTime.min, 1)],
                    ["Máximo", nf(model.cycleTime.max, 1)],
                  ]}
                />
              ) : (
                <p className={styles.loading}>Sem casos concluídos com datas.</p>
              )}
            </ChartCard>
            <ChartCard
              title="Casos recentes"
              subtitle={`${model.recent.length} mais recentes do período`}
              span="full"
            >
              <StatTable
                columns={["Caso", "Tipo", "Município", "Status", "Data do fato"]}
                rows={model.recent.map((c) => [
                  c.label,
                  c.tipo ?? "—",
                  c.municipio ?? "—",
                  c.status,
                  c.date ? new Date(c.date).toLocaleDateString("pt-BR") : "—",
                ])}
                emptyMessage="Nenhum caso no período."
              />
            </ChartCard>
          </div>
        </>
      )}
    </>
  );
}
