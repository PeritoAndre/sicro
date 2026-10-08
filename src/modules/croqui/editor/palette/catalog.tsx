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
  Signpost,
  Siren,
  Trees,
  Undo2,
  type LucideIcon,
} from "lucide-react";
import type { Tool } from "../useEditorState";
import type { FixtureSubtype, PersonPosicao, TraceSubtype, VehicleBodyType } from "../../engine";
import { getVehicleRealDims } from "../../engine/vehicleArt";
import { FixtureThumb, PersonThumb, SvgThumb, TraceThumb, VehicleThumb, roadThumb } from "./thumbs";

export interface PaletteItem {
  tool: Tool;
  label: string;
  thumb: ReactNode;
  /** Ação de atalho (keymap) que ativa este item; a letra aparece na miniatura. */
  action?: string;
}

export type RailEntry =
  | { kind: "tool"; tool: Tool; label: string; icon: LucideIcon; action?: string }
  | { kind: "group"; id: string; label: string; icon: LucideIcon; items: PaletteItem[]; action?: string }
  | { kind: "actions"; id: string; label: string; icon: LucideIcon }
  | { kind: "sep" };

const metros = (m: number) => `${m.toFixed(1).replace(".", ",")} m`;

const veiculo = (tool: Tool, body: VehicleBodyType, label: string): PaletteItem => {
  const dims = getVehicleRealDims(body);
  return { tool, label: dims ? `${label} · ${metros(dims.lengthM)}` : label, thumb: <VehicleThumb body={body} /> };
};
const elemento = (subtype: FixtureSubtype, label: string): PaletteItem => ({
  tool: `fixture_${subtype}` as Tool,
  label,
  thumb: <FixtureThumb subtype={subtype} />,
});
const vestigio = (subtype: TraceSubtype, label: string): PaletteItem => ({
  tool: `trace_${subtype}` as Tool,
  label,
  thumb: <TraceThumb subtype={subtype} />,
});
const pessoa = (posicao: PersonPosicao, label: string): PaletteItem => ({
  tool: `person_${posicao}` as Tool,
  label,
  thumb: <PersonThumb posicao={posicao} />,
});

const amarelaTrac = <path d="M0 20H64" stroke="#d6a200" strokeWidth="2" strokeDasharray="7 5" />;
const amarelaDupla = <path d="M0 18.5H64M0 21.5H64" stroke="#d6a200" strokeWidth="1.6" />;

