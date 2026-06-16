/**
 * Subsistema de tabelas (F7) — barrel público.
 */

export {
  TABLE_TEMPLATES,
  TABLE_TEMPLATES_BY_ID,
  findTableTemplate,
  type TableTemplateDefinition,
} from "./templates";
export {
  extractTables,
  buildTableList,
  type TableEntry,
  type NumberedTableEntry,
} from "./extractTables";
export {
  DEFAULT_TABLE_CONTENT_WIDTH_PX,
  MIN_SEEDED_COL_WIDTH_PX,
  REGISTRATION_BLOCK_WEIGHTS,
  seedEqualColWidths,
  seedWeightedColWidths,
  registrationBlockColWidths,
  buildSeededTableJson,
} from "./tableDefaults";
