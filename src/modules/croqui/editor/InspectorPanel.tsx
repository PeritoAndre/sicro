/**
 * Coluna direita: camadas, objetos agrupados por categoria e propriedades
 * do objeto selecionado.
 */

import { useMemo, useState } from "react";
import {
  ArrowDown,
  ArrowUp,
  Eye,
  EyeOff,
  Lock,
  Pencil,
  Trash2,
  Unlock,
} from "lucide-react";
import {
  angleDeg,
  distancePx,
  formatMeasurement,
  inferCategory,
  MARKER_STYLES,
  TRACE_SPECS,
  FIXTURE_SPECS,
  PERSON_POSES,
  PERSON_POSICOES,
  personApplyPose,
  personChangePosicao,
  personReadouts,
  type SicroPersonObject,
  fixtureReadouts,
  traceReadouts,
  type SicroFixtureObject,
  type SicroTraceObject,
  type TraceParamSpec,
  type ObjectCategory,
  type SicroCroquiLayer,
  type SicroCroquiScale,
  type SicroLineObject,
  type SicroMarkerObject,
  type SicroMeasurementObject,
  type SicroObject,
  type SicroTextObject,
  type SicroVehicleObject,
  type VehicleBodyType,
} from "../engine";
import {
  PARITY_ROAD_LARGURA_MAX_M,
  PARITY_ROAD_LARGURA_MIN_M,
  PARITY_ROUNDABOUT_LARGURA_MAX_M_FALLBACK,
  PARITY_ROUNDABOUT_LARGURA_MIN_M,
  PARITY_ROUNDABOUT_R_MAX_M,
  PARITY_ROUNDABOUT_R_MIN_M,
  type ParityMarcacao,
  type ParitySuperficie,
  type SicroRoadObject_parity,
  type SicroRoundaboutObject_parity,
  PARITY_ACOSTAMENTO_MAX_M,
  PARITY_TEMAS,
  marcacaoFromEixo,
  resolveParityEixo,
  resolveParityStyle,
  type ParityCalcadaEstilo,
  type ParityEixo,
  type ParityStyle,
  type ParityTema,
  type ParityVeiculosEstilo,
  type ParityVestigiosEstilo,
} from "../engine/road-parity";
import { CROQUI_EXPORT_WIDTH_PX_DEFAULT, type SicroCroquiCanvas, type SicroCroquiExportSettings, type SicroCroquiStyle } from "../engine/schema";
import { labelDefaults, type LabelFields } from "./labels";
import styles from "./InspectorPanel.module.css";

interface Props {
  layers: SicroCroquiLayer[];
  objects: SicroObject[];
  selectedId: string | null;
  scale: SicroCroquiScale | null;
  onSelectObject: (id: string | null) => void;
  onToggleLayerVisibility: (layerId: string) => void;
  onUpdateObject: (id: string, patch: Partial<SicroObject>) => void;
  onDeleteObject: (id: string) => void;
  onMoveObject: (id: string, direction: "up" | "down") => void;
  /** `doc.style` (parcial) e o patch que o editor aplica nele. */
  style?: SicroCroquiStyle;
  onUpdateStyle?: (patch: Partial<ParityStyle>) => void;
  /** Folha, grade e PNG. */
  canvas?: SicroCroquiCanvas;
  exportSettings?: SicroCroquiExportSettings;
  onUpdateCanvas?: (patch: Partial<SicroCroquiCanvas>) => void;
  onUpdateExportSettings?: (patch: Partial<SicroCroquiExportSettings>) => void;
  onFitSheet?: () => void;
  onCenterSheet?: () => void;
}

const CATEGORY_ORDER: ObjectCategory[] = [
  "vias",
  "veiculos",
  "referenciais",
  "vestigios",
  "pessoas",
  "sinalizacao",
  "entorno",
  "mobiliario_urbano",
  "medidas",
  "anotacoes",
  "outros",
];

const CATEGORY_LABEL: Record<ObjectCategory, string> = {
  vias: "Vias",
  veiculos: "Veículos",
  referenciais: "Referenciais (R1/R2)",
  vestigios: "Vestígios e pessoas",
  pessoas: "Pessoas",
  sinalizacao: "Sinalização",
  entorno: "Entorno",
  mobiliario_urbano: "Sinalização e entorno (antigos)",
  medidas: "Medidas",
  anotacoes: "Anotações",
  outros: "Outros",
};

export function InspectorPanel({
  layers,
  objects,
  selectedId,
  scale,
  onSelectObject,
  onToggleLayerVisibility,
  onUpdateObject,
  onDeleteObject,
  onMoveObject,
  style,
  onUpdateStyle,
  canvas,
  exportSettings,
  onUpdateCanvas,
  onUpdateExportSettings,
  onFitSheet,
  onCenterSheet,
}: Props) {
  const selected = selectedId
    ? objects.find((o) => o.id === selectedId) ?? null
    : null;

  const grouped = useMemo(() => {
    const map = new Map<ObjectCategory, SicroObject[]>();
    for (const o of objects) {
      const cat = o.category ?? inferCategory(o);
      const list = map.get(cat) ?? [];
      list.push(o);
      map.set(cat, list);
    }
    return CATEGORY_ORDER.filter((c) => (map.get(c) ?? []).length > 0).map(
      (c) => ({ category: c, items: map.get(c) ?? [] }),
    );
  }, [objects]);

  const objectsSection = (
    <section className={styles.section}>
      <h3 className={styles.sectionTitle}>Objetos</h3>
      {grouped.length === 0 && (
        <p className={styles.empty}>
          O canvas está vazio. Use a barra à esquerda para inserir um objeto.
        </p>
      )}
      {grouped.map(({ category, items }) => (
        <CategoryBlock
          key={category}
          category={category}
          items={items}
          selectedId={selectedId}
          onSelectObject={onSelectObject}
          onUpdateObject={onUpdateObject}
          onDeleteObject={onDeleteObject}
          onMoveObject={onMoveObject}
        />
      ))}
    </section>
  );

  // Uma coisa por vez: com seleção, só o objeto; sem seleção, o croqui.
  if (selected) {
    return (
      <aside className={styles.panel} aria-label="Painel de camadas e propriedades">
        <section className={styles.section}>
          <h3 className={styles.sectionTitle}>Propriedades · {summariseObject(selected)}</h3>
          <ObjectProperties
            object={selected}
            scale={scale}
            style={style}
            onChange={(patch) => onUpdateObject(selected.id, patch)}
          />
        </section>
        {objectsSection}
      </aside>
    );
  }

  return (
    <aside className={styles.panel} aria-label="Painel de camadas e propriedades">
      {objectsSection}

      {canvas && onUpdateCanvas && (
        <section className={styles.section}>
          <h3 className={styles.sectionTitle}>Folha e grade</h3>
          <SheetProps
            canvas={canvas}
            scale={scale}
            exportSettings={exportSettings}
            onChange={onUpdateCanvas}
            onChangeExport={onUpdateExportSettings}
            onFit={onFitSheet}
            onCenter={onCenterSheet}
          />
        </section>
      )}

      {onUpdateStyle && (
        <section className={styles.section}>
          <h3 className={styles.sectionTitle}>Estilo das vias</h3>
          <StyleProps style={style} onChange={onUpdateStyle} />
        </section>
      )}

      <section className={styles.section}>
        <h3 className={styles.sectionTitle}>Escala</h3>
        {scale ? (
          <dl className={styles.metaGrid}>
            <dt>px / m</dt>
            <dd className={styles.mono}>{scale.px_per_m.toFixed(2)}</dd>
            {scale.definition && (
              <>
                <dt>Calibração</dt>
                <dd className={styles.mono}>
                  {scale.definition.real_distance_m.toFixed(2)} m
                </dd>
              </>
            )}
          </dl>
        ) : (
          <p className={styles.empty}>
            Use a ferramenta <strong>Definir escala</strong> e clique em
            dois pontos para calibrar. Sem escala, as medidas aparecem
            em pixels.
          </p>
        )}
      </section>

      <section className={styles.section}>
        <h3 className={styles.sectionTitle}>Camadas globais</h3>
        <ul className={styles.layerList}>
          {layers.map((l) => (
            <li key={l.id} className={styles.layer}>
              <button
                type="button"
                className={styles.layerToggleBtn}
                onClick={() => onToggleLayerVisibility(l.id)}
                title={l.visible ? "Esconder camada" : "Mostrar camada"}
              >
                {l.visible ? <Eye size={12} /> : <EyeOff size={12} />}
              </button>
              <span className={styles.layerName}>{l.name}</span>
              <span className={styles.layerKind}>{l.kind}</span>
            </li>
          ))}
        </ul>
      </section>
    </aside>
  );
}

