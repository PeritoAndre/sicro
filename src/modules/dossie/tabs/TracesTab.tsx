/**
 * TracesTab — vestígios importados em tabela densa, com fotos/croqui
 * mostrados como chips de ID. A coluna "Dimensões" combina comprimento
 * × largura quando disponível.
 */

import { MapPin } from "lucide-react";
import { commands } from "@core/commands";
import type { Trace } from "@domain/dossie";
import shared from "./shared.module.css";
import { useDossieList } from "./useDossieList";

export function TracesTab({ workspacePath }: { workspacePath: string }) {
  const { items, loading, error } = useDossieList(workspacePath, commands.listDossieTraces);

  if (loading && items.length === 0) return <p className={shared.dim}>Carregando vestígios…</p>;
  if (error) return <p className={shared.error}>{error}</p>;
  if (items.length === 0) {
    return (
      <div className={shared.empty}>
        <MapPin size={28} aria-hidden />
        <span>O pacote não trouxe vestígios.</span>
      </div>
    );
  }

  return (
    <div className={shared.tab}>
      <table className={shared.table}>
        <thead>
          <tr>
            <th>ID</th>
            <th>Tipo</th>
            <th>Descrição</th>
            <th>Localização</th>
            <th>Dimensões</th>
            <th>Direção</th>
            <th>Fotos</th>
            <th>Croqui</th>
            <th>Obs.</th>
          </tr>
        </thead>
        <tbody>
          {items.map((t) => (
            <tr key={t.id}>
              <td className={shared.mono}>{t.identifier ?? t.original_id ?? "—"}</td>
              <td>{t.type ?? "—"}</td>
              <td>{t.description ?? <span className={shared.dim}>—</span>}</td>
              <td>{t.location_description ?? <span className={shared.dim}>—</span>}</td>
              <td>{formatDims(t)}</td>
              <td>{t.direction ?? <span className={shared.dim}>—</span>}</td>
              <td>
                <IdList json={t.photo_ids_json} />
              </td>
              <td>
                <IdList json={t.sketch_element_ids_json} />
              </td>
              <td>{t.note ?? <span className={shared.dim}>—</span>}</td>
            </tr>
          ))}
        </tbody>
      </table>
    </div>
  );
}

function formatDims(t: Trace): React.ReactNode {
  const unit = t.unit ?? "";
  if (t.length != null && t.width != null) {
    return `${t.length} × ${t.width} ${unit}`.trim();
  }
  if (t.length != null) return `${t.length} ${unit}`.trim();
  if (t.width != null) return `${t.width} ${unit}`.trim();
  return <span className={shared.dim}>—</span>;
}

function IdList({ json }: { json: string }) {
  let ids: string[] = [];
  try {
    ids = JSON.parse(json);
  } catch {
    ids = [];
  }
  if (ids.length === 0) return <span className={shared.dim}>—</span>;
  return (
    <span style={{ display: "inline-flex", gap: 4, flexWrap: "wrap" }}>
      {ids.map((id) => (
        <code key={id} className={shared.chip} style={{ fontSize: 10 }}>
          {id}
        </code>
      ))}
    </span>
  );
}
