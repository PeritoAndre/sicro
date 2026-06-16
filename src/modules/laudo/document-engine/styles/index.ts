/**
 * Sistema de estilos documentais do laudo pericial — barrel público.
 */

export {
  LAUDO_STYLES,
  LAUDO_STYLES_BY_ID,
  laudoStylesByCategory,
  findLaudoStyle,
  type LaudoStyleId,
  type LaudoStyleTarget,
  type LaudoStyleDefinition,
} from "./definitions";
export { LaudoStyleAttribute } from "./LaudoStyleAttribute";
export {
  applyLaudoStyle,
  removeLaudoStyle,
  getCurrentLaudoStyle,
} from "./applyStyle";
