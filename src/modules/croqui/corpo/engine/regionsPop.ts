/**
 * Nômina anatômica OFICIAL do POP LOCAL DE CRIME (SENASP/MJSP, 2024) —
 * Anexo 1, "Esquema para localização de lesões e achados de interesse
 * pericial" (fonte: SCPe/IC/PCDF, 2024; páginas 110–111 do PDF).
 *
 * Duas numerações INDEPENDENTES, transcritas VERBATIM do manual:
 *   - FRENTE (vista anterior): 1–28
 *   - COSTAS (vista posterior): 1–21
 * (Grafias do manual preservadas: "Espondiléia", "Deltodiana",
 * "Sacro-coccígea", "Face dorsal mão". O item "20. Vulvar" consta IGUAL nas
 * pranchas masculina e feminina do POP — mantido verbatim.)
 *
 * As posições (nx, ny ∈ [0,1]) são normalizadas pelo BOUNDING BOX de cada
 * vista (vide bodyTemplates.views[].box). Estes DEFAULTS foram calibrados
 * MANUALMENTE pelo perito sobre a prancha masculina (modo "Calibrar
 * numeração" do editor → arrastar + salvar), conforme o boneco numerado do
 * manual. O perito pode reajustar a qualquer momento pelo editor; a
 * calibração dele é gravada em `pop_calibration.json` (app_config_dir) e
 * sobrepõe estes defaults por prancha. Regiões bilaterais têm DOIS pontos
 * (lados D e E) com o MESMO número, como no manual.
 */

export type PopSide = "frente" | "costas";

export interface PopRegiao {
  /** Número oficial no POP (reinicia por vista). */
  n: number;
  /** Nome verbatim do manual. */
  label: string;
  side: PopSide;
  /** Pontos normalizados [nx, ny] no box da vista (1 = central, 2 = bilateral). */
  pts: ReadonlyArray<readonly [number, number]>;
}

export const POP_FRENTE: ReadonlyArray<PopRegiao> = [
  { n: 1, label: "Frontal", side: "frente", pts: [[0.5013, 0.0213]] },
  { n: 2, label: "Orbitárias", side: "frente", pts: [[0.4527, 0.0582], [0.5486, 0.0582]] },
  { n: 3, label: "Malares", side: "frente", pts: [[0.4292, 0.0869], [0.5708, 0.0865]] },
  { n: 4, label: "Mandibular", side: "frente", pts: [[0.4459, 0.1146], [0.5489, 0.115]] },
  { n: 5, label: "Mentoniana", side: "frente", pts: [[0.4955, 0.1235]] },
  { n: 6, label: "Cervical anterior", side: "frente", pts: [[0.5045, 0.1527]] },
  { n: 7, label: "Carotidianas", side: "frente", pts: [[0.417, 0.1452], [0.5807, 0.1463]] },
  { n: 8, label: "Supraclaviculares", side: "frente", pts: [[0.3379, 0.1658], [0.6753, 0.1675]] },
  { n: 9, label: "Infraclaviculares", side: "frente", pts: [[0.1796, 0.1889], [0.7992, 0.1841]] },
  { n: 10, label: "Esternal", side: "frente", pts: [[0.4987, 0.2437]] },
  { n: 11, label: "Torácicas", side: "frente", pts: [[0.3545, 0.2215], [0.6336, 0.2189]] },
  { n: 12, label: "Epigástricas", side: "frente", pts: [[0.4962, 0.3081]] },
  { n: 13, label: "Hipocôndrios", side: "frente", pts: [[0.3495, 0.3197], [0.6316, 0.3205]] },
  { n: 14, label: "Mesogástrica", side: "frente", pts: [[0.4975, 0.3596]] },
  { n: 15, label: "Flancos", side: "frente", pts: [[0.359, 0.3839], [0.6309, 0.383]] },
  { n: 16, label: "Hipogástrica", side: "frente", pts: [[0.5, 0.4325]] },
  { n: 17, label: "Fossas ilíacas", side: "frente", pts: [[0.3636, 0.4229], [0.632, 0.4197]] },
  { n: 18, label: "Pubiana", side: "frente", pts: [[0.4991, 0.461]] },
  { n: 19, label: "Inguinal", side: "frente", pts: [[0.3685, 0.4669], [0.6307, 0.4692]] },
  { n: 20, label: "Vulvar", side: "frente", pts: [[0.5018, 0.4936]] },
  { n: 21, label: "Braço", side: "frente", pts: [[0.1539, 0.2631], [0.8382, 0.2657]] },
  { n: 22, label: "Cubital", side: "frente", pts: [[0.1384, 0.3539], [0.8642, 0.3526]] },
  { n: 23, label: "Antebraço", side: "frente", pts: [[0.0899, 0.4235], [0.9062, 0.4187]] },
  { n: 24, label: "Palmar", side: "frente", pts: [[0.0687, 0.495], [0.9339, 0.4955]] },
  { n: 25, label: "Coxa", side: "frente", pts: [[0.355, 0.5754], [0.6618, 0.5726]] },
  { n: 26, label: "Joelho", side: "frente", pts: [[0.35, 0.6806], [0.6626, 0.6771]] },
  { n: 27, label: "Perna", side: "frente", pts: [[0.339, 0.7885], [0.6589, 0.785]] },
  { n: 28, label: "Dorso do pé", side: "frente", pts: [[0.3605, 0.9574], [0.6521, 0.9546]] },
];

