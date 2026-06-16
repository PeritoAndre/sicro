/**
 * Exportação das estatísticas em JSON (máquina), CSV (planilha) e HTML
 * (relatório imprimível, estilo anexo pericial). Tudo gerado a partir do
 * `StatisticsModel` — puro, sem I/O (a escrita em disco é do backend).
 */

import { fmtDurationS, fmtPct, nf } from "./format";
import type { GeneralModel } from "./general";
import type { CategorySlice, NumericSummary, StatisticsModel } from "./model";

export function buildJson(model: StatisticsModel): string {
  return JSON.stringify(model, null, 2);
}

// --------------------------------------------------------------------------

function csvEsc(s: string | number): string {
  const str = String(s);
  return /[";\n]/.test(str) ? `"${str.replace(/"/g, '""')}"` : str;
}

export function buildCsv(model: StatisticsModel): string {
  const rows: string[] = [];
  const line = (...cells: (string | number)[]) =>
    rows.push(cells.map(csvEsc).join(";"));

  line("Seção", "Categoria", "Valor");
  line("Caso", "Ocorrência", model.occurrence?.label ?? "—");
  line("Caso", "Gerado em", model.generatedAt);

  for (const [k, v] of Object.entries(model.counts)) line("Contagem", k, v);

  const dump = (section: string, slices: CategorySlice[]) =>
    slices.forEach((s) => line(section, s.label, round(s.value)));

  dump("Evidências por tipo", model.evidenceByKind);
  dump("Integridade", model.integrityByStatus);
  dump("Tamanho por tipo (MB)", model.sizeByKindMB);
  dump("Checklist", model.checklistBreakdown);
  dump("Checklist por categoria", model.checklistByCategory);
  dump("Entidades", model.entitiesSplit);
  dump("Vestígios por tipo", model.tracesByType);
  dump("Medições por unidade", model.measurementsByUnit);
  dump("Observações por prioridade", model.notesByPriority);
  dump("Laudos por status", model.laudosByStatus);
  dump("Laudos por assinatura", model.laudosBySignature);
  dump("Resoluções de vídeo", model.video.resolutions);

  const sumRows = (section: string, s: NumericSummary | null) => {
    if (!s) return;
    line(section, "n", s.count);
    line(section, "média", round(s.mean));
    line(section, "mediana", round(s.median));
    line(section, "mín", round(s.min));
    line(section, "máx", round(s.max));
    line(section, "desvio-padrão", round(s.stdev));
  };
  sumRows("Velocidade (km/h)", model.speedsSummary);
  sumRows("Distância (m)", model.distancesSummary);
  sumRows("Medições (valor)", model.measurementsSummary);

  return rows.join("\n");
}

function round(v: number): number {
  return Math.round(v * 1000) / 1000;
}

// --------------------------------------------------------------------------

function htmlEsc(s: string): string {
  return s
    .replace(/&/g, "&amp;")
    .replace(/</g, "&lt;")
    .replace(/>/g, "&gt;")
    .replace(/"/g, "&quot;");
}

function sliceTableHtml(
  title: string,
  slices: CategorySlice[],
  opts: { unit?: string; decimals?: number } = {},
): string {
  if (!slices.length) return "";
  const total = slices.reduce((s, d) => s + d.value, 0);
  const body = slices
    .map((s) => {
      const val = nf(s.value, opts.decimals ?? (Number.isInteger(s.value) ? 0 : 1));
      const u = opts.unit ? ` ${opts.unit}` : "";
      const p = total > 0 ? fmtPct((s.value / total) * 100, 0) : "—";
      return `<tr><td>${htmlEsc(s.label)}</td><td class="n">${val}${u}</td><td class="n dim">${p}</td></tr>`;
    })
    .join("");
  return `<section class="block"><h2>${htmlEsc(title)}</h2><table><thead><tr><th>Categoria</th><th class="n">Valor</th><th class="n">%</th></tr></thead><tbody>${body}</tbody></table></section>`;
}

function summaryTableHtml(
  title: string,
  s: NumericSummary | null,
  unit: string,
): string {
  if (!s) return "";
  const cell = (v: number) => `${nf(v, 2)} ${unit}`;
  return `<section class="block"><h2>${htmlEsc(title)}</h2><table><tbody>
    <tr><td>Amostras (n)</td><td class="n">${s.count}</td></tr>
    <tr><td>Média</td><td class="n">${cell(s.mean)}</td></tr>
    <tr><td>Mediana</td><td class="n">${cell(s.median)}</td></tr>
    <tr><td>Mínimo</td><td class="n">${cell(s.min)}</td></tr>
    <tr><td>Máximo</td><td class="n">${cell(s.max)}</td></tr>
    <tr><td>Desvio-padrão</td><td class="n">${cell(s.stdev)}</td></tr>
    <tr><td>Faixa p2,5–p97,5</td><td class="n">${nf(s.p2_5, 2)} – ${nf(s.p97_5, 2)} ${unit}</td></tr>
  </tbody></table></section>`;
}

export function buildHtml(model: StatisticsModel): string {
  const occ = model.occurrence;
  const generated = new Date(model.generatedAt).toLocaleString("pt-BR");

  const kpis = model.headline
    .map(
      (k) =>
        `<div class="kpi"><div class="kpiv">${htmlEsc(k.value)}</div><div class="kpil">${htmlEsc(
          k.label,
        )}</div>${k.sub ? `<div class="kpis">${htmlEsc(k.sub)}</div>` : ""}</div>`,
    )
    .join("");

  const occRows = occ
    ? [
        ["Ocorrência", occ.label],
        ["BO", occ.numero_bo ?? "—"],
        ["Tipo de perícia", occ.tipo_pericia ?? "—"],
        ["Natureza", occ.natureza ?? "—"],
        ["Município", occ.municipio ?? "—"],
        ["Status", occ.status],
        ["Peritos", occ.peritos.join(", ") || "—"],
        [
          "Duração em campo",
          occ.fieldDurationMin != null ? fmtDurationS(occ.fieldDurationMin * 60) : "—",
        ],
        [
          "Melhor precisão GPS",
          occ.gpsAccuracyM != null ? `${nf(occ.gpsAccuracyM, 1)} m` : "—",
        ],
      ]
        .map(
          ([k, v]) =>
            `<tr><td class="dim">${htmlEsc(k ?? "")}</td><td>${htmlEsc(v ?? "—")}</td></tr>`,
        )
        .join("")
    : "";

  const integrityNote =
    model.integrity.problems > 0
      ? `<p class="warn">⚠ ${nf(model.integrity.problems)} item(ns) com problema de integridade: ${nf(
          model.integrity.filesMissing,
        )} ausente(s), ${nf(model.integrity.hashMismatches)} hash divergente(s), ${nf(
          model.integrity.brokenLinks,
        )} link(s) quebrado(s), ${nf(model.integrity.unsafePaths)} caminho(s) inseguro(s).</p>`
      : `<p class="ok">✓ Integridade verificada sem problemas (${nf(model.integrity.filesOk)} arquivo(s)).</p>`;

  const sections = [
    sliceTableHtml("Evidências por tipo", model.evidenceByKind),
    sliceTableHtml("Integridade por status", model.integrityByStatus),
    sliceTableHtml("Tamanho em disco por tipo", model.sizeByKindMB, {
      unit: "MB",
      decimals: 1,
    }),
    sliceTableHtml("Checklist — respostas", model.checklistBreakdown),
    sliceTableHtml("Checklist por categoria", model.checklistByCategory),
    sliceTableHtml("Entidades", model.entitiesSplit),
    sliceTableHtml("Vestígios por tipo", model.tracesByType),
    sliceTableHtml("Medições por unidade", model.measurementsByUnit),
    sliceTableHtml("Observações por prioridade", model.notesByPriority),
    sliceTableHtml("Observações por categoria", model.notesByCategory),
    sliceTableHtml("Laudos por status", model.laudosByStatus),
    sliceTableHtml("Laudos por assinatura", model.laudosBySignature),
    sliceTableHtml("Resoluções de vídeo", model.video.resolutions),
    summaryTableHtml("Velocidades medidas (km/h)", model.speedsSummary, "km/h"),
    summaryTableHtml("Distâncias medidas (m)", model.distancesSummary, "m"),
    summaryTableHtml("Medições de campo (valor)", model.measurementsSummary, ""),
  ]
    .filter(Boolean)
    .join("\n");

  return `<!doctype html>
<html lang="pt-BR"><head><meta charset="utf-8" />
<title>Estatísticas — ${htmlEsc(occ?.label ?? "Ocorrência")}</title>
<style>
  :root { color-scheme: light; }
  * { box-sizing: border-box; }
  body { font-family: "Segoe UI", Inter, system-ui, sans-serif; color: #16202c; background: #fff; margin: 0; padding: 32px; }
  h1 { font-size: 20px; margin: 0 0 4px; }
  .sub { color: #66758c; font-size: 12px; margin: 0 0 20px; }
  h2 { font-size: 13px; text-transform: uppercase; letter-spacing: .05em; color: #44546a; border-bottom: 1px solid #e2e7ef; padding-bottom: 4px; margin: 0 0 8px; }
  .kpis-grid { display: grid; grid-template-columns: repeat(auto-fill, minmax(150px, 1fr)); gap: 10px; margin: 16px 0 24px; }
  .kpi { border: 1px solid #d3dae4; border-left: 3px solid #d7a84f; border-radius: 6px; padding: 10px 12px; }
  .kpiv { font-size: 22px; font-weight: 600; font-family: "JetBrains Mono", Consolas, monospace; }
  .kpil { font-size: 11px; text-transform: uppercase; letter-spacing: .04em; color: #44546a; }
  .kpis { font-size: 10px; color: #66758c; }
  .grid { display: grid; grid-template-columns: repeat(auto-fill, minmax(280px, 1fr)); gap: 18px; }
  .block table { width: 100%; border-collapse: collapse; font-size: 12px; }
  .block th, .block td { text-align: left; padding: 4px 8px; border-bottom: 1px solid #eef1f6; }
  .block th.n, .block td.n { text-align: right; font-family: "JetBrains Mono", Consolas, monospace; }
  .dim { color: #8a97a8; }
  .ok { color: #1f9d57; font-size: 12px; }
  .warn { color: #b9791f; font-size: 12px; }
  footer { margin-top: 28px; padding-top: 10px; border-top: 1px solid #e2e7ef; color: #8a97a8; font-size: 10px; }
  @media print { body { padding: 0; } }
</style></head>
<body>
  <h1>Estatísticas do caso — ${htmlEsc(occ?.label ?? "Ocorrência")}</h1>
  <p class="sub">Relatório descritivo gerado pelo SICRO Desktop em ${htmlEsc(generated)}. Contagens e distribuições do que o caso armazena — sem interpretação pericial.</p>
  ${occRows ? `<section class="block"><h2>Identificação</h2><table><tbody>${occRows}</tbody></table></section>` : ""}
  <div class="kpis-grid">${kpis}</div>
  ${integrityNote}
  <div class="grid">${sections}</div>
  <footer>SICRO Desktop 2.0 — módulo Estatísticas. Documento descritivo; não substitui o laudo nem constitui conclusão pericial.</footer>
</body></html>`;
}

// ==========================================================================
// Exportação das estatísticas GERAIS (entre casos)

export function buildGeneralJson(model: GeneralModel): string {
  return JSON.stringify(model, null, 2);
}

export function buildGeneralCsv(model: GeneralModel): string {
  const rows: string[] = [];
  const line = (...cells: (string | number)[]) =>
    rows.push(cells.map(csvEsc).join(";"));

  line("Seção", "Categoria", "Valor");
  line("Geral", "Período", model.filterLabel);
  line("Geral", "Casos no período", model.totalInPeriod);
  line("Geral", "Casos no total", model.totalAll);
  line("Geral", "Concluídos", model.concluded);
  line("Geral", "Em aberto", model.open);

  const dump = (section: string, slices: CategorySlice[]) =>
    slices.forEach((s) => line(section, s.label, s.value));

  dump("Por status", model.byStatus);
  dump("Por tipo de perícia", model.byType);
  dump("Por município", model.byMunicipio);
  dump("Por perito", model.byPerito);
  dump("Por natureza", model.byNatureza);
  dump("Por mês do ano", model.byMonthOfYear);
  dump("Por dia da semana", model.byWeekday);
  model.overTime.forEach((t) => line("Ao longo do tempo", t.label, t.value));

  if (model.cycleTime) {
    line("Tempo de conclusão (dias)", "n", model.cycleTime.count);
    line("Tempo de conclusão (dias)", "média", round(model.cycleTime.mean));
    line("Tempo de conclusão (dias)", "mediana", round(model.cycleTime.median));
    line("Tempo de conclusão (dias)", "mín", round(model.cycleTime.min));
    line("Tempo de conclusão (dias)", "máx", round(model.cycleTime.max));
  }

  return rows.join("\n");
}

export function buildGeneralHtml(model: GeneralModel): string {
  const generated = new Date(model.generatedAt).toLocaleString("pt-BR");
  const kpis = model.headline
    .map(
      (k) =>
        `<div class="kpi"><div class="kpiv">${htmlEsc(k.value)}</div><div class="kpil">${htmlEsc(
          k.label,
        )}</div>${k.sub ? `<div class="kpis">${htmlEsc(k.sub)}</div>` : ""}</div>`,
    )
    .join("");

  const overTimeSlices: CategorySlice[] = model.overTime.map((t) => ({
    label: t.label,
    value: t.value,
  }));

  const sections = [
    sliceTableHtml("Casos ao longo do tempo", overTimeSlices),
    sliceTableHtml("Por status", model.byStatus),
    sliceTableHtml("Por tipo de perícia", model.byType),
    sliceTableHtml("Por município", model.byMunicipio),
    sliceTableHtml("Por perito", model.byPerito),
    sliceTableHtml("Por natureza", model.byNatureza),
    sliceTableHtml("Sazonalidade (mês do ano)", model.byMonthOfYear),
    sliceTableHtml("Por dia da semana", model.byWeekday),
    summaryTableHtml("Tempo de conclusão (dias)", model.cycleTime, "dias"),
  ]
    .filter(Boolean)
    .join("\n");

  return `<!doctype html>
<html lang="pt-BR"><head><meta charset="utf-8" />
<title>Estatísticas gerais de trabalho</title>
<style>
  :root { color-scheme: light; }
  * { box-sizing: border-box; }
  body { font-family: "Segoe UI", Inter, system-ui, sans-serif; color: #16202c; background: #fff; margin: 0; padding: 32px; }
  h1 { font-size: 20px; margin: 0 0 4px; }
  .sub { color: #66758c; font-size: 12px; margin: 0 0 20px; }
  h2 { font-size: 13px; text-transform: uppercase; letter-spacing: .05em; color: #44546a; border-bottom: 1px solid #e2e7ef; padding-bottom: 4px; margin: 0 0 8px; }
  .kpis-grid { display: grid; grid-template-columns: repeat(auto-fill, minmax(150px, 1fr)); gap: 10px; margin: 16px 0 24px; }
  .kpi { border: 1px solid #d3dae4; border-left: 3px solid #d7a84f; border-radius: 6px; padding: 10px 12px; }
  .kpiv { font-size: 22px; font-weight: 600; font-family: "JetBrains Mono", Consolas, monospace; }
  .kpil { font-size: 11px; text-transform: uppercase; letter-spacing: .04em; color: #44546a; }
  .kpis { font-size: 10px; color: #66758c; }
  .grid { display: grid; grid-template-columns: repeat(auto-fill, minmax(280px, 1fr)); gap: 18px; }
  .block table { width: 100%; border-collapse: collapse; font-size: 12px; }
  .block th, .block td { text-align: left; padding: 4px 8px; border-bottom: 1px solid #eef1f6; }
  .block th.n, .block td.n { text-align: right; font-family: "JetBrains Mono", Consolas, monospace; }
  .dim { color: #8a97a8; }
  footer { margin-top: 28px; padding-top: 10px; border-top: 1px solid #e2e7ef; color: #8a97a8; font-size: 10px; }
  @media print { body { padding: 0; } }
</style></head>
<body>
  <h1>Estatísticas gerais de trabalho</h1>
  <p class="sub">Período: ${htmlEsc(model.filterLabel)} · ${model.totalInPeriod} de ${model.totalAll} caso(s) indexado(s). Gerado em ${htmlEsc(generated)}. Documento descritivo — sem interpretação pericial.</p>
  <div class="kpis-grid">${kpis}</div>
  <div class="grid">${sections}</div>
  <footer>SICRO Desktop 2.0 — Estatísticas gerais. O índice cobre os casos abertos/criados/importados neste computador.</footer>
</body></html>`;
}
