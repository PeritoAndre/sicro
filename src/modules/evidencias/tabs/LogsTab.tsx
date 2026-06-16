/**
 * LogsTab — consolida logs operacionais do workspace.
 *
 * Fontes atuais:
 *   - `video_operation_logs` (Spike F) — por vídeo, via `list_video_operation_logs`.
 *   - `evidence_links` (MVP 4) — uma linha por inserção de evidência em laudo.
 *
 * Lacunas conhecidas (documentadas no relatório do MVP 5):
 *   - O Importador não tem tabela própria de logs estruturados, apenas
 *     `imports.warnings_json` / `errors_json` (mostrados como linhas
 *     únicas aqui).
 *   - Croqui não tem log próprio: só persiste o último PNG exportado.
 *
 * Quando os logs de outros módulos forem padronizados (futuro), basta
 * adicioná-los ao agregador desta aba.
 */

import { useEffect, useMemo, useState } from "react";
import { commands } from "@core/commands";
import { toSicroError } from "@core/errors";
import type { EvidenceRegistryItem } from "@domain/evidence_registry";
import type { VideoOperationLog } from "@domain/video";
import type { EvidenceLink } from "@domain/evidence";
import type { Import } from "@domain/import";
import { formatDateTime } from "@core/formatters";
import styles from "../EvidenciasModule.module.css";

interface Props {
  workspacePath: string;
  /** Items kind=video — to know which media_hashes to query. */
  videos: EvidenceRegistryItem[];
}

interface LogEntry {
  ts: string; // ISO
  source: string;
  action: string;
  detail: string;
}

export function LogsTab({ workspacePath, videos }: Props) {
  const [logs, setLogs] = useState<LogEntry[] | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [filter, setFilter] = useState<string>("all");

  useEffect(() => {
    let cancelled = false;
    async function load() {
      setError(null);
      try {
        const all: LogEntry[] = [];

        // 1. Video operation logs (per media)
        const hashes = videos
          .map((v) => v.hash_sha256)
          .filter((h): h is string => !!h);
        for (const hash of hashes) {
          try {
            const list: VideoOperationLog[] =
              await commands.listVideoOperationLogs(workspacePath, hash, 200);
            for (const lg of list) {
              all.push({
                ts: lg.created_at,
                source: "video",
                action: lg.action,
                detail: truncateJson(lg.details_json),
              });
            }
          } catch {
            /* não bloquear */
          }
        }

        // 2. evidence_links
        try {
          const links: EvidenceLink[] =
            await commands.listEvidenceLinks(workspacePath);
          for (const l of links) {
            all.push({
              ts: l.created_at,
              source: "laudo",
              action: `inserção: ${l.source_kind}`,
              detail: `target=${l.target_type}/${l.target_id.slice(0, 8)}…${
                l.relative_path ? ` · ${l.relative_path}` : ""
              }`,
            });
          }
        } catch {
          /* */
        }

        // 3. imports — read warnings/errors as text rows so something is
        // visible until the importer has its own log table.
        try {
          const imports: Import[] =
            await commands.listWorkspaceImports(workspacePath);
          for (const imp of imports) {
            all.push({
              ts: imp.imported_at,
              source: "importer",
              action: `import.${imp.status}`,
              detail: `${imp.original_filename ?? imp.id} · sha ${imp.package_sha256.slice(0, 10)}…`,
            });
            const w = safeParseStringArray(imp.warnings_json);
            for (const msg of w) {
              all.push({
                ts: imp.imported_at,
                source: "importer",
                action: "import.warning",
                detail: msg,
              });
            }
            const e = safeParseStringArray(imp.errors_json);
            for (const msg of e) {
              all.push({
                ts: imp.imported_at,
                source: "importer",
                action: "import.error",
                detail: msg,
              });
            }
          }
        } catch {
          /* */
        }

        all.sort((a, b) => (a.ts < b.ts ? 1 : -1));
        if (!cancelled) setLogs(all);
      } catch (err) {
        if (!cancelled) setError(toSicroError(err).message);
      }
    }
    void load();
    return () => {
      cancelled = true;
    };
  }, [workspacePath, videos]);

  const filtered = useMemo(() => {
    if (!logs) return [];
    if (filter === "all") return logs;
    return logs.filter((l) => l.source === filter);
  }, [logs, filter]);

  if (logs === null) {
    return <p className={styles.tip}>Carregando logs…</p>;
  }
  if (error) {
    return <p className={`${styles.tip} ${styles.dim}`}>Falha: {error}</p>;
  }

  return (
    <div className={styles.section}>
      <div className={styles.toolbar}>
        <label htmlFor="lg-source">Fonte</label>
        <select
          id="lg-source"
          value={filter}
          onChange={(e) => setFilter(e.target.value)}
        >
          <option value="all">Todas ({logs.length})</option>
          <option value="video">Vídeo</option>
          <option value="laudo">Laudo / Evidência</option>
          <option value="importer">Importador</option>
        </select>
        <span className={styles.dim}>{filtered.length} entradas</span>
      </div>

      <div className={styles.card} style={{ padding: 0 }}>
        <table className={styles.table}>
          <thead>
            <tr>
              <th>Quando</th>
              <th>Fonte</th>
              <th>Ação</th>
              <th>Detalhe</th>
            </tr>
          </thead>
          <tbody>
            {filtered.length === 0 && (
              <tr>
                <td colSpan={4} className={styles.dim} style={{ textAlign: "center", padding: 16 }}>
                  Sem logs nesta fonte.
                </td>
              </tr>
            )}
            {filtered.map((l, idx) => (
              <tr key={idx}>
                <td>{formatDateTime(l.ts)}</td>
                <td>{l.source}</td>
                <td>
                  <code>{l.action}</code>
                </td>
                <td>{l.detail}</td>
              </tr>
            ))}
          </tbody>
        </table>
      </div>

      <p className={styles.tip}>
        O agregador desta aba consome logs de Vídeo (
        <code>video_operation_logs</code>), Laudo (<code>evidence_links</code>) e
        Importador (warnings/errors do <code>imports</code>). Outros
        módulos (Croqui, Dossiê, Exportação) ainda não emitem log
        estruturado — registrado como pendência no relatório do MVP 5.
      </p>
    </div>
  );
}

function truncateJson(json: string, max = 80): string {
  const s = json.replace(/\s+/g, " ").trim();
  if (s.length <= max) return s;
  return `${s.slice(0, max)}…`;
}
function safeParseStringArray(s: string | null | undefined): string[] {
  if (!s) return [];
  try {
    const v = JSON.parse(s);
    return Array.isArray(v) ? v.map(String) : [];
  } catch {
    return [];
  }
}
