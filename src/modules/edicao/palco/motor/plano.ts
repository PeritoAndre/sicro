/**
 * Plano da prévia viva (modo "Programa", P3.3; §7.2, §7.4): o que toca em cada slot do programa, montado do documento
 * e da linha resolvida com as MESMAS contas do Rust (`modelo/tempo.ts`, `modelo/sobreposto.ts`); `estadoEm` diz o que
 * pintar num slot. Puro, sem DOM. Dono: P3.3 (casca da abertura, contratos-fase4.md B8).
 */

import type { Id, LinhaResolvida, ProjetoEdicao, Racional, Seg, Velocidade } from "@domain/edicao";
import type { TransformacaoAvaliada } from "../../modelo/sobreposto";

/** Um clipe da principal em slots `[de_q, ate_q)` do programa. `fonte`: a mídia que toca (trecho, quadro congelado);
 *  `inicio_s` = tempo de fonte do quadro do slot `de_q`. `entrada`: a transição de entrada (os `n_q` primeiros slots
 *  misturam com o segmento anterior; "escurecer" passa pelo fundo). */
export interface SegmentoPrevia {
  clipe: Id;
  de_q: number;
  ate_q: number;
  fonte: null | { midia: Id; sha256: string; inicio_s: Seg; velocidade: Velocidade; congelado: boolean };
  entrada: null | { tipo: string; n_q: number };
}

/** Um sobreposto (título, texto livre, imagem) em slots `[de_q, ate_q)`, na ordem de pintura (trilhas de baixo para cima). */
export interface LivrePrevia {
  id: Id;
  trilha: Id;
  de_q: number;
  ate_q: number;
}

/** Som em slots `[de_q, ate_q)`: o do trecho (barra "original") ou um clipe de trilha de áudio ("narracao"). */
export interface SomPrevia {
  id: Id;
  midia: Id;
  sha256: string;
  de_q: number;
  ate_q: number;
  /** Tempo de fonte (trecho) ou do arquivo (clipe de áudio) no slot `de_q`. */
  inicio_s: Seg;
  ganho: number;
  barra: "original" | "narracao";
}

export interface PlanoPrevia {
  fps: Racional;
  total_q: number;
  segmentos: SegmentoPrevia[];
  livres: LivrePrevia[];
  sons: SomPrevia[];
}

/** Uma camada de vídeo no slot: o segmento, o tempo dele (fonte ou local) e o peso na transição (1 = sozinho). */
export interface CamadaPrevia {
  segmento: SegmentoPrevia;
  t: Seg;
  peso: number;
}

/** O que pintar no slot `q`: vídeo (um ou dois segmentos na transição) e os sobrepostos com a transformação. */
export interface EstadoPrevia {
  q: number;
  programa_s: Seg;
  camadas: CamadaPrevia[];
  livres: { livre: LivrePrevia; tau: Seg; transformacao: TransformacaoAvaliada }[];
}

export function planoDe(doc: ProjetoEdicao, linha: LinhaResolvida): PlanoPrevia {
  void doc;
  return { fps: linha.fps, total_q: linha.total_q, segmentos: [], livres: [], sons: [] };
}

export function estadoEm(plano: PlanoPrevia, q: number): EstadoPrevia {
  void plano;
  return { q, programa_s: 0, camadas: [], livres: [] };
}
