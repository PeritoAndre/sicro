/**
 * IntegridadePanel — modo "Integridade" do Dossiê (ex-Central de Evidências, MVP 5).
 *
 * Segunda lente do módulo Dossiê (a primeira é a Operacional). É a camada de
 * confiança/custódia: agrega TODA a evidência do workspace (fotos, croquis,
 * vídeos, frames, análises de imagem, laudos, vínculos) e verifica integridade
 * em disco (existência, tamanho, SHA-256, links quebrados, caminhos inseguros).
 *
 * Somente leitura — NUNCA muta o estado de outro módulo. As ações se limitam a
 * abrir arquivos, revelá-los na pasta, copiar caminhos/refs e gerar o relatório
 * de integridade em HTML.
 *
 * Renderiza APENAS a faixa de abas + o conteúdo — o cabeçalho da ocorrência e o
 * seletor de modo vivem no `DossieModule`.
 */

import { useCallback, useEffect, useMemo, useState } from "react";
import {
  AlertTriangle,
  Boxes,
  Camera,
  Film,
  FileScan,
  FileSearch,
  Headphones,
  ImagePlus,
  Layers,
  Link as LinkIcon,
  Map as MapIcon,
  ScrollText,
  ShieldCheck,
} from "lucide-react";
import { commands } from "@core/commands";
import { toSicroError } from "@core/errors";
import { useShortcuts } from "@core/useShortcuts";
import type {
  EvidenceRegistryItem,
  RegistrySummary,
  WorkspaceIntegrityReport,
} from "@domain/evidence_registry";
import { SummaryTab } from "./tabs/SummaryTab";
import { AllItemsTab } from "./tabs/AllItemsTab";
import { PhotosTab } from "./tabs/PhotosTab";
import { CroquisTab } from "./tabs/CroquisTab";
import { VideosTab } from "./tabs/VideosTab";
import { FramesTab } from "./tabs/FramesTab";
import { KindTab } from "./tabs/KindTab";
import { LaudosLinksTab } from "./tabs/LaudosLinksTab";
import { IntegrityTab } from "./tabs/IntegrityTab";
import { LogsTab } from "./tabs/LogsTab";
import styles from "./EvidenciasModule.module.css";

type TabKey =
  | "summary"
  | "all"
  | "photos"
  | "croquis"
  | "videos"
  | "frames"
  | "audios"
  | "imagens"
  | "documentos"
  | "laudos"
  | "integrity"
  | "logs";

interface TabDef {
  key: TabKey;
  label: string;
  icon: typeof Boxes;
  badge?: string | null;
  badgeKind?: "neutral" | "alert" | "critical";
}

interface Props {
  workspacePath: string;
}

