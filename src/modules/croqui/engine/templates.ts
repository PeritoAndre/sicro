/**
 * Road templates (MVP 6).
 *
 * A template is a function that, given an anchor point in canvas
 * coordinates, returns a *set of editable objects* the perito can move
 * and customise. Templates are not images and not frozen groups — they
 * just save the perito from drawing common road skeletons by hand.
 *
 * Every template stays inside `category: "vias"` so the layer panel can
 * group them. Lines are emitted as `SicroLineObject` (subtype road /
 * lane / lane_separator / sidewalk) so the rest of the engine keeps
 * working — no special-case rendering required.
 *
 * Fase S clean cut — Os antigos templates `via_pro_*` (que dependiam de
 * `makeRoad` + `SicroRoadObject` v1) foram REMOVIDOS. Esses templates
 * usavam o motor Road v1 que não existe mais. A criação de vias com
 * acabamento de via real agora é feita exclusivamente pela ferramenta
 * **Criar Via** (Python Parity Engine — vias com Bezier 4-point +
 * largura em metros).
 *
 * Os templates remanescentes são todos **line-based** — emitem
 * `SicroLineObject` (e nada mais). São úteis como esboço/esqueleto
 * rápido, complementando o motor parity (que cuida das vias "de
 * verdade").
 */

import type {
  SicroLineObject,
  SicroObject,
  SicroPoint,
} from "./schema";
import { makeLine } from "./factories";

export type TemplateId =
  | "via_reta"
  | "cruzamento_x"
  | "cruzamento_t"
  | "mao_dupla"
  | "mao_unica"
  | "rotatoria_simples"
  | "curva_simples"
  // MVP 9 — modelos avançados (linhas soltas)
  | "avenida_canteiro"
  | "cruzamento_y"
  | "curva_esquerda"
  | "curva_direita"
  | "faixa_pedestre_via"
  | "via_acostamento";

export interface RoadTemplate {
  id: TemplateId;
  label: string;
  description: string;
  build(anchor: SicroPoint): SicroObject[];
}

const DEFAULT_LANE_WIDTH = 60; // px (~3.5 m com ~17 px/m)

