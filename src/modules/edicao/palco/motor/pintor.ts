/**
 * Pintor da prévia viva (§7.2): fundo, UM `drawImage(video)` por camada com `A(τ)` (zoom que segue incluído:
 * `modelo/segue.ts`), as primitivas dos efeitos (`palco/cena.ts`), os sobrepostos com a transformação
 * (`modelo/sobreposto.ts`, primitivas de `modelo/titulos.ts`) e a transição (5 grupos; o resto como fundido).
 * Canvas `alpha: false`, até 1280×720 tocando. Dono: P3.3 (casca da abertura, contratos-fase4.md B8).
 */

import type { Id, ProjetoEdicao } from "@domain/edicao";
import type { EstadoPrevia } from "./plano";

export interface FontesPintor {
  /** Elemento pronto do segmento (null = ainda não: pinta o quadro anterior). */
  video(clipe: Id): HTMLVideoElement | null;
  /** Sprite de texto ou imagem do projeto (`src` das primitivas); null = ainda não chegou. */
  imagem(src: string): CanvasImageSource | null;
}

export function pinta(ctx: CanvasRenderingContext2D, estado: EstadoPrevia, doc: ProjetoEdicao, fontes: FontesPintor, tela: { w: number; h: number }): void {
  void ctx;
  void estado;
  void doc;
  void fontes;
  void tela;
}