function CategoryBlock({
  category,
  items,
  selectedId,
  onSelectObject,
  onUpdateObject,
  onDeleteObject,
  onMoveObject,
}: {
  category: ObjectCategory;
  items: SicroObject[];
  selectedId: string | null;
  onSelectObject: (id: string | null) => void;
  onUpdateObject: (id: string, patch: Partial<SicroObject>) => void;
  onDeleteObject: (id: string) => void;
  onMoveObject: (id: string, direction: "up" | "down") => void;
}) {
  return (
    <div className={styles.category}>
      <div className={styles.categoryTitle}>
        {CATEGORY_LABEL[category]} <span className={styles.dim}>({items.length})</span>
      </div>
      <ul className={styles.objectList}>
        {items.map((o) => (
          <ObjectRow
            key={o.id}
            obj={o}
            selected={selectedId === o.id}
            onSelect={() => onSelectObject(o.id)}
            onUpdate={(patch) => onUpdateObject(o.id, patch)}
            onDelete={() => onDeleteObject(o.id)}
            onMove={(dir) => onMoveObject(o.id, dir)}
          />
        ))}
      </ul>
    </div>
  );
}

function ObjectRow({
  obj,
  selected,
  onSelect,
  onUpdate,
  onDelete,
  onMove,
}: {
  obj: SicroObject;
  selected: boolean;
  onSelect: () => void;
  onUpdate: (patch: Partial<SicroObject>) => void;
  onDelete: () => void;
  onMove: (dir: "up" | "down") => void;
}) {
  const [renaming, setRenaming] = useState(false);
  const [draft, setDraft] = useState(obj.label ?? "");

  const visible = obj.visible !== false;
  const locked = obj.locked === true;

  const commitRename = () => {
    setRenaming(false);
    const trimmed = draft.trim();
    if (trimmed && trimmed !== (obj.label ?? "")) {
      onUpdate({ label: trimmed } as Partial<SicroObject>);
    }
  };

  return (
    <li
      className={`${styles.objectItem} ${
        selected ? styles.objectItemActive : ""
      }`}
    >
      <button
        type="button"
        className={styles.objectVisBtn}
        title={visible ? "Esconder" : "Mostrar"}
        onClick={(e) => {
          e.stopPropagation();
          onUpdate({ visible: !visible } as Partial<SicroObject>);
        }}
      >
        {visible ? <Eye size={11} /> : <EyeOff size={11} />}
      </button>
      <button
        type="button"
        className={styles.objectVisBtn}
        title={locked ? "Destravar" : "Travar"}
        onClick={(e) => {
          e.stopPropagation();
          onUpdate({ locked: !locked } as Partial<SicroObject>);
        }}
      >
        {locked ? <Lock size={11} /> : <Unlock size={11} />}
      </button>
      {renaming ? (
        <input
          type="text"
          value={draft}
          onChange={(e) => setDraft(e.target.value)}
          onBlur={commitRename}
          onKeyDown={(e) => {
            if (e.key === "Enter") commitRename();
            if (e.key === "Escape") setRenaming(false);
          }}
          autoFocus
          className={styles.renameInput}
        />
      ) : (
        <button
          type="button"
          className={styles.objectLabelBtn}
          onClick={onSelect}
          title={summariseObject(obj)}
        >
          <span className={styles.objectKindBadge}>{shortKind(obj)}</span>
          <span className={styles.objectLabel}>{summariseObject(obj)}</span>
        </button>
      )}
      <button
        type="button"
        className={styles.objectAction}
        title="Renomear"
        onClick={(e) => {
          e.stopPropagation();
          setDraft(obj.label ?? "");
          setRenaming(true);
        }}
      >
        <Pencil size={10} />
      </button>
      <button
        type="button"
        className={styles.objectAction}
        title="Para frente"
        onClick={(e) => {
          e.stopPropagation();
          onMove("up");
        }}
      >
        <ArrowUp size={10} />
      </button>
      <button
        type="button"
        className={styles.objectAction}
        title="Para trás"
        onClick={(e) => {
          e.stopPropagation();
          onMove("down");
        }}
      >
        <ArrowDown size={10} />
      </button>
      <button
        type="button"
        className={styles.objectAction}
        title="Excluir"
        onClick={(e) => {
          e.stopPropagation();
          onDelete();
        }}
      >
        <Trash2 size={10} />
      </button>
    </li>
  );
}

function shortKind(o: SicroObject): string {
  switch (o.kind) {
    case "vehicle":
      return "V";
    case "line":
      if (o.subtype === "r1") return "R1";
      if (o.subtype === "r2") return "R2";
      if (o.subtype === "arrow") return "→";
      return "L";
    case "marker":
      if (o.subtype === "collision_x") return "X";
      if (o.subtype === "brake_mark") return "B";
      if (o.subtype === "drag_mark") return "A";
      if (o.subtype === "fluid") return "F";
      if (o.subtype === "blood") return "S";
      if (o.subtype === "debris") return "D";
      if (o.subtype === "pedestrian") return "P";
      if (o.subtype === "body") return "C";
      return "·";
    case "text":
      return "T";
    case "measurement":
      return "↔";
    case "trace":
      return TRACE_SPECS[o.subtype]?.sigla ?? "·";
    case "fixture":
      return FIXTURE_SPECS[o.subtype]?.sigla ?? "·";
    case "person":
      return "P";
    case "road_parity":
      return "R";
    case "roundabout_parity":
      return "◯";
    default:
      return "?";
  }
}

