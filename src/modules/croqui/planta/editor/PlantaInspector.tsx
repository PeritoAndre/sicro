/** Inspetor da planta: uma coisa por vez (folha e opções sem seleção). */
import { useEffect, useState, type ReactNode } from "react";
import { fmtM, parseM } from "../model/geom";
import { setWallInner, wallGeometry } from "../model/walls";
import { builtArea, roomGeometry } from "../model/rooms";
import { clampT0, openingGaps } from "../model/openings";
import { evidenceMeasures, measureText, relabelAll } from "../model/evidence";
import { SYMBOL_BY_ID } from "../model/symbols";
import {
  EVIDENCE_TIPOS,
  OPENING_DEFAULTS,
  SCALES,
  sheetSize,
  WALL_THICKNESSES,
  type EvidenceTipo,
  type OpeningKind,
  type SicroPlantaDoc,
  type WallKind,
} from "../model/schema";
import { deleteSelection, fitSheetToContent } from "./actions";
import { usePlanta } from "./store";
import insp from "../../editor/InspectorPanel.module.css";
import styles from "./planta.module.css";

type Doc = SicroPlantaDoc;

function NumField({ label, value, onCommit, unit = "m", digits = 2, min }: { label: string; value: number; onCommit: (v: number) => void; unit?: string; digits?: number; min?: number }) {
  const [text, setText] = useState(fmtM(value, digits));
  useEffect(() => setText(fmtM(value, digits)), [value, digits]);
  const commit = () => {
    const v = parseM(text);
    if (v == null || (min != null && v < min)) setText(fmtM(value, digits));
    else if (Math.abs(v - value) > 1e-9) onCommit(v);
  };
  return (
    <label className={insp.field}>
      <span className={insp.fieldLabel}>
        {label} {unit && <span className={insp.dim}>({unit})</span>}
      </span>
      <input
        className={`${insp.fieldInput} ${insp.mono}`}
        value={text}
        inputMode="decimal"
        onChange={(e) => setText(e.target.value)}
        onBlur={commit}
        onKeyDown={(e) => {
          if (e.key === "Enter") (e.target as HTMLInputElement).blur();
          e.stopPropagation();
        }}
      />
    </label>
  );
}

function TextField({ label, value, onCommit, area = false, placeholder }: { label: string; value: string; onCommit: (v: string) => void; area?: boolean; placeholder?: string }) {
  const [text, setText] = useState(value);
  useEffect(() => setText(value), [value]);
  const props = {
    className: insp.fieldInput,
    value: text,
    placeholder,
    onChange: (e: { target: { value: string } }) => setText(e.target.value),
    onBlur: () => text !== value && onCommit(text),
    onKeyDown: (e: React.KeyboardEvent) => {
      if (e.key === "Enter" && !area) (e.target as HTMLInputElement).blur();
      e.stopPropagation();
    },
  };
  return (
    <label className={insp.field}>
      <span className={insp.fieldLabel}>{label}</span>
      {area ? <textarea {...props} rows={3} style={{ height: "auto", padding: 6, resize: "vertical" }} /> : <input {...props} />}
    </label>
  );
}

function Chips<T extends string | number>({ value, options, onPick }: { value: T; options: { v: T; label: string }[]; onPick: (v: T) => void }) {
  return (
    <div className={insp.temaRow}>
      {options.map((o) => (
        <button key={String(o.v)} type="button" className={`${insp.temaBtn} ${o.v === value ? insp.temaBtnActive : ""}`} onClick={() => onPick(o.v)}>
          {o.label}
        </button>
      ))}
    </div>
  );
}

function Section({ title, children }: { title: string; children: ReactNode }) {
  return (
    <section className={insp.section}>
      <h3 className={insp.sectionTitle}>{title}</h3>
      <div className={insp.props}>{children}</div>
    </section>
  );
}

