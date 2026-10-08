/** Trilho de ferramentas (ícone + texto) e prateleira com miniaturas: clique arma, arrasto solta na cena. */

import { useCallback, useEffect, useRef, useState, type PointerEvent as ReactPointerEvent } from "react";
import {
  ArrowLeft,
  FileImage,
  Image as ImageIcon,
  Layers,
  MapPin,
  Pin,
  Plus,
  Redo2,
  Trash2,
  Undo2,
  X,
} from "lucide-react";
import type { Tool } from "../useEditorState";
import { RAIL, groupOfTool, type PaletteItem, type RailEntry } from "./catalog";
import { ACTION_BY_ID } from "@core/keymapActions";
import { formatBinding } from "@core/keymap";
import { useKeymapStore } from "@stores/keymapStore";

type OpenEntry = Extract<RailEntry, { kind: "group" } | { kind: "actions" }>;

const EXPORT_ENTRY: OpenEntry = { kind: "actions", id: "exportar", label: "Exportar", icon: FileImage };
import styles from "./Palette.module.css";

interface Props {
  activeTool: Tool;
  onSelectTool: (t: Tool) => void;
  /** Miniatura solta sobre a tela (coordenadas de viewport). */
  onDropTool: (t: Tool, clientX: number, clientY: number) => void;
  canDelete: boolean;
  onDelete: () => void;
  canUndo: boolean;
  onUndo: () => void;
  canRedo: boolean;
  onRedo: () => void;
  canDuplicate: boolean;
  onDuplicate: () => void;
  onImportBackground: () => void;
  onImportDrone?: () => void;
  onImportOsm?: () => void;
  onCenterBackground?: () => void;
  onFitBackground?: () => void;
  onResetBackgroundRotation?: () => void;
  onRemoveBackground?: () => void;
  hasBackground: boolean;
  bgLocked: boolean;
  onToggleBackgroundLock: () => void;
  bgOpacity: number;
  onChangeBackgroundOpacity: (v: number) => void;
  onSave: () => void;
  onExportPng: () => void;
  onExportPngClean?: () => void;
  onBackToList: () => void;
  saving: boolean;
  exporting: boolean;
}