function summariseObject(o: SicroObject): string {
  if (o.label) return o.label;
  switch (o.kind) {
    case "vehicle":
      return `Veículo (${o.body_type ?? "car"})`;
    case "line":
      return `Linha ${o.subtype}`;
    case "marker":
      return MARKER_STYLES[o.subtype]?.defaultLabel ?? o.subtype;
    case "text":
      return o.text.slice(0, 32);
    case "measurement":
      return "Medição";
    case "trace":
      return TRACE_SPECS[o.subtype]?.nome ?? "Vestígio";
    case "fixture":
      return FIXTURE_SPECS[o.subtype]?.nome ?? "Elemento";
    case "person":
      return `Pessoa (${PERSON_POSICOES.find(([k]) => k === o.posicao)?.[1].toLowerCase() ?? o.posicao})`;
    case "road_parity":
      return "Via";
    case "roundabout_parity":
      return "Rotatória";
    default:
      return "Objeto";
  }
}

function ObjectProperties({
  object,
  scale,
  style,
  onChange,
}: {
  object: SicroObject;
  scale: SicroCroquiScale | null;
  style: SicroCroquiStyle | undefined;
  onChange: (patch: Partial<SicroObject>) => void;
}) {
  const st = resolveParityStyle(style);
  const isParity =
    object.kind === "road_parity" || object.kind === "roundabout_parity";
  // `color`/`notes` não existem nos kinds parity.
  const colorish = object as { color?: string | null; notes?: string | null };

  const freeLabel =
    object.kind === "vehicle" ||
    object.kind === "marker" ||
    object.kind === "line" ||
    object.kind === "measurement" ||
    object.kind === "trace" ||
    object.kind === "fixture" ||
    object.kind === "person";

  return (
    <div className={styles.props}>
      {object.kind !== "measurement" && (
        <Field
          label="Rótulo"
          value={object.label ?? ""}
          onChange={(v) => onChange({ label: v } as Partial<SicroObject>)}
        />
      )}
      {freeLabel && (
        <LabelProps object={object} outlineTema={st.veiculos === "traco" ? st.tema : undefined} onChange={onChange} />
      )}

      {object.kind === "vehicle" && (
        <VehicleProps object={object} outline={st.veiculos === "traco"} onChange={onChange} />
      )}
      {object.kind === "trace" && (
        <TraceProps object={object} scale={scale} onChange={onChange} />
      )}
      {object.kind === "fixture" && (
        <FixtureProps object={object} scale={scale} onChange={onChange} />
      )}
      {object.kind === "person" && <PersonProps object={object} onChange={onChange} />}
      {object.kind === "marker" && (
        <MarkerProps object={object} onChange={onChange} />
      )}
      {object.kind === "text" && (
        <TextProps object={object} onChange={onChange} />
      )}
      {object.kind === "line" && (
        <LineProps object={object} onChange={onChange} />
      )}
      {object.kind === "measurement" && (
        <MeasurementProps object={object} scale={scale} onChange={onChange} />
      )}
      {object.kind === "road_parity" && (
        <ParityRoadProps object={object} onChange={onChange} />
      )}
      {object.kind === "roundabout_parity" && (
        <ParityRoundaboutProps object={object} onChange={onChange} />
      )}

      {!isParity && (
        <>
          {object.kind !== "trace" && object.kind !== "fixture" && object.kind !== "person" && (
          <Field
            label="Cor"
            type="color"
            value={colorish.color ?? "#000000"}
            onChange={(v) => onChange({ color: v } as Partial<SicroObject>)}
          />
          )}
          <Field
            label="Observação"
            value={colorish.notes ?? ""}
            onChange={(v) => onChange({ notes: v } as Partial<SicroObject>)}
          />
        </>
      )}
      <CheckboxRow
        label="Visível"
        checked={object.visible !== false}
        onChange={(v) => onChange({ visible: v } as Partial<SicroObject>)}
      />
      <CheckboxRow
        label="Bloqueado"
        checked={object.locked === true}
        onChange={(v) => onChange({ locked: v } as Partial<SicroObject>)}
      />
    </div>
  );
}

/** Tamanho e cor do rótulo solto; "voltar" apaga o deslocamento que o arrasto gravou. */
function LabelProps({
  object,
  outlineTema,
  onChange,
}: {
  object: SicroObject;
  outlineTema?: ParityTema;
  onChange: (patch: Partial<SicroObject>) => void;
}) {
  const d = labelDefaults(object, outlineTema);
  const lf = object as LabelFields;
  const moved = lf.label_dx !== undefined || lf.label_dy !== undefined;
  const isMeasurement = object.kind === "measurement";
  // Padrão reto; na cota, null = acompanha a linha.
  const lineAngle = isMeasurement ? angleDeg(object.p1, object.p2) : 0;
  const alongRot = lineAngle > 90 ? lineAngle - 180 : lineAngle < -90 ? lineAngle + 180 : lineAngle;
  const rot = lf.label_rotation === null ? alongRot : (lf.label_rotation ?? 0);
  return (
    <>
      <NumberField
        label="Rótulo: tamanho"
        value={lf.label_size ?? d.size}
        onChange={(n) => onChange({ label_size: Math.max(6, n) } as Partial<SicroObject>)}
      />
      <Field
        label="Rótulo: cor"
        type="color"
        value={lf.label_color ?? d.color}
        onChange={(v) => onChange({ label_color: v } as Partial<SicroObject>)}
      />
      <NumberField
        label="Rótulo: rotação (°)"
        value={Math.round(rot * 10) / 10}
        onChange={(n) => onChange({ label_rotation: n } as Partial<SicroObject>)}
      />
      <div className={styles.temaRow}>
        <button
          type="button"
          className={styles.temaBtn}
          onClick={() => onChange({ label_rotation: 0 } as Partial<SicroObject>)}
          title="Rótulo na horizontal"
        >
          Reto
        </button>
        {isMeasurement && (
          <button
            type="button"
            className={styles.temaBtn}
            onClick={() => onChange({ label_rotation: null } as Partial<SicroObject>)}
            title="Rótulo acompanha a inclinação da cota"
          >
            Na linha
          </button>
        )}
        {moved && (
          <button
            type="button"
            className={styles.temaBtn}
            onClick={() =>
              onChange({ label_dx: undefined, label_dy: undefined } as Partial<SicroObject>)
            }
            title="Desfaz o arrasto do rótulo"
          >
            Voltar ao lugar
          </button>
        )}
      </div>
    </>
  );
}

