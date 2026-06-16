/**
 * SummaryTab — resumo OPERACIONAL da ocorrência (lente "SICRO Operacional").
 * A identificação do caso (BO, local, etc.) vive no cabeçalho editável do
 * Dossiê (CaseHeader); aqui ficam os dados específicos da coleta de campo:
 * tempos de deslocamento, origem do pacote .sicroapp e volumes importados.
 */

import { formatDateTime } from "@core/formatters";
import type { DossieSummary } from "@domain/dossie";
import shared from "./shared.module.css";

export function SummaryTab({ summary }: { summary: DossieSummary }) {
  const o = summary.occurrence;
  const imp = summary.latest_import;

  return (
    <div className={shared.tab}>
      <section className={shared.card}>
        <header className={shared.cardHeader}>
          <h2 className={shared.cardTitle}>Tempos</h2>
        </header>
        <dl className={shared.metaGrid}>
          <Row label="Acionamento" value={dt(o.data_acionamento)} />
          <Row label="Chegada" value={dt(o.data_chegada)} />
          <Row label="Encerramento" value={dt(o.data_encerramento)} />
          <Row
            label="Duração total"
            value={
              summary.stats?.duration_seconds != null
                ? humanDuration(summary.stats.duration_seconds)
                : null
            }
          />
        </dl>
      </section>

      <section className={shared.card}>
        <header className={shared.cardHeader}>
          <h2 className={shared.cardTitle}>Origem</h2>
        </header>
        <dl className={shared.metaGrid}>
          <Row label="ID mobile original" value={o.original_mobile_id ?? null} mono />
          <Row
            label="Pacote"
            value={imp ? `${imp.original_filename ?? "(.sicroapp)"}` : null}
          />
          <Row
            label="Versão do pacote"
            value={imp ? `${imp.format} ${imp.schema_version}` : null}
            mono
          />
          <Row
            label="SHA-256"
            value={imp ? `${imp.package_sha256.slice(0, 24)}…` : null}
            mono
          />
          <Row
            label="Importado em"
            value={imp ? formatDateTime(imp.imported_at) : null}
          />
        </dl>
      </section>

      <section className={shared.card}>
        <header className={shared.cardHeader}>
          <h2 className={shared.cardTitle}>Volumes importados</h2>
        </header>
        <div className={shared.summary}>
          <ChipStat label="Fotos" value={summary.counts.photos} />
          <ChipStat label="Veículos" value={summary.counts.vehicles} />
          <ChipStat label="Vítimas" value={summary.counts.victims} />
          <ChipStat label="Vestígios" value={summary.counts.traces} />
          <ChipStat label="Medições" value={summary.counts.measurements} />
          <ChipStat label="Observações" value={summary.counts.notes} />
          <ChipStat label="Eventos" value={summary.counts.timeline} />
          <ChipStat
            label="Checklist"
            value={`${summary.counts.checklist.answered}/${summary.counts.checklist.total}`}
          />
        </div>
      </section>
    </div>
  );
}

function Row({
  label,
  value,
  mono,
  icon,
}: {
  label: string;
  value: string | null | undefined;
  mono?: boolean;
  icon?: React.ReactNode;
}) {
  return (
    <>
      <dt>{label}</dt>
      <dd className={mono ? shared.mono : undefined}>
        {icon && <span className={shared.dim}>{icon}</span>}
        {value ?? <span className={shared.dim}>—</span>}
      </dd>
    </>
  );
}

function ChipStat({ label, value }: { label: string; value: number | string }) {
  return (
    <div>
      <strong>{value}</strong>
      <span>{label}</span>
    </div>
  );
}

function dt(iso: string | null | undefined): string | null {
  if (!iso) return null;
  return formatDateTime(iso);
}

function humanDuration(seconds: number): string {
  if (seconds < 60) return `${seconds}s`;
  const m = Math.floor(seconds / 60);
  if (m < 60) return `${m} min`;
  const h = Math.floor(m / 60);
  const rest = m % 60;
  return `${h}h ${rest}min`;
}