export function Palette(props: Props) {
  const { activeTool, onSelectTool, onDropTool, onBackToList } = props;
  // Tecla de cada ferramenta (o que o usuário configurou, senão o padrão).
  const overrides = useKeymapStore((s) => s.overrides);
  const keyOf = (action?: string) => {
    if (!action) return "";
    const raw = overrides[action] ?? ACTION_BY_ID[action]?.defaultBinding ?? "";
    return raw ? formatBinding(raw).replace(/Shift \+ /, "⇧") : "";
  };
  const [open, setOpen] = useState<string | null>(null);
  const [pinned, setPinned] = useState(false);
  const pinnedRef = useRef(false);
  pinnedRef.current = pinned;
  const activeGroup = groupOfTool(activeTool);

  useEffect(() => {
    if (!open) return;
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") setOpen(null);
    };
    document.addEventListener("keydown", onKey);
    return () => document.removeEventListener("keydown", onKey);
  }, [open]);

  const closeUnlessPinned = useCallback(() => {
    if (!pinnedRef.current) setOpen(null);
  }, []);

  const pickDirect = (tool: Tool) => {
    onSelectTool(tool);
    setOpen(null);
  };

  // Pressionar numa miniatura: soltou no lugar = arma; moveu = arrasta um fantasma até a cena.
  const onThumbPointerDown = (e: ReactPointerEvent<HTMLButtonElement>, item: PaletteItem) => {
    if (e.pointerType === "mouse" && e.button !== 0) return;
    const el = e.currentTarget;
    const startX = e.clientX;
    const startY = e.clientY;
    const pointerId = e.pointerId;
    let moved = false;
    let ghost: HTMLElement | null = null;
    el.setPointerCapture(pointerId);
    const move = (ev: PointerEvent) => {
      if (!moved && Math.hypot(ev.clientX - startX, ev.clientY - startY) > 5) {
        moved = true;
        ghost = makeGhost(el);
        document.body.appendChild(ghost);
      }
      if (ghost) {
        ghost.style.left = `${ev.clientX}px`;
        ghost.style.top = `${ev.clientY}px`;
      }
    };
    const up = (ev: PointerEvent) => {
      el.removeEventListener("pointermove", move);
      el.removeEventListener("pointerup", up);
      el.removeEventListener("pointercancel", up);
      try {
        el.releasePointerCapture(pointerId);
      } catch {
        /* já solto */
      }
      ghost?.remove();
      if (ev.type === "pointercancel") return;
      if (moved) onDropTool(item.tool, ev.clientX, ev.clientY);
      else onSelectTool(item.tool);
      closeUnlessPinned();
    };
    el.addEventListener("pointermove", move);
    el.addEventListener("pointerup", up);
    el.addEventListener("pointercancel", up);
  };

  const openEntry =
    open === "exportar"
      ? EXPORT_ENTRY
      : RAIL.find((x): x is OpenEntry => (x.kind === "group" || x.kind === "actions") && x.id === open);

  return (
    <div className={styles.root}>
      <nav className={styles.rail} aria-label="Ferramentas do croqui">
        <button type="button" className={styles.back} onClick={onBackToList} title="Voltar para a lista de croquis">
          <ArrowLeft size={12} /> Voltar
        </button>
        {RAIL.map((e, i) => {
          if (e.kind === "sep") return <div key={`sep${i}`} className={styles.sep} />;
          const Icon = e.icon;
          if (e.kind === "tool") {
            const active = activeTool === e.tool;
            return (
              <button
                key={e.tool}
                type="button"
                className={`${styles.entry} ${active ? styles.entryActive : ""}`}
                onClick={() => pickDirect(e.tool)}
                title={keyOf(e.action) ? `${e.label} (${keyOf(e.action)})` : e.label}
              >
                <Icon size={18} aria-hidden />
                <span>{e.label}</span>
                {keyOf(e.action) && <kbd className={styles.key}>{keyOf(e.action)}</kbd>}
              </button>
            );
          }
          const isOpen = open === e.id;
          const active = e.kind === "group" && activeGroup === e.id;
          return (
            <button
              key={e.id}
              type="button"
              className={[styles.entry, active ? styles.entryActive : "", isOpen ? styles.entryOpen : ""].join(" ")}
              onClick={() => setOpen(isOpen ? null : e.id)}
              aria-expanded={isOpen}
              title={e.kind === "group" && keyOf(e.action) ? `${e.label} (${keyOf(e.action)})` : e.label}
            >
              <Icon size={18} aria-hidden />
              <span>{e.label}</span>
              {e.kind === "group" && keyOf(e.action) && <kbd className={styles.key}>{keyOf(e.action)}</kbd>}
            </button>
          );
        })}
        <div className={styles.spacer} />
        <button type="button" className={styles.bottomBtn} onClick={props.onSave} disabled={props.saving} title="Salvar (Ctrl+S)">
          {props.saving ? "Salvando…" : "Salvar"}
        </button>
        <button
          type="button"
          className={`${styles.bottomBtn} ${styles.bottomPrimary}`}
          onClick={() => setOpen(open === "exportar" ? null : "exportar")}
          disabled={props.exporting}
          aria-expanded={open === "exportar"}
          title="Exportar PNG técnico (Ctrl+E) ou limpo (Ctrl+Shift+E)"
        >
          {props.exporting ? "Exportando…" : "Exportar"}
        </button>
      </nav>

      {openEntry && (
        <section className={styles.shelf} aria-label={openEntry.label}>
          <header className={styles.shelfHead}>
            <h4 className={styles.shelfTitle}>{openEntry.label}</h4>
            {openEntry.kind === "group" && (
              <button
                type="button"
                className={`${styles.iconBtn} ${pinned ? styles.iconBtnOn : ""}`}
                onClick={() => setPinned((v) => !v)}
                title={pinned ? "Prateleira fixa (clique para voltar a fechar sozinha)" : "Manter a prateleira aberta"}
                aria-pressed={pinned}
              >
                <Pin size={12} />
              </button>
            )}
            <button type="button" className={styles.iconBtn} onClick={() => setOpen(null)} title="Fechar (Esc)">
              <X size={12} />
            </button>
          </header>
          <div className={styles.shelfBody}>
            {openEntry.kind === "group" ? (
              <div className={styles.grid}>
                {openEntry.items.map((it) => (
                  <button
                    key={it.tool}
                    type="button"
                    className={`${styles.thumb} ${activeTool === it.tool ? styles.thumbActive : ""}`}
                    onPointerDown={(e) => onThumbPointerDown(e, it)}
                    onClick={(e) => {
                      // Só teclado (Enter/Espaço); o mouse já foi tratado no pointerdown.
                      if (e.detail === 0) {
                        onSelectTool(it.tool);
                        closeUnlessPinned();
                      }
                    }}
                    title={keyOf(it.action) ? `${it.label} (${keyOf(it.action)})` : it.label}
                  >
                    <div className={styles.thumbArt} data-thumb>
                      {it.thumb}
                      {keyOf(it.action) && <kbd className={styles.thumbKey}>{keyOf(it.action)}</kbd>}
                    </div>
                    <span className={styles.thumbLabel}>{it.label}</span>
                  </button>
                ))}
              </div>
            ) : openEntry.id === "imagem" ? (
              <ImageActions {...props} />
            ) : openEntry.id === "editar" ? (
              <EditActions {...props} />
            ) : (
              <ExportActions p={props} onDone={() => setOpen(null)} />
            )}
          </div>
          {openEntry.kind === "group" && (
            <div className={styles.shelfFoot}>Arraste para a cena, ou clique e depois clique na cena. Esc fecha.</div>
          )}
        </section>
      )}
    </div>
  );
}