function VehicleProps({
  object,
  outline,
  onChange,
}: {
  object: SicroVehicleObject;
  outline: boolean;
  onChange: (patch: Partial<SicroObject>) => void;
}) {
  return (
    <>
      {outline && (
        <CheckboxRow
          label="Contorno tracejado (posição no impacto)"
          checked={object.tracejado === true}
          onChange={(v) => onChange({ tracejado: v } as Partial<SicroObject>)}
        />
      )}
      <SelectField
        label="Tipo"
        value={object.body_type ?? "car"}
        options={[
          { v: "sedan", l: "Sedan" },
          { v: "suv", l: "SUV" },
          { v: "hatch", l: "Hatch" },
          { v: "car", l: "Carro (genérico)" },
          { v: "pickup", l: "Pickup" },
          { v: "van", l: "Van passageiro" },
          { v: "van_furgao", l: "Van furgão" },
          { v: "onibus", l: "Ônibus" },
          { v: "micro_onibus", l: "Micro-ônibus" },
          { v: "onibus_leito", l: "Ônibus leito" },
          { v: "truck", l: "Caminhão leve" },
          { v: "caminhao", l: "Caminhão (BR)" },
          { v: "caminhao_pesado", l: "Caminhão pesado" },
          { v: "carreta", l: "Carreta" },
          { v: "reboque_guincho", l: "Reboque guincho" },
          { v: "trator", l: "Trator" },
          { v: "moto", l: "Moto urbana" },
          { v: "moto_esportiva", l: "Moto esportiva" },
          { v: "moto_carga", l: "Moto carga" },
          { v: "bike", l: "Bicicleta urbana" },
          { v: "bike_estrada", l: "Bicicleta estrada" },
          { v: "bike_cargueira", l: "Bicicleta cargueira" },
          { v: "ambulancia", l: "Ambulância" },
          { v: "taxi", l: "Táxi" },
          { v: "vtr_pm", l: "VTR PM" },
          { v: "vtr_pc", l: "VTR PC" },
          { v: "vtr_pci", l: "VTR Polícia Científica" },
          { v: "vtr_bm", l: "VTR Bombeiros" },
          { v: "vtr_pp", l: "VTR PP" },
          { v: "other", l: "Outro" },
        ]}
        onChange={(v) =>
          onChange({ body_type: v as VehicleBodyType } as Partial<SicroObject>)
        }
      />
      <NumberField label="X" value={object.x} onChange={(n) => onChange({ x: n } as Partial<SicroObject>)} />
      <NumberField label="Y" value={object.y} onChange={(n) => onChange({ y: n } as Partial<SicroObject>)} />
      <NumberField label="Largura" value={object.width} onChange={(n) => onChange({ width: Math.max(4, n) } as Partial<SicroObject>)} />
      <NumberField label="Altura" value={object.height} onChange={(n) => onChange({ height: Math.max(4, n) } as Partial<SicroObject>)} />
      <NumberField label="Rotação (°)" value={object.rotation} onChange={(n) => onChange({ rotation: n } as Partial<SicroObject>)} />
    </>
  );
}

function MarkerProps({
  object,
  onChange,
}: {
  object: SicroMarkerObject;
  onChange: (patch: Partial<SicroObject>) => void;
}) {
  return (
    <>
      <Field label="Subtipo" value={object.subtype} readOnly />
      <NumberField label="X" value={object.x} onChange={(n) => onChange({ x: n } as Partial<SicroObject>)} />
      <NumberField label="Y" value={object.y} onChange={(n) => onChange({ y: n } as Partial<SicroObject>)} />
      <NumberField label="Tamanho" value={object.size} onChange={(n) => onChange({ size: Math.max(6, n) } as Partial<SicroObject>)} />
      <NumberField
        label="Rotação (°)"
        value={object.rotation ?? 0}
        onChange={(n) => onChange({ rotation: n } as Partial<SicroObject>)}
      />
    </>
  );
}

function TextProps({
  object,
  onChange,
}: {
  object: SicroTextObject;
  onChange: (patch: Partial<SicroObject>) => void;
}) {
  return (
    <>
      <Field
        label="Texto"
        value={object.text}
        onChange={(v) => onChange({ text: v } as Partial<SicroObject>)}
      />
      <NumberField label="X" value={object.x} onChange={(n) => onChange({ x: n } as Partial<SicroObject>)} />
      <NumberField label="Y" value={object.y} onChange={(n) => onChange({ y: n } as Partial<SicroObject>)} />
      <NumberField label="Tamanho da fonte" value={object.font_size} onChange={(n) => onChange({ font_size: Math.max(8, n) } as Partial<SicroObject>)} />
      <NumberField label="Rotação (°)" value={object.rotation ?? 0} onChange={(n) => onChange({ rotation: n } as Partial<SicroObject>)} />
    </>
  );
}

function LineProps({
  object,
  onChange,
}: {
  object: SicroLineObject;
  onChange: (patch: Partial<SicroObject>) => void;
}) {
  return (
    <>
      <Field label="Subtipo" value={object.subtype} readOnly />
      <NumberField
        label="Espessura"
        value={object.stroke_width}
        onChange={(n) =>
          onChange({ stroke_width: Math.max(1, n) } as Partial<SicroObject>)
        }
      />
      <CheckboxRow
        label="Tracejada"
        checked={!!object.dashed}
        onChange={(v) => onChange({ dashed: v } as Partial<SicroObject>)}
      />
    </>
  );
}

function MeasurementProps({
  object,
  scale,
  onChange,
}: {
  object: SicroMeasurementObject;
  scale: SicroCroquiScale | null;
  onChange: (patch: Partial<SicroObject>) => void;
}) {
  const px = distancePx(object.p1, object.p2);
  const label = formatMeasurement(px, scale?.px_per_m);
  return (
    <>
      <Field label="Distância" value={label} readOnly mono />
      <Field
        label="Texto no lugar da medida"
        value={object.label_override ?? ""}
        onChange={(v) =>
          onChange({
            label_override: v.trim() === "" ? null : v,
          } as Partial<SicroObject>)
        }
      />
    </>
  );
}

// ---------------------------------------------------------------------------
// Editores parity