function Check({ label, checked, onChange }: { label: string; checked: boolean; onChange: (v: boolean) => void }) {
  return (
    <label className={insp.checkboxRow}>
      <input type="checkbox" checked={checked} onChange={(e) => onChange(e.target.checked)} /> {label}
    </label>
  );
}

const thicknessOpts = WALL_THICKNESSES.map((t) => ({ v: t as number, label: `${Math.round(t * 100)} cm` }));

export function PlantaInspector({ newWall, onNewWall }: { newWall: { thickness: number; kind: WallKind }; onNewWall: (v: { thickness: number; kind: WallKind }) => void }) {
  const doc = usePlanta((s) => s.doc)!;
  const sel = usePlanta((s) => s.sel);
  const tool = usePlanta((s) => s.tool);
  const apply = usePlanta((s) => s.apply);
  const setSel = usePlanta((s) => s.setSel);
  const del = () => {
    apply((d) => deleteSelection(d, sel));
    setSel([]);
  };
  const delBtn = (label: string) => (
    <div className={styles.btnRow}>
      <button type="button" className={`${styles.btn} ${styles.btnDanger}`} onClick={del}>
        {label}
      </button>
    </div>
  );
  const upd = <K extends keyof Doc>(key: K, id: string, patch: Record<string, unknown>) =>
    apply((d) => ({ ...d, [key]: (d[key] as unknown as { id: string }[]).map((x) => (x.id === id ? { ...x, ...patch } : x)) }));

  let body: ReactNode = null;
  if (sel.length > 1) {
    body = (
      <Section title={`${sel.length} itens selecionados`}>
        <p className={styles.hint}>Arraste qualquer um deles para mover todos juntos.</p>
        {delBtn("Apagar seleção")}
      </Section>
    );
  } else if (sel.length === 1) {
    const s = sel[0]!;
    if (s.kind === "wall") {
      const w = doc.walls.find((x) => x.id === s.id);
      const g = w ? wallGeometry(doc).get(w.id) : undefined;
      if (w && g)
        body = (
          <Section title={w.kind === "muro" ? "Muro" : w.kind === "grade" ? "Grade ou cerca" : "Parede"}>
            <NumField label="Medida interna" value={g.inner} min={0.05} onCommit={(v) => apply((d) => setWallInner(d, w.id, v))} />
            <span className={insp.fieldLabel}>Espessura</span>
            <Chips value={w.thickness} options={thicknessOpts} onPick={(v) => upd("walls", w.id, { thickness: v })} />
            <NumField label="Espessura livre" value={w.thickness * 100} unit="cm" digits={0} min={2} onCommit={(v) => upd("walls", w.id, { thickness: v / 100 })} />
            <span className={insp.fieldLabel}>Tipo</span>
            <Chips value={w.kind} options={[{ v: "parede", label: "Parede" }, { v: "muro", label: "Muro" }, { v: "grade", label: "Grade" }]} onPick={(v) => upd("walls", w.id, { kind: v })} />
            <NumField label="Altura" value={w.height} min={0.1} onCommit={(v) => upd("walls", w.id, { height: v })} />
            <p className={styles.hint}>A medida é a da face de dentro, a que se tira com trena. A altura fica guardada para um 3D no futuro.</p>
            {delBtn("Apagar parede")}
          </Section>
        );
    } else if (s.kind === "opening") {
      const o = doc.openings.find((x) => x.id === s.id);
      const g = o ? wallGeometry(doc).get(o.wall) : undefined;
      if (o && g) {
        const gap = openingGaps(g, o);
        const isWin = o.kind === "janela" || o.kind === "basculante";
        const isDoor = o.kind === "porta" || o.kind === "porta_dupla";
        body = (
          <Section title={OPENING_DEFAULTS[o.kind].label}>
            <label className={insp.field}>
              <span className={insp.fieldLabel}>Tipo</span>
              <select className={insp.fieldInput} value={o.kind} onChange={(e) => upd("openings", o.id, { kind: e.target.value as OpeningKind, sill: OPENING_DEFAULTS[e.target.value as OpeningKind].sill, height: OPENING_DEFAULTS[e.target.value as OpeningKind].height })}>
                {(Object.keys(OPENING_DEFAULTS) as OpeningKind[]).map((k) => (
                  <option key={k} value={k}>
                    {OPENING_DEFAULTS[k].label}
                  </option>
                ))}
              </select>
            </label>
            <NumField label="Largura" value={o.width} min={0.2} onCommit={(v) => upd("openings", o.id, { width: v, t0: clampT0(g, o.t0, v) })} />
            <div className={styles.row2}>
              <NumField label="Até o canto" value={gap.before} min={0} onCommit={(v) => upd("openings", o.id, { t0: clampT0(g, g.marginA + v, o.width) })} />
              <NumField label="Até o outro" value={gap.after} min={0} onCommit={(v) => upd("openings", o.id, { t0: clampT0(g, g.frame.L - g.marginB - v - o.width, o.width) })} />
            </div>
            {(isDoor || o.kind === "correr") && (
              <div className={styles.btnRow}>
                <button type="button" className={styles.btn} onClick={() => upd("openings", o.id, { side: o.side === 1 ? -1 : 1 })}>
                  Inverter lado
                </button>
                {o.kind === "porta" && (
                  <button type="button" className={styles.btn} onClick={() => upd("openings", o.id, { hinge: o.hinge === "start" ? "end" : "start" })}>
                    Trocar dobradiça
                  </button>
                )}
              </div>
            )}
            <div className={styles.row2}>
              {isWin && <NumField label="Peitoril" value={o.sill} min={0} onCommit={(v) => upd("openings", o.id, { sill: v })} />}
              <NumField label="Altura" value={o.height} min={0.1} onCommit={(v) => upd("openings", o.id, { height: v })} />
            </div>
            <p className={styles.hint}>Espaço inverte o lado. Arraste para correr pela parede ou levar a outra.</p>
            {delBtn("Apagar")}
          </Section>
        );
      }
    } else if (s.kind === "room") {
      const rg = roomGeometry(doc).find((r) => r.room.id === s.id);
      const r = doc.rooms.find((x) => x.id === s.id);
      if (r)
        body = (
          <Section title="Cômodo">
            <TextField label="Nome" value={r.name} onCommit={(v) => v.trim() && upd("rooms", r.id, { name: v.trim() })} />
            <dl className={styles.summary}>
              <dt>Área útil</dt>
              <dd>{rg ? `${fmtM(rg.area)} m²` : "sem contorno fechado"}</dd>
            </dl>
            <Check label="Mostrar a área na planta" checked={r.show_area} onChange={(v) => upd("rooms", r.id, { show_area: v })} />
            {r.label && (
              <div className={styles.btnRow}>
                <button type="button" className={styles.btn} onClick={() => upd("rooms", r.id, { label: null })}>
                  Nome no centro
                </button>
              </div>
            )}
            <p className={styles.hint}>Arraste o nome para mudar de lugar. Apagar o cômodo tira só o nome; as paredes ficam.</p>
            {delBtn("Apagar cômodo")}
          </Section>
        );
    } else if (s.kind === "item") {
      const it = doc.items.find((x) => x.id === s.id);
      if (it)
        body = (
          <Section title={SYMBOL_BY_ID.get(it.symbol)?.label ?? "Peça"}>
            <div className={styles.row2}>
              <NumField label="Largura" value={it.w} min={0.05} onCommit={(v) => upd("items", it.id, { w: v })} />
              <NumField label="Profundidade" value={it.d} min={0.05} onCommit={(v) => upd("items", it.id, { d: v })} />
            </div>
            <NumField label="Rotação" value={it.rot} unit="graus" digits={0} onCommit={(v) => upd("items", it.id, { rot: ((v % 360) + 360) % 360 })} />
            <TextField label="Rótulo" value={it.label ?? ""} placeholder="opcional" onCommit={(v) => upd("items", it.id, { label: v.trim() || null })} />
            <p className={styles.hint}>R gira 90 graus. Perto da parede a peça encosta e gira junto; Alt solta.</p>
            {delBtn("Apagar")}
          </Section>
        );
    } else if (s.kind === "evidence") {
      const e = doc.evidences.find((x) => x.id === s.id);
      if (e)
        body = (
          <Section title={`Vestígio ${e.label}`}>
            <TextField label="Rótulo" value={e.label} onCommit={(v) => v.trim() && upd("evidences", e.id, { label: v.trim() })} />
            <label className={insp.field}>
              <span className={insp.fieldLabel}>Tipo</span>
              <select className={insp.fieldInput} value={e.tipo} onChange={(ev) => upd("evidences", e.id, { tipo: ev.target.value as EvidenceTipo })}>
                {(Object.keys(EVIDENCE_TIPOS) as EvidenceTipo[]).map((k) => (
                  <option key={k} value={k}>
                    {EVIDENCE_TIPOS[k]}
                  </option>
                ))}
              </select>
            </label>
            <TextField label="Descrição" value={e.descricao} area placeholder="Ex.: calibre .40, junto ao sofá" onCommit={(v) => upd("evidences", e.id, { descricao: v })} />
            <span className={insp.fieldLabel}>Medir por</span>
            <Chips value={e.measure} options={[{ v: "paredes", label: "Duas paredes" }, { v: "pontos", label: "Dois cantos" }]} onPick={(v) => upd("evidences", e.id, { measure: v })} />
            <p className={styles.hint}>{measureText(evidenceMeasures(doc, e)) || "Sem paredes por perto para medir."}</p>
            <Check label="Mostrar as linhas de medida na planta" checked={e.show_measure} onChange={(v) => upd("evidences", e.id, { show_measure: v })} />
            {delBtn("Apagar vestígio")}
          </Section>
        );
    } else if (s.kind === "person") {
      const p = doc.people.find((x) => x.id === s.id);
      if (p)
        body = (
          <Section title="Pessoa">
            <Chips value={p.pose} options={[{ v: "em_pe", label: "Em pé" }, { v: "caido", label: "Caída" }]} onPick={(v) => upd("people", p.id, { pose: v })} />
            <TextField label="Rótulo" value={p.label} onCommit={(v) => upd("people", p.id, { label: v })} />
            <TextField label="Descrição" value={p.descricao} area placeholder="Ex.: decúbito dorsal, cabeça a oeste" onCommit={(v) => upd("people", p.id, { descricao: v })} />
            <NumField label="Rotação" value={p.rot} unit="graus" digits={0} onCommit={(v) => upd("people", p.id, { rot: ((v % 360) + 360) % 360 })} />
            <p className={styles.hint}>A bolinha dourada gira. R gira 90 graus.</p>
            {delBtn("Apagar")}
          </Section>
        );
    } else if (s.kind === "traj") {
      const t = doc.trajectories.find((x) => x.id === s.id);
      if (t)
        body = (
          <Section title="Trajetória">
            <TextField label="Rótulo" value={t.label} onCommit={(v) => upd("trajectories", t.id, { label: v })} />
            <TextField label="Descrição" value={t.descricao} area onCommit={(v) => upd("trajectories", t.id, { descricao: v })} />
            <label className={insp.field}>
              <span className={insp.fieldLabel}>Cor</span>
              <input className={insp.fieldInput} type="color" value={t.color} onChange={(e) => upd("trajectories", t.id, { color: e.target.value })} />
            </label>
            <dl className={styles.summary}>
              <dt>Comprimento</dt>
              <dd>{fmtM(Math.hypot(t.b.x - t.a.x, t.b.y - t.a.y))} m</dd>
            </dl>
            {delBtn("Apagar")}
          </Section>
        );
    } else if (s.kind === "text") {
      const t = doc.texts.find((x) => x.id === s.id);
      if (t)
        body = (
          <Section title="Texto">
            <TextField label="Texto" value={t.text} area onCommit={(v) => v.trim() && upd("texts", t.id, { text: v })} />
            <div className={styles.row2}>
              <NumField label="Tamanho" value={t.size} min={0.05} onCommit={(v) => upd("texts", t.id, { size: v })} />
              <NumField label="Rotação" value={t.rot} unit="graus" digits={0} onCommit={(v) => upd("texts", t.id, { rot: v })} />
            </div>
            <label className={insp.field}>
              <span className={insp.fieldLabel}>Cor</span>
              <input className={insp.fieldInput} type="color" value={t.color} onChange={(e) => upd("texts", t.id, { color: e.target.value })} />
            </label>
            <Check label="Negrito" checked={t.bold} onChange={(v) => upd("texts", t.id, { bold: v })} />
            {delBtn("Apagar")}
          </Section>
        );
    } else if (s.kind === "dim") {
      const dm = doc.dims.find((x) => x.id === s.id);
      if (dm)
        body = (
          <Section title="Cota">
            <dl className={styles.summary}>
              <dt>Medida</dt>
              <dd>{fmtM(Math.hypot(dm.b.x - dm.a.x, dm.b.y - dm.a.y))} m</dd>
            </dl>
            <NumField label="Afastamento" value={dm.offset} onCommit={(v) => upd("dims", dm.id, { offset: v })} />
            <p className={styles.hint}>Arraste as pontas para medir outra coisa e o quadradinho para afastar a linha.</p>
            {delBtn("Apagar")}
          </Section>
        );
    } else if (s.kind === "compass") {
      body = (
        <Section title="Rosa dos ventos">
          <NumField label="Norte" value={doc.options.compass.deg} unit="graus" digits={0} onCommit={(v) => apply((d) => ({ ...d, options: { ...d.options, compass: { ...d.options.compass, deg: ((v % 360) + 360) % 360 } } }))} />
          <p className={styles.hint}>Os nomes das paredes na legenda (norte, sul, leste, oeste) seguem esta direção.</p>
          {delBtn("Esconder")}
        </Section>
      );
    }
  }
  if (!body) body = <SheetPanel doc={doc} tool={tool} newWall={newWall} onNewWall={onNewWall} />;
  return <aside className={insp.panel}>{body}</aside>;
}

