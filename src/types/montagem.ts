/** Espelha `Montagem*` em `src-tauri/src/models/video.rs` (snake_case = wire do serde). */

export type MontagemTamanho = "p" | "m" | "g";
export type MontagemPosicao = "topo" | "centro" | "rodape";
export type MontagemTransicao = "corte" | "fade";

export interface MontagemTexto {
  conteudo: string;
  posicao: MontagemPosicao;
  tamanho: MontagemTamanho;
  /** Mostra o tempo do vídeo original no canto. */
  carimbo_tempo: boolean;
}

export interface MontagemTrecho {
  tipo: "trecho";
  id: string;
  media_id: string;
  inicio_s: number;
  fim_s: number;
  /** 1 = normal, 0.5 = câmera lenta. */
  velocidade: number;
  texto?: MontagemTexto | null;
  transicao_entrada?: MontagemTransicao | null;
}

export interface MontagemCartela {
  tipo: "cartela";
  id: string;
  duracao_s: number;
  texto: string;
  cor_fundo: string;
  cor_texto: string;
  tamanho: MontagemTamanho;
  transicao_entrada?: MontagemTransicao | null;
}

export interface MontagemCongelar {
  tipo: "congelar";
  id: string;
  media_id: string;
  instante_s: number;
  duracao_s: number;
  texto?: MontagemTexto | null;
}

export type MontagemItem = MontagemTrecho | MontagemCartela | MontagemCongelar;

export interface MontagemSaida {
  largura: number;
  altura: number;
  fps: number;
  incluir_audio: boolean;
}

export interface Montagem {
  id: string;
  titulo: string;
  created_at: string;
  updated_at: string;
  itens: MontagemItem[];
  saida: MontagemSaida;
}

export interface RenderMontagemResult {
  output_relative_path: string;
  sha256: string;
  duracao_s: number;
}