function ParityRoadProps({
  object,
  onChange,
}: {
  object: SicroRoadObject_parity;
  onChange: (patch: Partial<SicroObject>) => void;
}) {
  // Mão + eixo num select só; `marcacao` acompanha para croqui antigo continuar legível.
  const eixoValue = object.mao_dupla ? resolveParityEixo(object) : "mao_unica";
  const applyEixo = (v: string) => {
    if (v === "mao_unica") {
      onChange({ mao_dupla: false, eixo: "nenhuma", marcacao: "nenhuma" } as Partial<SicroObject>);
      return;
    }
    const eixo = v as ParityEixo;
    onChange({ mao_dupla: true, eixo, marcacao: marcacaoFromEixo(eixo) } as Partial<SicroObject>);
  };
  const faixasValue = object.faixas == null ? "auto" : String(object.faixas);
  const calcadaValue = object.calcada_m == null ? "padrao" : String(object.calcada_m);

  return (
    <>
      <NumberField
        label="Largura da pista (m)"
        value={object.largura_m}
        step={0.5}
        onChange={(n) => {
          const clamped = Math.min(
            Math.max(n, PARITY_ROAD_LARGURA_MIN_M),
            PARITY_ROAD_LARGURA_MAX_M,
          );
          onChange({ largura_m: clamped } as Partial<SicroObject>);
        }}
      />
      <SelectField
        label="Eixo central"
        value={eixoValue}
        options={[
          { v: "amarela_dupla", l: "Mão dupla — amarela dupla contínua" },
          { v: "amarela_trac", l: "Mão dupla — amarela tracejada" },
          { v: "amarela_mista", l: "Mão dupla — contínua + tracejada" },
          { v: "branca_trac", l: "Mão dupla — branca tracejada" },
          { v: "nenhuma", l: "Mão dupla — sem eixo" },
          { v: "mao_unica", l: "Mão única (sem eixo)" },
        ]}
        onChange={applyEixo}
      />
      <SelectField
        label={object.mao_dupla ? "Faixas por sentido" : "Faixas"}
        value={faixasValue}
        options={[
          { v: "auto", l: "Automático (≈ 3,5 m)" },
          { v: "1", l: "1" },
          { v: "2", l: "2" },
          { v: "3", l: "3" },
          { v: "4", l: "4" },
        ]}
        onChange={(v) =>
          onChange({ faixas: v === "auto" ? null : Number(v) } as Partial<SicroObject>)
        }
      />
      <NumberField
        label="Acostamento (m, cada lado)"
        value={object.acostamento_m ?? 0}
        step={0.5}
        onChange={(n) =>
          onChange({
            acostamento_m: Math.min(Math.max(n, 0), PARITY_ACOSTAMENTO_MAX_M),
          } as Partial<SicroObject>)
        }
      />
      <SelectField
        label="Calçada"
        value={calcadaValue}
        options={[
          { v: "padrao", l: "Padrão do croqui" },
          { v: "0", l: "Sem calçada" },
          { v: "1.5", l: "1,5 m" },
          { v: "2", l: "2 m" },
          { v: "3", l: "3 m" },
          { v: "4", l: "4 m" },
        ]}
        onChange={(v) =>
          onChange({ calcada_m: v === "padrao" ? null : Number(v) } as Partial<SicroObject>)
        }
      />
      <SelectField
        label="Superfície"
        value={object.superficie}
        options={[
          { v: "asfalto", l: "Asfalto" },
          { v: "calcada", l: "Calçada" },
          { v: "terra", l: "Terra" },
        ]}
        onChange={(v) =>
          onChange({
            superficie: v as ParitySuperficie,
          } as Partial<SicroObject>)
        }
      />
    </>
  );
}

function ParityRoundaboutProps({
  object,
  onChange,
}: {
  object: SicroRoundaboutObject_parity;
  onChange: (patch: Partial<SicroObject>) => void;
}) {
  return (
    <>
      <NumberField
        label="Raio externo (m)"
        value={object.r_m}
        step={1}
        onChange={(n) => {
          const clamped = Math.min(
            Math.max(n, PARITY_ROUNDABOUT_R_MIN_M),
            PARITY_ROUNDABOUT_R_MAX_M,
          );
          onChange({ r_m: clamped } as Partial<SicroObject>);
        }}
      />
      <NumberField
        label="Largura do anel (m)"
        value={object.largura_m}
        step={0.5}
        onChange={(n) => {
          // O anel não pode engolir a ilha: deixa pelo menos 1 m dela.
          const ceil = Math.min(
            PARITY_ROUNDABOUT_LARGURA_MAX_M_FALLBACK,
            Math.max(PARITY_ROUNDABOUT_LARGURA_MIN_M, object.r_m - 1),
          );
          const clamped = Math.min(
            Math.max(n, PARITY_ROUNDABOUT_LARGURA_MIN_M),
            ceil,
          );
          onChange({ largura_m: clamped } as Partial<SicroObject>);
        }}
      />
      <SelectField
        label="Superfície"
        value={object.superficie}
        options={[
          { v: "asfalto", l: "Asfalto" },
          { v: "calcada", l: "Calçada" },
          { v: "terra", l: "Terra" },
        ]}
        onChange={(v) =>
          onChange({
            superficie: v as ParitySuperficie,
          } as Partial<SicroObject>)
        }
      />
      <Field
        label="Cor da ilha"
        type="color"
        value={object.inner_color ?? "#d9e8cf"}
        onChange={(v) =>
          onChange({ inner_color: v } as Partial<SicroObject>)
        }
      />
      <SelectField
        label="Eixo do anel (sem faixas automáticas)"
        value={object.marcacao ?? "nenhuma"}
        options={[
          { v: "nenhuma", l: "Sem eixo" },
          { v: "amarela", l: "Tracejado amarelo" },
          { v: "branca", l: "Tracejado branco" },
        ]}
        onChange={(v) =>
          onChange({ marcacao: v as ParityMarcacao } as Partial<SicroObject>)
        }
      />
    </>
  );
}

const pct = (v: number) => `${Math.round(v * 100)}%`;
function traceValueText(p: Extract<TraceParamSpec, { t: "range" }>, v: number): string {
  if (p.u === "%") return pct(v);
  if (p.u === "°") return `${Math.round(v)}°`;
  if (p.u === "m") return `${v.toFixed(2).replace(".", ",")} m`;
  if (p.u === "") return p.step < 1 ? v.toFixed(2).replace(".", ",") : String(Math.round(v));
  return `${Math.round(v)} ${p.u}`;
}

