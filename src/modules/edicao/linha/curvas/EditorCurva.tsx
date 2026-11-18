/**
 * Editor da curva de uma propriedade animável (posição, escala, rotação, opacidade do sobreposto; volume; região do
 * enquadramento): as chaves como losangos sobre o item na linha, arrastáveis, e a interpolação (Linear, Suave,
 * Degrau). Dono: P4.2 (casca da abertura).
 */

import type { Animavel, Seg } from "@domain/edicao";

export interface EditorCurvaProps {
  /** Rótulo da propriedade (texto exato de contratos-fase4.md (i)). */
  rotulo: string;
  valor: Animavel<number>;
  /** Base de tempo das chaves → slots do programa (o item na linha). */
  deSlot(q: number): Seg;
  paraSlot(t: Seg): number;
  aplicar(v: Animavel<number>): void;
}

export function EditorCurva(props: EditorCurvaProps) {
  void props;
  return null;
}
