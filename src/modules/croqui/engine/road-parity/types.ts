/**
 * Python Parity Engine — types.
 *
 * Modelo de via e rotatória inspirado diretamente no SICRO 1.0 Python
 * (`desenho/spline_via.py` + `desenho/osm_via.py`). Drasticamente mais
 * simples que o `SicroRoadObject` / `SicroRoundaboutObject` legados,
 * com a finalidade de:
 *
 *   - reduzir o número de campos manipuláveis a uma quantidade
 *     manejável (8 geométricos + 4 visuais para via);
 *   - usar largura física em metros (não pixels), permitindo render
 *     correto sob diferentes escalas;
 *   - eliminar primitivas que adicionam complexidade sem ganho visual
 *     (lane_count, lane_dividers, junction patches, flares, entries,
 *     closed_path, smoothing modes, road_style, surface texture).
 *
 * **Filosofia:** se o Python 1.0 fez sem, o Parity Engine faz sem.
 *
 * Estes tipos NÃO substituem `SicroRoadObject` / `SicroRoundaboutObject`
 * legados — são aditivos. Coexistem no mesmo `SicroCroquiDoc` durante
 * a migração. Cada objeto carrega `engine: "parity"` como discriminador
 * intrínseco para que type guards possam distinguir.
 *
 * Versão de schema-alvo: `.sicrocroqui` v0.4.
 *
 * Documentação de referência:
 *   - `docs/archive/ROAD_ENGINE_PYTHON_PARITY_SPEC.md` §1, §2 (campos).
 *   - `docs/archive/ROAD_ENGINE_1_PYTHON_AUDIT.md` (origem do Python).
 *   - `desenho/spline_via.py` (modelo `_via_spline`).
 *   - `desenho/osm_via.py` (modelo `_rotatoria` + tabela `_LARG_CLASSE`).
 */

import type { ObjectCategory } from "../schema";

// ---------------------------------------------------------------------------
// Constantes do domínio.

/**
 * Discriminador intrínseco do Python Parity Engine.
 *
 * Cada objeto parity carrega `engine: "parity"`. Type guards e
 * runtime checks dependem desse campo para distinguir entre v1/v2
 * (legados) e parity (novo).
 */
export const PARITY_ENGINE_TAG = "parity" as const;
export type ParityEngineTag = typeof PARITY_ENGINE_TAG;

/**
 * Superfície da via (paridade com `superficies.py` Python).
 *
 * Cada valor mapeia para uma cor sólida hardcoded no renderer:
 *   - `asfalto`  → `#1C1C1C` (cinza muito escuro)
 *   - `calcada`  → `#7C7460` (cinza-amarelado quente)
 *   - `terra`    → `#9C7A4E` (terra)
 *
 * **Sem texture, sem padrão granulado, sem cor customizável.** Se o
 * perito precisar de outra cor, a granularidade certa é adicionar uma
 * nova superfície ao enum — não permitir cor arbitrária.
 */
export type ParitySuperficie = "asfalto" | "calcada" | "terra";

/**
 * Tipo da marcação central da via (paridade com Python `marcacao`).
 *
 *   - `amarela`  → tracejado amarelo `#F5C518` (arteriais, mão dupla).
 *   - `branca`   → tracejado branco `#FFFFFF` (locais, mão dupla).
 *   - `nenhuma`  → sem eixo central (vias de mão única, calçadas).
 *
 * Renderer só desenha a marcação se `mao_dupla === true` E
 * `marcacao !== "nenhuma"`. Em via de mão única (`mao_dupla === false`)
 * a marcação é ignorada.
 */
export type ParityMarcacao = "amarela" | "branca" | "nenhuma";

// ---------------------------------------------------------------------------
// SicroRoadObject_parity — via Bezier 4-point.

