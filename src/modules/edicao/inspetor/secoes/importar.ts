/** Arquivo do perito que entra no projeto (imagem, narração): escolhido no diálogo do sistema e copiado pelo Rust. */

import { open } from "@tauri-apps/plugin-dialog";
import { commands } from "@core/commands";
import type { MidiaRef } from "@domain/edicao";
import { pushToast } from "@components/toast/toastStore";
import { separarErro, useEdicaoStore } from "../../store/edicaoStore";

// as mesmas de `armazem::{EXT_IMAGEM, EXT_AUDIO}`: o que o diálogo oferece o Rust aceita
export const EXTENSOES: Record<"imagem" | "audio", string[]> = {
  imagem: ["png", "jpg", "jpeg", "webp", "bmp"],
  audio: ["wav", "mp3", "m4a", "aac", "ogg", "opus", "flac", "webm", "mp4"],
};

/** null = o perito desistiu, ou o backend recusou (o erro sai no aviso passageiro, com a frase do catálogo). */
export async function escolherMidia(tipo: "imagem" | "audio"): Promise<MidiaRef | null> {
  const { ws, aberta } = useEdicaoStore.getState();
  if (!ws || !aberta) return null;
  const escolhido = await open({ multiple: false, directory: false, filters: [{ name: tipo === "imagem" ? "Imagem" : "Áudio", extensions: EXTENSOES[tipo] }] });
  if (typeof escolhido !== "string") return null;
  try {
    return await commands.edicaoMidiaImportar(ws, aberta.projetoId, escolhido, tipo);
  } catch (e) {
    pushToast("error", separarErro(e).frase);
    return null;
  }
}
