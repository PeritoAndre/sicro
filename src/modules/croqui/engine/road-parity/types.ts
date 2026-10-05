/**
 * Tipos do motor de vias "parity" (via Bezier de 4 pontos + rotatória circular),
 * modelo copiado do SICRO 1.0 Python. Geometria sempre em metros (mundo);
 * o renderer aplica `px_per_m` na hora de desenhar.
 */

import type { ObjectCategory } from "../schema";

// ---- Constantes do domínio ----

export const PARITY_ENGINE_TAG = "parity" as const;
export type ParityEngineTag = typeof PARITY_ENGINE_TAG;

/** Cada valor vira uma cor fixa no renderer; cor arbitrária não é permitida de propósito. */
export type ParitySuperficie = "asfalto" | "calcada" | "terra";

/** Eixo central (campo antigo); hoje vale `eixo`, e `marcacao` é lida quando `eixo` falta. */
export type ParityMarcacao = "amarela" | "branca" | "nenhuma";

/** Eixo da via de mão dupla, conforme a sinalização real (CONTRAN). */
export type ParityEixo =
  | "amarela_dupla"
  | "amarela_trac"
  | "amarela_mista"
  | "branca_trac"
  | "nenhuma";

/** Largura de referência de uma faixa (m) — divide a pista quando `faixas` é automático. */
export const PARITY_FAIXA_LARGURA_M = 3.5;

export const PARITY_ACOSTAMENTO_MAX_M = 4;

// ---- Via ----

/** Via em Bezier cúbica: âncoras A/B + controles C1/C2, tudo em metros. */
export interface SicroRoadObject_parity {
  id: string;
  /** Diferente de "road" para o TS fazer narrowing em `switch (obj.kind)`. */
  kind: "road_parity";
  /** Redundante com `kind`; guard de runtime pode checar consistência. */
  engine: ParityEngineTag;
  layer_id: string;
  category: ObjectCategory;

  ax: number;
  ay: number;
  bx: number;
  by: number;
  cx1: number;
  cy1: number;
  cx2: number;
  cy2: number;

  /** Largura da pista de rolamento em metros (sem acostamento). */
  largura_m: number;
  superficie: ParitySuperficie;
  mao_dupla: boolean;
  marcacao: ParityMarcacao;
  /** Ausente ⇒ derivado de `marcacao` (ver `resolveParityEixo`). */
  eixo?: ParityEixo;
  /** Faixas por sentido (mão dupla) ou no total (mão única). null/ausente ⇒ automático. */
  faixas?: number | null;
  /** Acostamento de cada lado (m). 0/ausente ⇒ sem. */
  acostamento_m?: number;
  /** Largura da calçada (m); null/ausente ⇒ a do estilo do croqui. */
  calcada_m?: number | null;

  visible: boolean;
  locked: boolean;
  label: string | null;

  /** Bag opaco (JSON); o adapter OSM guarda aqui as tags originais. */
  metadata_json: string | null;
}

// ---- Rotatória ----

/** Rotatória: centro + raio + largura do anel, em metros. Calçada externa fixa de 2 m. */
export interface SicroRoundaboutObject_parity {
  id: string;
  kind: "roundabout_parity";
  engine: ParityEngineTag;
  layer_id: string;
  category: ObjectCategory;

  cx: number;
  cy: number;
  r_m: number;
  largura_m: number;

  superficie: ParitySuperficie;
  /** Cor da ilha central; ausente ⇒ renderer usa `#3A6535`. */
  inner_color?: string;
  /** Tracejado no raio médio do anel; ausente ⇒ igual a "nenhuma". */
  marcacao?: ParityMarcacao;

  visible: boolean;
  locked: boolean;
  label: string | null;

  metadata_json: string | null;
}

// ---- Union ----

export type SicroParityObject =
  | SicroRoadObject_parity
  | SicroRoundaboutObject_parity;

// ---- Defaults e limites (metros) ----

export const PARITY_ROAD_LARGURA_PADRAO_M = 10.0;

export const PARITY_ROAD_LARGURA_MIN_M = 0.5;

export const PARITY_ROAD_LARGURA_MAX_M = 30.0;

export const PARITY_ROUNDABOUT_R_MIN_M = 2.0;

export const PARITY_ROUNDABOUT_R_MAX_M = 100.0;

export const PARITY_ROUNDABOUT_LARGURA_PADRAO_M = 7.0;

export const PARITY_ROUNDABOUT_LARGURA_MIN_M = 2.0;

/** Teto absoluto; o factory ainda limita a `r_m - 1` para a ilha continuar visível. */
export const PARITY_ROUNDABOUT_LARGURA_MAX_M_FALLBACK = 15.0;

/** Usado quando `doc.scale.px_per_m` está ausente ou inválido. */
export const PARITY_DEFAULT_PX_PER_M = 10;

/** Calçada automática das vias de asfalto; não é campo do objeto, o renderer aplica. */
export const PARITY_SIDEWALK_WIDTH_M = 2.0;

/** Eixo efetivo: `eixo` ou, em croqui antigo, a tradução de `marcacao`. */
export function resolveParityEixo(road: SicroRoadObject_parity): ParityEixo {
  if (!road.mao_dupla) return "nenhuma";
  if (road.eixo) return road.eixo;
  switch (road.marcacao) {
    case "amarela":
      return "amarela_dupla";
    case "branca":
      return "branca_trac";
    default:
      return "nenhuma";
  }
}

/** `marcacao` equivalente a um eixo — mantém o campo antigo coerente. */
export function marcacaoFromEixo(eixo: ParityEixo): ParityMarcacao {
  if (eixo === "nenhuma") return "nenhuma";
  return eixo === "branca_trac" ? "branca" : "amarela";
}
