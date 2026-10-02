/**
 * Nômina anatômica numerada da 1ª geração (prancha combinada 820×700), mantida
 * só pra resolver ids de docs antigos. Numeração e posições são originais do
 * SICRO (não reproduzem formulário algum); o perito escolhe a região.
 */

import { popRegiaoLabel } from "./regionsPop";

export type Lateralidade = "D" | "E" | "central";
type RegiaoSide = "ant" | "post";

interface RegiaoAnatomica {
  id: string;
  /** Número exibido na prancha e na legenda (por face). */
  n: number;
  side: RegiaoSide;
  label: string;
  grupo: string;
  /** Posição do número na prancha combinada (coords absolutas do viewBox). */
  x: number;
  y: number;
}

// --- ANTERIOR (corpo da esquerda na prancha combinada; centro x≈160) ---
const ANTERIOR: RegiaoAnatomica[] = [
  { id: "frontal", n: 1, side: "ant", label: "Frontal", grupo: "Cabeça/face", x: 160, y: 60 },
  { id: "orbitaria", n: 2, side: "ant", label: "Orbitária", grupo: "Cabeça/face", x: 160, y: 86 },
  { id: "nasal", n: 3, side: "ant", label: "Nasal", grupo: "Cabeça/face", x: 160, y: 98 },
  { id: "malar", n: 4, side: "ant", label: "Zigomática (malar)", grupo: "Cabeça/face", x: 137, y: 98 },
  { id: "mandibular", n: 5, side: "ant", label: "Mandibular", grupo: "Cabeça/face", x: 160, y: 116 },
  { id: "mentoniana", n: 6, side: "ant", label: "Mentoniana", grupo: "Cabeça/face", x: 160, y: 128 },
  { id: "cervical_ant", n: 7, side: "ant", label: "Cervical anterior", grupo: "Pescoço", x: 160, y: 150 },
  { id: "supraclavicular", n: 8, side: "ant", label: "Supraclavicular", grupo: "Tórax", x: 133, y: 166 },
  { id: "infraclavicular", n: 9, side: "ant", label: "Infraclavicular", grupo: "Tórax", x: 140, y: 184 },
  { id: "esternal", n: 10, side: "ant", label: "Esternal", grupo: "Tórax", x: 160, y: 192 },
  { id: "toracica_ant", n: 11, side: "ant", label: "Torácica (mamária)", grupo: "Tórax", x: 186, y: 190 },
  { id: "hipocondrio", n: 12, side: "ant", label: "Hipocôndrio", grupo: "Abdome", x: 186, y: 228 },
  { id: "epigastrica", n: 13, side: "ant", label: "Epigástrica", grupo: "Abdome", x: 160, y: 222 },
  { id: "mesogastrica", n: 14, side: "ant", label: "Mesogástrica (umbilical)", grupo: "Abdome", x: 160, y: 252 },
  { id: "flanco", n: 15, side: "ant", label: "Flanco", grupo: "Abdome", x: 132, y: 252 },
  { id: "fossa_iliaca", n: 16, side: "ant", label: "Fossa ilíaca", grupo: "Abdome", x: 138, y: 288 },
  { id: "hipogastrica", n: 17, side: "ant", label: "Hipogástrica", grupo: "Abdome", x: 160, y: 290 },
  { id: "inguinal", n: 18, side: "ant", label: "Inguinal", grupo: "Pelve", x: 144, y: 308 },
  { id: "pubica", n: 19, side: "ant", label: "Púbica / genital", grupo: "Pelve", x: 160, y: 322 },
  { id: "braco_ant", n: 20, side: "ant", label: "Braço", grupo: "Membro superior", x: 80, y: 205 },
  { id: "cotovelo_ant", n: 21, side: "ant", label: "Cotovelo (cubital)", grupo: "Membro superior", x: 66, y: 262 },
  { id: "antebraco_ant", n: 22, side: "ant", label: "Antebraço", grupo: "Membro superior", x: 60, y: 300 },
  { id: "mao_ant", n: 23, side: "ant", label: "Mão (palma)", grupo: "Membro superior", x: 50, y: 346 },
  { id: "coxa_ant", n: 24, side: "ant", label: "Coxa", grupo: "Membro inferior", x: 134, y: 402 },
  { id: "joelho_ant", n: 25, side: "ant", label: "Joelho", grupo: "Membro inferior", x: 131, y: 478 },
  { id: "perna_ant", n: 26, side: "ant", label: "Perna", grupo: "Membro inferior", x: 129, y: 548 },
  { id: "pe_ant", n: 27, side: "ant", label: "Pé (dorso)", grupo: "Membro inferior", x: 127, y: 626 },
];