/**
 * Via no Python Parity Engine.
 *
 * Geometria = Cubic Bezier com 4 pontos de controle em coordenadas
 * de **mundo (metros)**. O renderer multiplica por `doc.scale.px_per_m`
 * (default `10` se ausente) para obter pixels de canvas.
 *
 * **Campos geométricos (8):**
 *   - `ax`, `ay`  — âncora inicial.
 *   - `bx`, `by`  — âncora final.
 *   - `cx1`, `cy1` — controle 1.
 *   - `cx2`, `cy2` — controle 2.
 *
 * **Campos visuais (4):**
 *   - `largura_m`  — largura física da pista em metros.
 *   - `superficie` — asfalto / calçada / terra.
 *   - `mao_dupla`  — true ⇒ eixo central tracejado.
 *   - `marcacao`   — cor do eixo (amarela / branca / nenhuma).
 *
 * **Sem `lane_count`, `lane_width`, `road_style`, `smoothing.mode`,
 * `closed_path`, `direction`, `markings.{color,edge_line,...}`,
 * `curb.*`, `surface.fill/texture`, `spline_tension`, `bezier?`.**
 *
 * Esses campos foram removidos porque o Python não tem e o resultado
 * visual é equivalente (ou superior).
 */
export interface SicroRoadObject_parity {
  // --- Discriminador + sistema (5 campos) ---
  id: string;
  /**
   * `kind: "road_parity"` (não `"road"`) — distingue tipo
   * estaticamente do `SicroRoadObject` legado. Decisão de design para
   * que o TypeScript narrow corretamente em `switch (obj.kind)` sem
   * exigir guards adicionais no caso geral. Transparente para o
   * usuário (Layer Panel mostra "Via" ou similar).
   */
  kind: "road_parity";
  /**
   * Tag intrínseca redundante (kind já distingue). Mantido para
   * defesa em depth — runtime guard pode validar consistência.
   */
  engine: ParityEngineTag;
  layer_id: string;
  category: ObjectCategory;

  // --- Geometria — Bezier 4-point em mundo (metros) (8 campos) ---
  ax: number;
  ay: number;
  bx: number;
  by: number;
  cx1: number;
  cy1: number;
  cx2: number;
  cy2: number;

  // --- Aparência (4 campos) ---
  /** Largura física da pista em metros. */
  largura_m: number;
  superficie: ParitySuperficie;
  /** true ⇒ via de mão dupla; eixo central tracejado renderizado. */
  mao_dupla: boolean;
  marcacao: ParityMarcacao;

  // --- Estado da UI (3 campos) ---
  visible: boolean;
  locked: boolean;
  /** Rótulo humano (Av. Manoel Torrinha, BR-156, etc.). */
  label: string | null;

  // --- Metadados opacos (1 campo) ---
  /**
   * Bag de metadados opaco — preservado pelo serializer e pelo
   * coercer. Usado pelo adapter OSM para registrar tags originais
   * (`source: "osm"`, `osm_id`, `highway`, `oneway`, `lanes`, etc.)
   * e por features futuras (anotações, IDs cruzados, audit).
   */
  metadata_json: string | null;
}

// ---------------------------------------------------------------------------
// SicroRoundaboutObject_parity — rotatória simplificada.

/**
 * Rotatória no Python Parity Engine.
 *
 * Geometria = centro + raio + largura do anel, todos em **mundo (metros)**.
 * Renderer aplica `doc.scale.px_per_m` na hora.
 *
 * **Campos geométricos (4):**
 *   - `cx`, `cy`    — centro.
 *   - `r_m`         — raio externo do anel.
 *   - `largura_m`   — espessura do anel.
 *
 * **Campos visuais (2):**
 *   - `superficie` — geralmente "asfalto"; mantido configurável para
 *     consistência com via.
 *   - `inner_color?` — cor da ilha central. Opcional; default
 *     `#3A6535` (verde canteiro Python). Permite ao perito personalizar
 *     para casos especiais (ex: ilha de concreto cinza).
 *
 * **Sem `border_color`** (branco hardcoded), **sem `lane_count`**
 * (não faz sentido em anel), **sem `flares`** (renderer cuida da
 * conexão via clipping), **sem `entries`** (idem), **sem `curb`**
 * (calçada externa hardcoded em 2m).
 */
export interface SicroRoundaboutObject_parity {
  // --- Discriminador + sistema (5 campos) ---
  id: string;
  /** `"roundabout_parity"` — análogo a `SicroRoadObject_parity.kind`. */
  kind: "roundabout_parity";
  engine: ParityEngineTag;
  layer_id: string;
  category: ObjectCategory;

  // --- Geometria — mundo (metros) (4 campos) ---
  cx: number;
  cy: number;
  r_m: number;
  largura_m: number;