/** Parâmetros de vestígio/sinalização/entorno + medidas e contas de referência (não vão para o PNG). */
function ParamProps({
  object,
  params,
  defaults,
  readouts,
  scale,
  cotaLabel,
  onChange,
}: {
  object: SicroTraceObject | SicroFixtureObject;
  params: TraceParamSpec[];
  defaults: Record<string, number | string | boolean>;
  readouts: { rows: [string, string][]; formula?: string };
  scale: SicroCroquiScale | null;
  /** Só nos vestígios: liga a cota. */
  cotaLabel?: string;
  onChange: (patch: Partial<SicroObject>) => void;
}) {
  const ppm = scale?.px_per_m && scale.px_per_m > 0 ? scale.px_per_m : 10;
  // "bend" (flecha) e "rot" (ângulo do eixo p0 → p1) vivem na geometria, não em `params`.
  const axisDeg = () => ((((Math.atan2(object.p1.y - object.p0.y, object.p1.x - object.p0.x) * 180) / Math.PI) % 360) + 360) % 360;
  const value = (k: string) =>
    k === "bend" ? (object.bend ?? 0) / ppm : k === "rot" ? Math.round(axisDeg()) : (object.params?.[k] ?? defaults[k]);
  const set = (k: string, v: number | string | boolean) => {
    if (k === "rot") {
      const c = { x: (object.p0.x + object.p1.x) / 2, y: (object.p0.y + object.p1.y) / 2 };
      const half = Math.hypot(object.p1.x - object.p0.x, object.p1.y - object.p0.y) / 2;
      const a = (Number(v) * Math.PI) / 180;
      const d = { x: Math.cos(a) * half, y: Math.sin(a) * half };
      onChange({ p0: { x: c.x - d.x, y: c.y - d.y }, p1: { x: c.x + d.x, y: c.y + d.y } } as Partial<SicroObject>);
      return;
    }
    onChange(
      (k === "bend" ? { bend: Number(v) * ppm } : { params: { ...object.params, [k]: v } }) as Partial<SicroObject>,
    );
  };
  const visible = params.filter((p) => !p.when || p.when[1].includes(value(p.when[0]) as number | string | boolean));
  return (
    <>
      {visible.map((p) => {
        if (p.t === "seg") {
          return (
            <div key={p.k} className={styles.field}>
              <span className={styles.fieldLabel}>{p.label}</span>
              <div className={`${styles.temaRow} ${styles.segWrap}`} role="radiogroup" aria-label={p.label}>
                {p.opts.map(([v, l]) => (
                  <button
                    key={String(v)}
                    type="button"
                    role="radio"
                    aria-checked={value(p.k) === v}
                    className={`${styles.temaBtn} ${value(p.k) === v ? styles.temaBtnActive : ""}`}
                    onClick={() => set(p.k, v)}
                  >
                    {l}
                  </button>
                ))}
              </div>
            </div>
          );
        }
        if (p.t === "chk") {
          return <CheckboxRow key={p.k} label={p.label} checked={!!value(p.k)} onChange={(v) => set(p.k, v)} />;
        }
        if (p.t === "txt") {
          return (
            <Field
              key={p.k}
              label={p.label}
              value={String(value(p.k) ?? "")}
              onChange={(v) => set(p.k, p.max ? v.slice(0, p.max) : v)}
            />
          );
        }
        const v = Number(value(p.k)) || 0;
        return (
          <label key={p.k} className={styles.range}>
            <span className={styles.rangeTop}>
              <span className={styles.fieldLabel}>{p.label}</span>
              <span className={styles.rangeValue}>{traceValueText(p, v)}</span>
            </span>
            <input
              type="range"
              min={p.min}
              max={p.max}
              step={p.step}
              value={Math.min(p.max, Math.max(p.min, v))}
              onChange={(e) => set(p.k, Number(e.target.value))}
            />
          </label>
        );
      })}
      {cotaLabel && object.kind === "trace" && (
        <CheckboxRow
          label={cotaLabel}
          checked={object.show_measure === true}
          onChange={(v) => onChange({ show_measure: v } as Partial<SicroObject>)}
        />
      )}
      {readouts.rows.length > 0 && (
        <dl className={styles.readouts}>
          {readouts.rows.map(([k, val]) => (
            <div key={k}>
              <dt>{k}</dt>
              <dd>{val}</dd>
            </div>
          ))}
          {readouts.formula && <p>{readouts.formula}</p>}
          {!scale && <p>Sem escala definida: contas com 10 px/m.</p>}
        </dl>
      )}
    </>
  );
}

function TraceProps({
  object,
  scale,
  onChange,
}: {
  object: SicroTraceObject;
  scale: SicroCroquiScale | null;
  onChange: (patch: Partial<SicroObject>) => void;
}) {
  const ppm = scale?.px_per_m && scale.px_per_m > 0 ? scale.px_per_m : 10;
  const spec = TRACE_SPECS[object.subtype];
  const cotaLabel =
    object.subtype === "colisao" || object.subtype === "fluido"
      ? undefined
      : object.subtype === "derrapagem"
        ? "Mostrar corda e flecha"
        : object.subtype === "fragmentos"
          ? "Mostrar cota do alcance"
          : "Mostrar cota do comprimento";
  return (
    <ParamProps
      object={object}
      params={spec.params}
      defaults={spec.def}
      readouts={traceReadouts(object, ppm)}
      scale={scale}
      cotaLabel={cotaLabel}
      onChange={onChange}
    />
  );
}

function FixtureProps({
  object,
  scale,
  onChange,
}: {
  object: SicroFixtureObject;
  scale: SicroCroquiScale | null;
  onChange: (patch: Partial<SicroObject>) => void;
}) {
  const ppm = scale?.px_per_m && scale.px_per_m > 0 ? scale.px_per_m : 10;
  const spec = FIXTURE_SPECS[object.subtype];
  return (
    <ParamProps
      object={object}
      params={spec.params}
      defaults={spec.def}
      readouts={fixtureReadouts(object, ppm)}
      scale={scale}
      onChange={onChange}
    />
  );
}

/** Botões de escolha única no estilo dos temas. */
function SegRow<V extends string>({
  label,
  value,
  options,
  onChange,
}: {
  label: string;
  value: V;
  options: [V, string][];
  onChange: (v: V) => void;
}) {
  return (
    <div className={styles.field}>
      <span className={styles.fieldLabel}>{label}</span>
      <div className={`${styles.temaRow} ${styles.segWrap}`} role="radiogroup" aria-label={label}>
        {options.map(([v, l]) => (
          <button
            key={v}
            type="button"
            role="radio"
            aria-checked={value === v}
            className={`${styles.temaBtn} ${value === v ? styles.temaBtnActive : ""}`}
            onClick={() => onChange(v)}
          >
            {l}
          </button>
        ))}
      </div>
    </div>
  );
}

function RangeRow({
  label,
  value,
  min,
  max,
  step,
  text,
  onChange,
}: {
  label: string;
  value: number;
  min: number;
  max: number;
  step: number;
  text: string;
  onChange: (v: number) => void;
}) {
  return (
    <label className={styles.range}>
      <span className={styles.rangeTop}>
        <span className={styles.fieldLabel}>{label}</span>
        <span className={styles.rangeValue}>{text}</span>
      </span>
      <input type="range" min={min} max={max} step={step} value={value} onChange={(e) => onChange(Number(e.target.value))} />
    </label>
  );
}

