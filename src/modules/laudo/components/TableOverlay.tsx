/**
 * TableOverlay (F2/F3) — toolbar flutuante + handles pra TABELA com cursor.
 *
 * Espelha o TextBoxOverlay (mesma mecânica de posicionamento, zoom e
 * listeners de scroll/resize) mas pra um nó que NÃO é NodeSelection: a
 * tabela é detectada por `useSelectedTable` (cursor dentro de uma célula).
 *
 * Oferece:
 *   - F2 — Toolbar acima da tabela: inserir linha acima/abaixo, inserir
 *     coluna esq/dir, excluir linha, excluir coluna, mesclar/dividir célula,
 *     alternar cabeçalho, alinhar tabela (esq/centro/dir), propriedades,
 *     excluir tabela.
 *   - F2 — Menu de contexto (botão direito sobre a tabela) com as MESMAS ações.
 *   - F3 — Grip de "mover bloco" (canto sup-esq) + botões subir/descer:
 *     reordena a tabela inteira entre os blocos irmãos (estilo Word/Notion),
 *     mantendo a paginação.
 *   - F3 — Calhas de altura de linha (borda inferior de cada linha): arrastar
 *     define a altura mínima da linha (attr `rowHeight` em cm).
 *
 * Plugado pra corpo + cabeçalho + rodapé (3 instâncias no LaudoEditorView),
 * exatamente como o TextBoxOverlay. Só funciona enquanto aquela região é o
 * editor ativo (no clone estático não há editor → hook retorna null).
 */

import { useCallback, useEffect, useLayoutEffect, useRef, useState } from "react";
import type { Editor } from "@tiptap/react";
import {
  AlignCenter,
  AlignLeft,
  AlignRight,
  ArrowDownToLine,
  ArrowUpToLine,
  Captions,
  CaptionsOff,
  ChevronDown,
  ChevronUp,
  Ban,
  Columns2,
  GripVertical,
  Merge,
  PaintBucket,
  PanelLeftClose,
  PanelRightClose,
  Rows2,
  Settings2,
  Split,
  TableRowsSplit,
  Trash2,
} from "lucide-react";
import type { SelectedTable } from "../hooks/useSelectedTable";
import type { TableAlign } from "../document-engine";
import { buildTableOps } from "./tableOps";
import styles from "./TableOverlay.module.css";

const PX_PER_CM = 37.7952755906;
const MIN_ROW_HEIGHT_CM = 0.5;

export interface TableOverlayProps {
  editor: Editor | null;
  selected: SelectedTable | null;
  containerRef: React.RefObject<HTMLElement>;
  /** Abre o diálogo de propriedades da tabela (largura/bordas/padding). */
  onOpenProperties?: () => void;
}

interface ContextMenuState {
  x: number;
  y: number;
}

