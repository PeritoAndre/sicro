/** Paleta categórica dos gráficos. Mistura tokens (tema-aware) + alguns hexes. */
export const PALETTE = [
  "var(--sicro-accent)",
  "var(--sicro-info)",
  "var(--sicro-success)",
  "var(--sicro-warning)",
  "var(--sicro-danger)",
  "#9b8cff",
  "#4ec9c0",
  "#e6804f",
  "#7aa2f7",
  "#c792ea",
];

export function colorAt(i: number): string {
  const n = PALETTE.length;
  return PALETTE[((i % n) + n) % n] ?? "var(--sicro-accent)";
}