/** Pessoa articulada: posição, pose pronta, corpo e acabamento; a pose fina é pelas alças no canvas. */
function PersonProps({
  object,
  onChange,
}: {
  object: SicroPersonObject;
  onChange: (patch: Partial<SicroObject>) => void;
}) {
  const set = (patch: Partial<SicroPersonObject>) => onChange(patch as Partial<SicroObject>);
  const deitada = object.posicao !== "empe";
  return (
    <>
      <SegRow label="Posição" value={object.posicao} options={PERSON_POSICOES} onChange={(v) => set(personChangePosicao(object, v))} />
      {deitada && (
        <div className={styles.field}>
          <span className={styles.fieldLabel}>Pose pronta</span>
          <div className={`${styles.temaRow} ${styles.segWrap}`}>
            {Object.entries(PERSON_POSES).map(([k, p]) => (
              <button key={k} type="button" className={styles.temaBtn} onClick={() => set(personApplyPose(object, k))}>
                {p.nome}
              </button>
            ))}
          </div>
        </div>
      )}
      <RangeRow
        label="Altura"
        value={object.altura_m}
        min={1}
        max={2.05}
        step={0.01}
        text={`${object.altura_m.toFixed(2).replace(".", ",")} m`}
        onChange={(v) => set({ altura_m: v })}
      />
      <SegRow label="Compleição" value={object.comp} options={[["magro", "Magra"], ["medio", "Média"], ["robusto", "Robusta"]]} onChange={(v) => set({ comp: v })} />
      <SegRow label="Perfil" value={object.perfil} options={[["M", "Masculino"], ["F", "Feminino"]]} onChange={(v) => set({ perfil: v })} />
      {deitada && (
        <>
          <RangeRow label="Curvatura do tronco" value={object.curva} min={-40} max={40} step={1} text={`${Math.round(object.curva)}°`} onChange={(v) => set({ curva: v })} />
          <RangeRow label="Giro da cabeça" value={object.cabeca} min={-75} max={75} step={1} text={`${Math.round(object.cabeca)}°`} onChange={(v) => set({ cabeca: v })} />
        </>
      )}
      <RangeRow
        label="Rotação"
        value={((object.rotation % 360) + 360) % 360}
        min={0}
        max={359}
        step={1}
        text={`${Math.round(((object.rotation % 360) + 360) % 360)}°`}
        onChange={(v) => set({ rotation: v })}
      />
      <SegRow
        label="Acabamento"
        value={object.acab}
        options={[["normal", "Normal"], ["giz", "Giz"]]}
        // Giz nasce transparente (só o contorno); o preenchimento continua opcional.
        onChange={(v) => set(v === "giz" ? { acab: v, cor: "transparente" } : { acab: v, cor: object.cor === "transparente" ? "branco" : object.cor })}
      />
      <SegRow
        label="Preenchimento"
        value={object.cor}
        options={[["transparente", "Transparente"], ["branco", "Branco"], ["cinza", "Cinza"]]}
        onChange={(v) => set({ cor: v })}
      />
      <RangeRow
        label="Espessura do contorno"
        value={object.traco ?? 1}
        min={0.25}
        max={3}
        step={0.05}
        text={`${Math.round((object.traco ?? 1) * 100)}%`}
        onChange={(v) => set({ traco: v })}
      />
      <dl className={styles.readouts}>
        {personReadouts(object).map(([k, v]) => (
          <div key={k}>
            <dt>{k}</dt>
            <dd>{v}</dd>
          </div>
        ))}
      </dl>
    </>
  );
}

function CheckboxRow({
  label,
  checked,
  onChange,
}: {
  label: string;
  checked: boolean;
  onChange: (v: boolean) => void;
}) {
  return (
    <label className={styles.checkboxRow}>
      <input
        type="checkbox"
        checked={checked}
        onChange={(e) => onChange(e.target.checked)}
      />
      {label}
    </label>
  );
}

function SelectField({
  label,
  value,
  options,
  onChange,
}: {
  label: string;
  value: string;
  options: { v: string; l: string }[];
  onChange: (v: string) => void;
}) {
  return (
    <label className={styles.field}>
      <span className={styles.fieldLabel}>{label}</span>
      <select
        className={styles.fieldInput}
        value={value}
        onChange={(e) => onChange(e.target.value)}
      >
        {options.map((o) => (
          <option key={o.v} value={o.v}>
            {o.l}
          </option>
        ))}
      </select>
    </label>
  );
}

function Field({
  label,
  value,
  onChange,
  readOnly,
  mono,
  type,
}: {
  label: string;
  value: string;
  onChange?: (v: string) => void;
  readOnly?: boolean;
  mono?: boolean;
  type?: "text" | "color";
}) {
  return (
    <label className={styles.field}>
      <span className={styles.fieldLabel}>{label}</span>
      <input
        type={type ?? "text"}
        value={value}
        readOnly={readOnly || !onChange}
        onChange={(e) => onChange?.(e.target.value)}
        className={`${styles.fieldInput} ${mono ? styles.mono : ""}`}
      />
    </label>
  );
}

function NumberField({
  label,
  value,
  onChange,
  step,
}: {
  label: string;
  value: number;
  onChange: (n: number) => void;
  step?: number;
}) {
  return (
    <label className={styles.field}>
      <span className={styles.fieldLabel}>{label}</span>
      <input
        type="number"
        step={step ?? 1}
        value={Number.isFinite(value) ? value : 0}
        onChange={(e) => {
          const n = Number(e.target.value.replace(",", "."));
          if (Number.isFinite(n)) onChange(n);
        }}
        className={`${styles.fieldInput} ${styles.mono}`}
      />
    </label>
  );
}

// ---------------------------------------------------------------------------
// Estilo das vias (doc.style)