// --- POSTERIOR (corpo da direita; centro x≈440 = +280 do anterior) ---
const POSTERIOR: RegiaoAnatomica[] = [
  { id: "parietal", n: 1, side: "post", label: "Parietal", grupo: "Cabeça (trás)", x: 440, y: 58 },
  { id: "occipital", n: 2, side: "post", label: "Occipital", grupo: "Cabeça (trás)", x: 440, y: 84 },
  { id: "temporal", n: 3, side: "post", label: "Temporal", grupo: "Cabeça (trás)", x: 416, y: 92 },
  { id: "nucal", n: 4, side: "post", label: "Nucal (cervical post.)", grupo: "Pescoço", x: 440, y: 150 },
  { id: "supraescapular", n: 5, side: "post", label: "Supraescapular", grupo: "Dorso", x: 414, y: 166 },
  { id: "escapular", n: 6, side: "post", label: "Escapular", grupo: "Dorso", x: 410, y: 192 },
  { id: "dorsal", n: 7, side: "post", label: "Dorsal (interescapular)", grupo: "Dorso", x: 440, y: 212 },
  { id: "lombar", n: 8, side: "post", label: "Lombar", grupo: "Dorso", x: 440, y: 256 },
  { id: "sacro", n: 9, side: "post", label: "Sacrococcígea", grupo: "Dorso", x: 440, y: 300 },
  { id: "glutea", n: 10, side: "post", label: "Glútea", grupo: "Dorso", x: 416, y: 318 },
  { id: "deltoidea", n: 11, side: "post", label: "Deltoidea", grupo: "Membro superior", x: 498, y: 178 },
  { id: "braco_post", n: 12, side: "post", label: "Braço (posterior)", grupo: "Membro superior", x: 504, y: 226 },
  { id: "cotovelo_post", n: 13, side: "post", label: "Cotovelo (olécrano)", grupo: "Membro superior", x: 510, y: 262 },
  { id: "antebraco_post", n: 14, side: "post", label: "Antebraço (posterior)", grupo: "Membro superior", x: 514, y: 300 },
  { id: "mao_post", n: 15, side: "post", label: "Mão (dorso)", grupo: "Membro superior", x: 522, y: 346 },
  { id: "coxa_post", n: 16, side: "post", label: "Coxa (posterior)", grupo: "Membro inferior", x: 414, y: 402 },
  { id: "poplitea", n: 17, side: "post", label: "Poplítea", grupo: "Membro inferior", x: 412, y: 478 },
  { id: "panturrilha", n: 18, side: "post", label: "Perna (panturrilha)", grupo: "Membro inferior", x: 410, y: 548 },
  { id: "pe_post", n: 19, side: "post", label: "Calcâneo / pé", grupo: "Membro inferior", x: 408, y: 626 },
];

export const REGIOES: RegiaoAnatomica[] = [...ANTERIOR, ...POSTERIOR];

const REGIAO_INDEX: Record<string, RegiaoAnatomica> = REGIOES.reduce(
  (acc, r) => {
    acc[r.id] = r;
    return acc;
  },
  {} as Record<string, RegiaoAnatomica>,
);

export function regiaoLabel(id?: string | null): string {
  if (!id) return "";
  // Ids do POP ("pop_frente_N") têm prioridade; ids legados ainda resolvem pelo
  // índice antigo pra docs salvos antes da reformulação seguirem legíveis.
  return popRegiaoLabel(id) ?? REGIAO_INDEX[id]?.label ?? id;
}

export const LATERALIDADE_LABEL: Record<Lateralidade, string> = {
  D: "Direita",
  E: "Esquerda",
  central: "Central / linha média",
};

/** "Antebraço (D)" — combina região + lateralidade pra legenda. */
export function regiaoComLado(
  id?: string | null,
  lado?: Lateralidade | null,
): string {
  const base = regiaoLabel(id);
  if (!base) return "";
  if (!lado || lado === "central") return base;
  return `${base} (${lado})`;
}