export const POP_COSTAS: ReadonlyArray<PopRegiao> = [
  { n: 1, label: "Parietal", side: "costas", pts: [[0.4379, 0.025], [0.5521, 0.0253]] },
  { n: 2, label: "Occipital", side: "costas", pts: [[0.496, 0.0713]] },
  { n: 3, label: "Temporal", side: "costas", pts: [[0.391, 0.06], [0.601, 0.0613]] },
  { n: 4, label: "Cervical", side: "costas", pts: [[0.4955, 0.1277]] },
  { n: 5, label: "Supra-escapular", side: "costas", pts: [[0.4167, 0.1646], [0.5698, 0.1646]] },
  { n: 6, label: "Escapular", side: "costas", pts: [[0.3254, 0.2161], [0.6873, 0.2108]] },
  { n: 7, label: "Dorsal", side: "costas", pts: [[0.373, 0.2874], [0.636, 0.2865]] },
  { n: 8, label: "Lombar", side: "costas", pts: [[0.4137, 0.3558], [0.5889, 0.3568]] },
  { n: 9, label: "Ilíaca", side: "costas", pts: [[0.3302, 0.4139], [0.6536, 0.4157]] },
  { n: 10, label: "Espondiléia", side: "costas", pts: [[0.5, 0.2286]] },
  { n: 11, label: "Sacro-coccígea", side: "costas", pts: [[0.5009, 0.4169]] },
  { n: 12, label: "Glútea", side: "costas", pts: [[0.4021, 0.4701], [0.6077, 0.4737]] },
  { n: 13, label: "Coxa", side: "costas", pts: [[0.3572, 0.5675], [0.6491, 0.5667]] },
  { n: 14, label: "Poplítea", side: "costas", pts: [[0.3431, 0.6825], [0.6566, 0.682]] },
  { n: 15, label: "Perna", side: "costas", pts: [[0.3364, 0.7845], [0.6631, 0.7802]] },
  { n: 16, label: "Pé", side: "costas", pts: [[0.3523, 0.9692], [0.6427, 0.9671]] },
  { n: 17, label: "Deltodiana", side: "costas", pts: [[0.1649, 0.2064], [0.843, 0.2085]] },
  { n: 18, label: "Braço", side: "costas", pts: [[0.1502, 0.2752], [0.8484, 0.2812]] },
  { n: 19, label: "Cotovelo", side: "costas", pts: [[0.1376, 0.3418], [0.8665, 0.345]] },
  { n: 20, label: "Antebraço", side: "costas", pts: [[0.0924, 0.4143], [0.9009, 0.4102]] },
  { n: 21, label: "Face dorsal mão", side: "costas", pts: [[0.0561, 0.4932], [0.9439, 0.4986]] },
];

export const POP_REGIOES: ReadonlyArray<PopRegiao> = [
  ...POP_FRENTE,
  ...POP_COSTAS,
];

/** Id estável persistido em `marker.regiao` (ex.: "pop_frente_11"). */
export function popRegiaoId(side: PopSide, n: number): string {
  return `pop_${side}_${n}`;
}

const POP_INDEX: ReadonlyMap<string, PopRegiao> = new Map(
  POP_REGIOES.map((r) => [popRegiaoId(r.side, r.n), r]),
);

/** Rótulo "11. Torácicas (frente)" a partir do id; null se não-POP. */
export function popRegiaoLabel(id: string): string | null {
  const r = POP_INDEX.get(id);
  if (!r) return null;
  return `${r.n}. ${r.label} (${r.side === "frente" ? "frente" : "costas"})`;
}

/** Existe no catálogo POP? */
export function isPopRegiao(id: string): boolean {
  return POP_INDEX.has(id);
}

/** Linhas "N. Nome" pra listas de export/painel (uma vista). */
export function popListaLinhas(side: PopSide): string[] {
  const src = side === "frente" ? POP_FRENTE : POP_COSTAS;
  return src.map((r) => `${r.n}. ${r.label}`);
}

// ---------------------------------------------------------------------------
// Calibração manual das posições (modo "Calibrar numeração" do editor).
// O perito arrasta cada número e salva; o app passa a usar a posição dele em
// cima do default. Persistido GLOBAL em pop_calibration.json (app_config_dir).

/** Override de posição: chave → [nx, ny] (normalizado no box da vista). */
export type PopCalibration = Record<string, readonly [number, number]>;

/**
 * Chave estável de um PONTO de região (regiões bilaterais têm 2 pontos).
 * Inclui a PRANCHA porque masc/fem são artes distintas (proporções diferentes),
 * então calibram independente. Ex.: "corpo_masc_frente_18_0".
 */
export function popPointKey(
  templateId: string,
  side: PopSide,
  n: number,
  idx: number,
): string {
  return `${templateId}_${side}_${n}_${idx}`;
}