export function IntegridadePanel({ workspacePath }: Props) {
  const [items, setItems] = useState<EvidenceRegistryItem[] | null>(null);
  const [summary, setSummary] = useState<RegistrySummary | null>(null);
  const [report, setReport] = useState<WorkspaceIntegrityReport | null>(null);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [tab, setTab] = useState<TabKey>("summary");

  /** Reload registry + summary. The integrity tab manages its own
   *  deep-verification life cycle. */
  const reload = useCallback(async (path: string) => {
    setLoading(true);
    setError(null);
    try {
      // Run verify (deep=false) once — it gives us BOTH the items
      // already enriched with integrity status AND the summary.
      const r = await commands.verifyWorkspaceIntegrity(path, { deep: false });
      setReport(r);
      setItems(r.items);
      setSummary(r.summary);
    } catch (err) {
      setError(toSicroError(err).message);
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => {
    void reload(workspacePath);
  }, [workspacePath, reload]);

  const tabs = useMemo<TabDef[]>(() => {
    const s = summary;
    const overall = s?.overall_status ?? "ok";
    const badgeKindFor = (count: number, critical = false) =>
      count > 0 ? (critical ? "critical" : "alert") : "neutral";

    return [
      {
        key: "summary",
        label: "Resumo",
        icon: ShieldCheck,
        badge:
          overall === "ok"
            ? "íntegro"
            : overall === "warning"
              ? "atenção"
              : overall === "critical"
                ? "crítico"
                : null,
        badgeKind:
          overall === "critical"
            ? "critical"
            : overall === "warning"
              ? "alert"
              : "neutral",
      },
      {
        key: "all",
        label: "Todas",
        icon: Boxes,
        badge: s ? String(s.total_items) : null,
      },
      {
        key: "photos",
        label: "Fotos",
        icon: Camera,
        badge: s ? String(s.photos) : null,
      },
      {
        key: "croquis",
        label: "Croquis",
        icon: MapIcon,
        badge: s ? String(s.croquis) : null,
      },
      {
        key: "videos",
        label: "Vídeos",
        icon: Film,
        badge: s ? String(s.videos) : null,
      },
      {
        key: "frames",
        label: "Frames",
        icon: Layers,
        badge: s ? String(s.storyboard_frames) : null,
      },
      {
        key: "audios",
        label: "Áudios",
        icon: Headphones,
        badge: items
          ? String(items.filter((i) => i.kind === "audio").length)
          : null,
      },
      {
        key: "imagens",
        label: "Imagens",
        icon: ImagePlus,
        badge: items
          ? String(
              items.filter(
                (i) =>
                  i.kind === "image_analysis" || i.kind === "image_export",
              ).length,
            )
          : null,
      },
      {
        key: "documentos",
        label: "Documentoscopia",
        icon: FileScan,
        badge: items
          ? String(items.filter((i) => i.kind === "document").length)
          : null,
      },
      {
        key: "laudos",
        label: "Laudos & vínculos",
        icon: LinkIcon,
        badge: s ? `${s.laudos}/${s.linked_in_laudos}` : null,
      },
      {
        key: "integrity",
        label: "Integridade",
        icon: FileSearch,
        badge: s
          ? s.files_missing + s.broken_links + s.unsafe_paths + s.hash_mismatches >
            0
            ? String(
                s.files_missing +
                  s.broken_links +
                  s.unsafe_paths +
                  s.hash_mismatches,
              )
            : "ok"
          : null,
        badgeKind: s
          ? badgeKindFor(
              s.files_missing + s.broken_links,
              s.unsafe_paths + s.hash_mismatches > 0,
            )
          : "neutral",
      },
      {
        key: "logs",
        label: "Logs",
        icon: ScrollText,
        badge: null,
      },
    ];
  }, [summary, items]);

  // Atalhos: próxima/anterior aba (Ctrl+PgDn / Ctrl+PgUp por padrão).
  const cycleTab = (dir: number) => {
    const keys = tabs.map((t) => t.key);
    const idx = keys.indexOf(tab);
    const next = keys[(idx + dir + keys.length) % keys.length];
    if (next) setTab(next);
  };
  useShortcuts({
    "dossie.tab.next": () => cycleTab(1),
    "dossie.tab.prev": () => cycleTab(-1),
  });

  return (
    <>
      <nav className={styles.tabStrip} role="tablist">
        {tabs.map((t) => (
          <button
            key={t.key}
            type="button"
            role="tab"
            aria-selected={tab === t.key}
            className={`${styles.tab} ${tab === t.key ? styles.tabActive : ""}`}
            onClick={() => setTab(t.key)}
          >
            <t.icon size={14} aria-hidden />
            <span>{t.label}</span>
            {t.badge != null && (
              <span
                className={`${styles.badge} ${
                  t.badgeKind === "alert"
                    ? styles.badgeAlert
                    : t.badgeKind === "critical"
                      ? styles.badgeCritical
                      : ""
                }`}
              >
                {t.badge}
              </span>
            )}
          </button>
        ))}
      </nav>

      <main className={styles.content}>
        {error && (
          <div className={styles.errorBanner}>
            <AlertTriangle size={14} /> {error}
          </div>
        )}
        {loading && !summary ? (
          <p className={styles.loading}>Carregando registro…</p>
        ) : !summary || !items ? (
          <p className={styles.loading}>Sem dados.</p>
        ) : (
          <>
            {tab === "summary" && (
              <SummaryTab
                summary={summary}
                report={report}
                workspacePath={workspacePath}
                onReload={() => void reload(workspacePath)}
              />
            )}
            {tab === "all" && (
              <AllItemsTab
                items={items}
                workspacePath={workspacePath}
                onReload={() => void reload(workspacePath)}
              />
            )}
            {tab === "photos" && (
              <PhotosTab items={items} workspacePath={workspacePath} />
            )}
            {tab === "croquis" && (
              <CroquisTab items={items} workspacePath={workspacePath} />
            )}
            {tab === "videos" && (
              <VideosTab items={items} workspacePath={workspacePath} />
            )}
            {tab === "frames" && (
              <FramesTab items={items} workspacePath={workspacePath} />
            )}
            {tab === "audios" && (
              <KindTab
                items={items}
                workspacePath={workspacePath}
                kinds={["audio"]}
                emptyHint="Nenhum áudio neste caso."
              />
            )}
            {tab === "imagens" && (
              <KindTab
                items={items}
                workspacePath={workspacePath}
                kinds={["image_analysis", "image_export"]}
                emptyHint="Nenhuma análise de imagem neste caso."
              />
            )}
            {tab === "documentos" && (
              <KindTab
                items={items}
                workspacePath={workspacePath}
                kinds={["document"]}
                emptyHint="Nenhum documento (Documentoscopia) neste caso."
              />
            )}
            {tab === "laudos" && (
              <LaudosLinksTab
                items={items}
                workspacePath={workspacePath}
                brokenLinks={report?.broken_laudo_links ?? []}
              />
            )}
            {tab === "integrity" && (
              <IntegrityTab
                workspacePath={workspacePath}
                summary={summary}
                report={report}
                onReportRefresh={(r) => {
                  setReport(r);
                  setItems(r.items);
                  setSummary(r.summary);
                }}
              />
            )}
            {tab === "logs" && (
              <LogsTab
                workspacePath={workspacePath}
                videos={items.filter((i) => i.kind === "video")}
              />
            )}
          </>
        )}
      </main>
    </>
  );
}
