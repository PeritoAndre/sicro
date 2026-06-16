/**
 * TablePropertiesDialog — propriedades da tabela (overhaul F4).
 *
 * 4 abas, agora dirigindo os ATRIBUTOS de primeira classe do SicroTable
 * (não mais `data-*` soltos que o nó não lia):
 *   - Tabela:  toggle de bordas (on/off) + cor + espessura + alinhamento +
 *              padding de célula → `setTablePresentation`.
 *   - Linha:   altura preferencial (cm) → attr `rowHeight` no tableRow.
 *   - Coluna:  largura preferencial (cm) → `colwidth` (px) da célula atual
 *              via `setCellAttribute`.
 *   - Célula:  alinhamento vertical do conteúdo (data-valign) + toggle de
 *              cabeçalho na linha.
 *
 * Tudo serializa no clone estático/HTML/PDF (CSS via data-attrs + colgroup)
 * e no DOCX (walker Rust). Espelha o renderHTML do SicroTable.
 */

import { useEffect, useState } from "react";
import type { Editor } from "@tiptap/react";
import {
  AlignCenter,
  AlignLeft,
  AlignRight,
  X,
  Table as TableIcon,
} from "lucide-react";
import {
  DEFAULT_TABLE_BORDER_COLOR,
  DEFAULT_TABLE_BORDER_WIDTH,
  DEFAULT_TABLE_CELL_PADDING,
  type TableAlign,
} from "../document-engine";
import styles from "./TablePropertiesDialog.module.css";

type Tab = "tabela" | "linha" | "coluna" | "celula";

const PX_PER_CM = 37.7952755906;

interface TablePropertiesDialogProps {
  open: boolean;
  editor: Editor | null;
  onClose: () => void;
}

