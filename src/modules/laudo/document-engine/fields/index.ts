/**
 * Campos automáticos do laudo — barrel público.
 */

export {
  LAUDO_FIELDS,
  LAUDO_FIELDS_BY_KEY,
  FIELD_GROUPS,
  isKnownFieldKey,
  findField,
  fieldsByGroup,
  requiredFields,
  groupLabel,
  type LaudoFieldGroup,
  type LaudoFieldSource,
  type LaudoFieldDefinition,
} from "./catalog";
export {
  resolveFieldValue,
  resolveDefinition,
  resolveFromSource,
  resolveAllFields,
  resolveFieldSpansInHtml,
  findMissingRequiredFields,
  type FieldResolveContext,
} from "./resolver";
export { FieldPlaceholder } from "./FieldPlaceholder";
export { FieldSuggestion } from "./FieldSuggestion";
