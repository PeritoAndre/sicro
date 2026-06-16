export { Figure, FigCaption, type FigureKind, type FigureAlign } from "./Figure";
// Tabela como objeto de primeira classe (overhaul F1.2–F4): estende o
// @tiptap/extension-table com id + legenda + bordas/align/padding + resize.
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
} from "./SicroTable";
// Linhas/células com attrs aditivos (altura de linha F3 + valign F4).
export {
  SicroTableRow,
  SicroTableCell,
  SicroTableHeader,
} from "./SicroTableParts";
// Q — Shape: formas geométricas (rectangle, ellipse, arrow, line)
// pra anotação sobre fotos em laudos.
export { Shape, type ShapeKind, type ShapeWrapMode } from "./Shape";
// U — TextBox: caixa de texto editável estilo Word (objeto, fora do flow).
export {
  TextBox,
  type TextBoxWrapMode,
  type TextBoxBorderStyle,
} from "./TextBox";
export { Storyboard, StoryboardItem, type StoryboardEvidenceItem } from "./Storyboard";
export { SystemData, type SystemDataReviewStatus } from "./SystemData";
// F6 — Prancha fotográfica (1/2/4/6 fotos por página).
export {
  PhotoPlate,
  photoPlateSlots,
  photoPlateColumns,
  type PhotoPlateLayout,
  type PhotoPlateEntry,
} from "./PhotoPlate";
export {
  EvidenceTable,
  type EvidenceTableKind,
  type EvidenceTableColumn,
  type EvidenceTableRow,
} from "./EvidenceTable";
export {
  QuesitoList,
  QuesitoItem,
  QuesitoQuestion,
  QuesitoAnswer,
} from "./Quesito";
export { Signature } from "./Signature";
export { CrossReference } from "./CrossReference";
// Fórmula matemática (LaTeX), inline + bloco. Render via KaTeX.
export {
  MathInline,
  MathBlock,
  EDIT_MATH_EVENT,
  MATH_RENDER_VERSION,
} from "./MathFormula";
export {
  DynamicSummary,
  DynamicFigureList,
  DynamicTableList,
} from "./DynamicList";
