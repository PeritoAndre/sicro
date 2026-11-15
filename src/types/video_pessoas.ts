/** Espelho TS de `commands/pessoas_commands.rs` (rastreio no vídeo) (snake_case: o backend serializa assim). */

import type { EscolhaMotor, Id, Problema } from "./edicao";
import type { VideoStoryboardFrame } from "./video";

export interface PessoaView {
  id: string;
  numero: number;
  nome: string;
  cor: string;
  /** "auto" | "editado" */
  estado: "auto" | "editado";
  /** Classe COCO (0 = pessoa); -1 = mancha marcada à mão. */
  classe: number;
  /** Nome da classe (na mancha, o nome dado pelo perito). */
  classe_nome: string;
  origem: "auto" | "manual";
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
  /** Tamanho do vídeo exibido, enquanto não há rastreio. */
  tamanho: { width: number; height: number } | null;
}

export interface OpcoesRastreio {
  limiar: number;
  passo: number;
  /** Classes COCO a detectar. */
  classes: number[];
}

export interface ClasseInfo {
  id: number;
  nome: string;
  grupo: string;
}

export const CLASSE_MANUAL = -1;

export interface MarcaMancha {
  /** Trilha a continuar; ausente = mancha nova. */
  id?: string;
  t: number;
  x: number;
  y: number;
  w: number;
  h: number;
  nome: string;
  direcao: "frente" | "tras" | "ambos";
  /** Ausente = "auto". */
  motor?: EscolhaMotor;
}

export interface AvisoMancha {
  sentido: "frente" | "tras";
  motivo: "fim_do_video" | "confianca" | "saiu_do_quadro" | "limite";
  parou_em: number | null;
  quadros: number;
}

export interface ResultadoMancha {
  estado: PessoasEstado;
  id: string;
  avisos: AvisoMancha[];
}

export type ModoDesenho = "numero" | "numero_seta" | "completo";

export interface OpcoesDesenho {
  modo: ModoDesenho;
  fonte: number;
  /** Escrever o nome da classe ao lado do número. */
  classe: boolean;
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
  fase: "rastrear" | "exportar" | "mancha";
  feitos: number;
  total: number;
  pessoas: number;
}

/** "Atualizar" (§6.4): a análise da Edição diz que a trilha de origem desse rastreio mudou depois da cópia
 *  (`pessoas_commands::desatualizados`: `campo` = id do rastreio). */
export function rastreioDesatualizado(problemas: readonly Problema[], rastreio: Id): boolean {
  return problemas.some((p) => p.codigo === "retrato_desatualizado" && p.campo === rastreio);
}