export const RAIL: RailEntry[] = [
  { kind: "tool", tool: "select", label: "Selecionar", icon: MousePointer2, action: "croqui.tool.select" },
  { kind: "tool", tool: "pan", label: "Pan", icon: Hand, action: "croqui.tool.pan" },
  { kind: "tool", tool: "measurement", label: "Cota", icon: Ruler, action: "croqui.tool.measure" },
  { kind: "tool", tool: "set_scale", label: "Escala", icon: Crosshair, action: "croqui.tool.scale" },
  { kind: "tool", tool: "line_r1", label: "R1", icon: Minus, action: "croqui.tool.r1" },
  { kind: "tool", tool: "line_r2", label: "R2", icon: Minus, action: "croqui.tool.r2" },
  { kind: "sep" },
  {
    kind: "group",
    id: "via",
    action: "croqui.tool.roadUrban",
    label: "Via",
    icon: Route,
    items: [
      { tool: "road_urban", action: "croqui.tool.roadUrban", label: "Via urbana · 10 m", thumb: roadThumb(18, amarelaTrac) },
      { tool: "road_avenue", action: "croqui.tool.roadAvenue", label: "Avenida · 14 m", thumb: roadThumb(26, <>{amarelaDupla}<path d="M0 13H64M0 27H64" stroke="#fff" strokeWidth="1.4" strokeDasharray="5 5" /></>) },
      { tool: "road_highway", action: "croqui.tool.roadHighway", label: "Rodovia", thumb: roadThumb(22, <>{amarelaDupla}<path d="M0 11.5H64M0 28.5H64" stroke="#fff" strokeWidth="1.4" /></>) },
      { tool: "road_dirt", action: "croqui.tool.roadDirt", label: "Estrada de terra", thumb: roadThumb(16, null, "#d9c6a5") },
      { tool: "road_parking", action: "croqui.tool.roadParking", label: "Estacionamento", thumb: (<SvgThumb><rect x="4" y="6" width="56" height="28" fill="#e6e6e6" stroke="#111" /><path d="M15 6v28M26 6v28M37 6v28M48 6v28" stroke="#fff" strokeWidth="2" /></SvgThumb>) },
      { tool: "roundabout", action: "croqui.tool.roundabout", label: "Rotatória · r 15 m", thumb: (<SvgThumb><circle cx="32" cy="20" r="17" fill="#e6e6e6" stroke="#111" strokeWidth="1.5" /><circle cx="32" cy="20" r="8" fill="#d9e8cf" stroke="#111" strokeWidth="1.5" /></SvgThumb>) },
    ],
  },
  {
    kind: "group",
    id: "veiculo",
    action: "croqui.tool.vehicle",
    label: "Veículos",
    icon: Car,
    items: [
      { ...veiculo("vehicle_sedan", "sedan", "Sedan"), action: "croqui.tool.vehicle" },
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
    action: "croqui.tool.vestigio",
    label: "Vestígios",
    icon: MoreHorizontal,
    items: [
      vestigio("frenagem", "Frenagem"),
      vestigio("derrapagem", "Derrapagem em curva"),
      vestigio("arrasto", "Arrasto"),
      vestigio("sulcagem", "Sulcagem"),
      vestigio("ranhura", "Ranhuras"),
      vestigio("fluido", "Fluido"),
      vestigio("fragmentos", "Fragmentos"),
      { ...vestigio("colisao", "Ponto de colisão"), action: "croqui.tool.vestigio" },
    ],
  },
  {
    kind: "group",
    id: "sinalizacao",
    action: "croqui.tool.mobiliario",
    label: "Sinalização",
    icon: Signpost,
    items: [
      { ...elemento("placa", "Placa"), action: "croqui.tool.mobiliario" },
      elemento("semaforo", "Semáforo"),
      elemento("faixa_pedestre", "Faixa de pedestres"),
      elemento("retencao", "Linha de retenção"),
      elemento("lombada", "Lombada"),
      elemento("area_conflito", "Área de conflito (não bloqueie)"),
      elemento("seta", "Seta no pavimento"),
    ],
  },
  {
    kind: "group",
    id: "entorno",
    label: "Entorno",
    icon: Trees,
    items: [
      elemento("poste", "Poste"),
      elemento("arvore", "Árvore"),
      elemento("hidrante", "Hidrante"),
      elemento("abrigo", "Ponto de ônibus"),
      elemento("barreira", "Barreira / muro"),
      elemento("obstaculo", "Obstáculo"),
      elemento("camera", "Câmera"),
    ],
  },
  {
    kind: "group",
    id: "pessoa",
    action: "croqui.tool.pessoa",
    label: "Pessoas",
    icon: PersonStanding,
    items: [
      { ...pessoa("dorsal", "Deitada · dorsal"), action: "croqui.tool.pessoa" },
      pessoa("ventral", "Deitada · ventral"),
      pessoa("lat_e", "De lado · esquerdo"),
      pessoa("lat_d", "De lado · direito"),
      pessoa("empe", "Em pé"),
    ],
  },
  {
    kind: "group",
    id: "anotacao",
    action: "croqui.tool.text",
    label: "Anotação",
    icon: PenLine,
    items: [
      { tool: "text", action: "croqui.tool.text", label: "Texto / etiqueta", thumb: (<SvgThumb><text x="32" y="27" textAnchor="middle" fontFamily="Arial" fontWeight="700" fontSize="20" fill="#111827">Aa</text></SvgThumb>) },
      { tool: "line_callout", action: "croqui.tool.callout", label: "Chamada", thumb: (<SvgThumb><path d="M6 33 26 18" stroke="#0ea5e9" strokeWidth="1.5" strokeDasharray="4 3" /><rect x="26" y="8" width="32" height="14" rx="2" fill="#fff" stroke="#0ea5e9" /><path d="M31 15h22" stroke="#0ea5e9" strokeWidth="2" /></SvgThumb>) },
      { tool: "line_arrow", action: "croqui.tool.arrow", label: "Seta direcional", thumb: (<SvgThumb><path d="M6 20h40" stroke="#111827" strokeWidth="3" /><path d="M44 11l14 9-14 9z" fill="#111827" /></SvgThumb>) },
      { tool: "line_trajetoria", action: "croqui.tool.trajectory", label: "Trajetória", thumb: (<SvgThumb><path d="M4 32C20 6 40 34 60 10" stroke="#2563eb" strokeWidth="3" fill="none" strokeDasharray="6 4" /></SvgThumb>) },
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
