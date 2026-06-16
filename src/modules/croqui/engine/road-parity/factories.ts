/**
 * Python Parity Engine — factory functions.
 *
 * Helpers para criar `SicroRoadObject_parity` e
 * `SicroRoundaboutObject_parity` com defaults seguros + validação de
 * limites. Equivalente do `nova_spline` Python (`spline_via.py`).
 *
 * **Sem efeitos colaterais.** Cada factory retorna um objeto novo
 * pronto para inserir em `doc.objects`. Caller é responsável por
 * gerar id (geralmente via `uid()`) — factory aceita o id ou gera
 * com `crypto.randomUUID()`.
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
// Re-export tipos opcionais nos options sem precisar import duplicado:
import type { ParityMarcacao, ParitySuperficie } from "./types";

/**
 * Layer ID padrão para objetos viários (paridade com o resto do
 * SICRO 2.0 — todas as vias e rotatórias entram no layer "Objetos").
 */
const PARITY_DEFAULT_LAYER_ID = "layer_objects";

// ---------------------------------------------------------------------------
// Helpers internos.

function clamp(value: number, lo: number, hi: number): number {
  if (!Number.isFinite(value)) return lo;
  if (value < lo) return lo;
  if (value > hi) return hi;
  return value;
}

function genId(prefix: string): string {
  // Sem depender de `crypto.randomUUID` (não disponível em todos os
  // runtimes — Tauri webview em Windows mais antigo pode não ter).
  // Fallback simples: prefix + timestamp + random.
  const t = Date.now().toString(36);
  const r = Math.floor(Math.random() * 0xffffff)
    .toString(36)
    .padStart(4, "0");
  return `${prefix}_${t}_${r}`;
}

// ---------------------------------------------------------------------------
// Options aceitos pelas factories.

export interface MakeParityRoadOptions {
  id?: string;
  layer_id?: string;
  /** Controle 1 (Bezier). Quando ausente, calculado a 1/3 do segmento A→B. */
  cx1?: number;
  cy1?: number;
  /** Controle 2 (Bezier). Quando ausente, calculado a 2/3 do segmento A→B. */
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

export interface MakeParityRoundaboutOptions {
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

// ---------------------------------------------------------------------------
// Factory: SicroRoadObject_parity.

/**
 * Cria uma via parity entre dois pontos do mundo (metros).
 *
 * Defaults (paridade `nova_spline` Python):
 *   - largura_m = 7.0 m
 *   - superficie = asfalto
 *   - mao_dupla = true
 *   - marcacao = amarela
 *   - controles a 1/3 e 2/3 do segmento A→B (= reta visual)
 *
 * Validações:
 *   - largura_m é clamped em [0.5, 30] m.
 *   - id gerado automaticamente se ausente.
 *   - layer_id default = "layer_objects".
 *
 * @param ax,ay   âncora inicial (mundo, metros)
 * @param bx,by   âncora final (mundo, metros)
 * @param opts    overrides opcionais
 */
export function makeParityRoad(
  ax: number,
  ay: number,
  bx: number,
  by: number,
  opts: MakeParityRoadOptions = {},
): SicroRoadObject_parity {
  // Controles default: 1/3 e 2/3 do segmento A→B (reta visualmente).
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

/**
 * Cria uma via parity com Bezier explícito (4 pontos completos).
 * Útil para o adapter OSM (Hermite→Bezier) e templates.
 */
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

// ---------------------------------------------------------------------------
// Factory: SicroRoundaboutObject_parity.

/**
 * Cria uma rotatória parity.
 *
 * Defaults:
 *   - largura_m = 7.0 m (anel)
 *   - superficie = asfalto
 *   - inner_color = undefined (renderer aplica `#3A6535`)
 *
 * Validações:
 *   - r_m clamped em [2, 100] m.
 *   - largura_m clamped em [2, min(r_m - 1, 15)] — garante ilha visível.
 */
export function makeParityRoundabout(
  cx: number,
  cy: number,
  r_m: number,
  opts: MakeParityRoundaboutOptions = {},
): SicroRoundaboutObject_parity {
  const r_clamped = clamp(r_m, PARITY_ROUNDABOUT_R_MIN_M, PARITY_ROUNDABOUT_R_MAX_M);
  // Largura não pode ser >= raio (senão a ilha desaparece). Limita
  // ao mínimo entre o constante e (raio - 1m) para garantir ilha
  // visível.
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

  // `inner_color` opcional — só inclui no objeto se foi especificado.
  // Quando ausente, renderer aplica `#3A6535` (default Python).
  if (opts.inner_color !== undefined) {
    out.inner_color = opts.inner_color;
  }
  // `marcacao` opcional — quando ausente, renderer não desenha o eixo
  // central tracejado do anel.
  if (opts.marcacao !== undefined) {
    out.marcacao = opts.marcacao;
  }

  return out;
}
