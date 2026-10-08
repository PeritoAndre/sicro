/**
 * Estado transitório do editor (ferramenta, seleção, viewport, rascunhos,
 * undo/redo). O doc persistido vive no `croquiStore`.
 */

import { useCallback, useRef, useState } from "react";
import type { SicroObject, SicroPoint } from "../engine";

/** Uma string por ferramenta (em vez de structs) para a toolbar mapear direto. */
export type Tool =
  | "select"
  | "pan"
  // Veículos
  | "vehicle"            // alias antigo de "vehicle_car"
  | "vehicle_car"
  | "vehicle_sedan"
  | "vehicle_suv"
  | "vehicle_hatch"
  | "vehicle_moto"
  | "vehicle_truck"
  | "vehicle_bike"
  | "vehicle_pickup"
  | "vehicle_van"
  | "vehicle_onibus"
  | "vehicle_moto_esportiva"
  | "vehicle_moto_carga"
  | "vehicle_caminhao_pesado"
  | "vehicle_carreta"
  | "vehicle_van_furgao"
  | "vehicle_micro_onibus"
  | "vehicle_onibus_leito"
  | "vehicle_reboque_guincho"
  | "vehicle_trator"
  | "vehicle_bike_estrada"
  | "vehicle_bike_cargueira"
  // Viaturas/especiais (pintura fixa)
  | "vehicle_ambulancia"
  | "vehicle_taxi"
  | "vehicle_vtr_pm"
  | "vehicle_vtr_pc"
  | "vehicle_vtr_pci"
  | "vehicle_vtr_bm"
  | "vehicle_vtr_pp"
  // Pedestres em decúbito
  | "marker_pedestre_m_dorsal"
  | "marker_pedestre_m_lateral"
  | "marker_pedestre_m_ventral"
  | "marker_pedestre_f_dorsal"
  | "marker_pedestre_f_lateral"
  | "marker_pedestre_f_ventral"
  // Linhas
  | "line_road"
  | "line_lane"
  | "line_lane_separator"
  | "line_sidewalk"
  | "line_arrow"
  | "line_r1"
  | "line_r2"
  | "line_canteiro"
  | "line_acostamento"
  | "line_trajetoria"
  | "line_callout"
  // Vestígios paramétricos
  | "trace_frenagem"
  | "trace_derrapagem"
  | "trace_arrasto"
  | "trace_sulcagem"
  | "trace_ranhura"
  | "trace_fluido"
  | "trace_fragmentos"
  | "trace_colisao"
  // Pessoas articuladas
  | "person_dorsal"
  | "person_ventral"
  | "person_lat_e"
  | "person_lat_d"
  | "person_empe"
  // Sinalização e entorno paramétricos
  | "fixture_placa"
  | "fixture_semaforo"
  | "fixture_faixa_pedestre"
  | "fixture_retencao"
  | "fixture_lombada"
  | "fixture_area_conflito"
  | "fixture_seta"
  | "fixture_poste"
  | "fixture_arvore"
  | "fixture_hidrante"
  | "fixture_abrigo"
  | "fixture_barreira"
  | "fixture_obstaculo"
  | "fixture_camera"
  // Marcadores (colisão, vestígios, pessoas, mobiliário)
  | "marker_x"
  | "marker_brake"
  | "marker_drag"
  | "marker_fluid"
  | "marker_blood"
  | "marker_debris"
  | "marker_pedestrian"
  | "marker_body"
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
  // Anotação / medição / escala
  | "text"
  | "measurement"
  | "set_scale"
  // Vias multi-click, uma ferramenta por estilo
  | "road_urban"
  | "road_avenue"
  | "road_highway"
  | "road_dirt"
  | "road_parking"
  // Um clique insere uma rotatória default no ponto
  | "roundabout";

/** Primeiro clique de uma ferramenta de dois cliques (medição / escala / linha). */
interface PendingTwoClick {
  tool: Tool;
  first: SicroPoint;
}

/** Via em rascunho: pontos acumulados até Enter / duplo clique (Esc cancela). */
interface RoadDraft {
  tool: Tool;
  points: SicroPoint[];
}

interface Viewport {
  /** Zoom; 1 = 100%. */
  scale: number;
  /** Pan, em coordenadas da stage. */
  x: number;
  y: number;
}

export const DEFAULT_VIEWPORT: Viewport = { scale: 1, x: 0, y: 0 };

/** Marquee em curso, em coordenadas world (viewport aplicado). */
interface Marquee {
  startWorldX: number;
  startWorldY: number;
  currentWorldX: number;
  currentWorldY: number;
}

export function useEditorState() {
  const [tool, setTool] = useState<Tool>("select");
  // `selectedIds` é a fonte da verdade; `selectedId` é derivado (o primeiro)
  // para o código que só opera em um objeto. Delete/Duplicate usam o array.
  const [selectedIds, setSelectedIdsRaw] = useState<string[]>([]);
  const [marquee, setMarquee] = useState<Marquee | null>(null);
  const [pending, setPending] = useState<PendingTwoClick | null>(null);
  const [roadDraft, setRoadDraft] = useState<RoadDraft | null>(null);
  const [viewport, setViewport] = useState<Viewport>(DEFAULT_VIEWPORT);
  const [pointerWorld, setPointerWorld] = useState<SicroPoint>({ x: 0, y: 0 });
  // Pilhas em refs: o caller lê o pop no mesmo tick. Com useState o updater
  // é enfileirado (React 18) e o undo lia `null`. O `historyTick` só força
  // re-render de quem olha o comprimento (canUndo na Toolbar).
  const historyRef = useRef<SicroObject[][]>([]);
  const redoRef = useRef<SicroObject[][]>([]);
  const [, setHistoryTick] = useState(0);
  const bumpTick = useCallback(() => setHistoryTick((v) => v + 1), []);

  const pushHistory = useCallback(
    (snapshot: SicroObject[]) => {
      const cur = historyRef.current;
      // Dedup por referência: StrictMode roda o updater do setDoc duas vezes
      // e sem isso cada Ctrl+Z desfazia só "metade".
      if (cur.length > 0 && cur[cur.length - 1] === snapshot) return;
      const next = [...cur, snapshot];
      if (next.length > 50) next.shift();
      historyRef.current = next;
      // Mutação nova invalida o redo.
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

  const history = historyRef.current;
  const redoStack = redoRef.current;

  const setSelectedId = useCallback((id: string | null) => {
    setSelectedIdsRaw(id ? [id] : []);
  }, []);
  const setSelectedIds = useCallback((ids: string[]) => {
    setSelectedIdsRaw(Array.from(new Set(ids)));
  }, []);
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