/** Single-lane straight road (one road centerline). */
export const TEMPLATES: Record<TemplateId, RoadTemplate> = {
  via_reta: {
    id: "via_reta",
    label: "Via reta (esqueleto)",
    description:
      "Segmento reto horizontal com bordas e divisão central tracejada. Esboço rápido em linhas — use 'Criar Via' para via parity completa.",
    build(anchor) {
      const length = 400;
      const half = DEFAULT_LANE_WIDTH;
      const xL = anchor.x - length / 2;
      const xR = anchor.x + length / 2;
      const yT = anchor.y - half;
      const yB = anchor.y + half;
      return [
        roadEdge({ x: xL, y: yT }, { x: xR, y: yT }),
        roadEdge({ x: xL, y: yB }, { x: xR, y: yB }),
        laneSeparator({ x: xL, y: anchor.y }, { x: xR, y: anchor.y }),
      ];
    },
  },

  cruzamento_x: {
    id: "cruzamento_x",
    label: "Cruzamento em X (esqueleto)",
    description: "Dois eixos perpendiculares com via dupla cada.",
    build(anchor) {
      const arm = 240;
      const half = DEFAULT_LANE_WIDTH;
      const objs: SicroObject[] = [
        // Eixo horizontal — duas bordas + divisão
        roadEdge(
          { x: anchor.x - arm, y: anchor.y - half },
          { x: anchor.x + arm, y: anchor.y - half },
        ),
        roadEdge(
          { x: anchor.x - arm, y: anchor.y + half },
          { x: anchor.x + arm, y: anchor.y + half },
        ),
        laneSeparator(
          { x: anchor.x - arm, y: anchor.y },
          { x: anchor.x + arm, y: anchor.y },
        ),
        // Eixo vertical
        roadEdge(
          { x: anchor.x - half, y: anchor.y - arm },
          { x: anchor.x - half, y: anchor.y + arm },
        ),
        roadEdge(
          { x: anchor.x + half, y: anchor.y - arm },
          { x: anchor.x + half, y: anchor.y + arm },
        ),
        laneSeparator(
          { x: anchor.x, y: anchor.y - arm },
          { x: anchor.x, y: anchor.y + arm },
        ),
      ];
      return objs;
    },
  },

  cruzamento_t: {
    id: "cruzamento_t",
    label: "Cruzamento em T (esqueleto)",
    description: "Via horizontal contínua com derivação vertical descendente.",
    build(anchor) {
      const arm = 240;
      const half = DEFAULT_LANE_WIDTH;
      return [
        // Horizontal
        roadEdge(
          { x: anchor.x - arm, y: anchor.y - half },
          { x: anchor.x + arm, y: anchor.y - half },
        ),
        roadEdge(
          { x: anchor.x - arm, y: anchor.y + half },
          { x: anchor.x + arm, y: anchor.y + half },
        ),
        laneSeparator(
          { x: anchor.x - arm, y: anchor.y },
          { x: anchor.x + arm, y: anchor.y },
        ),
        // Vertical descendente (a partir do eixo inferior)
        roadEdge(
          { x: anchor.x - half, y: anchor.y + half },
          { x: anchor.x - half, y: anchor.y + arm },
        ),
        roadEdge(
          { x: anchor.x + half, y: anchor.y + half },
          { x: anchor.x + half, y: anchor.y + arm },
        ),
        laneSeparator(
          { x: anchor.x, y: anchor.y + half },
          { x: anchor.x, y: anchor.y + arm },
        ),
      ];
    },
  },

  mao_dupla: {
    id: "mao_dupla",
    label: "Via de mão dupla (esqueleto)",
    description:
      "Como `via_reta`, mas com divisória dupla contínua — destaca-se a separação de fluxos.",
    build(anchor) {
      const length = 400;
      const half = DEFAULT_LANE_WIDTH;
      const xL = anchor.x - length / 2;
      const xR = anchor.x + length / 2;
      const yT = anchor.y - half;
      const yB = anchor.y + half;
      const sep = 4;
      return [
        roadEdge({ x: xL, y: yT }, { x: xR, y: yT }),
        roadEdge({ x: xL, y: yB }, { x: xR, y: yB }),
        solidLane(
          { x: xL, y: anchor.y - sep },
          { x: xR, y: anchor.y - sep },
        ),
        solidLane(
          { x: xL, y: anchor.y + sep },
          { x: xR, y: anchor.y + sep },
        ),
      ];
    },
  },

  mao_unica: {
    id: "mao_unica",
    label: "Via de mão única (esqueleto)",
    description:
      "Via reta horizontal sem divisória central, com seta indicando o sentido.",
    build(anchor) {
      const length = 360;
      const half = DEFAULT_LANE_WIDTH;
      const xL = anchor.x - length / 2;
      const xR = anchor.x + length / 2;
      const yT = anchor.y - half;
      const yB = anchor.y + half;
      return [
        roadEdge({ x: xL, y: yT }, { x: xR, y: yT }),
        roadEdge({ x: xL, y: yB }, { x: xR, y: yB }),
        arrowSegment(
          { x: anchor.x - 60, y: anchor.y },
          { x: anchor.x + 60, y: anchor.y },
        ),
      ];
    },
  },

  rotatoria_simples: {
    id: "rotatoria_simples",
    label: "Rotatória octogonal (esqueleto)",
    description: "Octógono regular como aproximação técnica de uma rotatória. Use 'Criar Rotatória' para rotatória parity com anel.",
    build(anchor) {
      const radius = 90;
      const sides = 8;
      const points: SicroPoint[] = Array.from({ length: sides }, (_, i) => {
        const ang = (i / sides) * Math.PI * 2 - Math.PI / 2;
        return {
          x: anchor.x + Math.cos(ang) * radius,
          y: anchor.y + Math.sin(ang) * radius,
        };
      });
      const out: SicroObject[] = [];
      for (let i = 0; i < points.length; i++) {
        const a = points[i]!;
        const b = points[(i + 1) % points.length]!;
        out.push(roadEdge(a, b));
      }
      return out;
    },
  },

  curva_simples: {
    id: "curva_simples",
    label: "Curva simples (esqueleto)",
    description:
      "Curva discretizada em 8 segmentos (~90°) para representar uma curva moderada.",
    build(anchor) {
      const radius = 200;
      const segments = 8;
      const startAngle = Math.PI;
      const endAngle = Math.PI / 2; // 180° → 90°
      const half = DEFAULT_LANE_WIDTH;
      const outer = sampleArc(anchor, radius + half, startAngle, endAngle, segments);
      const inner = sampleArc(anchor, radius - half, startAngle, endAngle, segments);
      const out: SicroObject[] = [];
      for (let i = 0; i < segments; i++) {
        out.push(roadEdge(outer[i]!, outer[i + 1]!));
        out.push(roadEdge(inner[i]!, inner[i + 1]!));
      }
      return out;
    },
  },

  // -------------------------------------------------------------------------
  // MVP 9 — modelos avançados

  avenida_canteiro: {
    id: "avenida_canteiro",
    label: "Avenida com canteiro central",
    description:
      "Duas pistas separadas por canteiro central. Cada pista é uma via reta com divisão tracejada própria.",
    build(anchor) {
      const length = 500;
      const lane = DEFAULT_LANE_WIDTH;
      const canteiro = 24;
      const xL = anchor.x - length / 2;
      const xR = anchor.x + length / 2;
      const pistaNorte = anchor.y - canteiro / 2 - lane;
      const pistaSul = anchor.y + canteiro / 2 + lane;
      return [
        // Borda externa norte
        roadEdge({ x: xL, y: anchor.y - canteiro / 2 - lane * 2 }, { x: xR, y: anchor.y - canteiro / 2 - lane * 2 }),
        // Borda interna norte (limite com canteiro)
        roadEdge({ x: xL, y: anchor.y - canteiro / 2 }, { x: xR, y: anchor.y - canteiro / 2 }),
        // Divisão dentro da pista norte
        laneSeparator({ x: xL, y: pistaNorte }, { x: xR, y: pistaNorte }),
        // Canteiro central (linha verde grossa)
        canteiroCentral({ x: xL, y: anchor.y }, { x: xR, y: anchor.y }),
        // Borda interna sul
        roadEdge({ x: xL, y: anchor.y + canteiro / 2 }, { x: xR, y: anchor.y + canteiro / 2 }),
        // Borda externa sul
        roadEdge({ x: xL, y: anchor.y + canteiro / 2 + lane * 2 }, { x: xR, y: anchor.y + canteiro / 2 + lane * 2 }),
        // Divisão dentro da pista sul
        laneSeparator({ x: xL, y: pistaSul }, { x: xR, y: pistaSul }),
      ];
    },
  },

  cruzamento_y: {
    id: "cruzamento_y",
    label: "Cruzamento em Y",
    description:
      "Bifurcação: via vertical descendente que se divide em duas direções (~45°).",
    build(anchor) {
      const arm = 220;
      const half = DEFAULT_LANE_WIDTH;
      // Via vertical superior (entrada do Y)
      const objs: SicroObject[] = [
        roadEdge({ x: anchor.x - half, y: anchor.y - arm }, { x: anchor.x - half, y: anchor.y }),
        roadEdge({ x: anchor.x + half, y: anchor.y - arm }, { x: anchor.x + half, y: anchor.y }),
        laneSeparator({ x: anchor.x, y: anchor.y - arm }, { x: anchor.x, y: anchor.y }),
      ];
      // Ramo esquerdo (~135° → down-left)
      const dxL = Math.cos((3 * Math.PI) / 4);
      const dyL = Math.sin((3 * Math.PI) / 4);
      const endLx = anchor.x + dxL * arm;
      const endLy = anchor.y + dyL * arm;
      // Borda externa do ramo esquerdo: perpendicular ao eixo do ramo
      const perpLx = -dyL * half;
      const perpLy = dxL * half;
      objs.push(
        roadEdge(
          { x: anchor.x - half + perpLx, y: anchor.y + perpLy },
          { x: endLx + perpLx, y: endLy + perpLy },
        ),
        roadEdge(
          { x: anchor.x - half - perpLx, y: anchor.y - perpLy },
          { x: endLx - perpLx, y: endLy - perpLy },
        ),
      );
      // Ramo direito (~45° → down-right)
      const dxR = Math.cos(Math.PI / 4);
      const dyR = Math.sin(Math.PI / 4);
      const endRx = anchor.x + dxR * arm;
      const endRy = anchor.y + dyR * arm;
      const perpRx = -dyR * half;
      const perpRy = dxR * half;
      objs.push(
        roadEdge(
          { x: anchor.x + half + perpRx, y: anchor.y + perpRy },
          { x: endRx + perpRx, y: endRy + perpRy },
        ),
        roadEdge(
          { x: anchor.x + half - perpRx, y: anchor.y - perpRy },
          { x: endRx - perpRx, y: endRy - perpRy },
        ),
      );
      return objs;
    },
  },

  curva_esquerda: {
    id: "curva_esquerda",
    label: "Curva à esquerda",
    description: "Curva de ~90° à esquerda, discretizada em 8 segmentos por borda.",
    build(anchor) {
      return curvedRoadFromArc(anchor, Math.PI, (3 * Math.PI) / 2);
    },
  },

  curva_direita: {
    id: "curva_direita",
    label: "Curva à direita",
    description: "Curva de ~90° à direita, discretizada em 8 segmentos por borda.",
    build(anchor) {
      return curvedRoadFromArc(anchor, 0, Math.PI / 2);
    },
  },

  faixa_pedestre_via: {
    id: "faixa_pedestre_via",
    label: "Trecho com faixa de pedestre",
    description:
      "Via reta horizontal com 6 listras paralelas representando uma faixa de pedestres.",
    build(anchor) {
      const length = 400;
      const half = DEFAULT_LANE_WIDTH;
      const xL = anchor.x - length / 2;
      const xR = anchor.x + length / 2;
      const objs: SicroObject[] = [
        roadEdge({ x: xL, y: anchor.y - half }, { x: xR, y: anchor.y - half }),
        roadEdge({ x: xL, y: anchor.y + half }, { x: xR, y: anchor.y + half }),
        laneSeparator({ x: xL, y: anchor.y }, { x: xR, y: anchor.y }),
      ];
      // 6 listras verticais centradas em anchor.x
      const stripes = 6;
      const stripeW = 4;
      const stripeSpacing = 10;
      const totalW = stripes * (stripeW + stripeSpacing);
      const startX = anchor.x - totalW / 2;
      for (let i = 0; i < stripes; i++) {
        const x = startX + i * (stripeW + stripeSpacing);
        // Cada listra como uma linha grossa branca (representada via subtype `sidewalk`).
        objs.push(
          makeLine(
            { x, y: anchor.y - half + 2 },
            { x, y: anchor.y + half - 2 },
            "sidewalk",
          ),
        );
      }
      return objs;
    },
  },

  via_acostamento: {
    id: "via_acostamento",
    label: "Via com acostamento",
    description:
      "Via reta com acostamento lateral (linha cinza paralela à borda).",
    build(anchor) {
      const length = 460;
      const half = DEFAULT_LANE_WIDTH;
      const acostW = 18;
      const xL = anchor.x - length / 2;
      const xR = anchor.x + length / 2;
      return [
        roadEdge({ x: xL, y: anchor.y - half }, { x: xR, y: anchor.y - half }),
        roadEdge({ x: xL, y: anchor.y + half }, { x: xR, y: anchor.y + half }),
        laneSeparator({ x: xL, y: anchor.y }, { x: xR, y: anchor.y }),
        acostamentoLane(
          { x: xL, y: anchor.y - half - acostW },
          { x: xR, y: anchor.y - half - acostW },
        ),
        acostamentoLane(
          { x: xL, y: anchor.y + half + acostW },
          { x: xR, y: anchor.y + half + acostW },
        ),
      ];
    },
  },
};

