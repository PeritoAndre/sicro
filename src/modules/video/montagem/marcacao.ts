/** O que o usuário está marcando sobre a prévia do trecho. */
export type AlvoMarcacao =
  | { item: string; tipo: "zoom"; ponta: "inicio" | "fim" }
  | { item: string; tipo: "destaque"; id: string; ponto: boolean }
  | { item: string; tipo: "desfoque"; id: string };

/** Região (ou ponto, sem w/h) desenhada na prévia, normalizada. */
export interface Rascunho {
  x: number;
  y: number;
  w?: number;
  h?: number;
}

export const mesmoAlvo = (a: AlvoMarcacao | null, b: AlvoMarcacao): boolean => {
  if (!a || a.item !== b.item || a.tipo !== b.tipo) return false;
  if (a.tipo === "zoom" && b.tipo === "zoom") return a.ponta === b.ponta;
  return "id" in a && "id" in b && a.id === b.id;
};
