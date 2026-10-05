/** O que o trilho e a prateleira mostram. Ferramentas diretas, grupos com itens e grupos de ações. */

import type { ReactNode } from "react";
import {
  Car,
  Crosshair,
  Hand,
  Image as ImageIcon,
  Minus,
  MoreHorizontal,
  MousePointer2,
  PenLine,
  PersonStanding,
  Route,
  Ruler,
  Siren,
  TrafficCone,
  Undo2,
  type LucideIcon,
} from "lucide-react";
import type { Tool } from "../useEditorState";
import type { MarkerSubtype, VehicleBodyType } from "../../engine";
import { getVehicleRealDims } from "../../engine/vehicleArt";
import { MarkerThumb, PessoaThumb, SvgThumb, VehicleThumb, roadThumb } from "./thumbs";

export interface PaletteItem {
  tool: Tool;
  label: string;
  thumb: ReactNode;
}

export type RailEntry =
  | { kind: "tool"; tool: Tool; label: string; icon: LucideIcon; hint?: string }
  | { kind: "group"; id: string; label: string; icon: LucideIcon; items: PaletteItem[] }
  | { kind: "actions"; id: string; label: string; icon: LucideIcon }
  | { kind: "sep" };

const metros = (m: number) => `${m.toFixed(1).replace(".", ",")} m`;

const veiculo = (tool: Tool, body: VehicleBodyType, label: string): PaletteItem => {
  const dims = getVehicleRealDims(body);
  return { tool, label: dims ? `${label} · ${metros(dims.lengthM)}` : label, thumb: <VehicleThumb body={body} /> };
};
const marcador = (tool: Tool, subtype: MarkerSubtype, label: string): PaletteItem => ({
  tool,
  label,
  thumb: <MarkerThumb subtype={subtype} />,
});
const pessoa = (tool: Tool, subtype: string, label: string): PaletteItem => ({
  tool,
  label,
  thumb: <PessoaThumb subtype={subtype} />,
});

const amarelaTrac = <path d="M0 20H64" stroke="#d6a200" strokeWidth="2" strokeDasharray="7 5" />;
const amarelaDupla = <path d="M0 18.5H64M0 21.5H64" stroke="#d6a200" strokeWidth="1.6" />;