  // --- Aparência (3 campos) ---
  superficie: ParitySuperficie;
  /**
   * Cor da ilha central. Opcional. Quando ausente, renderer aplica
   * `#3A6535` (verde canteiro padrão SICRO 1.0 Python).
   */
  inner_color?: string;
  /**
   * Marcação central do anel — linha tracejada circular no raio médio
   * do anel (entre borda externa e borda interna). Análoga ao eixo
   * central de uma via reta. Opcional.
   *
   *   - `amarela`  → eixo tracejado amarelo (rotatórias com fluxo
   *                  bidirecional, típico de arteriais).
   *   - `branca`   → eixo tracejado branco (rotatórias com pistas
   *                  separadas).
   *   - `nenhuma`  → sem linha central (default — bem comum em mini
   *                  rotatórias e rotatórias urbanas pequenas).
   *
   * Quando ausente, o renderer NÃO desenha linha tracejada (mesmo
   * comportamento que `"nenhuma"`).
   */
  marcacao?: ParityMarcacao;

  // --- Estado da UI (3 campos) ---
  visible: boolean;
  locked: boolean;
  label: string | null;

  // --- Metadados opacos (1 campo) ---
  metadata_json: string | null;
}

// ---------------------------------------------------------------------------
// Union — coleção de objetos parity.

/**
 * União dos objetos que pertencem ao Python Parity Engine.
 *
 * Não inclui veículos, pessoas, vestígios, etc. — apenas via e
 * rotatória. Os outros objetos do `.sicrocroqui` continuam usando
 * seus tipos existentes (`SicroVehicleObject`, `SicroLineObject`,
 * `SicroMarkerObject`, `SicroTextObject`, `SicroMeasurementObject`)
 * sem alteração.
 */
export type SicroParityObject =
  | SicroRoadObject_parity
  | SicroRoundaboutObject_parity;

// ---------------------------------------------------------------------------
// Defaults & limites do domínio.

/**
 * Largura padrão de uma via reta criada manualmente (paridade Python
 * `LARGURA_PADRAO = 7.0`).
 */
export const PARITY_ROAD_LARGURA_PADRAO_M = 7.0;

/**
 * Largura mínima permitida para uma via (em metros). Abaixo disso a
 * via vira "linha fina" e perde semântica de pista.
 */
export const PARITY_ROAD_LARGURA_MIN_M = 0.5;

/**
 * Largura máxima permitida para uma via (em metros). Acima disso
 * provavelmente é erro de input ou caso patológico — Inspector pode
 * avisar.
 */
export const PARITY_ROAD_LARGURA_MAX_M = 30.0;

/**
 * Raio mínimo de rotatória (metros).
 */
export const PARITY_ROUNDABOUT_R_MIN_M = 2.0;

/**
 * Raio máximo de rotatória (metros).
 */
export const PARITY_ROUNDABOUT_R_MAX_M = 100.0;

/**
 * Largura padrão do anel de rotatória (metros).
 */
export const PARITY_ROUNDABOUT_LARGURA_PADRAO_M = 7.0;

/**
 * Largura mínima do anel (em metros). Abaixo disso a ilha "engole"
 * o anel.
 */
export const PARITY_ROUNDABOUT_LARGURA_MIN_M = 2.0;

/**
 * Largura máxima absoluta do anel (em metros). O fator final pode
 * ser menor — limitado por `r_m - 1` no factory para garantir ilha
 * visível.
 */
export const PARITY_ROUNDABOUT_LARGURA_MAX_M_FALLBACK = 15.0;

/**
 * Default de `px_per_m` quando `doc.scale.px_per_m` está ausente
 * ou inválido. Conservador — produz vias visíveis em canvas típico
 * (1600×1000) com cenas urbanas.
 */
export const PARITY_DEFAULT_PX_PER_M = 10;

/**
 * Largura da calçada automática para vias com `superficie === "asfalto"`
 * (em metros). Paridade com Python `superficies.py` `calcada_auto.largura`.
 *
 * Calçada não é um campo do `SicroRoadObject_parity` — é AUTOMÁTICA
 * quando a superfície é asfalto. O renderer sabe disso.
 */
export const PARITY_SIDEWALK_WIDTH_M = 2.0;
