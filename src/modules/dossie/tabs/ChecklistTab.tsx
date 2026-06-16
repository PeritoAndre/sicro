/**
 * ChecklistTab — exibe o checklist final do .sicroapp com contadores
 * (total / respondidos / não verificados / não se aplica / obrigatórios
 * pendentes) e tabela com categoria + pergunta + resposta + observação.
 *
 * Visualização only — não editamos checklist no Desktop nesta versão.
 */

import { useMemo, useState } from "react";
import { CheckSquare } from "lucide-react";
import { commands } from "@core/commands";
import type { ChecklistItem } from "@domain/dossie";
import shared from "./shared.module.css";
import { useDossieList } from "./useDossieList";

export function ChecklistTab({ workspacePath }: { workspacePath: string }) {
  const { items, loading, error } = useDossieList(workspacePath, commands.listDossieChecklist);
  const [filter, setFilter] = useState<"all" | "required" | "pending" | "na">("all");

  const summary = useMemo(() => summarise(items), [items]);

  const visible = useMemo(() => {
    switch (filter) {
      case "required":
        return items.filter((i) => i.required);
      case "pending":
        return items.filter((i) => i.required && i.answer === "nao_verificado");
      case "na":
        return items.filter((i) => i.answer === "nao_se_aplica");
      default:
        return items;
    }
  }, [items, filter]);

  if (loading && items.length === 0) return <p className={shared.dim}>Carregando checklist…</p>;
  if (error) return <p className={shared.error}>{error}</p>;
  if (items.length === 0) {
    return (
      <div className={shared.empty}>
        <CheckSquare size={28} aria-hidden />
        <span>O pacote não trouxe checklist (checklist.json ausente ou vazio).</span>
      </div>
    );
  }

  return (
    <div className={shared.tab}>
      <div className={shared.summary}>
        <div>
          <strong>{summary.total}</strong>
          <span>Total</span>
        </div>
        <div>
          <strong>{summary.answered}</strong>
          <span>Respondidos</span>
        </div>
        <div>
          <strong>{summary.notVerified}</strong>
          <span>Não verificados</span>
        </div>
        <div>
          <strong>{summary.notApplicable}</strong>
          <span>Não se aplica</span>
        </div>
        <div>
          <strong>{summary.requiredTotal}</strong>
          <span>Obrigatórios</span>
        </div>
        <div>
          <strong>{summary.requiredPending}</strong>
          <span>Obrig. pendentes</span>
        </div>
      </div>

      <div className={shared.toolbar}>
        <label htmlFor="ck-filter">Filtro</label>
        <select
          id="ck-filter"
          value={filter}
          onChange={(e) => setFilter(e.target.value as typeof filter)}
        >
          <option value="all">Todos ({items.length})</option>
          <option value="required">Obrigatórios</option>
          <option value="pending">Obrig. pendentes</option>
          <option value="na">Não se aplica</option>
        </select>
        <span className={shared.dim} style={{ fontSize: "var(--text-xs)" }}>
          {visible.length} item(ns)
        </span>
      </div>

      <table className={shared.table}>
        <thead>
          <tr>
            <th>Categoria</th>
            <th>Pergunta</th>
            <th>Resp.</th>
            <th>Obs.</th>
            <th>Origem</th>
          </tr>
        </thead>
        <tbody>
          {visible.map((item) => (
            <tr key={item.id}>
              <td>{item.category ?? "—"}</td>
              <td>
                <strong>
                  {item.required ? "* " : ""}
                  {item.question}
                </strong>
                {item.default_note && (
                  <div className={shared.dim} style={{ fontSize: "var(--text-xs)" }}>
                    {item.default_note}
                  </div>
                )}
              </td>
              <td>
                <AnswerChip answer={item.answer} />
              </td>
              <td>{item.note ?? <span className={shared.dim}>—</span>}</td>
              <td>
                <span className={shared.chip}>{item.origin}</span>
              </td>
            </tr>
          ))}
        </tbody>
      </table>
    </div>
  );
}

function AnswerChip({ answer }: { answer: string }) {
  const cls =
    answer === "sim"
      ? shared.chipOk
      : answer === "nao"
        ? shared.chipBad
        : answer === "nao_se_aplica"
          ? shared.chipMuted
          : shared.chipWarn;
  return <span className={`${shared.chip} ${cls}`}>{answer}</span>;
}

function summarise(items: ChecklistItem[]) {
  let answered = 0;
  let notVerified = 0;
  let notApplicable = 0;
  let requiredTotal = 0;
  let requiredPending = 0;
  for (const i of items) {
    if (i.required) requiredTotal++;
    if (i.answer === "sim" || i.answer === "nao") answered++;
    else if (i.answer === "nao_se_aplica") notApplicable++;
    else notVerified++;
    if (i.required && i.answer === "nao_verificado") requiredPending++;
  }
  return {
    total: items.length,
    answered,
    notVerified,
    notApplicable,
    requiredTotal,
    requiredPending,
  };
}