export const RAIL: RailEntry[] = [
  { kind: "tool", tool: "select", label: "Selecionar", icon: MousePointer2, hint: "V" },
  { kind: "tool", tool: "pan", label: "Pan", icon: Hand, hint: "H" },
  { kind: "tool", tool: "measurement", label: "Cota", icon: Ruler },
  { kind: "tool", tool: "set_scale", label: "Escala", icon: Crosshair },
  { kind: "tool", tool: "line_r1", label: "R1", icon: Minus },
  { kind: "tool", tool: "line_r2", label: "R2", icon: Minus },
  { kind: "sep" },
  {
    kind: "group",
    id: "via",
    label: "Via",
    icon: Route,
    items: [
      { tool: "road_urban", label: "Via urbana · 10 m", thumb: roadThumb(18, amarelaTrac) },
      { tool: "road_avenue", label: "Avenida · 14 m", thumb: roadThumb(26, <>{amarelaDupla}<path d="M0 13H64M0 27H64" stroke="#fff" strokeWidth="1.4" strokeDasharray="5 5" /></>) },
      { tool: "road_highway", label: "Rodovia", thumb: roadThumb(22, <>{amarelaDupla}<path d="M0 11.5H64M0 28.5H64" stroke="#fff" strokeWidth="1.4" /></>) },
      { tool: "road_dirt", label: "Estrada de terra", thumb: roadThumb(16, null, "#d9c6a5") },
      { tool: "road_parking", label: "Estacionamento", thumb: (<SvgThumb><rect x="4" y="6" width="56" height="28" fill="#e6e6e6" stroke="#111" /><path d="M15 6v28M26 6v28M37 6v28M48 6v28" stroke="#fff" strokeWidth="2" /></SvgThumb>) },
      { tool: "roundabout", label: "Rotatória · r 15 m", thumb: (<SvgThumb><circle cx="32" cy="20" r="17" fill="#e6e6e6" stroke="#111" strokeWidth="1.5" /><circle cx="32" cy="20" r="8" fill="#d9e8cf" stroke="#111" strokeWidth="1.5" /></SvgThumb>) },
    ],
  },
  {
    kind: "group",
    id: "veiculo",
    label: "Veículos",
    icon: Car,
    items: [
      veiculo("vehicle_sedan", "sedan", "Sedan"),
      veiculo("vehicle_hatch", "hatch", "Hatch"),
      veiculo("vehicle_suv", "suv", "SUV"),
      veiculo("vehicle_pickup", "pickup", "Pickup"),
      veiculo("vehicle_van", "van", "Van passageiro"),
      veiculo("vehicle_van_furgao", "van_furgao", "Van furgão"),
      veiculo("vehicle_onibus", "onibus", "Ônibus"),
      veiculo("vehicle_micro_onibus", "micro_onibus", "Micro-ônibus"),
      veiculo("vehicle_onibus_leito", "onibus_leito", "Ônibus leito"),
      veiculo("vehicle_truck", "truck", "Caminhão leve"),
      veiculo("vehicle_caminhao_pesado", "caminhao_pesado", "Caminhão pesado"),
      veiculo("vehicle_carreta", "carreta", "Carreta"),
      veiculo("vehicle_reboque_guincho", "reboque_guincho", "Reboque guincho"),
      veiculo("vehicle_trator", "trator", "Trator"),
      veiculo("vehicle_moto", "moto", "Moto urbana"),
      veiculo("vehicle_moto_esportiva", "moto_esportiva", "Moto esportiva"),
      veiculo("vehicle_moto_carga", "moto_carga", "Moto carga"),
      veiculo("vehicle_bike", "bike", "Bicicleta urbana"),
      veiculo("vehicle_bike_estrada", "bike_estrada", "Bicicleta estrada"),
      veiculo("vehicle_bike_cargueira", "bike_cargueira", "Bicicleta cargueira"),
    ],
  },
  {
    kind: "group",
    id: "viatura",
    label: "Viaturas",
    icon: Siren,
    items: [
      veiculo("vehicle_vtr_pm", "vtr_pm", "VTR PM"),
      veiculo("vehicle_vtr_pc", "vtr_pc", "VTR PC"),
      veiculo("vehicle_vtr_pci", "vtr_pci", "VTR Pol. Científica"),
      veiculo("vehicle_vtr_bm", "vtr_bm", "VTR Bombeiros"),
      veiculo("vehicle_vtr_pp", "vtr_pp", "VTR PP"),
      veiculo("vehicle_ambulancia", "ambulancia", "Ambulância"),
      veiculo("vehicle_taxi", "taxi", "Táxi"),
    ],
  },
  {
    kind: "group",
    id: "vestigio",
    label: "Vestígios",
    icon: MoreHorizontal,
    items: [
      marcador("marker_x", "collision_x", "Ponto de colisão"),
      marcador("marker_rest_position", "rest_position", "Repouso final"),
      marcador("marker_brake", "brake_mark", "Frenagem"),
      marcador("marker_drag", "drag_mark", "Arrasto"),
      marcador("marker_skid_curve", "skid_curve", "Derrapagem em curva"),
      marcador("marker_sulcagem", "sulcagem", "Sulcagem"),
      marcador("marker_ranhura", "ranhura", "Ranhura"),
      marcador("marker_debris", "debris", "Fragmentos"),
      marcador("marker_fluid", "fluid", "Fluido"),
      marcador("marker_blood", "blood", "Sangue"),
      marcador("marker_impact_area", "impact_area", "Área de impacto"),
    ],
  },
  {
    kind: "group",
    id: "mobiliario",
    label: "Mobiliário",
    icon: TrafficCone,
    items: [
      marcador("marker_semaforo", "semaforo", "Semáforo"),
      marcador("marker_placa_pare", "placa_pare", "Placa PARE"),
      marcador("marker_placa_preferencia", "placa_preferencia", "Placa Preferência"),
      marcador("marker_poste", "poste", "Poste"),
      marcador("marker_arvore", "arvore", "Árvore"),
      marcador("marker_guia", "guia", "Guia / meio-fio"),
      marcador("marker_faixa_pedestre", "faixa_pedestre", "Faixa de pedestres"),
    ],
  },
  {
    kind: "group",
    id: "pessoa",
    label: "Pessoas",
    icon: PersonStanding,
    items: [
      pessoa("marker_pedestre_m_dorsal", "pedestre_m_dorsal", "Vítima M · dorsal"),
      pessoa("marker_pedestre_m_lateral", "pedestre_m_lateral", "Vítima M · lateral"),
      pessoa("marker_pedestre_m_ventral", "pedestre_m_ventral", "Vítima M · ventral"),
      pessoa("marker_pedestre_f_dorsal", "pedestre_f_dorsal", "Vítima F · dorsal"),
      pessoa("marker_pedestre_f_lateral", "pedestre_f_lateral", "Vítima F · lateral"),
      pessoa("marker_pedestre_f_ventral", "pedestre_f_ventral", "Vítima F · ventral"),
    ],
  },
  {
    kind: "group",
    id: "anotacao",
    label: "Anotação",
    icon: PenLine,
    items: [
      { tool: "text", label: "Texto / etiqueta", thumb: (<SvgThumb><text x="32" y="27" textAnchor="middle" fontFamily="Arial" fontWeight="700" fontSize="20" fill="#111827">Aa</text></SvgThumb>) },
      { tool: "line_callout", label: "Chamada", thumb: (<SvgThumb><path d="M6 33 26 18" stroke="#0ea5e9" strokeWidth="1.5" strokeDasharray="4 3" /><rect x="26" y="8" width="32" height="14" rx="2" fill="#fff" stroke="#0ea5e9" /><path d="M31 15h22" stroke="#0ea5e9" strokeWidth="2" /></SvgThumb>) },
      { tool: "line_arrow", label: "Seta direcional", thumb: (<SvgThumb><path d="M6 20h40" stroke="#111827" strokeWidth="3" /><path d="M44 11l14 9-14 9z" fill="#111827" /></SvgThumb>) },
      { tool: "line_trajetoria", label: "Trajetória", thumb: (<SvgThumb><path d="M4 32C20 6 40 34 60 10" stroke="#2563eb" strokeWidth="3" fill="none" strokeDasharray="6 4" /></SvgThumb>) },
    ],
  },
  { kind: "sep" },
  { kind: "actions", id: "imagem", label: "Imagem", icon: ImageIcon },
  { kind: "actions", id: "editar", label: "Editar", icon: Undo2 },
];

/** Grupo (id) a que a ferramenta pertence, para acender o trilho. */
export function groupOfTool(tool: Tool): string | null {
  for (const e of RAIL) {
    if (e.kind === "group" && e.items.some((i) => i.tool === tool)) return e.id;
  }
  return null;
}
