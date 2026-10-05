/**
 * Bancada de filtros: a pilha ocupa o painel direito (Ajustes + filtros, na ordem de aplicação),
 * "+ Filtro" abre a galeria com miniaturas da própria imagem, e receitas montam pilhas prontas.
 */

import { useEffect, useMemo, useRef, useState, type ReactNode } from "react";
import {
  ArrowDown,
  ArrowUp,
  BarChart3,
  ChevronDown,
  Eye,
  EyeOff,
  History,
  Layers,
  Plus,
  Search,
  SlidersHorizontal,
  Trash2,
  X,
} from "lucide-react";
import { commands } from "@core/commands";
import type { BackendAdjustments, BackendOperation } from "@domain/image_analysis";
import type { ProcessingOp, ProcessingOpKind, SicroImageSelection } from "../engine/schema";
import {
  FILTER_INDEX,
  OpControls,
  isMaskableOpKind,
  makeProcessingOp,
  processingOpToBackendOperation,
} from "./ProcessingStackPanel";
import styles from "./FilterBench.module.css";
import { askText } from "@components/Dialog/ask";

export type SidePanel = "filtros" | "camadas" | "analise" | "historico";

const LABEL = new Map(FILTER_INDEX.map((f) => [f.kind, f.label] as const));

// ---------------------------------------------------------------------------
// Receitas

export interface Recipe {
  name: string;
  builtin?: boolean;
  ops: { kind: ProcessingOpKind; params?: Record<string, unknown> }[];
}

const BUILTIN_RECIPES: Recipe[] = [
  { name: "Placa no escuro", builtin: true, ops: [{ kind: "auto_levels" }, { kind: "clahe" }, { kind: "unsharp_mask" }] },
  { name: "Bordas e marcas", builtin: true, ops: [{ kind: "auto_levels" }, { kind: "edge_sobel" }] },
  { name: "Adulteração (ELA)", builtin: true, ops: [{ kind: "ela" }] },
];
const RECIPES_KEY = "sicro.imagem.receitas.v1";

function loadSaved(): Recipe[] {
  try {
    const v: unknown = JSON.parse(localStorage.getItem(RECIPES_KEY) ?? "[]");
    return Array.isArray(v) ? (v as Recipe[]) : [];
  } catch {
    return [];
  }
}
function storeSaved(list: Recipe[]) {
  try {
    localStorage.setItem(RECIPES_KEY, JSON.stringify(list));
  } catch {
    /* sem armazenamento: a receita vale só nesta sessão */
  }
}

// ---------------------------------------------------------------------------
// Painel da pilha

