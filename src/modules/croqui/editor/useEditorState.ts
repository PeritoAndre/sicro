/**
 * useEditorState — transient editor state shared between Toolbar, Stage and
 * InspectorPanel. Owns the active tool, the selected object id, the viewport
 * (pan/zoom), the "scale calibration" in-progress state, and a small undo
 * history (kept simple — full undo/redo lives in a future spike).
 *
 * The persisted doc lives in `croquiStore`. This file is purely "what is
 * the user doing right now?"
 */

import { useCallback, useRef, useState } from "react";
import type { SicroObject, SicroPoint } from "../engine";

/**
 * Tool keys. MVP 6 adds dedicated tools per vehicle subtype, per marker
 * subtype, per line subtype and a generic `template:<id>` shape for road
 * templates. Keeping a single string union (instead of nested structs)
 * makes the toolbar trivially mappable.
 */
export type Tool =
  | "select"
  | "pan"
  // Vehicles
  | "vehicle"            // legacy alias for "vehicle_car"
  | "vehicle_car"
  | "vehicle_sedan"
  | "vehicle_suv"
  | "vehicle_hatch"
  | "vehicle_moto"
  | "vehicle_truck"
  | "vehicle_bike"
  // MVP 9 — frota expandida
  | "vehicle_pickup"
  | "vehicle_van"
  | "vehicle_onibus"
  | "vehicle_moto_esportiva"
  | "vehicle_moto_carga"
  | "vehicle_caminhao_pesado"
  | "vehicle_carreta"
  // Frota SVG do designer — civis
  | "vehicle_van_furgao"
  | "vehicle_micro_onibus"
  | "vehicle_onibus_leito"
  | "vehicle_reboque_guincho"
  | "vehicle_trator"
  | "vehicle_bike_estrada"
  | "vehicle_bike_cargueira"
  // Frota SVG do designer — viaturas/especiais (pintura fixa)
  | "vehicle_ambulancia"
  | "vehicle_taxi"
  | "vehicle_vtr_pm"
  | "vehicle_vtr_pc"
  | "vehicle_vtr_pci"
  | "vehicle_vtr_bm"
  | "vehicle_vtr_pp"
  // Pedestres em decúbito (frota SVG do designer)
  | "marker_pedestre_m_dorsal"
  | "marker_pedestre_m_lateral"
  | "marker_pedestre_m_ventral"
  | "marker_pedestre_f_dorsal"
  | "marker_pedestre_f_lateral"
  | "marker_pedestre_f_ventral"
  // Lines
  | "line_road"
  | "line_lane"
  | "line_lane_separator"
  | "line_sidewalk"
  | "line_arrow"
  | "line_r1"
  | "line_r2"
  // MVP 9 — linhas adicionais
  | "line_canteiro"
  | "line_acostamento"
  | "line_trajetoria"
  | "line_callout"
  // Markers (collision + vestígios + pessoas)
  | "marker_x"
  | "marker_brake"
  | "marker_drag"
  | "marker_fluid"
  | "marker_blood"
  | "marker_debris"
  | "marker_pedestrian"
  | "marker_body"
  // MVP 9 — vestígios + mobiliário urbano
  | "marker_skid_curve"
  | "marker_sulcagem"
  | "marker_ranhura"
  | "marker_impact_area"
  | "marker_rest_position"
  | "marker_semaforo"
  | "marker_placa_pare"
  | "marker_placa_preferencia"
  | "marker_poste"
  | "marker_arvore"
  | "marker_guia"
  | "marker_faixa_pedestre"
  // Annotation / measurement / scale
  | "text"
  | "measurement"
  | "set_scale"
  // MVP 9 Road Engine Pro — multi-click road creation, one tool per style.
  | "road_urban"
  | "road_avenue"
  | "road_highway"
  | "road_dirt"
  | "road_parking"
  // Road Engine 2.0 Ciclo 2 — rotatória primitiva. Click once para
  // inserir uma rotatória default no ponto clicado.
  | "roundabout";

/** Stage of a two-click tool (measurement / set_scale / line). */
export interface PendingTwoClick {
  tool: Tool;
  first: SicroPoint;
}

/**
 * Multi-click road drafting state (MVP 9 Road Engine Pro). The user
 * clicks any number of control points; pressing Enter / double-click
 * finalises the road, Esc cancels.
 */
export interface RoadDraft {
  tool: Tool;
  points: SicroPoint[];
}

export interface Viewport {
  /** Stage scale (zoom). 1 = 100%. */
  scale: number;
  /** Stage position (pan), in stage coordinates. */
  x: number;
  y: number;
}

export const DEFAULT_VIEWPORT: Viewport = { scale: 1, x: 0, y: 0 };

/**
 * Marquee em curso: retângulo em coordenadas WORLD (stage coords, com
 * viewport aplicado). `null` quando o usuário não está arrastando.
 * Snapshot dos `selectedIds` no início do drag pra permitir
 * Shift+drag aditivo no futuro (por ora não usado).
 */
