/** Paleta da planta: trilho com ícone e nome, prateleira com miniaturas (clique arma, arrasto solta na tela). */
import { useCallback, useEffect, useRef, useState, type PointerEvent as ReactPointerEvent, type ReactNode } from "react";
import {
  ArrowLeft,
  BrickWall,
  Building2,
  Car,
  DoorOpen,
  FileImage,
  Image as ImageIcon,
  MapPin,
  Maximize,
  MousePointer2,
  MoveUpRight,
  PersonStanding,
  Pin,
  Redo2,
  Ruler,
  Sofa,
  Square,
  Trash2,
  Type,
  Undo2,
  X,
  type LucideIcon,
} from "lucide-react";
import { OPENING_DEFAULTS, type OpeningKind } from "../model/schema";
import { PERSON_PRIMS, PERSON_SIZE, primsSvg, SYMBOLS, type PrimStyle } from "../model/symbols";
import type { Tool } from "./store";
import pal from "../../editor/palette/Palette.module.css";
import styles from "./planta.module.css";

interface ShelfItem {
  tool: Tool;
  label: string;
  thumb: ReactNode;
  section?: string;
}
type Entry =
  | { kind: "tool"; tool: Tool; label: string; icon: LucideIcon; hint?: string }
  | { kind: "group"; id: string; label: string; icon: LucideIcon; items: ShelfItem[] }
  | { kind: "actions"; id: string; label: string; icon: LucideIcon }
  | { kind: "sep" };

const TH: PrimStyle = { ink: "#334155", paper: "#ffffff", tone: "#dbe1ea", body: "#fde8e8", bodyInk: "#9f2a2a", lw: 0.03 };

function SymThumb({ prims, w, d, body = false }: { prims: Parameters<typeof primsSvg>[0]; w: number; d: number; body?: boolean }) {
  const m = Math.max(w, d) * 1.18;
  const lw = m / 40;
  return (
    <svg className={styles.thumbSvg} viewBox={`${-m / 2} ${-m / 2} ${m} ${m}`} aria-hidden dangerouslySetInnerHTML={{ __html: primsSvg(prims, w, d, { ...TH, lw }, body) }} />
  );
}

function OpeningThumb({ kind }: { kind: OpeningKind }) {
  const c = "#334155";
  const wall = (x: number, w: number) => <rect x={x} y={52} width={w} height={8} fill={c} />;
  const parts: Record<OpeningKind, ReactNode> = {
    porta: (
      <>
        {wall(4, 20)}
        {wall(76, 20)}
        <line x1={24} y1={52} x2={24} y2={8} stroke={c} strokeWidth={2.4} />
        <path d="M24 8 A44 44 0 0 1 76 52" fill="none" stroke={c} strokeWidth={1.5} strokeDasharray="4 3" />
      </>
    ),
    porta_dupla: (
      <>
        {wall(0, 10)}
        {wall(90, 10)}
        <line x1={10} y1={52} x2={10} y2={14} stroke={c} strokeWidth={2.4} />
        <line x1={90} y1={52} x2={90} y2={14} stroke={c} strokeWidth={2.4} />
        <path d="M10 14 A40 40 0 0 1 50 52" fill="none" stroke={c} strokeWidth={1.5} strokeDasharray="4 3" />
        <path d="M90 14 A40 40 0 0 0 50 52" fill="none" stroke={c} strokeWidth={1.5} strokeDasharray="4 3" />
      </>
    ),
    correr: (
      <>
        {wall(4, 14)}
        {wall(82, 14)}
        <rect x={18} y={51} width={36} height={3.5} fill={c} />
        <rect x={46} y={57} width={36} height={3.5} fill={c} />
        <path d="M34 38 H64 M58 33 L66 38 L58 43" fill="none" stroke={c} strokeWidth={1.6} />
      </>
    ),
    janela: (
      <>
        {wall(4, 24)}
        {wall(72, 24)}
        <rect x={28} y={52} width={44} height={8} fill="none" stroke={c} strokeWidth={1.2} />
        <line x1={28} y1={54.8} x2={72} y2={54.8} stroke={c} strokeWidth={1} />
        <line x1={28} y1={57.2} x2={72} y2={57.2} stroke={c} strokeWidth={1} />
      </>
    ),
    basculante: (
      <>
        {wall(4, 34)}
        {wall(62, 34)}
        <rect x={38} y={52} width={24} height={8} fill="none" stroke={c} strokeWidth={1.2} />
        <line x1={38} y1={60} x2={62} y2={52} stroke={c} strokeWidth={1} />
      </>
    ),
    vao: (
      <>
        {wall(4, 22)}
        {wall(74, 22)}
        <line x1={26} y1={52} x2={74} y2={52} stroke={c} strokeWidth={1.2} strokeDasharray="4 3" />
        <line x1={26} y1={60} x2={74} y2={60} stroke={c} strokeWidth={1.2} strokeDasharray="4 3" />
      </>
    ),
  };
  return (
    <svg className={styles.thumbSvg} viewBox="0 0 100 70" aria-hidden>
      {parts[kind]}
    </svg>
  );
}

