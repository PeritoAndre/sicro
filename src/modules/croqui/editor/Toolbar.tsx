/**
 * Toolbar — left vertical strip with the available tools (Round 3 of
 * MVP 9 — Road Engine Pro, compact layout).
 *
 * The strip is now organised around **category buttons**: each major
 * category (Veículo, Vestígio, Mobiliário, Pessoa, Anotação, Via)
 * shows a single chip that opens a popover with the actual subtypes.
 * The chip remembers the most recently picked subtype so the user can
 * re-pick "the same kind I picked last time" with one click.
 *
 * Tools without enough variations to deserve a popover (Selecionar,
 * Pan, Medida, Escala, R1/R2) live in a small "Atalhos" group at the
 * top.
 *
 * The legacy "Via (linhas soltas)" group from Round 2 is gone — the
 * Road Engine Pro is the only path for creating new vias. Renderers
 * and serializers still accept `SicroLineObject` instances saved by
 * older croquis (read-side compat), but the toolbar doesn't surface
 * the old line subtypes anymore.
 */

import { useEffect, useMemo, useRef, useState } from "react";
import {
  Ambulance,
  ArrowLeft,
  Bike,
  Bus,
  Car,
  CarTaxiFront,
  Circle,
  CornerDownRight,
  Crosshair,
  FileImage,
  Hand,
  Image as ImageIcon,
  Layers,
  MapPin,
  MessageSquareQuote,
  Minus,
  MoreHorizontal,
  MousePointer2,
  PenLine,
  PersonStanding,
  Plus,
  Redo2,
  Route,
  Ruler,
  Save,
  Signpost,
  Siren,
  Square,
  StretchHorizontal,
  Tractor,
  TrafficCone,
  Trash2,
  TreeDeciduous,
  Truck,
  Type,
  Undo2,
} from "lucide-react";
import type { LucideIcon } from "lucide-react";
import { Button } from "@components/Button/Button";
import type { Tool } from "./useEditorState";
import styles from "./Toolbar.module.css";

interface SubTool {
  key: Tool;
  label: string;
  icon: LucideIcon;
  hint?: string;
}

interface CategoryDef {
  id: string;
  label: string;
  icon: LucideIcon;
  /** Pre-pickup state — used when the user has never picked a subtype yet. */
  defaultTool: Tool;
  /** Subtools shown in the popover. */
  subtools: SubTool[];
}

// ---------------------------------------------------------------------------
// Atomic tools — surfaced directly without a popover.

const ATOMIC_TOOLS: SubTool[] = [
  { key: "select", label: "Selecionar", icon: MousePointer2, hint: "V" },
  { key: "pan", label: "Pan", icon: Hand, hint: "H" },
  { key: "measurement", label: "Medida / cota", icon: Ruler },
  { key: "set_scale", label: "Definir escala", icon: Crosshair },
];

const REFERENCIAL_TOOLS: SubTool[] = [
  { key: "line_r1", label: "R1", icon: Minus },
  { key: "line_r2", label: "R2", icon: Minus },
];

// ---------------------------------------------------------------------------
// Category buttons — each opens a popover with subtypes.

