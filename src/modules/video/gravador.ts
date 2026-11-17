/**
 * Importação de vídeo de gravador (DVR, P4.6): o formato é conferido antes de registrar (Hikvision antigo é
 * recusado com a frase do catálogo); registrado o original, entra a cópia de trabalho (MKV com os mesmos quadros),
 * que é o que abre. Vídeo comum segue como sempre.
 */

import { commands } from "@core/commands";
import type { VideoMedia } from "@domain/video";
import { separarErro } from "@modules/edicao/store/edicaoStore";
import { useVideoStore } from "./store/videoStore";

/** Extensões de gravador no diálogo de vídeos (Hikvision IMKH chega como `.mp4`; AVI sem índice, como `.avi`). */
export const EXT_GRAVADOR = ["dav", "ifv", "lvf", "264", "h264", "265", "h265", "hevc"];

/** Cópia de trabalho de gravador (`derivation_json.kind = "copia_dvr"`): o original, o relógio do gravador no começo
 *  e as lacunas de pts; null se não for. */
export function copiaDeGravador(
  json: string | null | undefined,
): { origem: { sha256: string; filename: string }; relogio: string | null; lacunas: number } | null {
  if (!json) return null;
  try {
    const d = JSON.parse(json) as { kind?: string; source?: { sha256?: unknown; filename?: unknown }; relogio?: unknown; lacunas?: unknown };
    if (d?.kind !== "copia_dvr" || typeof d.source?.sha256 !== "string" || typeof d.source.filename !== "string") return null;
    return {
      origem: { sha256: d.source.sha256, filename: d.source.filename },
      relogio: typeof d.relogio === "string" ? d.relogio : null,
      lacunas: Array.isArray(d.lacunas) ? d.lacunas.length : 0,
    };
  } catch {
    return null;
  }
}

/** Erro com código do catálogo (`codigo: frase`) vira só a frase. */
function soFrase(e: unknown): Error {
  const { codigo, frase } = separarErro(e);
  return new Error(codigo && frase ? frase : codigo ?? frase);
}

/** Registra o arquivo no caso pela store; de gravador, faz também a cópia de trabalho e devolve ela. */
export async function registrarVideo(workspacePath: string, caminho: string): Promise<VideoMedia> {
  let formato;
  try {
    formato = await commands.videoDvrConferir(caminho);
  } catch (e) {
    throw soFrase(e);
  }
  const original = await useVideoStore.getState().registerMedia(workspacePath, caminho);
  if (!formato) return original;
  try {
    const { media } = await commands.videoDvrCopia(workspacePath, original.id);
    useVideoStore.setState((s) => ({ list: [media, ...s.list.filter((m) => m.id !== media.id)] }));
    return media;
  } catch (e) {
    throw soFrase(e);
  }
}