export function TablePropertiesDialog({
  open,
  editor,
  onClose,
}: TablePropertiesDialogProps) {
  const [tab, setTab] = useState<Tab>("tabela");

  // Tabela
  const [bordersOn, setBordersOn] = useState(true);
  const [tableAlign, setTableAlign] = useState<TableAlign>("left");
  const [borderColor, setBorderColor] = useState<string>(
    DEFAULT_TABLE_BORDER_COLOR,
  );
  const [borderWidthPx, setBorderWidthPx] = useState<string>(
    String(DEFAULT_TABLE_BORDER_WIDTH),
  );
  const [cellPaddingPx, setCellPaddingPx] = useState<string>(
    String(DEFAULT_TABLE_CELL_PADDING),
  );

  // Linha / Coluna / Célula
  const [rowHeightCm, setRowHeightCm] = useState<string>("");
  const [colWidthCm, setColWidthCm] = useState<string>("");
  const [cellVAlign, setCellVAlign] = useState<"top" | "middle" | "bottom">(
    "top",
  );
  // Cor de fundo da célula (attr `backgroundColor`). null = sem cor.
  const [cellBg, setCellBg] = useState<string | null>(null);

  useEffect(() => {
    if (!open || !editor) return;
    const t = editor.getAttributes("table") as Record<string, unknown>;
    setBordersOn((t.borderStyle as string | undefined) !== "none");
    setTableAlign(((t.tableAlign as TableAlign) ?? "left"));
    setBorderColor((t.borderColor as string) ?? DEFAULT_TABLE_BORDER_COLOR);
    setBorderWidthPx(String(t.borderWidth ?? DEFAULT_TABLE_BORDER_WIDTH));
    setCellPaddingPx(String(t.cellPadding ?? DEFAULT_TABLE_CELL_PADDING));

    const row = editor.getAttributes("tableRow") as Record<string, unknown>;
    const rh = row.rowHeight as number | undefined;
    setRowHeightCm(rh ? String(rh) : "");

    // colwidth atual (px) → cm (best-effort: 1ª entrada do array).
    const cell = editor.getAttributes("tableCell") as Record<string, unknown>;
    const header = editor.getAttributes("tableHeader") as Record<string, unknown>;
    const cw =
      (cell.colwidth as number[] | null) ??
      (header.colwidth as number[] | null) ??
      null;
    if (cw && cw.length && cw[0]) {
      setColWidthCm((cw[0] / PX_PER_CM).toFixed(2));
    } else {
      setColWidthCm("");
    }
    const valign =
      (cell["data-valign"] as string | undefined) ??
      (header["data-valign"] as string | undefined) ??
      "top";
    setCellVAlign(valign === "middle" || valign === "bottom" ? valign : "top");

    // Cor de fundo da célula (tableCell OU tableHeader na 1ª linha).
    setCellBg(
      (cell.backgroundColor as string | null) ??
        (header.backgroundColor as string | null) ??
        null,
    );

    setTab("tabela");
  }, [open, editor]);

  // Esc fecha.
  useEffect(() => {
    if (!open) return undefined;
    const handler = (e: KeyboardEvent) => {
      if (e.key === "Escape") {
        e.preventDefault();
        onClose();
      }
    };
    window.addEventListener("keydown", handler);
    return () => window.removeEventListener("keydown", handler);
  }, [open, onClose]);

  if (!open) return null;

  const handleApply = () => {
    if (!editor) {
      onClose();
      return;
    }
    // F4 — Apresentação da tabela (atributos de primeira classe).
    editor
      .chain()
      .focus()
      .setTablePresentation({
        tableAlign,
        borderStyle: bordersOn ? "all" : "none",
        borderColor,
        borderWidth: Number(borderWidthPx) || DEFAULT_TABLE_BORDER_WIDTH,
        cellPadding: Number(cellPaddingPx) || DEFAULT_TABLE_CELL_PADDING,
      })
      .run();

    // Altura da linha onde está o cursor.
    const rh = Number(rowHeightCm);
    if (rowHeightCm.trim() !== "" && Number.isFinite(rh) && rh > 0) {
      editor
        .chain()
        .focus()
        .updateAttributes("tableRow", { rowHeight: rh })
        .run();
    } else if (rowHeightCm.trim() === "") {
      // Vazio = volta pra auto (remove a altura fixa).
      editor.chain().focus().updateAttributes("tableRow", { rowHeight: null }).run();
    }

    // Largura da coluna (célula atual) em px.
    const cwCm = Number(colWidthCm);
    if (colWidthCm.trim() !== "" && Number.isFinite(cwCm) && cwCm > 0) {
      const px = Math.round(cwCm * PX_PER_CM);
      editor.chain().focus().setCellAttribute("colwidth", [px]).run();
    }

    // Alinhamento vertical (data-valign — lido pelo CSS + walker DOCX).
    editor
      .chain()
      .focus()
      .updateAttributes("tableCell", { "data-valign": cellVAlign })
      .run();
    editor
      .chain()
      .focus()
      .updateAttributes("tableHeader", { "data-valign": cellVAlign })
      .run();

    // Cor de fundo da célula (attr `backgroundColor`). `setCellAttribute` usa
    // o setCellAttr do prosemirror-tables → aplica em TODAS as células de uma
    // CellSelection (ou na célula única). null = limpa (sem cor).
    editor.chain().focus().setCellAttribute("backgroundColor", cellBg).run();

    onClose();
  };

  return (
    <div
      className={styles.overlay}
      role="dialog"
      aria-modal="true"
      aria-labelledby="tableprops-title"
      onClick={(e) => {
        if (e.target === e.currentTarget) onClose();
      }}
    >
      <div className={styles.dialog}>
        <header className={styles.header}>
          <strong id="tableprops-title">
            <TableIcon size={16} /> Propriedades da tabela
          </strong>
          <button
            type="button"
            className={styles.closeBtn}
            onClick={onClose}
            aria-label="Fechar"
          >
            <X size={14} />
          </button>
        </header>

        <div className={styles.tabs} role="tablist">
          <TabButton active={tab === "tabela"} onClick={() => setTab("tabela")}>
            Tabela
          </TabButton>
          <TabButton active={tab === "linha"} onClick={() => setTab("linha")}>
            Linha
          </TabButton>
          <TabButton active={tab === "coluna"} onClick={() => setTab("coluna")}>
            Coluna
          </TabButton>
          <TabButton active={tab === "celula"} onClick={() => setTab("celula")}>
            Célula
          </TabButton>
        </div>

        <div className={styles.body}>
          {tab === "tabela" && (
            <div className={styles.section}>
              <div className={styles.sectionTitle}>Bordas</div>
              <label className={styles.checkRow}>
                <input
                  type="checkbox"
                  checked={bordersOn}
                  onChange={(e) => setBordersOn(e.target.checked)}
                />
                <span>
                  Mostrar bordas (grade). Desmarque para só o retângulo
                  externo (estilo bloco de registro).
                </span>
              </label>
              <div className={styles.fieldRow}>
                <label className={styles.field}>
                  <span>Cor da borda</span>
                  <input
                    type="color"
                    value={normalizeColor(borderColor)}
                    onChange={(e) => setBorderColor(e.target.value)}
                    disabled={!bordersOn}
                  />
                </label>
                <label className={styles.field}>
                  <span>Espessura</span>
                  <div className={styles.inputUnit}>
                    <input
                      type="number"
                      min={0.5}
                      max={6}
                      step={0.5}
                      value={borderWidthPx}
                      onChange={(e) => setBorderWidthPx(e.target.value)}
                      disabled={!bordersOn}
                    />
                    <span>px</span>
                  </div>
                </label>
              </div>

              <div className={styles.sectionTitle}>Espaçamento</div>
              <label className={styles.field}>
                <span>Padding das células</span>
                <div className={styles.inputUnit}>
                  <input
                    type="number"
                    min={0}
                    max={24}
                    step={1}
                    value={cellPaddingPx}
                    onChange={(e) => setCellPaddingPx(e.target.value)}
                  />
                  <span>px</span>
                </div>
              </label>

              <div className={styles.sectionTitle}>Alinhamento</div>
              <div className={styles.alignGroup}>
                <AlignBtn
                  active={tableAlign === "left"}
                  onClick={() => setTableAlign("left")}
                  icon={<AlignLeft size={16} />}
                  label="À esquerda"
                />
                <AlignBtn
                  active={tableAlign === "center"}
                  onClick={() => setTableAlign("center")}
                  icon={<AlignCenter size={16} />}
                  label="Centralizada"
                />
                <AlignBtn
                  active={tableAlign === "right"}
                  onClick={() => setTableAlign("right")}
                  icon={<AlignRight size={16} />}
                  label="À direita"
                />
              </div>
            </div>
          )}

          {tab === "linha" && (
            <div className={styles.section}>
              <div className={styles.sectionTitle}>Tamanho da linha</div>
              <label className={styles.field}>
                <span>Altura preferencial</span>
                <div className={styles.inputUnit}>
                  <input
                    type="text"
                    value={rowHeightCm}
                    onChange={(e) => setRowHeightCm(e.target.value)}
                    placeholder="auto"
                  />
                  <span>cm</span>
                </div>
              </label>
              <p className={styles.hint}>
                Aplica à linha onde está o cursor. Deixe em branco para a
                altura seguir o conteúdo. (Também é possível arrastar a borda
                inferior da linha direto na tabela.)
              </p>
            </div>
          )}

          {tab === "coluna" && (
            <div className={styles.section}>
              <div className={styles.sectionTitle}>Tamanho da coluna</div>
              <label className={styles.field}>
                <span>Largura preferencial</span>
                <div className={styles.inputUnit}>
                  <input
                    type="text"
                    value={colWidthCm}
                    onChange={(e) => setColWidthCm(e.target.value)}
                    placeholder="auto"
                  />
                  <span>cm</span>
                </div>
              </label>
              <p className={styles.hint}>
                Aplica à coluna da célula onde está o cursor. (Também é
                possível arrastar a borda da coluna direto na tabela.)
              </p>
            </div>
          )}

          {tab === "celula" && (
            <div className={styles.section}>
              <div className={styles.sectionTitle}>
                Alinhamento vertical do conteúdo
              </div>
              <div className={styles.alignGroup}>
                <AlignBtn
                  active={cellVAlign === "top"}
                  onClick={() => setCellVAlign("top")}
                  icon={<span style={{ fontSize: 14 }}>⬆</span>}
                  label="Topo"
                />
                <AlignBtn
                  active={cellVAlign === "middle"}
                  onClick={() => setCellVAlign("middle")}
                  icon={<span style={{ fontSize: 14 }}>↕</span>}
                  label="Centro"
                />
                <AlignBtn
                  active={cellVAlign === "bottom"}
                  onClick={() => setCellVAlign("bottom")}
                  icon={<span style={{ fontSize: 14 }}>⬇</span>}
                  label="Base"
                />
              </div>

              <div className={styles.sectionTitle}>Cor de fundo</div>
              <div className={styles.fieldRow}>
                <label className={styles.field}>
                  <span>Cor de fundo da célula</span>
                  <input
                    type="color"
                    value={normalizeColor(cellBg)}
                    onChange={(e) => setCellBg(e.target.value)}
                  />
                </label>
                <div
                  className={styles.field}
                  style={{ justifyContent: "flex-end" }}
                >
                  <span>&nbsp;</span>
                  <button
                    type="button"
                    className={styles.btnSecondary}
                    onClick={() => setCellBg(null)}
                    disabled={!cellBg}
                    title="Remover a cor de fundo (transparente)"
                  >
                    Sem cor
                  </button>
                </div>
              </div>

              <p className={styles.hint}>
                Aplica à(s) célula(s) selecionada(s). Selecione várias células
                (arrastando) para colorir todas de uma vez. "Sem cor" deixa o
                fundo transparente.
              </p>
            </div>
          )}
        </div>

        <footer className={styles.footer}>
          <button
            type="button"
            className={styles.btnSecondary}
            onClick={onClose}
          >
            Cancelar
          </button>
          <button
            type="button"
            className={styles.btnPrimary}
            onClick={handleApply}
          >
            OK
          </button>
        </footer>
      </div>
    </div>
  );
}

function TabButton({
  active,
  onClick,
  children,
}: {
  active: boolean;
  onClick: () => void;
  children: React.ReactNode;
}) {
  return (
    <button
      type="button"
      role="tab"
      aria-selected={active}
      className={`${styles.tab} ${active ? styles.tabActive : ""}`}
      onClick={onClick}
    >
      {children}
    </button>
  );
}

function AlignBtn({
  active,
  onClick,
  icon,
  label,
}: {
  active: boolean;
  onClick: () => void;
  icon: React.ReactNode;
  label: string;
}) {
  return (
    <button
      type="button"
      className={`${styles.alignBtn} ${active ? styles.alignBtnActive : ""}`}
      onClick={onClick}
      title={label}
    >
      {icon}
      <span>{label}</span>
    </button>
  );
}

/** Normaliza cor pra `#RRGGBB` aceito por `<input type="color">`. */
function normalizeColor(color: string | null): string {
  if (!color) return "#000000";
  if (color.startsWith("#") && color.length === 7) return color;
  if (color.startsWith("#") && color.length === 4) {
    const r = color[1];
    const g = color[2];
    const b = color[3];
    return `#${r}${r}${g}${g}${b}${b}`;
  }
  return "#000000";
}