const symItems = (groups: string[]): ShelfItem[] =>
  SYMBOLS.filter((s) => groups.includes(s.group)).map((s) => ({ tool: `item:${s.id}` as Tool, label: s.label, section: s.group, thumb: <SymThumb prims={s.prims} w={s.w} d={s.d} /> }));

export const RAIL: Entry[] = [
  { kind: "tool", tool: "select", label: "Selecionar", icon: MousePointer2, hint: "V" },
  { kind: "sep" },
  { kind: "tool", tool: "wall", label: "Parede", icon: BrickWall, hint: "P" },
  { kind: "tool", tool: "room", label: "Cômodo", icon: Square, hint: "C" },
  {
    kind: "group",
    id: "aberturas",
    label: "Aberturas",
    icon: DoorOpen,
    items: (Object.keys(OPENING_DEFAULTS) as OpeningKind[]).map((k) => ({ tool: `opening:${k}` as Tool, label: OPENING_DEFAULTS[k].label, thumb: <OpeningThumb kind={k} /> })),
  },
  { kind: "group", id: "estrutura", label: "Estrutura", icon: Building2, items: symItems(["Estrutura"]) },
  { kind: "group", id: "mobilia", label: "Mobília", icon: Sofa, items: symItems(["Sala", "Jantar", "Quarto", "Cozinha", "Serviço", "Banheiro", "Escritório"]) },
  { kind: "group", id: "externo", label: "Externo", icon: Car, items: symItems(["Externo"]) },
  { kind: "sep" },
  { kind: "tool", tool: "dim", label: "Cota", icon: Ruler },
  { kind: "tool", tool: "evidence", label: "Vestígio", icon: MapPin },
  {
    kind: "group",
    id: "pessoas",
    label: "Pessoa",
    icon: PersonStanding,
    items: [
      { tool: "person:em_pe", label: "Em pé", thumb: <SymThumb prims={PERSON_PRIMS.em_pe} w={PERSON_SIZE.em_pe.w} d={PERSON_SIZE.em_pe.d} body /> },
      { tool: "person:caido", label: "Caída", thumb: <SymThumb prims={PERSON_PRIMS.caido} w={PERSON_SIZE.caido.w} d={PERSON_SIZE.caido.d} body /> },
    ],
  },
  { kind: "tool", tool: "traj", label: "Trajetória", icon: MoveUpRight },
  { kind: "tool", tool: "text", label: "Texto", icon: Type },
  { kind: "sep" },
  { kind: "actions", id: "fundo", label: "Fundo", icon: ImageIcon },
  { kind: "actions", id: "editar", label: "Editar", icon: Undo2 },
];

export function groupOf(t: Tool): string | null {
  for (const e of RAIL) if (e.kind === "group" && e.items.some((i) => i.tool === t)) return e.id;
  return null;
}

export interface PaletteProps {
  activeTool: Tool;
  onSelectTool: (t: Tool) => void;
  onDropTool: (t: Tool, clientX: number, clientY: number) => void;
  onBack: () => void;
  onSave: () => void;
  saving: boolean;
  onExport: (v: "tecnico" | "limpo") => void;
  exporting: boolean;
  canUndo: boolean;
  onUndo: () => void;
  canRedo: boolean;
  onRedo: () => void;
  canDelete: boolean;
  onDelete: () => void;
  onFit: () => void;
  hasBackground: boolean;
  onImportBackground: () => void;
  onRemoveBackground: () => void;
  bgOpacity: number;
  onBgOpacity: (v: number) => void;
  onScaleBackground: () => void;
  bgLocked: boolean;
  onToggleBgLock: () => void;
}