export function FilterStackPanel({
  stack,
  onChange,
  adjustmentsSlot,
  adjustmentsActive,
  onResetAdjustments,
  activeSelection,
  onOpenGallery,
  busy,
}: {
  stack: ProcessingOp[];
  onChange: (next: ProcessingOp[]) => void;
  /** Controles de brilho/contraste/… (o primeiro passo da pilha). */
  adjustmentsSlot: ReactNode;
  adjustmentsActive: boolean;
  onResetAdjustments: () => void;
  activeSelection: SicroImageSelection | null;
  onOpenGallery: () => void;
  busy: boolean;
}) {
  const [open, setOpen] = useState<Set<string>>(new Set());
  const [adjOpen, setAdjOpen] = useState(false);
  const [saved, setSaved] = useState<Recipe[]>(loadSaved);
  const known = useRef(new Set(stack.map((o) => o.id)));

  // Filtro recém-adicionado já aparece aberto, com os controles.
  useEffect(() => {
    const fresh = stack.filter((o) => !known.current.has(o.id)).map((o) => o.id);
    known.current = new Set(stack.map((o) => o.id));
    if (fresh.length) setOpen((s) => new Set([...s, ...fresh]));
  }, [stack]);

  const patch = (id: string, p: Partial<ProcessingOp>) => onChange(stack.map((o) => (o.id === id ? { ...o, ...p } : o)));
  const move = (i: number, d: number) => {
    const j = i + d;
    if (j < 0 || j >= stack.length) return;
    const next = [...stack];
    [next[i], next[j]] = [next[j]!, next[i]!];
    onChange(next);
  };
  const toggle = (id: string) =>
    setOpen((s) => {
      const n = new Set(s);
      if (n.has(id)) n.delete(id);
      else n.add(id);
      return n;
    });

  const applyRecipe = (r: Recipe) =>
    onChange(r.ops.map((o) => ({ ...makeProcessingOp(o.kind), ...(o.params ? { params: { ...o.params } } : {}) })));
  const saveRecipe = async () => {
    const name = await askText({
      title: "Salvar receita",
      message: "Nome da receita. Ela fica disponível em qualquer caso.",
      confirmLabel: "Salvar",
    });
    if (!name?.trim()) return;
    const r: Recipe = { name: name.trim(), ops: stack.map((o) => ({ kind: o.kind, params: o.params })) };
    const next = [...saved.filter((x) => x.name !== r.name), r];
    setSaved(next);
    storeSaved(next);
  };
  const deleteRecipe = (name: string) => {
    const next = saved.filter((x) => x.name !== name);
    setSaved(next);
    storeSaved(next);
  };

  const active = stack.filter((o) => o.enabled !== false).length;

  return (
    <div className={styles.panel}>
      <header className={styles.head}>
        <h3>Pilha de filtros</h3>
        <span className={styles.count}>{busy ? "aplicando…" : `${active} ativo${active === 1 ? "" : "s"}`}</span>
        {stack.length > 0 && (
          <button type="button" className={styles.iconBtn} onClick={() => onChange([])} title="Tirar todos os filtros">
            <Trash2 size={13} />
          </button>
        )}
      </header>

      <div className={styles.body}>
        <button type="button" className={styles.add} onClick={onOpenGallery}>
          <Plus size={15} /> Filtro
        </button>

        <div className={styles.recipes}>
          {[...BUILTIN_RECIPES, ...saved].map((r) => (
            <span key={r.name} className={styles.recipe}>
              <button type="button" onClick={() => applyRecipe(r)} title={r.ops.map((o) => LABEL.get(o.kind) ?? o.kind).join(" → ")}>
                {r.name}
              </button>
              {!r.builtin && (
                <button type="button" className={styles.recipeDel} onClick={() => deleteRecipe(r.name)} title="Apagar receita">
                  <X size={10} />
                </button>
              )}
            </span>
          ))}
          {stack.length > 0 && (
            <button type="button" className={styles.recipeSave} onClick={() => void saveRecipe()}>
              Salvar como receita
            </button>
          )}
        </div>

        <p className={styles.order}>Aplicados de cima para baixo. O original nunca muda.</p>

        <section className={`${styles.card} ${styles.adjCard}`}>
          <div className={styles.cardHead}>
            <span className={styles.idx}>0</span>
            <button type="button" className={styles.cardTitle} onClick={() => setAdjOpen((v) => !v)}>
              <ChevronDown size={13} className={adjOpen ? styles.chevOpen : styles.chev} />
              Ajustes
              <small>{adjustmentsActive ? "ativos" : "padrão"}</small>
            </button>
            {adjustmentsActive && (
              <button type="button" className={styles.iconBtn} onClick={onResetAdjustments} title="Voltar ao padrão">
                <X size={12} />
              </button>
            )}
          </div>
          {adjOpen && <div className={styles.cardBody}>{adjustmentsSlot}</div>}
        </section>

        {stack.length === 0 && (
          <p className={styles.empty}>Nenhum filtro. Use "+ Filtro" para escolher pela miniatura, ou uma receita.</p>
        )}

        {stack.map((op, i) => {
          const isOpen = open.has(op.id);
          const off = op.enabled === false;
          return (
            <section key={op.id} className={`${styles.card} ${off ? styles.cardOff : ""}`}>
              <div className={styles.cardHead}>
                <span className={styles.idx}>{i + 1}</span>
                <button type="button" className={styles.cardTitle} onClick={() => toggle(op.id)}>
                  <ChevronDown size={13} className={isOpen ? styles.chevOpen : styles.chev} />
                  <span className={styles.cardName}>{LABEL.get(op.kind) ?? op.kind}</span>
                  {op.scope === "selection" && <small className={styles.scopeChip}>seleção</small>}
                </button>
                <button
                  type="button"
                  className={styles.iconBtn}
                  onClick={() => patch(op.id, { enabled: off })}
                  title={off ? "Ligar" : "Desligar"}
                >
                  {off ? <EyeOff size={12} /> : <Eye size={12} />}
                </button>
                <button type="button" className={styles.iconBtn} onClick={() => move(i, -1)} disabled={i === 0} title="Aplicar antes">
                  <ArrowUp size={12} />
                </button>
                <button
                  type="button"
                  className={styles.iconBtn}
                  onClick={() => move(i, 1)}
                  disabled={i === stack.length - 1}
                  title="Aplicar depois"
                >
                  <ArrowDown size={12} />
                </button>
                <button type="button" className={styles.iconBtn} onClick={() => onChange(stack.filter((o) => o.id !== op.id))} title="Remover">
                  <X size={12} />
                </button>
              </div>
              {isOpen && (
                <div className={styles.cardBody}>
                  <OpControls op={op} onParam={(k, v) => patch(op.id, { params: { ...op.params, [k]: v } })} />
                  {isMaskableOpKind(op.kind) && (
                    <div className={styles.scope}>
                      <button
                        type="button"
                        className={op.scope !== "selection" ? styles.scopeOn : undefined}
                        onClick={() => patch(op.id, { scope: "image", mask: null })}
                      >
                        Imagem inteira
                      </button>
                      <button
                        type="button"
                        className={op.scope === "selection" ? styles.scopeOn : undefined}
                        disabled={op.scope !== "selection" && !activeSelection}
                        title={!activeSelection && op.scope !== "selection" ? "Faça uma seleção na imagem primeiro" : undefined}
                        onClick={() => activeSelection && patch(op.id, { scope: "selection", mask: activeSelection })}
                      >
                        Só a seleção
                      </button>
                    </div>
                  )}
                </div>
              )}
            </section>
          );
        })}
      </div>
    </div>
  );
}

