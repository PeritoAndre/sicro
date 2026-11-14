/**
 * Comandos do Diagnóstico (`commands/edicao_diagnostico_commands.rs`). Ficam aqui até o integrador da Fase 0
 * levá-los para `src/core/commands.ts`.
 */
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { toSicroError } from "@core/errors";

/** `p0_s` = pts do 1º quadro de vídeo; `inicio_s` = início do contêiner (os dois mudam com a versão do ffmpeg). */
export interface MidiaDiagnostico { nome: string; caminho: string; p0_s: number; inicio_s: number }
export interface AmbienteDiagnostico {
  so: string;
  arch: string;
  webview: string | null;
  sicro: string;
  variaveis: Record<string, string | null>;
}
/** `ffmpeg` = primeira linha do `-version` do binário que gerou as mídias. */
export interface MidiasDiagnostico { pasta: string; ffmpeg: string; midias: MidiaDiagnostico[]; ambiente: AmbienteDiagnostico }

/** Progresso do preparo: [feitas, total]. */
export const EVENTO_MIDIAS = "edicao:diagnostico_midias";

async function chamar<T>(cmd: string, args: Record<string, unknown>): Promise<T> {
  try {
    return await invoke<T>(cmd, args);
  } catch (err) {
    throw toSicroError(err);
  }
}

/** Prepara (ou reaproveita) as mídias de teste no cache da Edição. */
export async function edicaoDiagnosticoMidias(aoAvancar?: (feitas: number, total: number) => void): Promise<MidiasDiagnostico> {
  const parar = aoAvancar
    ? await listen<[number, number]>(EVENTO_MIDIAS, (e) => aoAvancar(e.payload[0], e.payload[1])).catch(() => null)
    : null;
  try {
    return await chamar<MidiasDiagnostico>("edicao_diagnostico_midias", {});
  } finally {
    parar?.();
  }
}

/** Grava o JSON em `app_cache_dir()/edicao/diagnostico-<data>.json`; devolve o caminho absoluto. */
export function edicaoDiagnosticoSalvar(json: unknown): Promise<string> {
  return chamar<string>("edicao_diagnostico_salvar", { json });
}
