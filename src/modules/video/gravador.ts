/**
 * Importação de vídeo de gravador (DVR, P4.6): o formato é conferido antes de registrar (Hikvision antigo é
 * recusado com a frase do catálogo); a cópia de trabalho (MKV com os mesmos quadros) é feita e provada a partir do
 * arquivo escolhido, com avanço e Cancelar (P4.6b), e só então o original e a cópia entram no caso; abre a cópia.
 * Vídeo comum segue como sempre.
 */

import { listen } from "@tauri-apps/api/event";
import { create } from "zustand";
import { commands } from "@core/commands";
import type { AvancoGravador, VideoMedia } from "@domain/video";
import { separarErro } from "@modules/edicao/store/edicaoStore";
import { useVideoStore } from "./store/videoStore";

/** Extensões de gravador no diálogo de vídeos (Hikvision IMKH chega como `.mp4`; AVI sem índice, como `.avi`). */
export const EXT_GRAVADOR = ["dav", "ifv", "lvf", "264", "h264", "265", "h265", "hevc"];

/** Evento do avanço da cópia (`video_dvr_commands.rs::EVENTO_AVANCO`). */
export const EVENTO_AVANCO = "dvr-progress";

/** Fase da importação na tela: as duas do backend e o registro no caso (sem Cancelar). */
export type FaseImportacao = AvancoGravador["fase"] | "registro";

export const useImportacaoGravador = create<{ fase: FaseImportacao | null; fracao: number | null }>(() => ({ fase: null, fracao: null }));

/** Pede o fim da cópia em curso; a importação volta com `ImportacaoCancelada` e nada entra no caso. */
export function cancelarImportacao(): void {
  void commands.videoDvrCancelar().catch(() => undefined);
}

export class ImportacaoCancelada extends Error {
  constructor() {
    super("cancelado");
  }
}

export function ehCancelamento(e: unknown): boolean {
  return e instanceof ImportacaoCancelada || separarErro(e).codigo === "cancelado";
}

/** Cópia de trabalho de gravador (`derivation_json.kind = "copia_dvr"`): o original, o relógio do gravador no começo,
 *  as lacunas de pts e a taxa suposta (fluxo cru sem temporização, "25"); null se não for. */
export function copiaDeGravador(
  json: string | null | undefined,
): { origem: { sha256: string; filename: string }; relogio: string | null; lacunas: number; taxaSuposta: string | null } | null {
  if (!json) return null;
  try {
    const d = JSON.parse(json) as {
      kind?: string;
      source?: { sha256?: unknown; filename?: unknown };
      relogio?: unknown;
      lacunas?: unknown;
      taxa?: unknown;
      taxa_suposta?: unknown;
    };
    if (d?.kind !== "copia_dvr" || typeof d.source?.sha256 !== "string" || typeof d.source.filename !== "string") return null;
    const [num, den] = typeof d.taxa === "string" ? d.taxa.split("/").map(Number) : [];
    return {
      origem: { sha256: d.source.sha256, filename: d.source.filename },
      relogio: typeof d.relogio === "string" ? d.relogio : null,
      lacunas: Array.isArray(d.lacunas) ? d.lacunas.length : 0,
      taxaSuposta: d.taxa_suposta === true && num! > 0 && den! > 0 ? String(Math.round((num! / den!) * 1000) / 1000).replace(".", ",") : null,
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

/** Registra o arquivo no caso pela store; de gravador, faz antes a cópia de trabalho (avanço em
 *  `useImportacaoGravador`) e devolve ela. Cancelada: `ImportacaoCancelada`, nada no caso. Cópia que não sai: o
 *  original entra mesmo assim e o erro é a frase. */
export async function registrarVideo(workspacePath: string, caminho: string): Promise<VideoMedia> {
  let formato;
  try {
    formato = await commands.videoDvrConferir(caminho);
  } catch (e) {
    throw soFrase(e);
  }
  if (!formato) return useVideoStore.getState().registerMedia(workspacePath, caminho);
  const avanco = useImportacaoGravador.setState;
  avanco({ fase: "copia", fracao: null });
  const parar = await listen<AvancoGravador>(EVENTO_AVANCO, (e) => {
    if (useImportacaoGravador.getState().fase !== "registro") avanco({ fase: e.payload.fase, fracao: e.payload.fracao });
  }).catch(() => () => {});
  try {
    let preparo;
    try {
      preparo = await commands.videoDvrPreparar(workspacePath, caminho);
    } catch (e) {
      if (ehCancelamento(e)) throw new ImportacaoCancelada();
      await useVideoStore.getState().registerMedia(workspacePath, caminho).catch(() => undefined);
      throw soFrase(e);
    }
    avanco({ fase: "registro", fracao: null });
    const original = await useVideoStore.getState().registerMedia(workspacePath, caminho);
    try {
      const { media } = await commands.videoDvrRegistrar(workspacePath, original.id, preparo.chave);
      useVideoStore.setState((s) => ({ list: [media, ...s.list.filter((m) => m.id !== media.id)] }));
      return media;
    } catch (e) {
      throw soFrase(e);
    }
  } finally {
    parar();
    avanco({ fase: null, fracao: null });
  }
}
