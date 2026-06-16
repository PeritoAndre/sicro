/**
 * document-engine — barrel público (SICRO 3.0).
 *
 * Com a aposentadoria do editor de texto in-app, o motor de documento ficou
 * reduzido às peças que ainda sustentam o fluxo `.docx`: schema/coerção,
 * layout de página, campos automáticos + resolver, templates (institucional e
 * de conteúdo), branding institucional e os helpers de path de evidência. As
 * extensões TipTap, nós, marcas, paginação, render HTML/PDF, comentários,
 * snapshots, estilos e numeração viviam só no editor e foram removidos.
 */

export {
  coerceSicroDoc,
  // Migração das fotos flutuantes (rebase do frame global antigo).
  sanitizeFloatingFigures,
  FLOATING_FIGURE_REBASE_THRESHOLD_CM,
  emptyDocContent,
  SCHEMA_VERSION,
  type SicroDoc,
  type SicroDocLayout,
  type SicroDocMetadata,
  type SicroDocPage,
  type SicroDocPageMargins,
  // Numeração de página configurável
  type SicroDocPageNumber,
  type PageNumberAlign,
  DEFAULT_PAGE_NUMBER,
  PAGE_NUMBER_FONTS,
  resolvePageNumber,
  // Schema 1.1.0
  type SicroDocStatus,
  type SicroDocComment,
  type SicroDocCommentReply,
  type SicroDocSnapshot,
  type SicroDocFinalization,
  // Assinatura digital
  type SicroDocSignature,
  // Schema 1.2.0 (cabeçalho Word-style)
  type SicroDocHeader,
  emptyHeaderContent,
  clampHeaderHeightCm,
  DEFAULT_HEADER_HEIGHT_CM,
  HEADER_HEIGHT_MIN_CM,
  HEADER_HEIGHT_MAX_CM,
  // Rodapé Word-style (simétrico ao cabeçalho)
  type SicroDocFooter,
  emptyFooterContent,
  clampFooterHeightCm,
  DEFAULT_FOOTER_HEIGHT_CM,
  FOOTER_HEIGHT_MIN_CM,
  FOOTER_HEIGHT_MAX_CM,
} from "./schema";
export {
  A4_PAGE,
  DEFAULT_PAGE_MARGINS,
  formatCm,
  marginsInCm,
  parseLengthCm,
  resolveEffectiveMargins,
} from "./page-layout";
export {
  TEMPLATES,
  findTemplate,
  findTemplateWithLegacyAlias,
  type LaudoTemplate,
  type OccurrenceContext,
} from "./templates";
export {
  INSTITUTIONAL_TEMPLATES,
  BLANK_V1,
  PCA_PADRAO_V1,
  findInstitutionalTemplate,
  resolveHeaderField,
  seedHeaderContentFromInstitutionalTemplate,
  type InstitutionalTemplate,
  type InstitutionalTemplateId,
} from "./institutional-templates";
export {
  brandingPaths,
  getCachedBrandingAssets,
  invalidateBrandingCache,
  loadBrandingAssets,
  type BrandingAssets,
} from "./branding";
export {
  collectEvidencePaths,
  inlineEvidenceAssets,
  embedAssetsAsPortable,
  loadEvidenceAssets,
  type EvidenceAssetMap,
} from "./evidence-assets";
export {
  resolveEvidenceSrcsForEditor,
  normalizeEvidenceSrcsForSave,
} from "./relative-src";

// Campos automáticos `{{var}}` + resolver + catálogo.
export {
  LAUDO_FIELDS,
  LAUDO_FIELDS_BY_KEY,
  FIELD_GROUPS,
  isKnownFieldKey,
  findField,
  fieldsByGroup,
  requiredFields,
  groupLabel,
  resolveFieldValue,
  resolveDefinition,
  resolveFromSource,
  resolveAllFields,
  resolveFieldSpansInHtml,
  findMissingRequiredFields,
  type LaudoFieldGroup,
  type LaudoFieldSource,
  type LaudoFieldDefinition,
  type FieldResolveContext,
} from "./fields";