const CATEGORIES: CategoryDef[] = [
  {
    id: "via",
    label: "Via",
    icon: Route,
    defaultTool: "road_urban",
    subtools: [
      { key: "road_urban", label: "Via urbana", icon: Route },
      { key: "road_avenue", label: "Avenida", icon: Route },
      { key: "road_highway", label: "Rodovia", icon: Route },
      { key: "road_dirt", label: "Estrada de terra", icon: Route },
      { key: "road_parking", label: "Estacionamento", icon: Square },
      // Road Engine 2.0 Ciclo 2 — rotatória primitiva (single-click insert).
      { key: "roundabout", label: "Rotatória", icon: Circle },
    ],
  },
  {
    id: "veiculo",
    label: "Veículo",
    icon: Car,
    defaultTool: "vehicle_sedan",
    subtools: [
      { key: "vehicle_sedan", label: "Sedan", icon: Car },
      { key: "vehicle_hatch", label: "Hatch", icon: Car },
      { key: "vehicle_suv", label: "SUV", icon: Car },
      { key: "vehicle_pickup", label: "Pickup", icon: Car },
      { key: "vehicle_van", label: "Van passageiro", icon: Bus },
      { key: "vehicle_van_furgao", label: "Van furgão", icon: Bus },
      { key: "vehicle_onibus", label: "Ônibus", icon: Bus },
      { key: "vehicle_micro_onibus", label: "Micro-ônibus", icon: Bus },
      { key: "vehicle_onibus_leito", label: "Ônibus leito", icon: Bus },
      { key: "vehicle_truck", label: "Caminhão leve", icon: Truck },
      { key: "vehicle_caminhao_pesado", label: "Caminhão pesado", icon: Truck },
      { key: "vehicle_carreta", label: "Carreta", icon: Truck },
      { key: "vehicle_reboque_guincho", label: "Reboque guincho", icon: Truck },
      { key: "vehicle_trator", label: "Trator", icon: Tractor },
      { key: "vehicle_moto", label: "Moto urbana", icon: Bike },
      { key: "vehicle_moto_esportiva", label: "Moto esportiva", icon: Bike },
      { key: "vehicle_moto_carga", label: "Moto carga", icon: Bike },
      { key: "vehicle_bike", label: "Bicicleta urbana", icon: Bike },
      { key: "vehicle_bike_estrada", label: "Bicicleta estrada", icon: Bike },
      { key: "vehicle_bike_cargueira", label: "Bicicleta cargueira", icon: Bike },
    ],
  },
  {
    // Frota SVG do designer — pintura oficial fixa (a cor do Inspector não
    // repinta viatura/ambulância/táxi; vide engine/vehicleArt.ts).
    id: "viatura",
    label: "Viaturas",
    icon: Siren,
    defaultTool: "vehicle_vtr_pm",
    subtools: [
      { key: "vehicle_vtr_pm", label: "VTR PM", icon: Siren },
      { key: "vehicle_vtr_pc", label: "VTR PC", icon: Siren },
      { key: "vehicle_vtr_pci", label: "VTR Polícia Científica", icon: Siren },
      { key: "vehicle_vtr_bm", label: "VTR Bombeiros", icon: Siren },
      { key: "vehicle_vtr_pp", label: "VTR PP", icon: Siren },
      { key: "vehicle_ambulancia", label: "Ambulância", icon: Ambulance },
      { key: "vehicle_taxi", label: "Táxi", icon: CarTaxiFront },
    ],
  },
  {
    id: "vestigio",
    label: "Vestígio",
    icon: MoreHorizontal,
    defaultTool: "marker_x",
    subtools: [
      { key: "marker_x", label: "Ponto de colisão (X)", icon: Plus },
      { key: "marker_rest_position", label: "Repouso final", icon: Square },
      { key: "marker_brake", label: "Frenagem", icon: StretchHorizontal },
      { key: "marker_drag", label: "Arrasto", icon: StretchHorizontal },
      { key: "marker_skid_curve", label: "Derrapagem em curva", icon: StretchHorizontal },
      { key: "marker_sulcagem", label: "Sulcagem", icon: StretchHorizontal },
      { key: "marker_ranhura", label: "Ranhura", icon: StretchHorizontal },
      { key: "marker_debris", label: "Fragmentos", icon: MoreHorizontal },
      { key: "marker_fluid", label: "Fluido", icon: MoreHorizontal },
      { key: "marker_blood", label: "Sangue", icon: MoreHorizontal },
      { key: "marker_impact_area", label: "Área de impacto", icon: MoreHorizontal },
    ],
  },
  {
    id: "mobiliario",
    label: "Mobiliário",
    icon: TrafficCone,
    defaultTool: "marker_semaforo",
    subtools: [
      { key: "marker_semaforo", label: "Semáforo", icon: TrafficCone },
      { key: "marker_placa_pare", label: "Placa PARE", icon: Square },
      { key: "marker_placa_preferencia", label: "Placa Preferência", icon: Square },
      { key: "marker_poste", label: "Poste", icon: TrafficCone },
      { key: "marker_arvore", label: "Árvore", icon: TreeDeciduous },
      { key: "marker_guia", label: "Guia / meio-fio", icon: Signpost },
      { key: "marker_faixa_pedestre", label: "Faixa de pedestres", icon: StretchHorizontal },
    ],
  },
  {
    id: "pessoa",
    label: "Pessoa",
    icon: PersonStanding,
    defaultTool: "marker_pedestre_m_dorsal",
    subtools: [
      // Frota SVG do designer — vítimas em decúbito, escala humana real.
      // Os marcadores genéricos "Pedestre" e "Vítima / cadáver" saíram da
      // trilha em favor das artes; os subtipos `pedestrian`/`body` seguem no
      // schema e no renderer para abrir croquis antigos sem quebrar.
      { key: "marker_pedestre_m_dorsal", label: "Vítima M — dec. dorsal", icon: PersonStanding },
      { key: "marker_pedestre_m_lateral", label: "Vítima M — dec. lateral", icon: PersonStanding },
      { key: "marker_pedestre_m_ventral", label: "Vítima M — dec. ventral", icon: PersonStanding },
      { key: "marker_pedestre_f_dorsal", label: "Vítima F — dec. dorsal", icon: PersonStanding },
      { key: "marker_pedestre_f_lateral", label: "Vítima F — dec. lateral", icon: PersonStanding },
      { key: "marker_pedestre_f_ventral", label: "Vítima F — dec. ventral", icon: PersonStanding },
    ],
  },
  {
    id: "anotacao",
    label: "Anotação",
    icon: PenLine,
    defaultTool: "text",
    subtools: [
      { key: "text", label: "Texto / etiqueta", icon: Type },
      { key: "line_callout", label: "Chamada (callout)", icon: MessageSquareQuote },
      { key: "line_arrow", label: "Seta direcional", icon: CornerDownRight },
      { key: "line_trajetoria", label: "Trajetória", icon: CornerDownRight },
    ],
  },
];

