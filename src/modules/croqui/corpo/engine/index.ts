/** Barrel do engine corporal (carta de lesões). */
export {
  BODY_TEMPLATES,
  BODY_VIEW_ORDER,
  BODY_TEMPLATE_GROUPS,
  LEGACY_TEMPLATE_MAP,
  type BodyTemplateView,
  type BodyView,
} from "../assets/bodyTemplates";
export {
  LESAO_TIPOS,
  lesaoMeta,
  isLesaoTipo,
  type LesaoTipo,
} from "./lesions";
export {
  REGIOES,
  regiaoLabel,
  regiaoComLado,
  LATERALIDADE_LABEL,
  type Lateralidade,
} from "./regions";
export {
  POP_FRENTE,
  POP_COSTAS,
  popRegiaoId,
  popRegiaoLabel,
  isPopRegiao,
  popListaLinhas,
  popPointKey,
  type PopRegiao,
  type PopSide,
  type PopCalibration,
} from "./regionsPop";
export {
  coerceCorpoDoc,
  nextMarkerNumber,
  type SicroCorpoDoc,
  type SicroLesaoMarker,
} from "./schema";
export { makeCorpoDoc, makeLesao } from "./factories";
export {
  buildLegend,
  summarizeLesoes,
  type LegendRow,
} from "./legend";
