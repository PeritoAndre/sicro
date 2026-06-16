/**
 * InsertTableDialog — modal para criar tabela N×M.
 *
 * F7.1 — Substitui o template "Tabela 3×3 em branco" por um diálogo
 * que pede ao perito o número de linhas e colunas + opção de header.
 *
 * Visual:
 *   - inputs numéricos com clamp 1..20 (linhas) e 1..10 (colunas);
 *   - mini-preview do grid resultante (até 5×5);
 *   - checkbox "Primeira linha como cabeçalho";
 *   - botões Cancelar / Inserir.
 */

import { useEffect, useState } from "react";
import { Table as TableIcon, X } from "lucide-react";
import styles from "./InsertTableDialog.module.css";

interface InsertTableDialogProps {
  open: boolean;
  onCancel: () => void;
  onConfirm: (params: {
    rows: number;
    cols: number;
    withHeaderRow: boolean;
  }) => void;
}

const MIN_ROWS = 1;
const MAX_ROWS = 30;
const MIN_COLS = 1;
const MAX_COLS = 12;
const DEFAULT_ROWS = 3;
const DEFAULT_COLS = 3;

export function InsertTableDialog({
  open,
  onCancel,
  onConfirm,
}: InsertTableDialogProps) {
  const [rows, setRows] = useState(DEFAULT_ROWS);
  const [cols, setCols] = useState(DEFAULT_COLS);
  // Padrão DESMARCADO (estilo Word): tabela nova nasce com células comuns,
  // sem linha de cabeçalho — nada de negrito/cinza que o perito não pediu.
  const [withHeader, setWithHeader] = useState(false);

  // Reset ao abrir.
  useEffect(() => {
    if (open) {
      setRows(DEFAULT_ROWS);
      setCols(DEFAULT_COLS);
      setWithHeader(false);
    }
  }, [open]);

  // Esc fecha.
  useEffect(() => {
    if (!open) return undefined;
    const handler = (e: KeyboardEvent) => {
      if (e.key === "Escape") {
        e.preventDefault();
        onCancel();
      }
    };
    window.addEventListener("keydown", handler);
    return () => window.removeEventListener("keydown", handler);
  }, [open, onCancel]);

  if (!open) return null;

  const clampedRows = clamp(rows, MIN_ROWS, MAX_ROWS);
  const clampedCols = clamp(cols, MIN_COLS, MAX_COLS);
  const previewRows = Math.min(clampedRows, 5);
  const previewCols = Math.min(clampedCols, 5);

  const handleConfirm = () => {
    onConfirm({
      rows: clampedRows,
      cols: clampedCols,
      withHeaderRow: withHeader,
    });
  };

  return (
    <div
      className={styles.overlay}
      role="dialog"
      aria-modal="true"
      aria-labelledby="insert-table-title"
      onClick={(e) => {
        if (e.target === e.currentTarget) onCancel();
      }}
    >
      <div className={styles.dialog}>
        <header className={styles.header}>
          <strong id="insert-table-title">
            <TableIcon size={16} /> Inserir tabela
          </strong>
          <button
            type="button"
            className={styles.closeBtn}
            onClick={onCancel}
            aria-label="Fechar"
          >
            <X size={14} />
          </button>
        </header>

        <div className={styles.body}>
          <p className={styles.intro}>
            Informe as dimensões da nova tabela. Você poderá ajustar linhas e
            colunas depois pelo menu "Tabela" da barra superior.
          </p>

          <div className={styles.grid}>
            <label className={styles.field}>
              <span>Linhas</span>
              <input
                type="number"
                value={rows}
                min={MIN_ROWS}
                max={MAX_ROWS}
                onChange={(e) =>
                  setRows(clamp(Number(e.target.value), MIN_ROWS, MAX_ROWS))
                }
                autoFocus
              />
              <small>
                {MIN_ROWS}–{MAX_ROWS}
              </small>
            </label>

            <label className={styles.field}>
              <span>Colunas</span>
              <input
                type="number"
                value={cols}
                min={MIN_COLS}
                max={MAX_COLS}
                onChange={(e) =>
                  setCols(clamp(Number(e.target.value), MIN_COLS, MAX_COLS))
                }
              />
              <small>
                {MIN_COLS}–{MAX_COLS}
              </small>
            </label>
          </div>

          <label className={styles.checkboxRow}>
            <input
              type="checkbox"
              checked={withHeader}
              onChange={(e) => setWithHeader(e.target.checked)}
            />
            <span>Primeira linha como cabeçalho</span>
          </label>

          <div className={styles.previewLabel}>
            Pré-visualização ({clampedRows}×{clampedCols}
            {clampedRows > 5 || clampedCols > 5 ? " — limitada a 5×5" : ""})
          </div>
          <div className={styles.preview}>
            <table className={styles.previewTable}>
              <tbody>
                {Array.from({ length: previewRows }).map((_, r) => (
                  <tr key={r}>
                    {Array.from({ length: previewCols }).map((_, c) => {
                      const isHeader = withHeader && r === 0;
                      return isHeader ? (
                        <th key={c}>&nbsp;</th>
                      ) : (
                        <td key={c}>&nbsp;</td>
                      );
                    })}
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
        </div>

        <footer className={styles.footer}>
          <button
            type="button"
            className={styles.btnSecondary}
            onClick={onCancel}
          >
            Cancelar
          </button>
          <button
            type="button"
            className={styles.btnPrimary}
            onClick={handleConfirm}
          >
            Inserir tabela
          </button>
        </footer>
      </div>
    </div>
  );
}

function clamp(v: number, lo: number, hi: number): number {
  if (!Number.isFinite(v)) return lo;
  if (v < lo) return lo;
  if (v > hi) return hi;
  return Math.floor(v);
}