export function findTemplate(id: TemplateId): RoadTemplate | undefined {
  return TEMPLATES[id];
}

/**
 * Templates surfaced by the Croqui Toolbar.
 *
 * Fase S clean cut — removidos os antigos `via_pro_*` (legados — usavam
 * `SicroRoadObject` v1). Os templates restantes são esboços em linhas
 * úteis como starter para o perito. Vias parity completas (com
 * acabamento de asfalto, eixo central, calçada automática) são criadas
 * via ferramenta **Criar Via** (Python Parity Engine).
 */
export const TOOLBAR_TEMPLATES: ReadonlyArray<TemplateId> = [
  "via_reta",
  "cruzamento_x",
  "cruzamento_t",
  "cruzamento_y",
  "mao_dupla",
  "mao_unica",
  "curva_simples",
  "curva_esquerda",
  "curva_direita",
  "rotatoria_simples",
  "avenida_canteiro",
  "faixa_pedestre_via",
  "via_acostamento",
];

// ---------------------------------------------------------------------------
// Internal helpers

function roadEdge(a: SicroPoint, b: SicroPoint): SicroLineObject {
  return makeLine(a, b, "road");
}
function laneSeparator(a: SicroPoint, b: SicroPoint): SicroLineObject {
  return makeLine(a, b, "lane_separator");
}
function solidLane(a: SicroPoint, b: SicroPoint): SicroLineObject {
  return makeLine(a, b, "lane");
}
function arrowSegment(a: SicroPoint, b: SicroPoint): SicroLineObject {
  return makeLine(a, b, "arrow");
}

