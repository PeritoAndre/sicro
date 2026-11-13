/** Markdown das notas em texto corrido: títulos, negrito e links viram texto. */
export function notasEmTexto(md: string): string {
  return md
    .replace(/^#{1,6}\s*/gm, "")
    .replace(/\*\*([^*]+)\*\*/g, "$1")
    .replace(/`([^`]+)`/g, "$1")
    .replace(/\[([^\]]+)\]\([^)]+\)/g, "$1")
    .replace(/^\|.*$/gm, "")
    .replace(/\n{3,}/g, "\n\n")
    .trim();
}
