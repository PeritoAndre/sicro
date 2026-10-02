/**
 * Factories de via e rotatória parity: defaults seguros + clamp de limites.
 * Sem efeitos colaterais; devolvem objeto novo pronto para `doc.objects`.
 */

import {
  PARITY_ENGINE_TAG,
  PARITY_ROAD_LARGURA_MAX_M,
  PARITY_ROAD_LARGURA_MIN_M,
  PARITY_ROAD_LARGURA_PADRAO_M,
  PARITY_ROUNDABOUT_LARGURA_MAX_M_FALLBACK,
  PARITY_ROUNDABOUT_LARGURA_MIN_M,
  PARITY_ROUNDABOUT_LARGURA_PADRAO_M,
  PARITY_ROUNDABOUT_R_MAX_M,
  PARITY_ROUNDABOUT_R_MIN_M,
  type SicroRoadObject_parity,
  type SicroRoundaboutObject_parity,
} from "./types";
import type { ParityMarcacao, ParitySuperficie } from "./types";

const PARITY_DEFAULT_LAYER_ID = "layer_objects";

// ---- Helpers ----

function clamp(value: number, lo: number, hi: number): number {
  if (!Number.isFinite(value)) return lo;
  if (value < lo) return lo;
  if (value > hi) return hi;
  return value;
}

function genId(prefix: string): string {
  // Sem `crypto.randomUUID`: webview Tauri em Windows antigo pode não ter.
  const t = Date.now().toString(36);
  const r = Math.floor(Math.random() * 0xffffff)
    .toString(36)
    .padStart(4, "0");
  return `${prefix}_${t}_${r}`;
}

// ---- Options ----

interface MakeParityRoadOptions {
  id?: string;
  layer_id?: string;
  /** Controles Bezier; ausentes ⇒ a 1/3 e 2/3 do segmento A→B (reta). */
  cx1?: number;
  cy1?: number;
  cx2?: number;
  cy2?: number;
  largura_m?: number;
  superficie?: ParitySuperficie;
  mao_dupla?: boolean;
  marcacao?: ParityMarcacao;
  label?: string | null;
  metadata_json?: string | null;
  visible?: boolean;
  locked?: boolean;
}

interface MakeParityRoundaboutOptions {
  id?: string;
  layer_id?: string;
  largura_m?: number;
  superficie?: ParitySuperficie;
  inner_color?: string;
  marcacao?: ParityMarcacao;
  label?: string | null;
  metadata_json?: string | null;
  visible?: boolean;
  locked?: boolean;
}

// ---- Via ----

/** Via entre A e B (metros). Defaults: 7 m, asfalto, mão dupla, amarela; largura clampada em [0.5, 30]. */
export function makeParityRoad(
  ax: number,
  ay: number,
  bx: number,
  by: number,
  opts: MakeParityRoadOptions = {},
): SicroRoadObject_parity {
  const dx = bx - ax;
  const dy = by - ay;
  const cx1 = opts.cx1 ?? ax + dx / 3;
  const cy1 = opts.cy1 ?? ay + dy / 3;
  const cx2 = opts.cx2 ?? ax + (2 * dx) / 3;
  const cy2 = opts.cy2 ?? ay + (2 * dy) / 3;

  return {
    id: opts.id ?? genId("rdp"),
    kind: "road_parity",
    engine: PARITY_ENGINE_TAG,
    layer_id: opts.layer_id ?? PARITY_DEFAULT_LAYER_ID,
    category: "vias",
    ax,
    ay,
    bx,
    by,
    cx1,
    cy1,
    cx2,
    cy2,
    largura_m: clamp(
      opts.largura_m ?? PARITY_ROAD_LARGURA_PADRAO_M,
      PARITY_ROAD_LARGURA_MIN_M,
      PARITY_ROAD_LARGURA_MAX_M,
    ),
    superficie: opts.superficie ?? "asfalto",
    mao_dupla: opts.mao_dupla ?? true,
    marcacao: opts.marcacao ?? "amarela",
    visible: opts.visible !== false,
    locked: opts.locked === true,
    label: opts.label ?? null,
    metadata_json: opts.metadata_json ?? null,
  };
}

/** Via com os 4 pontos Bezier explícitos (adapter OSM e templates). */
export function makeParityRoadBezier(
  ax: number,
  ay: number,
  cx1: number,
  cy1: number,
  cx2: number,
  cy2: number,
  bx: number,
  by: number,
  opts: Omit<MakeParityRoadOptions, "cx1" | "cy1" | "cx2" | "cy2"> = {},
): SicroRoadObject_parity {
  return makeParityRoad(ax, ay, bx, by, {
    ...opts,
    cx1,
    cy1,
    cx2,
    cy2,
  });
}

// ---- Rotatória ----

/** Rotatória. `r_m` clampado em [2, 100]; `largura_m` em [2, min(r_m − 1, 15)] para a ilha ficar visível. */
export function makeParityRoundabout(
  cx: number,
  cy: number,
  r_m: number,
  opts: MakeParityRoundaboutOptions = {},
): SicroRoundaboutObject_parity {
  const r_clamped = clamp(r_m, PARITY_ROUNDABOUT_R_MIN_M, PARITY_ROUNDABOUT_R_MAX_M);
  const largura_default = opts.largura_m ?? PARITY_ROUNDABOUT_LARGURA_PADRAO_M;
  const largura_max_real = Math.min(
    PARITY_ROUNDABOUT_LARGURA_MAX_M_FALLBACK,
    Math.max(PARITY_ROUNDABOUT_LARGURA_MIN_M, r_clamped - 1),
  );
  const largura_m = clamp(
    largura_default,
    PARITY_ROUNDABOUT_LARGURA_MIN_M,
    largura_max_real,
  );

  const out: SicroRoundaboutObject_parity = {
    id: opts.id ?? genId("rbp"),
    kind: "roundabout_parity",
    engine: PARITY_ENGINE_TAG,
    layer_id: opts.layer_id ?? PARITY_DEFAULT_LAYER_ID,
    category: "vias",
    cx,
    cy,
    r_m: r_clamped,
    largura_m,
    superficie: opts.superficie ?? "asfalto",
    visible: opts.visible !== false,
    locked: opts.locked === true,
    label: opts.label ?? null,
    metadata_json: opts.metadata_json ?? null,
  };

  // Opcionais só entram no objeto se especificados (ausente ≠ undefined na serialização).
  if (opts.inner_color !== undefined) {
    out.inner_color = opts.inner_color;
  }
  if (opts.marcacao !== undefined) {
    out.marcacao = opts.marcacao;
  }

  return out;
}
