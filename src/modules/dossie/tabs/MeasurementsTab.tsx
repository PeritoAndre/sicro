/**
 * MeasurementsTab — medições importadas. Tabela densa: rótulo,
 * pontos A/B, valor, unidade, método, fotos, croqui.
 */

import { Ruler } from "lucide-react";
import { commands } from "@core/commands";
import shared from "./shared.module.css";
import { useDossieList } from "./useDossieList";

export function MeasurementsTab({ workspacePath }: { workspacePath: string }) {
  const { items, loading, error } = useDossieList(
    workspacePath,
    commands.listDossieMeasurements,
  );

  if (loading && items.length === 0) return <p className={shared.dim}>Carregando medições…</p>;
  if (error) return <p className={shared.error}>{error}</p>;
  if (items.length === 0) {
    return (
      <div className={shared.empty}>
        <Ruler size={28} aria-hidden />
        <span>O pacote não trouxe medições.</span>
      </div>
    );
  }

  return (
    <div className={shared.tab}>
      <table className={shared.table}>
        <thead>
          <tr>
            <th>Rótulo</th>
            <th>Ponto A</th>
            <th>Ponto B</th>
            <th>Valor</th>
            <th>Unidade</th>
            <th>Método</th>
            <th>Fotos</th>
            <th>Croqui</th>
            <th>Obs.</th>
          </tr>
        </thead>
        <tbody>
          {items.map((m) => (
            <tr key={m.id}>
              <td>
                <strong>{m.label ?? m.original_id ?? "—"}</strong>
              </td>
              <td>{m.point_a ?? <span className={shared.dim}>—</span>}</td>
              <td>{m.point_b ?? <span className={shared.dim}>—</span>}</td>
              <td className={shared.mono}>
                {m.value != null ? m.value : <span className={shared.dim}>—</span>}
              </td>
              <td>{m.unit ?? <span className={shared.dim}>—</span>}</td>
              <td>{m.method ?? <span className={shared.dim}>—</span>}</td>
              <td>
                <IdList json={m.photo_ids_json} />
              </td>
              <td>
                <IdList json={m.sketch_element_ids_json} />
              </td>
              <td>{m.note ?? <span className={shared.dim}>—</span>}</td>
            </tr>
          ))}
        </tbody>
      </table>
    </div>
  );
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