interface Props {
  activeTool: Tool;
  onSelectTool: (t: Tool) => void;
  canDelete: boolean;
  onDelete: () => void;
  canUndo: boolean;
  onUndo: () => void;
  canRedo: boolean;
  onRedo: () => void;
  canDuplicate: boolean;
  onDuplicate: () => void;
  onImportBackground: () => void;
  onPickFromDossie: () => void;
  /** MVP 9 Round 4 — open the drone pre-processing wizard. */
  onImportDrone?: () => void;
  /** MVP 10 — open the OSM road import wizard. */
  onImportOsm?: () => void;
  /** MVP 9 Round 5 — background framing helpers. */
  onCenterBackground?: () => void;
  onFitBackground?: () => void;
  onResetBackgroundRotation?: () => void;
  onRemoveBackground?: () => void;
  hasBackground: boolean;
  bgLocked: boolean;
  onToggleBackgroundLock: () => void;
  bgOpacity: number;
  onChangeBackgroundOpacity: (v: number) => void;
  onInsertInLaudo?: () => void;
  onSave: () => void;
  onExportPng: () => void;
  /** MVP 9 — variante sem carimbo, ideal para inserir no laudo. */
  onExportPngClean?: () => void;
  onBackToList: () => void;
  saving: boolean;
  exporting: boolean;
}