function canteiroCentral(a: SicroPoint, b: SicroPoint): SicroLineObject {
  return makeLine(a, b, "canteiro");
}

function acostamentoLane(a: SicroPoint, b: SicroPoint): SicroLineObject {
  return makeLine(a, b, "acostamento");
}

/**
 * Curva 90° genérica — discretiza outer/inner edges em 8 segmentos.
 * `startAngle` / `endAngle` em radianos (sentido horário).
 */
function curvedRoadFromArc(
  anchor: SicroPoint,
  startAngle: number,
  endAngle: number,
): SicroObject[] {
  const radius = 200;
  const segments = 8;
  const half = DEFAULT_LANE_WIDTH;
  const outer = sampleArc(anchor, radius + half, startAngle, endAngle, segments);
  const inner = sampleArc(anchor, radius - half, startAngle, endAngle, segments);
  const out: SicroObject[] = [];
  for (let i = 0; i < segments; i++) {
    out.push(roadEdge(outer[i]!, outer[i + 1]!));
    out.push(roadEdge(inner[i]!, inner[i + 1]!));
  }
  return out;
}

function sampleArc(
  center: SicroPoint,
  radius: number,
  start: number,
  end: number,
  segments: number,
): SicroPoint[] {
  const out: SicroPoint[] = [];
  for (let i = 0; i <= segments; i++) {
    const t = i / segments;
    const a = start + (end - start) * t;
    out.push({
      x: center.x + Math.cos(a) * radius,
      y: center.y + Math.sin(a) * radius,
    });
  }
  return out;
}