function StyleProps({
  style,
  onChange,
}: {
  style: SicroCroquiStyle | undefined;
  onChange: (patch: Partial<ParityStyle>) => void;
}) {
  const st = resolveParityStyle(style);
  const temas: { v: ParityTema; l: string }[] = [
    { v: "tecnico", l: "Planta técnica" },
    { v: "pb", l: "P&B" },
    { v: "escuro", l: "Escuro" },
  ];
  return (
    <>
      <div className={styles.temaRow} role="radiogroup" aria-label="Tema">
        {temas.map((t) => (
          <button
            key={t.v}
            type="button"
            role="radio"
            aria-checked={st.tema === t.v}
            className={`${styles.temaBtn} ${st.tema === t.v ? styles.temaBtnActive : ""}`}
            // Trocar o tema zera os ajustes: os valores do tema entram inteiros.
            onClick={() => onChange({ ...PARITY_TEMAS[t.v] })}
          >
            {t.l}
          </button>
        ))}
      </div>
      <Field label="Asfalto" type="color" value={st.asfalto} onChange={(v) => onChange({ asfalto: v })} />
      <Field label="Meio-fio" type="color" value={st.borda} onChange={(v) => onChange({ borda: v })} />
      <NumberField label="Meio-fio (px)" value={st.borda_px} step={0.5} onChange={(n) => onChange({ borda_px: Math.min(Math.max(n, 0.5), 6) })} />
      <NumberField label="Sinalização (px)" value={st.marcacao_px} step={0.5} onChange={(n) => onChange({ marcacao_px: Math.min(Math.max(n, 0.5), 6) })} />
      <Field label="Amarela" type="color" value={st.amarela} onChange={(v) => onChange({ amarela: v })} />
      <Field label="Branca" type="color" value={st.branca} onChange={(v) => onChange({ branca: v })} />
      <SelectField
        label="Calçada"
        value={st.calcada}
        options={[
          { v: "hachura", l: "Hachura" },
          { v: "cinza", l: "Cinza" },
          { v: "linha", l: "Só a linha externa" },
          { v: "nenhuma", l: "Nenhuma" },
        ]}
        onChange={(v) => onChange({ calcada: v as ParityCalcadaEstilo })}
      />
      <NumberField label="Calçada (m)" value={st.calcada_m} step={0.5} onChange={(n) => onChange({ calcada_m: Math.min(Math.max(n, 0), 6) })} />
      <NumberField label="Traço (m)" value={st.traco_m} step={0.5} onChange={(n) => onChange({ traco_m: Math.min(Math.max(n, 0.5), 12) })} />
      <NumberField label="Espaço (m)" value={st.espaco_m} step={0.5} onChange={(n) => onChange({ espaco_m: Math.min(Math.max(n, 0.5), 12) })} />
      <CheckboxRow
        label="Dividir faixas (≈ 3,5 m)"
        checked={st.faixas_auto}
        onChange={(v) => onChange({ faixas_auto: v })}
      />
      <SelectField
        label="Vestígios"
        value={st.vestigios}
        options={[
          { v: "textura", l: "Textura (banda de rodagem, sombra, estrias)" },
          { v: "traco", l: "Traço limpo" },
        ]}
        onChange={(v) => onChange({ vestigios: v as ParityVestigiosEstilo })}
      />
      <SelectField
        label="Veículos"
        value={st.veiculos}
        options={[
          { v: "traco", l: "Em traço" },
          { v: "arte", l: "Arte colorida" },
        ]}
        onChange={(v) => onChange({ veiculos: v as ParityVeiculosEstilo })}
      />
    </>
  );
}

// ---------------------------------------------------------------------------
// Folha (o que vai para o PNG), grade e resolução

const SHEET_PRESETS_M: [number, number][] = [
  [25, 18],
  [50, 35],
  [100, 71],
  [200, 141],
];

function SheetProps({
  canvas,
  scale,
  exportSettings,
  onChange,
  onChangeExport,
  onFit,
  onCenter,
}: {
  canvas: SicroCroquiCanvas;
  scale: SicroCroquiScale | null;
  exportSettings?: SicroCroquiExportSettings;
  onChange: (patch: Partial<SicroCroquiCanvas>) => void;
  onChangeExport?: (patch: Partial<SicroCroquiExportSettings>) => void;
  onFit?: () => void;
  onCenter?: () => void;
}) {
  const ppm = scale?.px_per_m ?? null;
  const grid = canvas.grid ?? { enabled: true, size_px: 50 };
  const gridM = grid.size_m ?? (ppm ? grid.size_px / ppm : null);
  const wM = ppm ? canvas.width_px / ppm : null;
  const hM = ppm ? canvas.height_px / ppm : null;
  const presetValue =
    wM != null && hM != null
      ? (SHEET_PRESETS_M.find(([w, h]) => Math.abs(w - wM) < 0.5 && Math.abs(h - hM) < 0.5)?.join("x") ?? "custom")
      : "custom";
  const setSizeM = (w: number, h: number) => {
    if (!ppm) return;
    onChange({ width_px: Math.round(w * ppm), height_px: Math.round(h * ppm) });
  };
  const pngWidth = exportSettings?.png_width_px ?? CROQUI_EXPORT_WIDTH_PX_DEFAULT;

  return (
    <>
      {ppm ? (
        <>
          <SelectField
            label="Tamanho da folha"
            value={presetValue}
            options={[
              ...SHEET_PRESETS_M.map(([w, h]) => ({ v: `${w}x${h}`, l: `${w} × ${h} m` })),
              { v: "custom", l: "Personalizado" },
            ]}
            onChange={(v) => {
              const p = SHEET_PRESETS_M.find(([w, h]) => `${w}x${h}` === v);
              if (p) setSizeM(p[0], p[1]);
            }}
          />
          <NumberField
            label="Largura (m)"
            value={Number((wM ?? 0).toFixed(1))}
            step={1}
            onChange={(n) => setSizeM(Math.max(5, n), hM ?? 10)}
          />
          <NumberField
            label="Altura (m)"
            value={Number((hM ?? 0).toFixed(1))}
            step={1}
            onChange={(n) => setSizeM(wM ?? 10, Math.max(5, n))}
          />
          <SelectField
            label="Grade"
            value={gridM != null ? String(gridM) : "1"}
            options={[
              // Grade herdada em px que não bate com os presets aparece como está.
              ...(gridM != null && ![0.5, 1, 2, 5, 10].includes(gridM)
                ? [{ v: String(gridM), l: `≈ ${gridM.toFixed(1).replace(".", ",")} m (atual)` }]
                : []),
              { v: "0.5", l: "0,5 m" },
              { v: "1", l: "1 m" },
              { v: "2", l: "2 m" },
              { v: "5", l: "5 m" },
              { v: "10", l: "10 m" },
            ]}
            onChange={(v) =>
              onChange({ grid: { ...grid, size_m: Number(v), size_px: Math.round(Number(v) * ppm) } })
            }
          />
        </>
      ) : (
        <>
          <NumberField label="Largura (px)" value={canvas.width_px} step={100} onChange={(n) => onChange({ width_px: Math.max(200, Math.round(n)) })} />
          <NumberField label="Altura (px)" value={canvas.height_px} step={100} onChange={(n) => onChange({ height_px: Math.max(200, Math.round(n)) })} />
          <p className={styles.empty}>Defina a escala para trabalhar em metros.</p>
        </>
      )}
      <CheckboxRow
        label="Mostrar grade"
        checked={grid.enabled !== false}
        onChange={(v) => onChange({ grid: { ...grid, enabled: v } })}
      />
      <div className={styles.temaRow}>
        {onCenter && (
          <button type="button" className={styles.temaBtn} onClick={onCenter} title="Centraliza a folha no que está na tela">
            Folha aqui
          </button>
        )}
        {onFit && (
          <button type="button" className={styles.temaBtn} onClick={onFit} title="Folha em volta de tudo que foi desenhado">
            Ajustar à cena
          </button>
        )}
      </div>
      {onChangeExport && (
        <SelectField
          label="PNG exportado"
          value={String(pngWidth)}
          options={[
            { v: "2480", l: "2480 px (A4 a 200 dpi)" },
            { v: "3508", l: "3508 px (A4 a 300 dpi)" },
            { v: "4961", l: "4961 px (A3 a 300 dpi)" },
            { v: "7016", l: "7016 px (A2 a 300 dpi)" },
          ]}
          onChange={(v) => onChangeExport({ png_width_px: Number(v) })}
        />
      )}
      <p className={styles.empty}>O PNG é a folha inteira, independente do zoom da tela.</p>
    </>
  );
}