function SheetPanel({ doc, tool, newWall, onNewWall }: { doc: Doc; tool: string; newWall: { thickness: number; kind: WallKind }; onNewWall: (v: { thickness: number; kind: WallKind }) => void }) {
  const apply = usePlanta((s) => s.apply);
  const s = doc.sheet;
  const setSheet = (patch: Partial<Doc["sheet"]>) =>
    apply((d) => {
      const next = { ...d.sheet, ...patch };
      if (next.paper !== "custom") Object.assign(next, sheetSize(next.paper, next.orientation, next.scale));
      return { ...d, sheet: next };
    });
  const opt = (patch: Partial<Doc["options"]>) => apply((d) => ({ ...d, options: { ...d.options, ...patch } }));
  const rooms = roomGeometry(doc);
  const usable = rooms.reduce((a, r) => a + r.area, 0);
  const built = builtArea(doc);
  return (
    <>
      {(tool === "wall" || tool === "room") && (
        <Section title="Paredes novas">
          <span className={insp.fieldLabel}>Espessura</span>
          <Chips value={newWall.thickness} options={thicknessOpts} onPick={(v) => onNewWall({ ...newWall, thickness: v })} />
          <span className={insp.fieldLabel}>Tipo</span>
          <Chips value={newWall.kind} options={[{ v: "parede", label: "Parede" }, { v: "muro", label: "Muro" }, { v: "grade", label: "Grade" }]} onPick={(v) => onNewWall({ ...newWall, kind: v })} />
          <p className={styles.hint}>
            {tool === "wall"
              ? "Clique ponto a ponto. Digite a medida interna e Enter para a parede sair no tamanho certo."
              : "Arraste o retângulo. A medida que aparece já é a interna, como se mede no local."}
          </p>
        </Section>
      )}
      <Section title="Folha e grade">
        <Chips value={s.paper} options={[{ v: "A4", label: "A4" }, { v: "A3", label: "A3" }, { v: "custom", label: "Livre" }]} onPick={(v) => setSheet({ paper: v })} />
        {s.paper !== "custom" ? (
          <>
            <Chips value={s.orientation} options={[{ v: "paisagem", label: "Deitada" }, { v: "retrato", label: "Em pé" }]} onPick={(v) => setSheet({ orientation: v })} />
            <label className={insp.field}>
              <span className={insp.fieldLabel}>Escala</span>
              <select className={insp.fieldInput} value={s.scale} onChange={(e) => setSheet({ scale: Number(e.target.value) })}>
                {SCALES.map((k) => {
                  const z = sheetSize(s.paper as "A4" | "A3", s.orientation, k);
                  return (
                    <option key={k} value={k}>
                      1:{k} · {fmtM(z.w, 1)} × {fmtM(z.h, 1)} m
                    </option>
                  );
                })}
              </select>
            </label>
          </>
        ) : (
          <div className={styles.row2}>
            <NumField label="Largura" value={s.w} min={2} onCommit={(v) => setSheet({ w: v })} />
            <NumField label="Altura" value={s.h} min={2} onCommit={(v) => setSheet({ h: v })} />
          </div>
        )}
        <span className={insp.fieldLabel}>Grade</span>
        <Chips value={s.grid} options={[{ v: 0.5, label: "0,5 m" }, { v: 1, label: "1 m" }]} onPick={(v) => setSheet({ grid: v })} />
        <Check label="Mostrar grade" checked={s.show_grid} onChange={(v) => setSheet({ show_grid: v })} />
        <div className={styles.btnRow}>
          <button type="button" className={styles.btn} onClick={() => apply(fitSheetToContent)}>
            Folha em volta da planta
          </button>
        </div>
      </Section>
      <Section title="No PNG técnico">
        <Check label="Cotas externas automáticas" checked={doc.options.auto_dims} onChange={(v) => opt({ auto_dims: v })} />
        <Check label="Medida interna em cada parede" checked={doc.options.wall_measures} onChange={(v) => opt({ wall_measures: v })} />
        <Check label="Nome e área dos cômodos" checked={doc.options.room_areas} onChange={(v) => opt({ room_areas: v })} />
        <span className={insp.fieldLabel}>Resolução</span>
        <Chips value={s.png_width} options={[{ v: 2480, label: "Normal" }, { v: 3508, label: "Alta" }, { v: 4961, label: "Máxima" }]} onPick={(v) => setSheet({ png_width: v })} />
      </Section>
      <Section title="Vestígios e norte">
        <span className={insp.fieldLabel}>Rótulos dos vestígios</span>
        <Chips value={doc.options.label_kind} options={[{ v: "letra", label: "A, B, C" }, { v: "numero", label: "1, 2, 3" }]} onPick={(v) => apply((d) => relabelAll(d, v))} />
        <Check label="Rosa dos ventos" checked={doc.options.compass.show} onChange={(v) => opt({ compass: { ...doc.options.compass, show: v } })} />
        <NumField label="Norte" value={doc.options.compass.deg} unit="graus" digits={0} onCommit={(v) => opt({ compass: { ...doc.options.compass, deg: ((v % 360) + 360) % 360 } })} />
      </Section>
      <Section title="Resumo">
        <dl className={styles.summary}>
          <dt>Área útil</dt>
          <dd>{fmtM(usable)} m²</dd>
          <dt>Área construída</dt>
          <dd>{fmtM(built)} m²</dd>
          <dt>Cômodos</dt>
          <dd>{rooms.length}</dd>
          <dt>Vestígios</dt>
          <dd>{doc.evidences.length}</dd>
        </dl>
      </Section>
    </>
  );
}
