/**
 * Espelha `src-tauri/src/commands/realce_commands.rs` (snake_case = wire do serde).
 * Realce é apoio de leitura: nada aqui substitui o vídeo original.
 */

import type { VideoMedia, VideoStoryboardFrame } from "./video";

export interface Roi {
  x: number;
  y: number;
  w: number;
  h: number;
}

export interface ResultadoRealce {
  id: string;
  media_hash: string;
  /** media | media_quadro | superres | retificada | quadro_lente | estabilizado | lente_video */
  tipo: string;
  /** Relativo ao caso. */
  caminho: string;
  sha256: string;
  tamanho: number;
  largura: number | null;
  altura: number | null;
  parametros: Record<string, unknown>;
  aviso: string;
  created_at: string;
}

export interface ArquivoRealce {
  resultado: ResultadoRealce;
  absoluto: string;
}

export interface QuadroTrabalho {
  absoluto: string;
  largura: number;
  altura: number;
  /** Instante real do quadro mostrado (s). */
  t: number;
  com_lente: boolean;
}

export interface ParamsMedia {
  t: number;
  roi: Roi;
  quadros: number;
  mediana: boolean;
  homografia: boolean;
  com_lente: boolean;
}

export interface InfoQuadro {
  indice: number;
  dx: number;
  dy: number;
  ncc: number;
  aceito: boolean;
}

export interface ResultadoMedia {
  roi: ArquivoRealce;
  quadro: ArquivoRealce;
  /** A mesma região crua do quadro de referência (lado a lado). */
  crua: string;
  usados: number;
  total: number;
  quadros: InfoQuadro[];
  t_ref: number;
  tempo_s: number;
}

export interface ParamsSuperRes {
  resultado_id: string | null;
  t: number;
  roi: Roi | null;
  escala: 2 | 4;
  com_lente: boolean;
}

export interface ResultadoSuperRes {
  arquivo: ArquivoRealce;
  /** Entrada sem realce, para o lado a lado. */
  original: string;
  tempo_s: number;
}

export interface ResultadoSuperResImagem {
  relative_path: string;
  absoluto: string;
  sha256: string;
  largura: number;
  altura: number;
  aviso: string;
  tempo_s: number;
}

export interface PerfilLente {
  nome: string;
  k1: number;
  k2: number;
  cx: number;
  cy: number;
}

export interface PerfilDeVideo {
  media_hash: string;
  filename: string;
  perfil: PerfilLente;
}

export interface PreviaLente {
  absoluto: string;
  largura: number;
  altura: number;
}

export interface ParIn {
  px: number;
  py: number;
  mx: number;
  my: number;
}

export interface ParamsPerspectiva {
  t: number;
  com_lente: boolean;
  pontos: ParIn[];
  margem: number;
  px_por_m: number | null;
  origem: string;
}

export interface InfoRetificada {
  largura: number;
  altura: number;
  px_por_m: number;
  /** Plano mostrado em metros: x mín., x máx., y mín., y máx. */
  mundo: [number, number, number, number];
}

export interface ResultadoPerspectiva {
  arquivo: ArquivoRealce;
  info: InfoRetificada;
}

export type BordaEstab = "cortar" | "preencher";

export interface ParamsEstabilizar {
  suavizar: number;
  borda: BordaEstab;
}

export interface VideoDerivado {
  media: VideoMedia;
  arquivo: ArquivoRealce;
  motor: string;
}

export interface EstadoEstabilizacao {
  vidstab: boolean;
  deshake: boolean;
}

/** Evento Tauri "realce-progress". */
export interface RealceProgresso {
  media_hash: string;
  fase: "ler" | "alinhar" | "superres" | "estab-analise" | "estab-grava" | "lente-video";
  feitos: number;
  total: number;
}

export type { VideoStoryboardFrame };
