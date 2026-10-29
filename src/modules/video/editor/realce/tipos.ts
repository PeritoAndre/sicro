import type { VideoMedia } from "@domain/video";

/** O que cada aba do Realce recebe do editor de vídeo. */
export interface CtxRealce {
  workspacePath: string;
  media: VideoMedia;
  /** Quadro de trabalho pedido ao instante atual do player. */
  currentTime: number;
  /** Uma cópia de vídeo foi criada (estabilizada, com lente): atualiza a lista e abre. */
  onVideoDerivado: (media: VideoMedia) => void;
  /** Escolhe um quadro: refaz o quadro de trabalho no instante dado (lente ligada ou não). */
  usarQuadro: (t: number) => Promise<void>;
}

export const fmtNum = (v: number, casas = 3): string => v.toFixed(casas).replace(".", ",");
