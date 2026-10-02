/**
 * Pranchas do croqui corporal: PNGs (assets do Vite) tratados pelo perito a
 * partir do POP Local de Crime (SENASP/MJSP 2024, Anexo 1). Cada prancha tem
 * vistas lado a lado com box medido por varredura de silhueta; só os corpos
 * inteiros frente+costas recebem a numeração do POP, como no manual.
 */

import corpoMascSrc from "./art/corpo-masculino-frente-costas.png";
import corpoFemSrc from "./art/corpo-feminino-frente-costas.png";
import corpoLateralSrc from "./art/corpo-lateral.png";
import cabecaFrontalDorsalSrc from "./art/cabeca-frontal-dorsal.png";
import cabecaLateralSrc from "./art/cabeca-lateral.png";
import maoDireitaSrc from "./art/mao-direita.png";
import maoEsquerdaSrc from "./art/mao-esquerda.png";
import pernaDireitaSrc from "./art/perna-direita.png";
import pernaEsquerdaSrc from "./art/perna-esquerda.png";
import orelhasSrc from "./art/orelhas.png";

export type BodyView =
  | "corpo_masc"
  | "corpo_fem"
  | "corpo_lateral_masc"
  | "corpo_lateral_fem"
  | "cabeca_frontal_dorsal"
  | "cabeca_lateral"
  | "mao_direita"
  | "mao_esquerda"
  | "perna_direita"
  | "perna_esquerda"
  | "orelhas";

/** Retângulo (px da arte) que delimita uma vista dentro da prancha. */
interface ArtViewBox {
  x: number;
  y: number;
  w: number;
  h: number;
}

/** Uma vista nomeada da prancha (FRENTE, DORSO, PERFIL ESQUERDO…). */
export interface BodyTemplateView {
  /** "frente" | "costas" | rótulos livres das demais pranchas. */
  id: string;
  /** Título desenhado sob a vista e usado nas listas do export. */
  label: string;
  box: ArtViewBox;
}

interface BodyTemplate {
  id: BodyView;
  label: string;
  group: string;
  /** Dimensões nativas da arte (px): sistema de coordenadas dos markers. */
  width: number;
  height: number;
  src: string;
  /** Recorte (px do PNG original) quando a prancha usa só parte da imagem;
   *  `width`/`height` viram as dims do recorte e os boxes das vistas são
   *  relativos a ele. */
  crop?: ArtViewBox;
  views: BodyTemplateView[];
  /** true = recebe a numeração do POP (frente 1–28 / costas 1–21). */
  numbered: boolean;
}