export function TableOverlay({
  editor,
  selected,
  containerRef,
  onOpenProperties,
}: TableOverlayProps) {
  const [rect, setRect] = useState<DOMRect | null>(null);
  const [contextMenu, setContextMenu] = useState<ContextMenuState | null>(null);
  const [dragTooltip, setDragTooltip] = useState<{
    x: number;
    y: number;
    label: string;
  } | null>(null);
  /** Posições verticais (relativas ao topo do overlay) das bordas inferiores
   *  de cada linha — pras calhas de altura (F3). */
  const [rowEdges, setRowEdges] = useState<number[]>([]);
  // Bump a cada selectionUpdate pra reler a cor de fundo da CÉLULA ativa (o
  // hook useSelectedTable não re-renderiza ao mover o cursor DENTRO da mesma
  // tabela, pois pos/attrs do nó table não mudam; mas o swatch precisa refletir
  // a célula sob o cursor). Não causa loop — selectionUpdate só dispara em
  // mudança real de seleção.
  const [, setSelectionTick] = useState(0);
  const overlayRef = useRef<HTMLDivElement | null>(null);

  // Zoom robusto via ancestral não-rotacionado (igual TextBoxOverlay).
  const getZoom = useCallback((): number => {
    const el = containerRef.current;
    if (!el) return 1;
    const w = el.offsetWidth;
    if (w <= 0) return 1;
    return el.getBoundingClientRect().width / w;
  }, [containerRef]);

  const recompute = useCallback(() => {
    if (!selected || !containerRef.current || !editor) {
      setRect(null);
      setRowEdges([]);
      return;
    }
    // DOM fresco via nodeDOM (o NodeView expõe o bloco; pegamos a <table>).
    let domEl: HTMLElement | null = null;
    try {
      const dom = editor.view.nodeDOM(selected.pos);
      if (dom instanceof HTMLElement && dom.isConnected) {
        domEl =
          dom.tagName === "TABLE"
            ? dom
            : (dom.querySelector("table") as HTMLElement | null) ?? dom;
      }
    } catch {
      // ignore
    }
    if (!domEl && selected.domEl.isConnected) domEl = selected.domEl;
    if (!domEl) {
      setRect(null);
      return;
    }
    const r = domEl.getBoundingClientRect();
    const c = containerRef.current.getBoundingClientRect();
    setRect(new DOMRect(r.left - c.left, r.top - c.top, r.width, r.height));

    // Bordas inferiores das linhas (pra calhas de altura). Em coords
    // relativas ao topo da tabela (que é o topo do overlay).
    const rowsEls = Array.from(
      domEl.querySelectorAll(":scope > tbody > tr"),
    ) as HTMLElement[];
    const edges: number[] = [];
    for (const tr of rowsEls) {
      const rr = tr.getBoundingClientRect();
      edges.push(rr.bottom - r.top);
    }
    setRowEdges(edges);
  }, [selected, containerRef, editor]);

  useLayoutEffect(() => {
    recompute();
  }, [recompute]);

  // Re-renderiza ao mudar a seleção (pra o swatch refletir a célula ativa).
  useEffect(() => {
    if (!editor) return undefined;
    const bump = () => setSelectionTick((t) => t + 1);
    editor.on("selectionUpdate", bump);
    return () => {
      editor.off("selectionUpdate", bump);
    };
  }, [editor]);

  // Fecha o menu de contexto ao desselecionar.
  useEffect(() => {
    if (!selected) setContextMenu(null);
  }, [selected]);

  // Scroll/resize listeners — coleta scrollers ancestrais (igual os outros).
  useEffect(() => {
    if (!selected || !containerRef.current) return undefined;
    const el = containerRef.current;
    const onScroll = () => {
      recompute();
      setContextMenu(null);
    };
    const scrollers: HTMLElement[] = [];
    let cur: HTMLElement | null = selected.domEl.parentElement;
    while (cur && cur !== document.body) {
      const cs = window.getComputedStyle(cur);
      if (
        cs.overflowY === "auto" ||
        cs.overflowY === "scroll" ||
        cs.overflowX === "auto" ||
        cs.overflowX === "scroll"
      ) {
        scrollers.push(cur);
      }
      cur = cur.parentElement;
    }
    for (const sc of scrollers) {
      sc.addEventListener("scroll", onScroll, { passive: true });
    }
    el.addEventListener("scroll", onScroll, { passive: true });
    window.addEventListener("resize", onScroll);
    window.addEventListener("scroll", onScroll, { passive: true });
    const ro = new ResizeObserver(() => recompute());
    ro.observe(selected.domEl);
    ro.observe(el);
    return () => {
      for (const sc of scrollers) sc.removeEventListener("scroll", onScroll);
      el.removeEventListener("scroll", onScroll);
      window.removeEventListener("resize", onScroll);
      window.removeEventListener("scroll", onScroll);
      ro.disconnect();
    };
  }, [selected, containerRef, recompute]);

  // ---- Menu de contexto (botão direito sobre a tabela) ----
  useEffect(() => {
    if (!editor || !selected) return undefined;
    const tableEl = selected.domEl;
    const onContext = (e: MouseEvent) => {
      // Só intercepta o botão direito DENTRO desta tabela.
      e.preventDefault();
      e.stopPropagation();
      setContextMenu({ x: e.clientX, y: e.clientY });
    };
    tableEl.addEventListener("contextmenu", onContext);
    return () => tableEl.removeEventListener("contextmenu", onContext);
  }, [editor, selected]);

  // Fecha o menu de contexto ao clicar fora / Esc.
  useEffect(() => {
    if (!contextMenu) return undefined;
    const onDown = (e: MouseEvent) => {
      const target = e.target as HTMLElement | null;
      if (target && target.closest(`.${styles.contextMenu}`)) return;
      setContextMenu(null);
    };
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") setContextMenu(null);
    };
    // mousedown captura cliques fora; usa timeout pra não fechar no próprio
    // contextmenu que abriu.
    const t = setTimeout(() => {
      window.addEventListener("mousedown", onDown);
    }, 0);
    window.addEventListener("keydown", onKey);
    return () => {
      clearTimeout(t);
      window.removeEventListener("mousedown", onDown);
      window.removeEventListener("keydown", onKey);
    };
  }, [contextMenu]);

  // ---- F3 — Arrasto de altura de linha ----
  const handleRowResizeDown = useCallback(
    (rowIndex: number) => (e: React.MouseEvent) => {
      if (!editor || !selected) return;
      if (e.button !== 0) return;
      e.preventDefault();
      e.stopPropagation();

      const tableEl = selected.domEl;
      const rowsEls = Array.from(
        tableEl.querySelectorAll(":scope > tbody > tr"),
      ) as HTMLElement[];
      const rowEl = rowsEls[rowIndex];
      if (!rowEl) return;
      const zoom = getZoom();
      const startY = e.clientY;
      const startHeightVisual = rowEl.getBoundingClientRect().height;
      const startHeightLayout = startHeightVisual / Math.max(0.0001, zoom);
      let latestCm = startHeightLayout / PX_PER_CM;

      const onMove = (ev: MouseEvent) => {
        const dyVisual = ev.clientY - startY;
        const dyLayout = dyVisual / Math.max(0.0001, zoom);
        const newLayoutPx = Math.max(
          MIN_ROW_HEIGHT_CM * PX_PER_CM,
          startHeightLayout + dyLayout,
        );
        latestCm = newLayoutPx / PX_PER_CM;
        // Preview imediato no DOM.
        rowEl.style.height = `${latestCm.toFixed(2)}cm`;
        for (const cell of Array.from(rowEl.children) as HTMLElement[]) {
          cell.style.height = `${latestCm.toFixed(2)}cm`;
        }
        setDragTooltip({
          x: ev.clientX,
          y: ev.clientY,
          label: `altura ${latestCm.toFixed(2)} cm`,
        });
        recompute();
      };
      const onUp = () => {
        window.removeEventListener("mousemove", onMove);
        window.removeEventListener("mouseup", onUp);
        setDragTooltip(null);
        commitRowHeight(rowIndex, Number(latestCm.toFixed(2)));
      };
      window.addEventListener("mousemove", onMove);
      window.addEventListener("mouseup", onUp);
    },
    // commitRowHeight stable below
    // eslint-disable-next-line react-hooks/exhaustive-deps
    [editor, selected, getZoom, recompute],
  );

  /** Comita a altura (cm) numa linha (attr `rowHeight` no nó tableRow). */
  const commitRowHeight = useCallback(
    (rowIndex: number, heightCm: number) => {
      if (!editor || !selected) return;
      const { state } = editor;
      const tableNode = state.doc.nodeAt(selected.pos);
      if (!tableNode || tableNode.type.name !== "table") return;
      // Localiza a posição do tableRow de índice rowIndex.
      let rowPos: number | null = null;
      let idx = 0;
      tableNode.forEach((child, offset) => {
        if (child.type.name === "tableRow") {
          if (idx === rowIndex) {
            rowPos = selected.pos + 1 + offset; // +1 entra no table
          }
          idx += 1;
        }
      });
      if (rowPos == null) return;
      const rowNode = state.doc.nodeAt(rowPos);
      if (!rowNode || rowNode.type.name !== "tableRow") return;
      const tr = state.tr.setNodeMarkup(rowPos, undefined, {
        ...rowNode.attrs,
        rowHeight: heightCm,
      });
      editor.view.dispatch(tr);
    },
    [editor, selected],
  );

  // ---- F3 — Mover bloco via grip (arraste pra cima/baixo) ----
  const ops = editor && selected ? buildTableOps(editor, selected.pos) : null;

  const handleMoveGripDown = useCallback(
    (e: React.MouseEvent) => {
      if (!editor || !selected || !ops) return;
      if (e.button !== 0) return;
      e.preventDefault();
      e.stopPropagation();
      const startY = e.clientY;
      let moved = false;
      const onMove = (ev: MouseEvent) => {
        const dy = ev.clientY - startY;
        // Limiar de 1 "linha de bloco" (~28px visual) por passo.
        const STEP = 28;
        if (!moved && Math.abs(dy) < STEP) return;
        if (dy <= -STEP) {
          if (ops.canMoveUp()) ops.moveBlock("up");
          moved = true;
          // Encerra após um passo (o recompute reposiciona o overlay).
          cleanup();
        } else if (dy >= STEP) {
          if (ops.canMoveDown()) ops.moveBlock("down");
          moved = true;
          cleanup();
        }
      };
      const cleanup = () => {
        window.removeEventListener("mousemove", onMove);
        window.removeEventListener("mouseup", cleanup);
      };
      window.addEventListener("mousemove", onMove);
      window.addEventListener("mouseup", cleanup);
    },
    [editor, selected, ops],
  );

  if (!selected || !rect || !editor || !ops) return null;

  const align = (selected.attrs.tableAlign as TableAlign | null) ?? "left";

  // Cor de fundo da(s) célula(s) sob o cursor/seleção. Lê do nó tableCell OU
  // tableHeader (a 1ª linha é header). null = sem cor (transparente). O swatch
  // mostra a cor atual; aplicar usa setCellBackground (multi-célula via
  // CellSelection). Lido a cada render (selected muda a cada selectionUpdate).
  const cellAttrs = editor.getAttributes("tableCell") as Record<string, unknown>;
  const headerAttrs = editor.getAttributes("tableHeader") as Record<
    string,
    unknown
  >;
  const cellBg =
    (cellAttrs.backgroundColor as string | null) ??
    (headerAttrs.backgroundColor as string | null) ??
    null;

  const closeMenuThen = (fn: () => void) => () => {
    fn();
    setContextMenu(null);
  };

  return (
    <>
      <div
        ref={overlayRef}
        className={styles.overlay}
        style={{
          left: `${rect.left}px`,
          top: `${rect.top}px`,
          width: `${rect.width}px`,
          height: `${rect.height}px`,
        }}
        // Não captura cliques no miolo (deixa editar células); só os
        // elementos interativos (toolbar/handles) têm pointer-events: auto.
        onMouseDown={(e) => {
          // Permite cliques passarem pro editor, exceto nos nossos controles.
          const target = e.target as HTMLElement;
          if (target === overlayRef.current) {
            // clique no overlay vazio → não faz nada (deixa propagar p/ tabela)
          }
        }}
      >
        {/* Borda de seleção */}
        <div className={styles.selectionFrame} aria-hidden />

        {/* Grip de mover (canto superior-esquerdo) */}
        <div
          className={styles.moveGrip}
          onMouseDown={handleMoveGripDown}
          title="Arraste para mover a tabela entre os blocos (ou use ↑/↓ na barra)"
        >
          <GripVertical size={14} />
        </div>

        {/* Toolbar acima */}
        <div
          className={styles.toolbar}
          style={{ top: -42, left: 0 }}
          onMouseDown={(e) => e.stopPropagation()}
        >
          {/* Mover bloco */}
          <button
            type="button"
            className={styles.toolBtn}
            onClick={() => ops.moveBlock("up")}
            disabled={!ops.canMoveUp()}
            title="Mover tabela para cima (bloco anterior)"
          >
            <ChevronUp size={14} />
          </button>
          <button
            type="button"
            className={styles.toolBtn}
            onClick={() => ops.moveBlock("down")}
            disabled={!ops.canMoveDown()}
            title="Mover tabela para baixo (bloco seguinte)"
          >
            <ChevronDown size={14} />
          </button>
          <div className={styles.toolDivider} />
          {/* Linhas */}
          <button
            type="button"
            className={styles.toolBtn}
            onClick={ops.addRowBefore}
            title="Inserir linha acima"
          >
            <ArrowUpToLine size={14} />
          </button>
          <button
            type="button"
            className={styles.toolBtn}
            onClick={ops.addRowAfter}
            title="Inserir linha abaixo"
          >
            <ArrowDownToLine size={14} />
          </button>
          <button
            type="button"
            className={styles.toolBtn}
            onClick={ops.deleteRow}
            title="Excluir linha"
          >
            <Rows2 size={14} />
          </button>
          <div className={styles.toolDivider} />
          {/* Colunas */}
          <button
            type="button"
            className={styles.toolBtn}
            onClick={ops.addColumnBefore}
            title="Inserir coluna à esquerda"
          >
            <PanelLeftClose size={14} />
          </button>
          <button
            type="button"
            className={styles.toolBtn}
            onClick={ops.addColumnAfter}
            title="Inserir coluna à direita"
          >
            <PanelRightClose size={14} />
          </button>
          <button
            type="button"
            className={styles.toolBtn}
            onClick={ops.deleteColumn}
            title="Excluir coluna"
          >
            <Columns2 size={14} />
          </button>
          <div className={styles.toolDivider} />
          {/* Células */}
          <button
            type="button"
            className={styles.toolBtn}
            onClick={ops.mergeCells}
            disabled={!ops.canMergeCells()}
            title="Mesclar células selecionadas"
          >
            <Merge size={14} />
          </button>
          <button
            type="button"
            className={styles.toolBtn}
            onClick={ops.splitCell}
            disabled={!ops.canSplitCell()}
            title="Dividir célula"
          >
            <Split size={14} />
          </button>
          <button
            type="button"
            className={styles.toolBtn}
            onClick={ops.toggleHeaderRow}
            title="Alternar linha de cabeçalho"
          >
            <TableRowsSplit size={14} />
          </button>
          <div className={styles.toolDivider} />
          {/* Alinhamento da tabela */}
          <button
            type="button"
            className={`${styles.toolBtn} ${align === "left" ? styles.toolBtnActive : ""}`}
            onClick={() => ops.setAlign("left")}
            title="Alinhar tabela à esquerda"
          >
            <AlignLeft size={14} />
          </button>
          <button
            type="button"
            className={`${styles.toolBtn} ${align === "center" ? styles.toolBtnActive : ""}`}
            onClick={() => ops.setAlign("center")}
            title="Centralizar tabela"
          >
            <AlignCenter size={14} />
          </button>
          <button
            type="button"
            className={`${styles.toolBtn} ${align === "right" ? styles.toolBtnActive : ""}`}
            onClick={() => ops.setAlign("right")}
            title="Alinhar tabela à direita"
          >
            <AlignRight size={14} />
          </button>
          <div className={styles.toolDivider} />
          {/* Cor de fundo da(s) célula(s). Swatch (espelha o fill do TextBox/
              Shape) + botão "sem cor". Aplica via setCellBackground, que cobre
              seleção de múltiplas células (CellSelection). */}
          <PaintBucket
            size={14}
            style={{ color: "#5b6b8b", marginRight: 2 }}
            aria-hidden
          />
          <label
            className={styles.colorSwatch}
            // Quando há cor, mostra a cor; quando "sem cor", um xadrez sutil
            // indica transparente (leitura visual de "nenhuma cor"). Usa só
            // longhands pra não misturar shorthand `background` com `background-
            // image` (a ordem importaria).
            style={
              cellBg
                ? { backgroundColor: cellBg, backgroundImage: "none" }
                : {
                    backgroundColor: "transparent",
                    backgroundImage:
                      "linear-gradient(45deg,#e6e9f0 25%,transparent 25%,transparent 75%,#e6e9f0 75%),linear-gradient(45deg,#e6e9f0 25%,transparent 25%,transparent 75%,#e6e9f0 75%)",
                    backgroundSize: "8px 8px",
                    backgroundPosition: "0 0, 4px 4px",
                  }
            }
            title="Cor de fundo da célula"
          >
            <input
              type="color"
              value={normalizeColor(cellBg)}
              onChange={(e) => ops.setCellBackground(e.target.value)}
            />
          </label>
          <button
            type="button"
            className={styles.toolBtn}
            onClick={() => ops.setCellBackground(null)}
            disabled={!cellBg}
            title="Sem cor de fundo (limpar)"
          >
            <Ban size={14} />
          </button>
          <div className={styles.toolDivider} />
          {onOpenProperties && (
            <button
              type="button"
              className={styles.toolBtn}
              onClick={onOpenProperties}
              title="Propriedades da tabela"
            >
              <Settings2 size={14} />
            </button>
          )}
          <button
            type="button"
            className={`${styles.toolBtn} ${styles.toolBtnDanger}`}
            onClick={ops.deleteTable}
            title="Excluir tabela"
          >
            <Trash2 size={14} />
          </button>
        </div>

        {/* F3 — Calhas de altura de linha (borda inferior de cada linha). */}
        {rowEdges.map((y, i) => (
          <div
            key={i}
            className={styles.rowResizeHandle}
            style={{ top: `${y}px`, width: `${rect.width}px` }}
            onMouseDown={handleRowResizeDown(i)}
            title="Arraste para ajustar a altura da linha"
          />
        ))}
      </div>

      {/* Menu de contexto (botão direito) */}
      {contextMenu && (
        <div
          className={styles.contextMenu}
          style={{ left: contextMenu.x, top: contextMenu.y }}
          onContextMenu={(e) => e.preventDefault()}
        >
          <MenuItem icon={<ArrowUpToLine size={14} />} label="Inserir linha acima" onClick={closeMenuThen(ops.addRowBefore)} />
          <MenuItem icon={<ArrowDownToLine size={14} />} label="Inserir linha abaixo" onClick={closeMenuThen(ops.addRowAfter)} />
          <MenuItem icon={<Rows2 size={14} />} label="Excluir linha" onClick={closeMenuThen(ops.deleteRow)} />
          <div className={styles.menuDivider} />
          <MenuItem icon={<PanelLeftClose size={14} />} label="Inserir coluna à esquerda" onClick={closeMenuThen(ops.addColumnBefore)} />
          <MenuItem icon={<PanelRightClose size={14} />} label="Inserir coluna à direita" onClick={closeMenuThen(ops.addColumnAfter)} />
          <MenuItem icon={<Columns2 size={14} />} label="Excluir coluna" onClick={closeMenuThen(ops.deleteColumn)} />
          <div className={styles.menuDivider} />
          <MenuItem icon={<Merge size={14} />} label="Mesclar células" onClick={closeMenuThen(ops.mergeCells)} disabled={!ops.canMergeCells()} />
          <MenuItem icon={<Split size={14} />} label="Dividir célula" onClick={closeMenuThen(ops.splitCell)} disabled={!ops.canSplitCell()} />
          <MenuItem icon={<TableRowsSplit size={14} />} label="Alternar cabeçalho" onClick={closeMenuThen(ops.toggleHeaderRow)} />
          {ops.canToggleCaption() && (
            <MenuItem
              icon={ops.captionVisible() ? <CaptionsOff size={14} /> : <Captions size={14} />}
              label={ops.captionVisible() ? "Remover legenda" : "Adicionar legenda"}
              onClick={closeMenuThen(ops.toggleCaption)}
            />
          )}
          <div className={styles.menuDivider} />
          <MenuItem icon={<ChevronUp size={14} />} label="Mover para cima" onClick={closeMenuThen(() => ops.moveBlock("up"))} disabled={!ops.canMoveUp()} />
          <MenuItem icon={<ChevronDown size={14} />} label="Mover para baixo" onClick={closeMenuThen(() => ops.moveBlock("down"))} disabled={!ops.canMoveDown()} />
          {onOpenProperties && (
            <>
              <div className={styles.menuDivider} />
              <MenuItem icon={<Settings2 size={14} />} label="Propriedades…" onClick={closeMenuThen(onOpenProperties)} />
            </>
          )}
          <div className={styles.menuDivider} />
          <MenuItem
            icon={<Trash2 size={14} />}
            label="Excluir tabela"
            onClick={closeMenuThen(ops.deleteTable)}
            danger
          />
        </div>
      )}

      {dragTooltip && (
        <div
          className={styles.dragTooltip}
          style={{ left: `${dragTooltip.x + 14}px`, top: `${dragTooltip.y + 14}px` }}
        >
          {dragTooltip.label}
        </div>
      )}
    </>
  );
}

/** Normaliza a cor da célula pra `#RRGGBB` aceito por `<input type="color">`.
 *  null/sem cor → branco (o input precisa de um valor; o swatch já mostra o
 *  estado "transparente" via xadrez). Espelha o normalizeColor do TextBox. */
function normalizeColor(color: string | null): string {
  if (!color) return "#ffffff";
  if (color.startsWith("#") && color.length === 7) return color;
  if (color.startsWith("#") && color.length === 4) {
    const r = color[1];
    const g = color[2];
    const b = color[3];
    return `#${r}${r}${g}${g}${b}${b}`;
  }
  return "#ffffff";
}

function MenuItem({
  icon,
  label,
  onClick,
  disabled,
  danger,
}: {
  icon: React.ReactNode;
  label: string;
  onClick: () => void;
  disabled?: boolean;
  danger?: boolean;
}) {
  return (
    <button
      type="button"
      className={`${styles.menuItem} ${danger ? styles.menuItemDanger : ""}`}
      onClick={onClick}
      disabled={disabled}
    >
      {icon}
      <span className={styles.menuLabel}>{label}</span>
    </button>
  );
}