export function PlantaPalette(p: PaletteProps) {
  const [open, setOpen] = useState<string | null>(null);
  const [pinned, setPinned] = useState(false);
  const pinnedRef = useRef(false);
  pinnedRef.current = pinned;
  const activeGroup = groupOf(p.activeTool);

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

  const onThumbDown = (e: ReactPointerEvent<HTMLButtonElement>, it: ShelfItem) => {
    if (e.pointerType === "mouse" && e.button !== 0) return;
    const el = e.currentTarget;
    const sx = e.clientX;
    const sy = e.clientY;
    const pid = e.pointerId;
    let moved = false;
    let ghost: HTMLElement | null = null;
    el.setPointerCapture(pid);
    const move = (ev: PointerEvent) => {
      if (!moved && Math.hypot(ev.clientX - sx, ev.clientY - sy) > 5) {
        moved = true;
        ghost = document.createElement("div");
        ghost.className = pal.ghost ?? "";
        const art = el.querySelector<HTMLElement>("[data-thumb]");
        if (art) {
          const c = art.cloneNode(true) as HTMLElement;
          c.style.transform = "scale(1.25)";
          ghost.appendChild(c);
        }
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
        el.releasePointerCapture(pid);
      } catch {
        /* já solto */
      }
      ghost?.remove();
      if (ev.type === "pointercancel") return;
      if (moved) p.onDropTool(it.tool, ev.clientX, ev.clientY);
      else p.onSelectTool(it.tool);
      closeUnlessPinned();
    };
    el.addEventListener("pointermove", move);
    el.addEventListener("pointerup", up);
    el.addEventListener("pointercancel", up);
  };

  const openEntry = open === "exportar" ? ({ kind: "actions", id: "exportar", label: "Exportar", icon: FileImage } as const) : RAIL.find((x) => (x.kind === "group" || x.kind === "actions") && x.id === open);

  return (
    <div className={pal.root}>
      <nav className={pal.rail} aria-label="Ferramentas da planta">
        <button type="button" className={pal.back} onClick={p.onBack} title="Voltar para a lista de croquis">
          <ArrowLeft size={12} /> Voltar
        </button>
        {RAIL.map((e, i) => {
          if (e.kind === "sep") return <div key={`s${i}`} className={pal.sep} />;
          const Icon = e.icon;
          if (e.kind === "tool") {
            const active = p.activeTool === e.tool;
            return (
              <button
                key={e.tool}
                type="button"
                className={`${pal.entry} ${active ? pal.entryActive : ""}`}
                onClick={() => {
                  p.onSelectTool(e.tool);
                  setOpen(null);
                }}
                title={e.hint ? `${e.label} (${e.hint})` : e.label}
              >
                <Icon size={18} aria-hidden />
                <span>{e.label}</span>
              </button>
            );
          }
          const isOpen = open === e.id;
          const active = e.kind === "group" && activeGroup === e.id;
          return (
            <button
              key={e.id}
              type="button"
              className={[pal.entry, active ? pal.entryActive : "", isOpen ? pal.entryOpen : ""].join(" ")}
              onClick={() => setOpen(isOpen ? null : e.id)}
              aria-expanded={isOpen}
              title={e.label}
            >
              <Icon size={18} aria-hidden />
              <span>{e.label}</span>
            </button>
          );
        })}
        <div className={pal.spacer} />
        <button type="button" className={pal.bottomBtn} onClick={p.onSave} disabled={p.saving} title="Salvar (Ctrl+S)">
          {p.saving ? "Salvando…" : "Salvar"}
        </button>
        <button
          type="button"
          className={`${pal.bottomBtn} ${pal.bottomPrimary}`}
          onClick={() => setOpen(open === "exportar" ? null : "exportar")}
          disabled={p.exporting}
          aria-expanded={open === "exportar"}
          title="Exportar PNG técnico (Ctrl+E) ou limpo (Ctrl+Shift+E)"
        >
          {p.exporting ? "Exportando…" : "Exportar"}
        </button>
      </nav>
      {openEntry && openEntry.kind !== "tool" && openEntry.kind !== "sep" && (
        <section className={pal.shelf} aria-label={openEntry.label}>
          <header className={pal.shelfHead}>
            <h4 className={pal.shelfTitle}>{openEntry.label}</h4>
            {openEntry.kind === "group" && (
              <button
                type="button"
                className={`${pal.iconBtn} ${pinned ? pal.iconBtnOn : ""}`}
                onClick={() => setPinned((v) => !v)}
                title={pinned ? "Prateleira fixa (clique para voltar a fechar sozinha)" : "Manter a prateleira aberta"}
                aria-pressed={pinned}
              >
                <Pin size={12} />
              </button>
            )}
            <button type="button" className={pal.iconBtn} onClick={() => setOpen(null)} title="Fechar (Esc)">
              <X size={12} />
            </button>
          </header>
          <div className={pal.shelfBody}>
            {openEntry.kind === "group" ? (
              <Shelf items={openEntry.items} active={p.activeTool} onDown={onThumbDown} onKey={(t) => { p.onSelectTool(t); closeUnlessPinned(); }} />
            ) : openEntry.id === "fundo" ? (
              <div className={pal.actions}>
                <button type="button" className={pal.actionBtn} onClick={p.onImportBackground}>
                  <ImageIcon size={13} /> {p.hasBackground ? "Trocar imagem" : "Importar imagem"}
                </button>
                {p.hasBackground && (
                  <>
                    <button type="button" className={pal.actionBtn} onClick={p.onScaleBackground}>
                      <Ruler size={13} /> Escala por dois pontos
                    </button>
                    <button type="button" className={pal.actionBtn} onClick={p.onToggleBgLock}>
                      <Pin size={13} /> {p.bgLocked ? "Destravar para mover" : "Travar no lugar"}
                    </button>
                    <div className={pal.slider}>
                      <span>Opacidade</span>
                      <input type="range" min={0.1} max={1} step={0.05} value={p.bgOpacity} onChange={(e) => p.onBgOpacity(Number(e.target.value))} />
                    </div>
                    <button type="button" className={pal.actionBtn} onClick={p.onRemoveBackground}>
                      <Trash2 size={13} /> Remover fundo
                    </button>
                  </>
                )}
                <div className={pal.actionHint}>Foto do croqui feito no local ou planta do imóvel, para decalcar por cima. Não sai no PNG.</div>
              </div>
            ) : openEntry.id === "editar" ? (
              <div className={pal.actions}>
                <button type="button" className={pal.actionBtn} onClick={p.onUndo} disabled={!p.canUndo}>
                  <Undo2 size={13} /> Desfazer <kbd>Ctrl+Z</kbd>
                </button>
                <button type="button" className={pal.actionBtn} onClick={p.onRedo} disabled={!p.canRedo}>
                  <Redo2 size={13} /> Refazer <kbd>Ctrl+Y</kbd>
                </button>
                <button type="button" className={pal.actionBtn} onClick={p.onDelete} disabled={!p.canDelete}>
                  <Trash2 size={13} /> Apagar seleção <kbd>Del</kbd>
                </button>
                <button type="button" className={pal.actionBtn} onClick={p.onFit}>
                  <Maximize size={13} /> Ver a folha inteira <kbd>Ctrl+0</kbd>
                </button>
              </div>
            ) : (
              <div className={pal.actions}>
                <button type="button" className={pal.actionBtn} disabled={p.exporting} onClick={() => { p.onExport("tecnico"); setOpen(null); }}>
                  <FileImage size={13} /> PNG técnico <kbd>Ctrl+E</kbd>
                </button>
                <div className={pal.actionHint}>Com cabeçalho, cotas, legenda dos vestígios e áreas. Para anexar ao laudo.</div>
                <button type="button" className={pal.actionBtn} disabled={p.exporting} onClick={() => { p.onExport("limpo"); setOpen(null); }}>
                  <FileImage size={13} /> PNG limpo <kbd>Ctrl+Shift+E</kbd>
                </button>
                <div className={pal.actionHint}>Só o desenho. Para o corpo do laudo.</div>
                <div className={pal.actionHint}>A pasta abre com o arquivo exportado.</div>
              </div>
            )}
          </div>
          {openEntry.kind === "group" && <div className={pal.shelfFoot}>Arraste para a tela, ou clique e depois clique na tela. Esc fecha.</div>}
        </section>
      )}
    </div>
  );
}

function Shelf({ items, active, onDown, onKey }: { items: ShelfItem[]; active: Tool; onDown: (e: ReactPointerEvent<HTMLButtonElement>, it: ShelfItem) => void; onKey: (t: Tool) => void }) {
  const sections = [...new Set(items.map((i) => i.section ?? ""))];
  return (
    <>
      {sections.map((sec) => (
        <div key={sec || "_"}>
          {sec && sections.length > 1 && <div className={styles.shelfSection}>{sec}</div>}
          <div className={pal.grid}>
            {items
              .filter((i) => (i.section ?? "") === sec)
              .map((it) => (
                <button
                  key={it.tool}
                  type="button"
                  className={`${pal.thumb} ${active === it.tool ? pal.thumbActive : ""}`}
                  onPointerDown={(e) => onDown(e, it)}
                  onClick={(e) => {
                    if (e.detail === 0) onKey(it.tool);
                  }}
                  title={it.label}
                >
                  <div className={pal.thumbArt} data-thumb>
                    {it.thumb}
                  </div>
                  <span className={pal.thumbLabel}>{it.label}</span>
                </button>
              ))}
          </div>
        </div>
      ))}
    </>
  );
}
