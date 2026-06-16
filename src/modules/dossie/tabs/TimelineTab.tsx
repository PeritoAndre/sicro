/**
 * TimelineTab — eventos automáticos da sessão operacional, ordenados
 * pelo backend por `occurred_at`. Lista simples com timestamp + tipo +
 * título + descrição.
 */

import { History } from "lucide-react";
import { commands } from "@core/commands";
import { formatDateTime } from "@core/formatters";
import shared from "./shared.module.css";
import { useDossieList } from "./useDossieList";

export function TimelineTab({ workspacePath }: { workspacePath: string }) {
  const { items, loading, error } = useDossieList(workspacePath, commands.listDossieTimeline);

  if (loading && items.length === 0) return <p className={shared.dim}>Carregando timeline…</p>;
  if (error) return <p className={shared.error}>{error}</p>;
  if (items.length === 0) {
    return (
      <div className={shared.empty}>
        <History size={28} aria-hidden />
        <span>O pacote não trouxe eventos de timeline.</span>
      </div>
    );
  }

  return (
    <div className={shared.tab}>
      <table className={shared.table}>
        <thead>
          <tr>
            <th>Quando</th>
            <th>Tipo</th>
            <th>Título</th>
            <th>Descrição</th>
          </tr>
        </thead>
        <tbody>
          {items.map((e) => (
            <tr key={e.id}>
              <td className={shared.mono}>
                {e.occurred_at ? formatDateTime(e.occurred_at) : "—"}
              </td>
              <td>
                <span className={shared.chip}>{e.type ?? "—"}</span>
              </td>
              <td>{e.title ?? <span className={shared.dim}>—</span>}</td>
              <td>{e.description ?? <span className={shared.dim}>—</span>}</td>
            </tr>
          ))}
        </tbody>
      </table>
    </div>
  );
}