// ---------------------------------------------------------------------------
// Galeria com miniaturas

const thumbCache = new Map<string, string[]>();

/** Miniaturas (~320 px) da imagem com a pilha atual + cada filtro do catálogo; cache por pilha. */
export function useFilterThumbnails(opts: {
  open: boolean;
  image: HTMLImageElement | null;
  stack: ProcessingOp[];
  adjustments: BackendAdjustments;
  sourceWidth: number;
  sourceHeight: number;
  cacheKey: string;
}) {
  const { open, image, stack, adjustments, sourceWidth: sw, sourceHeight: sh, cacheKey } = opts;
  const enabled = stack.filter((o) => o.enabled !== false);
  const key = `${cacheKey}|${JSON.stringify(enabled.map((o) => [o.kind, o.params, o.scope ?? null, o.mask ?? null]))}|${JSON.stringify(adjustments)}`;
  const [thumbs, setThumbs] = useState<string[] | null>(() => thumbCache.get(key) ?? null);

  useEffect(() => {
    if (!open || !image || !sw || !sh) return;
    const hit = thumbCache.get(key);
    if (hit) {
      setThumbs(hit);
      return;
    }
    setThumbs(null);
    let cancelled = false;
    const k = Math.min(1, 320 / Math.max(sw, sh));
    const tw = Math.max(1, Math.round(sw * k));
    const th = Math.max(1, Math.round(sh * k));
    const c = document.createElement("canvas");
    c.width = tw;
    c.height = th;
    const ctx = c.getContext("2d");
    if (!ctx) return;
    ctx.imageSmoothingQuality = "high";
    ctx.drawImage(image, 0, 0, tw, th);
    const b64 = c.toDataURL("image/png").replace(/^data:image\/png;base64,/, "");
    const ops = enabled.map((o) => {
      const be = processingOpToBackendOperation(o, sw, sh);
      if (o.kind === "crop") {
        for (const f of ["x", "y", "width", "height"]) be[f] = Math.max(1, Math.round(Number(be[f]) * k));
      }
      return be as unknown as BackendOperation;
    });
    const candidates = FILTER_INDEX.map(
      (f) => processingOpToBackendOperation(makeProcessingOp(f.kind), tw, th) as unknown as BackendOperation,
    );
    commands
      .filterThumbnails({ image_base64: b64, operations: ops, adjustments, candidates })
      .then((list) => {
        if (cancelled) return;
        const urls = list.map((s) => (s ? `data:image/jpeg;base64,${s}` : ""));
        thumbCache.set(key, urls);
        setThumbs(urls);
      })
      .catch(() => !cancelled && setThumbs([]));
    return () => {
      cancelled = true;
    };
    // `key` resume pilha, ajustes e imagem.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [open, image, key]);

  return thumbs;
}

export function FilterGalleryOverlay({
  thumbs,
  onAdd,
  onClose,
  selectionActive,
}: {
  thumbs: string[] | null;
  onAdd: (kind: ProcessingOpKind) => void;
  onClose: () => void;
  selectionActive: boolean;
}) {
  const [q, setQ] = useState("");
  const thumbOf = useMemo(() => new Map(FILTER_INDEX.map((f, i) => [f.kind, thumbs?.[i] ?? null] as const)), [thumbs]);
  const groups = useMemo(() => {
    const f = q
      .normalize("NFD")
      .replace(/\p{Diacritic}/gu, "")
      .toLowerCase()
      .trim();
    const map = new Map<string, typeof FILTER_INDEX>();
    for (const it of FILTER_INDEX) {
      const hay = `${it.label} ${it.group} ${it.note ?? ""} ${it.keywords ?? ""}`
        .normalize("NFD")
        .replace(/\p{Diacritic}/gu, "")
        .toLowerCase();
      if (f && !hay.includes(f)) continue;
      map.set(it.group, [...(map.get(it.group) ?? []), it]);
    }
    return [...map.entries()];
  }, [q]);

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => e.key === "Escape" && onClose();
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [onClose]);

  return (
    <div className={styles.gallery} role="dialog" aria-label="Adicionar filtro">
      <header className={styles.gHead}>
        <h3>Adicionar filtro</h3>
        <span className={styles.gNote}>
          Miniaturas: a sua imagem com a pilha atual + o filtro{selectionActive ? " · entra só na seleção" : ""}
        </span>
        <label className={styles.gSearch}>
          <Search size={13} />
          <input autoFocus placeholder="Buscar filtro" value={q} onChange={(e) => setQ(e.target.value)} />
        </label>
        <button type="button" className={styles.iconBtn} onClick={onClose} title="Fechar (Esc)">
          <X size={14} />
        </button>
      </header>
      <div className={styles.gBody}>
        {groups.map(([g, items]) => (
          <section key={g}>
            <h4>{g}</h4>
            <div className={styles.gGrid}>
              {items.map((it) => {
                const t = thumbOf.get(it.kind);
                return (
                  <button key={it.kind} type="button" className={styles.tile} onClick={() => onAdd(it.kind)} title={it.note ?? it.label}>
                    {t ? (
                      <img src={t} alt="" draggable={false} />
                    ) : (
                      <span className={`${styles.thumbPh} ${thumbs === null ? styles.thumbLoading : ""}`} />
                    )}
                    <b>{it.label}</b>
                    {it.note && <small>{it.note}</small>}
                  </button>
                );
              })}
            </div>
          </section>
        ))}
        {groups.length === 0 && <p className={styles.empty}>Nenhum filtro com "{q}".</p>}
      </div>
    </div>
  );
}

// ---------------------------------------------------------------------------
// Trilho direito

const RAIL: { key: SidePanel; label: string; Icon: typeof Layers }[] = [
  { key: "filtros", label: "Filtros", Icon: SlidersHorizontal },
  { key: "camadas", label: "Camadas", Icon: Layers },
  { key: "analise", label: "Análise", Icon: BarChart3 },
  { key: "historico", label: "Histórico", Icon: History },
];

export function SideRail({
  value,
  onChange,
  filterCount,
}: {
  value: SidePanel;
  onChange: (p: SidePanel) => void;
  filterCount: number;
}) {
  return (
    <nav className={styles.rail} aria-label="Painéis">
      {RAIL.map(({ key, label, Icon }) => (
        <button
          key={key}
          type="button"
          className={value === key ? styles.railOn : undefined}
          onClick={() => onChange(key)}
          aria-pressed={value === key}
        >
          <Icon size={17} />
          <span>{label}</span>
          {key === "filtros" && filterCount > 0 && <i className={styles.badge}>{filterCount}</i>}
        </button>
      ))}
    </nav>
  );
}

/** Painel simples de altura inteira para Camadas, Análise e Histórico. */
export function SidePanelFrame({ title, children }: { title: string; children: ReactNode }) {
  return (
    <div className={styles.panel}>
      <header className={styles.head}>
        <h3>{title}</h3>
      </header>
      <div className={styles.body}>{children}</div>
    </div>
  );
}

export function SideSection({ title, children }: { title: string; children: ReactNode }) {
  return (
    <section className={styles.section}>
      <h4>{title}</h4>
      {children}
    </section>
  );
}
