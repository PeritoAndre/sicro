/**
 * OperacionalPanel — modo "Operacional" do Dossiê.
 *
 * É uma das DUAS lentes do módulo Dossiê (a outra é a Integridade/Custódia,
 * em `@modules/evidencias/IntegridadePanel`). Mostra o conteúdo semântico
 * coletado em campo e importado do pacote `.sicroapp`: resumo, fotos,
 * checklist, entidades, vestígios, medições, observações, timeline e a
 * auditoria da importação.
 *
 * Renderiza APENAS a faixa de abas + o conteúdo — o cabeçalho da ocorrência
 * e o seletor de modo vivem no `DossieModule`. A re-hidratação manual fica na
 * aba "Importação"; aqui só disparamos a automática (uma vez) quando o dossiê
 * parece vazio mas há um pacote staged.
 */

import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import {
  AlertTriangle,
  CheckSquare,
  FileArchive,
  FileText,
  History,
  Image as ImageIcon,
  MapPin,
  Ruler,
  StickyNote,
  Users,
} from "lucide-react";
import { commands } from "@core/commands";
import { toSicroError } from "@core/errors";
import { useShortcuts } from "@core/useShortcuts";
import type { DossieSummary } from "@domain/dossie";
import styles from "./DossieModule.module.css";

import { SummaryTab } from "./tabs/SummaryTab";
import { PhotosTab } from "./tabs/PhotosTab";
import { ChecklistTab } from "./tabs/ChecklistTab";
import { EntitiesTab } from "./tabs/EntitiesTab";
import { TracesTab } from "./tabs/TracesTab";
import { MeasurementsTab } from "./tabs/MeasurementsTab";
import { NotesTab } from "./tabs/NotesTab";
import { TimelineTab } from "./tabs/TimelineTab";
import { ImportTab } from "./tabs/ImportTab";

type TabKey =
  | "summary"
  | "photos"
  | "checklist"
  | "entities"
  | "traces"
  | "measurements"
  | "notes"
  | "timeline"
  | "import";

interface TabDef {
  key: TabKey;
  label: string;
  icon: typeof ImageIcon;
  /** Badge text shown next to the label; null = hide. */
  badge?: string | null;
}

interface Props {
  workspacePath: string;
}

export function OperacionalPanel({ workspacePath }: Props) {
  const [summary, setSummary] = useState<DossieSummary | null>(null);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [tab, setTab] = useState<TabKey>("summary");
  const autoRehydrateAttemptedRef = useRef(false);

  const loadSummary = useCallback(async (path: string) => {
    setLoading(true);
    setError(null);
    try {
      const s = await commands.getDossieSummary(path);
      setSummary(s);
    } catch (err) {
      setError(toSicroError(err).message);
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => {
    void loadSummary(workspacePath);
  }, [workspacePath, loadSummary]);

  // Auto-rehydrate when the dossier looks empty but there's a staged package
  // available. Runs once per mount. The manual button lives on the "Importação"
  // tab, so failures here can stay silent.
  useEffect(() => {
    if (!summary) return;
    if (autoRehydrateAttemptedRef.current) return;
    if (!summary.latest_import) return;

    const c = summary.counts;
    const isEmpty =
      c.checklist.total === 0 &&
      c.vehicles === 0 &&
      c.victims === 0 &&
      c.traces === 0 &&
      c.measurements === 0 &&
      c.notes === 0 &&
      c.timeline === 0;
    if (!isEmpty) return;

    autoRehydrateAttemptedRef.current = true;
    void (async () => {
      try {
        const outcome = await commands.rehydrateDossie(workspacePath);
        if (outcome.rehydrated) {
          await loadSummary(workspacePath);
        }
      } catch {
        // Silent — manual button still exposed on the Importação tab.
      }
    })();
  }, [workspacePath, summary, loadSummary]);

  const tabs = useMemo<TabDef[]>(
    () => [
      { key: "summary", label: "Resumo", icon: FileText, badge: null },
      {
        key: "photos",
        label: "Fotos",
        icon: ImageIcon,
        badge: summary ? String(summary.counts.photos) : null,
      },
      {
        key: "checklist",
        label: "Checklist",
        icon: CheckSquare,
        badge: summary
          ? `${summary.counts.checklist.answered}/${summary.counts.checklist.total}`
          : null,
      },
      {
        key: "entities",
        label: "Entidades",
        icon: Users,
        badge: summary
          ? String(summary.counts.vehicles + summary.counts.victims)
          : null,
      },
      {
        key: "traces",
        label: "Vestígios",
        icon: MapPin,
        badge: summary ? String(summary.counts.traces) : null,
      },
      {
        key: "measurements",
        label: "Medições",
        icon: Ruler,
        badge: summary ? String(summary.counts.measurements) : null,
      },
      {
        key: "notes",
        label: "Observações",
        icon: StickyNote,
        badge: summary ? String(summary.counts.notes) : null,
      },
      {
        key: "timeline",
        label: "Timeline",
        icon: History,
        badge: summary ? String(summary.counts.timeline) : null,
      },
      { key: "import", label: "Importação", icon: FileArchive, badge: null },
    ],
    [summary],
  );

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
            {t.badge != null && <span className={styles.badge}>{t.badge}</span>}
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
          <p className={styles.loading}>Carregando dossiê…</p>
        ) : !summary ? (
          <p className={styles.loading}>Sem dados.</p>
        ) : (
          <>
            {tab === "summary" && <SummaryTab summary={summary} />}
            {tab === "photos" && <PhotosTab workspacePath={workspacePath} />}
            {tab === "checklist" && (
              <ChecklistTab workspacePath={workspacePath} />
            )}
            {tab === "entities" && <EntitiesTab workspacePath={workspacePath} />}
            {tab === "traces" && <TracesTab workspacePath={workspacePath} />}
            {tab === "measurements" && (
              <MeasurementsTab workspacePath={workspacePath} />
            )}
            {tab === "notes" && <NotesTab workspacePath={workspacePath} />}
            {tab === "timeline" && <TimelineTab workspacePath={workspacePath} />}
            {tab === "import" && (
              <ImportTab
                workspacePath={workspacePath}
                summary={summary}
                onRehydrated={() => void loadSummary(workspacePath)}
              />
            )}
          </>
        )}
      </main>
    </>
  );
}