export function Toolbar({
  activeTool,
  onSelectTool,
  canDelete,
  onDelete,
  canUndo,
  onUndo,
  canRedo,
  onRedo,
  canDuplicate,
  onDuplicate,
  onImportBackground,
  onPickFromDossie,
  onImportDrone,
  onImportOsm,
  onCenterBackground,
  onFitBackground,
  onResetBackgroundRotation,
  onRemoveBackground,
  hasBackground,
  bgLocked,
  onToggleBackgroundLock,
  bgOpacity,
  onChangeBackgroundOpacity,
  onInsertInLaudo,
  onSave,
  onExportPng,
  onExportPngClean,
  onBackToList,
  saving,
  exporting,
}: Props) {
  // The popover that's currently open. Only one at a time — picking a
  // subtype on one closes it, clicking outside closes it, Esc closes it.
  const [openPopover, setOpenPopover] = useState<string | null>(null);

  // Remember the most recently selected subtype per category — so the
  // category chip shows it (and re-clicking the chip activates it
  // directly, no popover needed).
  const [lastPick, setLastPick] = useState<Record<string, Tool>>({});

  // When the parent flips the tool externally (keyboard shortcut, click
  // on canvas, etc.), remember the new pick if it belongs to a category.
  useEffect(() => {
    for (const cat of CATEGORIES) {
      const found = cat.subtools.find((s) => s.key === activeTool);
      if (found) {
        setLastPick((prev) =>
          prev[cat.id] === activeTool ? prev : { ...prev, [cat.id]: activeTool },
        );
        return;
      }
    }
  }, [activeTool]);

  // Close popovers when the user clicks outside the toolbar.
  const rootRef = useRef<HTMLElement | null>(null);
  useEffect(() => {
    if (!openPopover) return;
    const handler = (e: MouseEvent) => {
      const root = rootRef.current;
      if (!root) return;
      if (e.target instanceof Node && root.contains(e.target)) return;
      setOpenPopover(null);
    };
    const onEsc = (e: KeyboardEvent) => {
      if (e.key === "Escape") setOpenPopover(null);
    };
    document.addEventListener("mousedown", handler);
    document.addEventListener("keydown", onEsc);
    return () => {
      document.removeEventListener("mousedown", handler);
      document.removeEventListener("keydown", onEsc);
    };
  }, [openPopover]);

  return (
    <aside
      className={styles.toolbar}
      aria-label="Ferramentas do croqui"
      ref={rootRef}
    >
      <button
        type="button"
        className={styles.backBtn}
        onClick={onBackToList}
        title="Voltar para a lista de croquis"
      >
        <ArrowLeft size={14} /> Voltar
      </button>

      {/* Atalhos diretos (sem popover) */}
      <div className={styles.group}>
        <div className={styles.groupTitle}>Atalhos</div>
        {ATOMIC_TOOLS.map((t) => (
          <ToolButton
            key={t.key}
            tool={t}
            active={activeTool === t.key}
            onSelect={(k) => {
              setOpenPopover(null);
              onSelectTool(k);
            }}
          />
        ))}
      </div>

      <div className={styles.group}>
        <div className={styles.groupTitle}>Referencial</div>
        {REFERENCIAL_TOOLS.map((t) => (
          <ToolButton
            key={t.key}
            tool={t}
            active={activeTool === t.key}
            onSelect={(k) => {
              setOpenPopover(null);
              onSelectTool(k);
            }}
          />
        ))}
      </div>

      {/* Categorias com popover */}
      {CATEGORIES.map((cat) => (
        <CategoryButton
          key={cat.id}
          cat={cat}
          activeTool={activeTool}
          lastPick={lastPick[cat.id]}
          isOpen={openPopover === cat.id}
          onToggle={() =>
            setOpenPopover((curr) => (curr === cat.id ? null : cat.id))
          }
          onPick={(key) => {
            setLastPick((prev) => ({ ...prev, [cat.id]: key }));
            setOpenPopover(null);
            onSelectTool(key);
          }}
        />
      ))}

      {/* Imagem de fundo */}
      <div className={styles.group}>
        <div className={styles.groupTitle}>
          <ImageIcon size={11} /> Imagem
        </div>
        <button
          type="button"
          className={styles.actionBtn}
          onClick={onImportBackground}
          title="Importar imagem do disco como fundo"
        >
          <ImageIcon size={12} /> Importar
        </button>
        <button
          type="button"
          className={styles.actionBtn}
          onClick={onPickFromDossie}
          title="Usar uma foto do Dossiê como fundo"
        >
          <MapPin size={12} /> Do Dossiê
        </button>
        {onImportDrone && (
          <button
            type="button"
            className={styles.actionBtn}
            onClick={onImportDrone}
            title="Abre o assistente de drone: correção radial de lente + crop + sidecar JSON antes de usar como fundo"
          >
            <FileImage size={12} /> Importar Drone…
          </button>
        )}
        {onImportOsm && (
          <button
            type="button"
            className={styles.actionBtn}
            onClick={onImportOsm}
            title="Abre o mapa OSM para localizar o sinistro e importar as vias da região como RoadObjects editáveis"
          >
            <MapPin size={12} /> Importar OSM…
          </button>
        )}
        {hasBackground && (
          <>
            <button
              type="button"
              className={styles.actionBtn}
              onClick={onToggleBackgroundLock}
              title={
                bgLocked
                  ? "Desbloquear imagem de fundo"
                  : "Bloquear imagem de fundo"
              }
            >
              <Layers size={12} />
              {bgLocked ? "Desbloquear fundo" : "Bloquear fundo"}
            </button>
            <label className={styles.slider}>
              <span>Opacidade</span>
              <input
                type="range"
                min={0.1}
                max={1}
                step={0.05}
                value={bgOpacity}
                onChange={(e) =>
                  onChangeBackgroundOpacity(Number(e.target.value))
                }
              />
              <span className={styles.sliderValue}>
                {Math.round(bgOpacity * 100)}%
              </span>
            </label>
            {/* MVP 9 Round 5 — framing helpers */}
            {onCenterBackground && (
              <button
                type="button"
                className={styles.actionBtn}
                onClick={onCenterBackground}
                title="Centralizar o fundo na área útil do croqui"
              >
                Centralizar
              </button>
            )}
            {onFitBackground && (
              <button
                type="button"
                className={styles.actionBtn}
                onClick={onFitBackground}
                title="Ajustar o fundo à área útil (10% margem)"
              >
                Ajustar à área útil
              </button>
            )}
            {onResetBackgroundRotation && (
              <button
                type="button"
                className={styles.actionBtn}
                onClick={onResetBackgroundRotation}
                title="Reiniciar a rotação do fundo para 0°"
              >
                Reset rotação
              </button>
            )}
            {onRemoveBackground && (
              <button
                type="button"
                className={styles.actionBtn}
                onClick={onRemoveBackground}
                title="Remover o fundo (mantém o croqui sem imagem)"
              >
                Remover fundo
              </button>
            )}
          </>
        )}
      </div>

      {/* Edição básica */}
      <div className={styles.group}>
        <div className={styles.groupTitle}>Editar</div>
        <div className={styles.row}>
          <button
            type="button"
            className={styles.actionBtn}
            onClick={onUndo}
            disabled={!canUndo}
            title="Desfazer (Ctrl+Z)"
          >
            <Undo2 size={12} />
          </button>
          <button
            type="button"
            className={styles.actionBtn}
            onClick={onRedo}
            disabled={!canRedo}
            title="Refazer (Ctrl+Y)"
          >
            <Redo2 size={12} />
          </button>
          <button
            type="button"
            className={styles.actionBtn}
            onClick={onDuplicate}
            disabled={!canDuplicate}
            title="Duplicar (Ctrl+D)"
          >
            <Plus size={12} />
          </button>
          <button
            type="button"
            className={styles.actionBtn}
            onClick={onDelete}
            disabled={!canDelete}
            title="Excluir (Del)"
          >
            <Trash2 size={12} />
          </button>
        </div>
      </div>

      <div className={styles.spacer} />

      {/* Save + Export */}
      <Button
        variant="secondary"
        leftIcon={<Save size={14} />}
        onClick={onSave}
        disabled={saving}
      >
        {saving ? "Salvando…" : "Salvar"}
      </Button>
      <Button
        variant="primary"
        leftIcon={<FileImage size={14} />}
        onClick={onExportPng}
        disabled={exporting}
        title="Exporta com cabeçalho técnico (título · BO · escala · timestamp)"
      >
        {exporting ? "Exportando…" : "Exportar PNG técnico"}
      </Button>
      {onExportPngClean && (
        <Button
          variant="secondary"
          leftIcon={<FileImage size={14} />}
          onClick={onExportPngClean}
          disabled={exporting}
          title="Exporta sem carimbo — ideal para inserir no corpo do laudo"
        >
          PNG limpo
        </Button>
      )}
      {onInsertInLaudo && (
        <Button
          variant="secondary"
          leftIcon={<Square size={14} />}
          onClick={onInsertInLaudo}
        >
          Abrir Laudo
        </Button>
      )}
    </aside>
  );
}