export interface Marquee {
  startWorldX: number;
  startWorldY: number;
  currentWorldX: number;
  currentWorldY: number;
}

export function useEditorState() {
  const [tool, setTool] = useState<Tool>("select");
  // `selectedIds` é a fonte da verdade. `selectedId` (singular) virou
  // um getter derivado que aponta sempre pra primeira seleção — assim
  // todo o código antigo que lê `selectedId` continua funcionando
  // (Inspector mostra o primeiro, etc.), e Delete/Duplicate operam em
  // TODOS via `selectedIds`. Marquee preenche o array inteiro.
  const [selectedIds, setSelectedIdsRaw] = useState<string[]>([]);
  const [marquee, setMarquee] = useState<Marquee | null>(null);
  const [pending, setPending] = useState<PendingTwoClick | null>(null);
  const [roadDraft, setRoadDraft] = useState<RoadDraft | null>(null);
  const [viewport, setViewport] = useState<Viewport>(DEFAULT_VIEWPORT);
  const [pointerWorld, setPointerWorld] = useState<SicroPoint>({ x: 0, y: 0 });
  /**
   * Undo/redo stacks via REFS — leitura síncrona dentro do mesmo event
   * tick. Antes usávamos useState com `setHistory((h) => { popped = ...; })`
   * tentando retornar `popped` logo depois — mas no React 18 o updater
   * é enfileirado, então `popped` continuava `null` quando o caller lia
   * e o undo nunca aplicava nada (Ctrl+Z parecia quebrado). Com ref a
   * mutação é instantânea.
   *
   * O `historyTick` é um contador useState que incrementamos a cada
   * push/pop pra forçar re-render dos consumidores que olham comprimento
   * (`canUndo={editor.history.length > 0}` na Toolbar, por ex).
   */
  const historyRef = useRef<SicroObject[][]>([]);
  const redoRef = useRef<SicroObject[][]>([]);
  const [, setHistoryTick] = useState(0);
  const bumpTick = useCallback(() => setHistoryTick((v) => v + 1), []);

  const pushHistory = useCallback(
    (snapshot: SicroObject[]) => {
      const cur = historyRef.current;
      // Dedup: se o último snapshot é o MESMO REFERENCE, não duplica.
      // Necessário pq React.StrictMode (dev) roda os updaters do setDoc
      // duas vezes — o `pushHistory` é chamado de dentro do updater do
      // `mutateObjects`, então sem dedup teríamos 2 entradas idênticas
      // e cada Ctrl+Z só desfaz "metade" (aparente no-op).
      if (cur.length > 0 && cur[cur.length - 1] === snapshot) return;
      const next = [...cur, snapshot];
      // Keep the stack bounded — spike, not a production editor.
      if (next.length > 50) next.shift();
      historyRef.current = next;
      // Any new mutation invalidates the redo branch.
      redoRef.current = [];
      bumpTick();
    },
    [bumpTick],
  );

  const popHistory = useCallback((): SicroObject[] | null => {
    const h = historyRef.current;
    if (h.length === 0) return null;
    const popped = h[h.length - 1] ?? null;
    historyRef.current = h.slice(0, -1);
    bumpTick();
    return popped;
  }, [bumpTick]);

  const pushRedo = useCallback(
    (snapshot: SicroObject[]) => {
      const next = [...redoRef.current, snapshot];
      if (next.length > 50) next.shift();
      redoRef.current = next;
      bumpTick();
    },
    [bumpTick],
  );
  const popRedo = useCallback((): SicroObject[] | null => {
    const r = redoRef.current;
    if (r.length === 0) return null;
    const popped = r[r.length - 1] ?? null;
    redoRef.current = r.slice(0, -1);
    bumpTick();
    return popped;
  }, [bumpTick]);

  // Exposed as plain arrays (read-only). Re-render trigger é o tick.
  const history = historyRef.current;
  const redoStack = redoRef.current;

  // Setter "compat" que aceita uma string ou null — equivalente a
  // selecionar exatamente um (ou limpar).
  const setSelectedId = useCallback((id: string | null) => {
    setSelectedIdsRaw(id ? [id] : []);
  }, []);
  // Setter "multi" — usado pelo marquee e por shortcuts futuros.
  const setSelectedIds = useCallback((ids: string[]) => {
    // Dedup defensivo (marquee não deveria repetir, mas garante).
    setSelectedIdsRaw(Array.from(new Set(ids)));
  }, []);
  // Getter derivado: primeira seleção. Inspector e outros lugares que
  // só sabem operar em 1 objeto continuam funcionando lendo isso.
  const selectedId: string | null = selectedIds[0] ?? null;

  return {
    tool,
    setTool,
    selectedId,
    selectedIds,
    setSelectedId,
    setSelectedIds,
    marquee,
    setMarquee,
    pending,
    setPending,
    roadDraft,
    setRoadDraft,
    viewport,
    setViewport,
    pointerWorld,
    setPointerWorld,
    history,
    pushHistory,
    popHistory,
    redoStack,
    pushRedo,
    popRedo,
  };
}

export type EditorState = ReturnType<typeof useEditorState>;
