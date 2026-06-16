/** Barrel do engine corporal (carta de lesões). */
export {
  BODY_TEMPLATES,
  BODY_VIEW_ORDER,
  BODY_TEMPLATE_GROUPS,
  LEGACY_TEMPLATE_MAP,
  type ArtViewBox,
  type BodyTemplate,
  type BodyTemplateView,
  type BodyView,
} from "../assets/bodyTemplates";
export {
  LESAO_TIPOS,
  lesaoMeta,
  isLesaoTipo,
  type LesaoTipo,
  type LesaoTipoMeta,
} from "./lesions";
export {
  REGIOES,
  regiaoLabel,
  regiaoComLado,
  LATERALIDADE_LABEL,
  type Lateralidade,
  type RegiaoAnatomica,
} from "./regions";
export {
  POP_FRENTE,
  POP_COSTAS,
  POP_REGIOES,
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
  CORPO_SCHEMA_VERSION,
  coerceCorpoDoc,
  nextMarkerNumber,
  type SicroCorpoDoc,
  type SicroCorpoCanvas,
  type SicroLesaoMarker,
} from "./schema";
export { makeCorpoDoc, makeLesao, type MakeCorpoDocOptions } from "./factories";
export {
  buildLegend,
  summarizeLesoes,
  type LegendRow,
} from "./legend";