function CategoryButton({
  cat,
  activeTool,
  lastPick,
  isOpen,
  onToggle,
  onPick,
}: {
  cat: CategoryDef;
  activeTool: Tool;
  lastPick: Tool | undefined;
  isOpen: boolean;
  onToggle: () => void;
  onPick: (k: Tool) => void;
}) {
  const subtoolsByKey = useMemo(() => {
    const m = new Map<Tool, SubTool>();
    for (const s of cat.subtools) m.set(s.key, s);
    return m;
  }, [cat]);

  // The category is "active" when the user currently has any of its
  // subtools selected.
  const isCategoryActive = subtoolsByKey.has(activeTool);
  // The label shown on the chip — last picked subtool, or the default.
  const chipKey =
    lastPick && subtoolsByKey.has(lastPick) ? lastPick : cat.defaultTool;
  const chipDisplay = subtoolsByKey.get(chipKey) ?? cat.subtools[0];
  if (!chipDisplay) return null;

  const ChipIcon = chipDisplay.icon;
  const CatIcon = cat.icon;

  return (
    <div className={styles.subAction}>
      <div className={styles.groupTitle}>
        <CatIcon size={11} /> {cat.label}
      </div>
      <div className={styles.row}>
        {/* Main chip: activates last-picked subtool */}
        <button
          type="button"
          className={`${styles.tool} ${
            isCategoryActive ? styles.toolActive : ""
          }`}
          onClick={() => onPick(chipKey)}
          title={`${cat.label} — ${chipDisplay.label}`}
        >
          <ChipIcon size={13} aria-hidden />
          <span>{chipDisplay.label}</span>
        </button>
        {/* Disclosure caret: opens popover */}
        <button
          type="button"
          className={styles.actionBtn}
          onClick={onToggle}
          title={`Mais opções de ${cat.label}`}
          aria-expanded={isOpen}
        >
          ▾
        </button>
      </div>
      {isOpen && (
        <div className={styles.dropdown}>
          {cat.subtools.map((s) => (
            <button
              type="button"
              key={s.key}
              className={`${styles.dropdownItem} ${
                activeTool === s.key ? styles.toolActive : ""
              }`}
              onClick={() => onPick(s.key)}
            >
              <s.icon size={11} aria-hidden style={{ marginRight: 6 }} />
              {s.label}
            </button>
          ))}
        </div>
      )}
    </div>
  );
}

function ToolButton({
  tool,
  active,
  onSelect,
}: {
  tool: SubTool;
  active: boolean;
  onSelect: (t: Tool) => void;
}) {
  return (
    <button
      type="button"
      className={`${styles.tool} ${active ? styles.toolActive : ""}`}
      onClick={() => onSelect(tool.key)}
      title={tool.hint ? `${tool.label} (${tool.hint})` : tool.label}
    >
      <tool.icon size={13} aria-hidden />
      <span>{tool.label}</span>
    </button>
  );
}