// Boxes medidos por varredura de silhueta (threshold 235) sobre as artes.
export const BODY_TEMPLATES: Record<BodyView, BodyTemplate> = {
  corpo_masc: {
    id: "corpo_masc",
    label: "Masculino — frente e costas",
    group: "Corpo inteiro",
    width: 1536,
    height: 1024,
    src: corpoMascSrc,
    numbered: true,
    views: [
      { id: "frente", label: "FRENTE", box: { x: 320, y: 82, w: 280, h: 850 } },
      { id: "costas", label: "COSTAS", box: { x: 932, y: 82, w: 280, h: 841 } },
    ],
  },
  corpo_fem: {
    id: "corpo_fem",
    label: "Feminino — frente e costas",
    group: "Corpo inteiro",
    width: 1536,
    height: 1024,
    src: corpoFemSrc,
    numbered: true,
    views: [
      { id: "frente", label: "FRENTE", box: { x: 337, y: 82, w: 250, h: 838 } },
      { id: "costas", label: "COSTAS", box: { x: 949, y: 82, w: 246, h: 834 } },
    ],
  },
  // Os dois perfis dividem a mesma arte (corpo-lateral.png), recortada por
  // figura; são pranchas independentes, cada uma com suas marcações.
  corpo_lateral_masc: {
    id: "corpo_lateral_masc",
    label: "Masculino — em pé, perfil",
    group: "Corpo inteiro",
    width: 330,
    height: 1272,
    src: corpoLateralSrc,
    crop: { x: 139, y: 120, w: 330, h: 1272 },
    numbered: false,
    views: [
      { id: "perfil", label: "PERFIL — MASCULINO", box: { x: 50, y: 50, w: 229, h: 1172 } },
    ],
  },
  corpo_lateral_fem: {
    id: "corpo_lateral_fem",
    label: "Feminino — em pé, perfil",
    group: "Corpo inteiro",
    width: 309,
    height: 1257,
    src: corpoLateralSrc,
    crop: { x: 586, y: 134, w: 309, h: 1257 },
    numbered: false,
    views: [
      { id: "perfil", label: "PERFIL — FEMININO", box: { x: 50, y: 50, w: 209, h: 1157 } },
    ],
  },
  cabeca_frontal_dorsal: {
    id: "cabeca_frontal_dorsal",
    label: "Cabeça/pescoço — frontal e dorsal",
    group: "Cabeça e pescoço",
    width: 1536,
    height: 1024,
    src: cabecaFrontalDorsalSrc,
    numbered: false,
    views: [
      { id: "frontal", label: "FRONTAL", box: { x: 158, y: 134, w: 567, h: 641 } },
      { id: "dorsal", label: "DORSAL", box: { x: 829, y: 134, w: 596, h: 642 } },
    ],
  },
  cabeca_lateral: {
    id: "cabeca_lateral",
    label: "Cabeça/pescoço — perfis",
    group: "Cabeça e pescoço",
    width: 1536,
    height: 1024,
    src: cabecaLateralSrc,
    numbered: false,
    views: [
      { id: "perfil_esq", label: "PERFIL ESQUERDO", box: { x: 207, y: 145, w: 474, h: 691 } },
      { id: "perfil_dir", label: "PERFIL DIREITO", box: { x: 910, y: 146, w: 480, h: 690 } },
    ],
  },
  mao_direita: {
    id: "mao_direita",
    label: "Mão direita — palma e dorso",
    group: "Mãos",
    width: 1024,
    height: 1536,
    src: maoDireitaSrc,
    numbered: false,
    views: [
      { id: "palma", label: "PALMA", box: { x: 112, y: 415, w: 388, h: 626 } },
      { id: "dorso", label: "DORSO", box: { x: 551, y: 418, w: 381, h: 622 } },
    ],
  },
  mao_esquerda: {
    id: "mao_esquerda",
    label: "Mão esquerda — palma e dorso",
    group: "Mãos",
    width: 1014,
    height: 806,
    src: maoEsquerdaSrc,
    numbered: false,
    views: [
      { id: "palma", label: "PALMA", box: { x: 60, y: 60, w: 383, h: 630 } },
      { id: "dorso", label: "DORSO", box: { x: 533, y: 60, w: 421, h: 686 } },
    ],
  },
  perna_direita: {
    id: "perna_direita",
    label: "Perna direita — anterior e posterior",
    group: "Pernas",
    width: 1024,
    height: 1536,
    src: pernaDireitaSrc,
    numbered: false,
    views: [
      { id: "anterior", label: "VISTA ANTERIOR", box: { x: 135, y: 268, w: 278, h: 939 } },
      { id: "posterior", label: "VISTA POSTERIOR", box: { x: 665, y: 269, w: 223, h: 937 } },
    ],
  },
  perna_esquerda: {
    id: "perna_esquerda",
    label: "Perna esquerda — anterior e posterior",
    group: "Pernas",
    width: 1024,
    height: 1536,
    src: pernaEsquerdaSrc,
    numbered: false,
    views: [
      // Arte espelhada da perna direita: posterior à esquerda, anterior à direita.
      { id: "posterior", label: "VISTA POSTERIOR", box: { x: 136, y: 269, w: 223, h: 937 } },
      { id: "anterior", label: "VISTA ANTERIOR", box: { x: 611, y: 268, w: 278, h: 939 } },
    ],
  },
  orelhas: {
    id: "orelhas",
    label: "Orelhas — esquerda e direita",
    group: "Orelhas",
    width: 1024,
    height: 1536,
    src: orelhasSrc,
    numbered: false,
    views: [
      { id: "orelha_esq", label: "ORELHA ESQUERDA", box: { x: 148, y: 451, w: 281, h: 502 } },
      { id: "orelha_dir", label: "ORELHA DIREITA", box: { x: 612, y: 451, w: 282, h: 502 } },
    ],
  },
};

/** Ordem de exibição no seletor (agrupada). */
export const BODY_VIEW_ORDER: BodyView[] = [
  "corpo_masc",
  "corpo_fem",
  "corpo_lateral_masc",
  "corpo_lateral_fem",
  "cabeca_frontal_dorsal",
  "cabeca_lateral",
  "mao_direita",
  "mao_esquerda",
  "perna_direita",
  "perna_esquerda",
  "orelhas",
];

/** Grupos pro seletor (<optgroup>) na ordem de exibição. */
export const BODY_TEMPLATE_GROUPS: ReadonlyArray<{
  group: string;
  views: BodyView[];
}> = [
  {
    group: "Corpo inteiro",
    views: ["corpo_masc", "corpo_fem", "corpo_lateral_masc", "corpo_lateral_fem"],
  },
  { group: "Cabeça e pescoço", views: ["cabeca_frontal_dorsal", "cabeca_lateral"] },
  { group: "Mãos", views: ["mao_direita", "mao_esquerda"] },
  { group: "Pernas", views: ["perna_direita", "perna_esquerda"] },
  { group: "Orelhas", views: ["orelhas"] },
];

/** Templates legados (pranchas SVG antigas) → prancha nova; o coerceCorpoDoc
 *  re-escala os markers. */
export const LEGACY_TEMPLATE_MAP: Record<string, BodyView> = {
  corpo_completo: "corpo_masc",
  anterior: "corpo_masc",
  posterior: "corpo_masc",
  cabeca_frontal: "cabeca_frontal_dorsal",
  // Prancha antiga com os dois perfis juntos → perfil masculino.
  corpo_lateral: "corpo_lateral_masc",
};
