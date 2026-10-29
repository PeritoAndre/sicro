/** Espelho TS de `commands/pessoas_commands.rs` (snake_case: o backend serializa assim). */

import type { VideoStoryboardFrame } from "./video";

export interface PessoaView {
  id: string;
  numero: number;
  nome: string;
  cor: string;
  /** "auto" | "editado" */
  estado: "auto" | "editado";
  primeiro_s: number;
  ultimo_s: number;
  quadros: number;
  /** Caixas efetivas (automática + correções): [t, x, y, w, h] em pixels do vídeo. */
  caixas: [number, number, number, number, number][];
  /** Instantes com correção manual. */
  ajustes_t: number[];
}

export interface PessoasRun {
  media_hash: string;
  model: string;
  runtime: string;
  params_json: string;
  width: number;
  height: number;
  frame_dur_s: number;
  step: number;
  frames_total: number;
  frames_analyzed: number;
  elapsed_s: number;
  created_at: string;
}

export interface PessoasEstado {
  run: PessoasRun | null;
  pessoas: PessoaView[];
}

export interface OpcoesRastreio {
  limiar: number;
  passo: number;
}

export type ModoDesenho = "numero" | "numero_seta" | "completo";

export interface OpcoesDesenho {
  modo: ModoDesenho;
  fonte: number;
}

export type EdicaoPessoa =
  | { op: "juntar"; destino: string; origem: string }
  | { op: "separar"; id: string; t: number }
  | { op: "apagar"; id: string }
  | { op: "nome"; id: string; nome: string }
  | { op: "cor"; id: string; cor: string }
  | { op: "ordem"; ids: string[] }
  | { op: "numero"; id: string; numero: number }
  | { op: "ajustar"; id: string; t: number; x: number; y: number; w: number; h: number }
  | { op: "limpar_ajuste"; id: string; t: number };

export interface ArquivoGerado {
  caminho: string;
  absoluto: string;
  sha256: string;
  tamanho: number;
}

export interface QuadroPessoas {
  frame: VideoStoryboardFrame;
  sha256: string;
  numeros: number[];
}

export interface TabelaGerada {
  csv: ArquivoGerado;
  html: ArquivoGerado;
}

/** Evento Tauri "pessoas-progress". */
export interface PessoasProgresso {
  media_hash: string;
  fase: "rastrear" | "exportar";
  feitos: number;
  total: number;
  pessoas: number;
}