/** Cópia visual da miniatura que segue o cursor (canvases copiados pixel a pixel). */
function makeGhost(from: HTMLElement): HTMLElement {
  const g = document.createElement("div");
  g.className = styles.ghost ?? "";
  const art = from.querySelector<HTMLElement>("[data-thumb]");
  if (art) {
    const clone = art.cloneNode(true) as HTMLElement;
    const src = art.querySelectorAll("canvas");
    clone.querySelectorAll("canvas").forEach((c, i) => {
      const s = src[i];
      if (!s) return;
      c.width = s.width;
      c.height = s.height;
      c.getContext("2d")?.drawImage(s, 0, 0);
    });
    clone.style.transform = "scale(1.25)";
    g.appendChild(clone);
  }
  return g;
}

function ImageActions(p: Props) {
  return (
    <div className={styles.actions}>
      <button type="button" className={styles.actionBtn} onClick={p.onImportBackground} title="Imagem do disco como fundo">
        <ImageIcon size={13} /> Importar imagem
      </button>
      {p.onImportDrone && (
        <button type="button" className={styles.actionBtn} onClick={p.onImportDrone} title="Correção de lente, recorte e sidecar antes de usar como fundo">
          <FileImage size={13} /> Importar drone…
        </button>
      )}
      {p.onImportOsm && (
        <button type="button" className={styles.actionBtn} onClick={p.onImportOsm} title="Vias reais do OpenStreetMap, editáveis">
          <MapPin size={13} /> Importar OSM…
        </button>
      )}
      {p.hasBackground && (
        <>
          <div className={styles.actionsTitle}>Fundo atual</div>
          <button type="button" className={styles.actionBtn} onClick={p.onToggleBackgroundLock}>
            <Layers size={13} /> {p.bgLocked ? "Desbloquear fundo" : "Bloquear fundo"}
          </button>
          <label className={styles.slider}>
            <span>Opacidade</span>
            <input
              type="range"
              min={0.1}
              max={1}
              step={0.05}
              value={p.bgOpacity}
              onChange={(e) => p.onChangeBackgroundOpacity(Number(e.target.value))}
            />
            <span>{Math.round(p.bgOpacity * 100)}%</span>
          </label>
          {p.onCenterBackground && (
            <button type="button" className={styles.actionBtn} onClick={p.onCenterBackground}>Centralizar na folha</button>
          )}
          {p.onFitBackground && (
            <button type="button" className={styles.actionBtn} onClick={p.onFitBackground}>Tamanho original</button>
          )}
          {p.onResetBackgroundRotation && (
            <button type="button" className={styles.actionBtn} onClick={p.onResetBackgroundRotation}>Rotação a 0°</button>
          )}
          {p.onRemoveBackground && (
            <button type="button" className={styles.actionBtn} onClick={p.onRemoveBackground}>Remover fundo</button>
          )}
        </>
      )}
    </div>
  );
}

function EditActions(p: Props) {
  return (
    <div className={styles.actions}>
      <button type="button" className={styles.actionBtn} onClick={p.onUndo} disabled={!p.canUndo}>
        <Undo2 size={13} /> Desfazer <kbd>Ctrl+Z</kbd>
      </button>
      <button type="button" className={styles.actionBtn} onClick={p.onRedo} disabled={!p.canRedo}>
        <Redo2 size={13} /> Refazer <kbd>Ctrl+Y</kbd>
      </button>
      <button type="button" className={styles.actionBtn} onClick={p.onDuplicate} disabled={!p.canDuplicate}>
        <Plus size={13} /> Duplicar <kbd>Ctrl+D</kbd>
      </button>
      <button type="button" className={styles.actionBtn} onClick={p.onDelete} disabled={!p.canDelete}>
        <Trash2 size={13} /> Excluir <kbd>Del</kbd>
      </button>
    </div>
  );
}

function ExportActions({ p, onDone }: { p: Props; onDone: () => void }) {
  return (
    <div className={styles.actions}>
      <button
        type="button"
        className={styles.actionBtn}
        disabled={p.exporting}
        onClick={() => {
          p.onExportPng();
          onDone();
        }}
      >
        <FileImage size={13} /> PNG técnico <kbd>Ctrl+E</kbd>
      </button>
      <div className={styles.actionHint}>Com carimbo: folha e data. Para anexar ao laudo.</div>
      {p.onExportPngClean && (
        <>
          <button
            type="button"
            className={styles.actionBtn}
            disabled={p.exporting}
            onClick={() => {
              p.onExportPngClean?.();
              onDone();
            }}
          >
            <FileImage size={13} /> PNG limpo <kbd>Ctrl+Shift+E</kbd>
          </button>
          <div className={styles.actionHint}>Só o desenho. Para o corpo do laudo.</div>
        </>
      )}
      <div className={styles.actionHint}>A pasta abre com o arquivo exportado.</div>
    </div>
  );
}
