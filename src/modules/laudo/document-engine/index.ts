export { laudoExtensions } from "./extensions";
// N — Extensões TipTap reduzidas para o cabeçalho Word-style.
export {
  headerExtensions,
  type HeaderExtensionsOptions,
} from "./header-extensions";
export {
  coerceSicroDoc,
  // P25 — Migração das fotos flutuantes (rebase do frame global antigo).
  sanitizeFloatingFigures,
  FLOATING_FIGURE_REBASE_THRESHOLD_CM,
  emptyDocContent,
  SCHEMA_VERSION,
  type SicroDoc,
  type SicroDocLayout,
  type SicroDocMetadata,
  type SicroDocPage,
  type SicroDocPageMargins,
  // Numeração de página configurável (cabeçalho do PDF)
  type SicroDocPageNumber,
  type PageNumberAlign,
  DEFAULT_PAGE_NUMBER,
  PAGE_NUMBER_FONTS,
  resolvePageNumber,
  // F8 — Schema 1.1.0
  type SicroDocStatus,
  type SicroDocComment,
  type SicroDocCommentReply,
  type SicroDocSnapshot,
  type SicroDocFinalization,
  // F12.11 — Assinatura digital
  type SicroDocSignature,
  // N — Schema 1.2.0 (cabeçalho Word-style)
  type SicroDocHeader,
  emptyHeaderContent,
  clampHeaderHeightCm,
  DEFAULT_HEADER_HEIGHT_CM,
  HEADER_HEIGHT_MIN_CM,
  HEADER_HEIGHT_MAX_CM,
  // W — Rodapé Word-style (simétrico ao cabeçalho)
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
export { buildSicroDoc, unwrapContent } from "./serializer";
export { renderSicroDocToHtml, type RenderOptions } from "./renderer";
export { numberFigures } from "./numbering";
export {
  validateSicroDoc,
  type DocumentWarning,
  type WarningSeverity,
  type ValidationCategory,
} from "./validators";
export {
  TEMPLATES,
  findTemplate,
  type LaudoTemplate,
  type OccurrenceContext,
} from "./templates";
export {
  INSTITUTIONAL_TEMPLATES,
  BLANK_V1,
  PCA_PADRAO_V1,
  findInstitutionalTemplate,
  resolveHeaderField,
  // N12 — Migração de docs legados.
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
  joinWorkspace,
} from "./relative-src";
export {
  Figure,
  FigCaption,
  Storyboard,
  StoryboardItem,
  SystemData,
  EvidenceTable,
  QuesitoList,
  QuesitoItem,
  QuesitoQuestion,
  QuesitoAnswer,
  Signature,
  PhotoPlate,
  photoPlateSlots,
  photoPlateColumns,
  type FigureKind,
  type FigureAlign,
  type StoryboardEvidenceItem,
  type SystemDataReviewStatus,
  type EvidenceTableKind,
  type EvidenceTableColumn,
  type EvidenceTableRow,
  type PhotoPlateLayout,
  type PhotoPlateEntry,
} from "./nodes";

// F6 — Extração + numeração de figuras (paralelo a sections).
//
// Note: `buildFigureList` é distinto do `numberFigures` legado em
// `./numbering.ts` (que injeta números na legenda do JSON inteiro durante
// render). O `buildFigureList` apenas extrai uma lista numerada para a UI.
export {
  extractFigures,
  buildFigureList,
  type FigureEntry,
  type NumberedFigureEntry,
  type NumberingMode,
} from "./figures";

// F7.5 — Setter dinâmico das opções de paginação (margens, gap…). O
// EditorPage chama isto em useEffect quando margens mudam para fazer o
// conteúdo se reorganizar automaticamente.
export {
  setPaginationOptions,
  type PaginationOptions,
} from "./pagination";

// F10 — Biblioteca de blocos reutilizáveis (texto pericial padronizado).
export {
  BLOCK_CATEGORIES,
  BUILTIN_BLOCKS,
  deleteCustomBlock,
  findBlock as findBlockDef,
  listAllBlocks,
  listBlocksByCategory,
  loadCustomBlocks,
  saveCustomBlock,
  type BlockCategory,
  type BlockDefinition,
} from "./blocks";

// F7 — Tabelas-modelo periciais + extração/numeração de tabelas.
export {
  TABLE_TEMPLATES,
  TABLE_TEMPLATES_BY_ID,
  findTableTemplate,
  extractTables,
  buildTableList,
  type TableTemplateDefinition,
  type TableEntry,
  type NumberedTableEntry,
  // F1.2 — Geração de larguras de coluna (colwidth) padrão.
  seedEqualColWidths,
  seedWeightedColWidths,
  registrationBlockColWidths,
  buildSeededTableJson,
  DEFAULT_TABLE_CONTENT_WIDTH_PX,
  MIN_SEEDED_COL_WIDTH_PX,
} from "./tables";

// F1.2/F4 — Tabela de primeira classe (id + legenda + bordas/align/padding).
export {
  SicroTable,
  generateTableId,
  tablePresentationStyle,
  tablePresentationDataAttrs,
  DEFAULT_TABLE_BORDER_COLOR,
  DEFAULT_TABLE_BORDER_WIDTH,
  DEFAULT_TABLE_CELL_PADDING,
  type TableAlign,
  type TableBorderStyle,
} from "./nodes";

// F4 — Sistema de estilos documentais (12+ estilos).
export {
  LAUDO_STYLES,
  LAUDO_STYLES_BY_ID,
  laudoStylesByCategory,
  findLaudoStyle,
  LaudoStyleAttribute,
  applyLaudoStyle,
  removeLaudoStyle,
  getCurrentLaudoStyle,
  type LaudoStyleId,
  type LaudoStyleTarget,
  type LaudoStyleDefinition,
} from "./styles";

// F4 — Sumário / outline / numeração automática de seções.
export {
  extractOutline,
  numberOutline,
  type OutlineEntry,
  type NumberedOutlineEntry,
} from "./sections";

// F8 — Comments service + snapshots service.
export {
  createComment,
  addComment,
  updateComment,
  resolveComment,
  unresolveComment,
  deleteComment,
  addReply,
  countActiveComments,
  extractCommentAnchors,
  type CommentAnchorInfo,
} from "./comments";
export {
  MAX_SNAPSHOTS,
  createSnapshot,
  pushSnapshot,
  deleteSnapshot,
} from "./snapshots";

// F5 — Campos automáticos `{{var}}` + resolver + catálogo.
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
  FieldPlaceholder,
  type LaudoFieldGroup,
  type LaudoFieldSource,
  type LaudoFieldDefinition,
  type FieldResolveContext,
} from "./fields";
